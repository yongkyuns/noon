use noon_core::{
    resolve_animation_options, validate_track_definition, AnimationDefaults, AnimationOptions,
    AnimationOptionsError, CompositionTimeMap, ObjectId, Property, SemanticLoweringError,
    SemanticNodeId, SemanticObjectProperty, SemanticSceneOperationError, SemanticStore,
    TimelineError, TrackDefinition, TrackId, TrackTiming, TrackValues,
};

use super::super::{
    projection::SemanticLoweringError as StyleLoweringError, SemanticExecutionIndex,
};
use super::affine::{
    lower_affine_channels, validate_affine_payload, AffinePayloadIssue,
    EffectiveAnimationProperties, SemanticAffineAnimationField, SemanticAnimationCompletion,
};

/// One supported channel prepared for an animation declaration that has not committed yet.
///
/// This carries only execution-derived identity. Semantic animation identity is allocated by
/// the authoritative store commit after every execution mutation has passed preflight.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSemanticAffineTrack {
    pub target: SemanticNodeId,
    pub execution_object_id: ObjectId,
    pub property: Property,
    pub completion: SemanticAnimationCompletion,
    pub values: TrackValues,
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
}

impl PreparedSemanticAffineTrack {
    pub fn with_track_id(
        &self,
        id: TrackId,
    ) -> Result<TrackDefinition, PreparedSemanticTransformToError> {
        let track = TrackDefinition {
            id,
            object: self.execution_object_id,
            property: self.property,
            values: self.values.clone(),
            timing: self.timing,
            time_map: self.time_map.clone(),
        };
        validate_track_definition(&track)
            .map_err(PreparedSemanticTransformToError::InvalidTrack)?;
        Ok(track)
    }
}

/// Candidate-sized compiler projection used by atomic post-bootstrap `TransformTo` activation.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSemanticTransformToProjection {
    run_time: f64,
    tracks: Vec<PreparedSemanticAffineTrack>,
}

impl PreparedSemanticTransformToProjection {
    pub const fn run_time(&self) -> f64 {
        self.run_time
    }

    pub fn tracks(&self) -> &[PreparedSemanticAffineTrack] {
        &self.tracks
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedSemanticTransformToError {
    InvalidStartTime(f64),
    Options(AnimationOptionsError),
    Target {
        node: SemanticNodeId,
        error: SemanticSceneOperationError,
    },
    MissingExecutionTarget(SemanticNodeId),
    MissingEffectiveTransform {
        target: SemanticNodeId,
        execution_object_id: ObjectId,
    },
    InvalidEffectiveTransform(SemanticNodeId),
    InvalidEffectiveStyle(SemanticNodeId),
    UnsupportedContentChange,
    UnsupportedStyleChange,
    UnsupportedPainterOrderChange,
    UnsupportedBindingChange,
    UnsupportedDepthChange(SemanticAffineAnimationField),
    UnsupportedLifecycle {
        remover: bool,
        introducer: bool,
    },
    ReactiveDriverConflict(SemanticObjectProperty),
    InvalidTargetValue {
        field: SemanticAffineAnimationField,
        error: SemanticLoweringError,
    },
    TargetValueOutOfRange(SemanticAffineAnimationField),
    InvalidTargetStyle(StyleLoweringError),
    InvalidTrack(TimelineError),
}

impl std::fmt::Display for PreparedSemanticTransformToError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "prepared semantic TransformTo lowering failed: {self:?}"
        )
    }
}

impl std::error::Error for PreparedSemanticTransformToError {}

impl PreparedSemanticTransformToError {
    /// Whether this is an explicitly unsupported semantic payload rather than
    /// invalid provenance, identity, value, or execution state.
    pub const fn is_unsupported_payload(&self) -> bool {
        matches!(
            self,
            Self::UnsupportedContentChange
                | Self::UnsupportedStyleChange
                | Self::UnsupportedPainterOrderChange
                | Self::UnsupportedBindingChange
                | Self::UnsupportedDepthChange(_)
                | Self::UnsupportedLifecycle { .. }
                | Self::ReactiveDriverConflict(_)
        )
    }
}

/// Validate an inert TransformTo payload before any declaration, runtime, or
/// execution identity is created.
///
/// This is the shared capability check for language facades deciding whether a
/// typed ordinary transform can enter the canonical session. It deliberately
/// reads only the two store-owned semantic object states and resolved options.
pub fn validate_semantic_transform_to_payload(
    store: &SemanticStore,
    target: SemanticNodeId,
    target_state: SemanticNodeId,
    options: AnimationOptions,
) -> Result<(), PreparedSemanticTransformToError> {
    let source = store
        .semantic_object_state_checked(target)
        .map_err(|error| PreparedSemanticTransformToError::Target {
            node: target,
            error,
        })?;
    let target_object = store
        .semantic_object_state_checked(target_state)
        .map_err(|error| PreparedSemanticTransformToError::Target {
            node: target_state,
            error,
        })?;
    let options = resolve_animation_options(AnimationDefaults::MANIM, options, options)
        .map_err(PreparedSemanticTransformToError::Options)?;
    validate_affine_payload(source, target_object, options)
        .map_err(|issue| prepared_payload_error(target, issue))
}

/// Lower one not-yet-committed affine declaration without reserving semantic identity.
///
/// The result is inert until the session publishes it together with the semantic declaration.
/// Reads are limited to the source, target state, their execution mapping, and one current
/// effective transform/style row.
pub fn lower_prepared_semantic_transform_to<F>(
    store: &SemanticStore,
    index: &SemanticExecutionIndex,
    target: SemanticNodeId,
    target_state: SemanticNodeId,
    options: AnimationOptions,
    start_time: f64,
    mut effective_properties: F,
) -> Result<PreparedSemanticTransformToProjection, PreparedSemanticTransformToError>
where
    F: FnMut(ObjectId) -> Option<EffectiveAnimationProperties>,
{
    if !start_time.is_finite() {
        return Err(PreparedSemanticTransformToError::InvalidStartTime(
            start_time,
        ));
    }
    let source = store
        .semantic_object_state_checked(target)
        .map_err(|error| PreparedSemanticTransformToError::Target {
            node: target,
            error,
        })?;
    let target_object = store
        .semantic_object_state_checked(target_state)
        .map_err(|error| PreparedSemanticTransformToError::Target {
            node: target_state,
            error,
        })?;
    let execution_object_id = index.execution_object_id(target).ok_or(
        PreparedSemanticTransformToError::MissingExecutionTarget(target),
    )?;
    let options = resolve_animation_options(AnimationDefaults::MANIM, options, options)
        .map_err(PreparedSemanticTransformToError::Options)?;
    validate_affine_payload(source, target_object, options)
        .map_err(|issue| prepared_payload_error(target, issue))?;
    let from = effective_properties(execution_object_id).ok_or(
        PreparedSemanticTransformToError::MissingEffectiveTransform {
            target,
            execution_object_id,
        },
    )?;
    let channels = lower_affine_channels(target_state, source, target_object, from)
        .map_err(|issue| prepared_payload_error(target, issue))?;
    let timing = TrackTiming::new(start_time, options.run_time, options.rate_func);
    let tracks = channels
        .into_iter()
        .map(|channel| PreparedSemanticAffineTrack {
            target,
            execution_object_id,
            property: channel.property,
            completion: channel.completion,
            values: channel.values,
            timing,
            time_map: CompositionTimeMap::identity(),
        })
        .collect();
    Ok(PreparedSemanticTransformToProjection {
        run_time: options.run_time,
        tracks,
    })
}

fn prepared_payload_error(
    target: SemanticNodeId,
    issue: AffinePayloadIssue,
) -> PreparedSemanticTransformToError {
    match issue {
        AffinePayloadIssue::InvalidEffectiveTransform => {
            PreparedSemanticTransformToError::InvalidEffectiveTransform(target)
        }
        AffinePayloadIssue::InvalidEffectiveStyle => {
            PreparedSemanticTransformToError::InvalidEffectiveStyle(target)
        }
        AffinePayloadIssue::UnsupportedContentChange => {
            PreparedSemanticTransformToError::UnsupportedContentChange
        }
        AffinePayloadIssue::UnsupportedStyleChange => {
            PreparedSemanticTransformToError::UnsupportedStyleChange
        }
        AffinePayloadIssue::UnsupportedPainterOrderChange => {
            PreparedSemanticTransformToError::UnsupportedPainterOrderChange
        }
        AffinePayloadIssue::UnsupportedBindingChange => {
            PreparedSemanticTransformToError::UnsupportedBindingChange
        }
        AffinePayloadIssue::UnsupportedDepthChange(field) => {
            PreparedSemanticTransformToError::UnsupportedDepthChange(field)
        }
        AffinePayloadIssue::UnsupportedLifecycle {
            remover,
            introducer,
        } => PreparedSemanticTransformToError::UnsupportedLifecycle {
            remover,
            introducer,
        },
        AffinePayloadIssue::ReactiveDriverConflict(property) => {
            PreparedSemanticTransformToError::ReactiveDriverConflict(property)
        }
        AffinePayloadIssue::InvalidTargetValue { field, error } => {
            PreparedSemanticTransformToError::InvalidTargetValue { field, error }
        }
        AffinePayloadIssue::TargetValueOutOfRange(field) => {
            PreparedSemanticTransformToError::TargetValueOutOfRange(field)
        }
        AffinePayloadIssue::InvalidTargetStyle(error) => {
            PreparedSemanticTransformToError::InvalidTargetStyle(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        Color, RateFunction, SemanticObjectState, SemanticPaint, SemanticVec3, StoredGeometry,
        Transform2D, Vec2,
    };

    use super::*;
    use crate::{lower_semantic_affine_animation_tracks, lower_semantic_animation_schedule};

    fn effective(transform: Transform2D) -> EffectiveAnimationProperties {
        EffectiveAnimationProperties {
            transform,
            style: noon_core::Style::default(),
        }
    }

    #[test]
    fn prepared_transform_reads_only_its_effective_target_once() {
        let mut store = SemanticStore::new();
        let source =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(source).unwrap();
        let mut target = store.semantic_object_state_checked(source).unwrap().clone();
        target.transform.translation = SemanticVec3::new(4.0, 2.0, 0.0);
        target.transform.scale = SemanticVec3::new(2.0, 3.0, 1.0);
        let target = store.insert_semantic_object(target);
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        let mut reads = 0;

        let projection = lower_prepared_semantic_transform_to(
            &store,
            &index,
            source,
            target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
            3.0,
            |_| {
                reads += 1;
                Some(effective(Transform2D {
                    translation: Vec2::new(1.0, 0.0),
                    rotation: 0.0,
                    scale: Vec2::new(1.0, 1.0),
                }))
            },
        )
        .unwrap();

        assert_eq!(reads, 1);
        assert_eq!(projection.run_time(), 2.0);
        assert_eq!(projection.tracks().len(), 2);
        assert!(projection
            .tracks()
            .iter()
            .all(|track| track.timing.start_time == 3.0));
    }

    #[test]
    fn prepared_transform_rejects_unsupported_payload_before_identity_allocation() {
        let mut store = SemanticStore::new();
        let source =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(source).unwrap();
        let target =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Rectangle {
                size: Vec2::new(2.0, 2.0),
            }));
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();

        assert_eq!(
            lower_prepared_semantic_transform_to(
                &store,
                &index,
                source,
                target,
                AnimationOptions::new(),
                0.0,
                |_| Some(effective(Transform2D::default())),
            ),
            Err(PreparedSemanticTransformToError::UnsupportedContentChange)
        );
    }

    #[test]
    fn prepared_style_payload_rejects_unsupported_stroke_change_before_capture() {
        let mut store = SemanticStore::new();
        let source =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(source).unwrap();
        let mut target = store.semantic_object_state_checked(source).unwrap().clone();
        target.style.stroke = Some(SemanticPaint::Solid(Color::RED));
        let target = store.insert_semantic_object(target);
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        let mut captures = 0;

        assert_eq!(
            lower_prepared_semantic_transform_to(
                &store,
                &index,
                source,
                target,
                AnimationOptions::new(),
                0.0,
                |_| {
                    captures += 1;
                    Some(effective(Transform2D::default()))
                },
            ),
            Err(PreparedSemanticTransformToError::UnsupportedStyleChange)
        );
        assert_eq!(captures, 0);
    }

    #[test]
    fn prepared_and_predeclared_paths_share_exact_affine_channel_semantics() {
        let mut store = SemanticStore::new();
        let source =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(source).unwrap();
        let mut target = store.semantic_object_state_checked(source).unwrap().clone();
        target.transform.translation = SemanticVec3::new(7.0, -1.0, 0.0);
        target.transform.rotation_z = 0.75;
        target.transform.scale = SemanticVec3::new(1.5, 0.5, 1.0);
        target.style.fill = Some(SemanticPaint::Solid(Color::RED));
        target.style.fill_opacity = 0.5;
        target.style.object_opacity = 0.25;
        let target = store.insert_semantic_object(target);
        let options = AnimationOptions::new()
            .run_time(2.5)
            .rate_func(RateFunction::Linear);
        let animation = store
            .insert_semantic_transform_animation(source, target, options)
            .unwrap();
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        let mut effective = effective(Transform2D {
            translation: Vec2::new(2.0, 3.0),
            rotation: 0.25,
            scale: Vec2::new(1.0, 2.0),
        });
        effective.style.fill = Some(Color::BLUE);
        effective.style.opacity = 0.75;

        let schedule =
            lower_semantic_animation_schedule(&store, &index, animation, 4.0, options).unwrap();
        let existing =
            lower_semantic_affine_animation_tracks(&store, &schedule, |_| Some(effective)).unwrap();
        let prepared = lower_prepared_semantic_transform_to(
            &store,
            &index,
            source,
            target,
            options,
            4.0,
            |_| Some(effective),
        )
        .unwrap();

        assert_eq!(existing.tracks().len(), prepared.tracks().len());
        for (existing, prepared) in existing.tracks().iter().zip(prepared.tracks()) {
            assert_eq!(existing.target, prepared.target);
            assert_eq!(existing.execution_object_id, prepared.execution_object_id);
            assert_eq!(existing.property, prepared.property);
            assert_eq!(existing.completion, prepared.completion);
            assert_eq!(existing.values, prepared.values);
            assert_eq!(existing.timing, prepared.timing);
            assert_eq!(existing.time_map, prepared.time_map);
        }
    }
}
