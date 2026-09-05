use noon_core::{
    resolve_animation_options, validate_track_definition, AnimationDefaults, AnimationOptions,
    AnimationOptionsError, CompositionTimeMap, ObjectId, Property, ResolvedAnimationOptions,
    SemanticLoweringError, SemanticNodeId, SemanticObjectProperty, SemanticSceneOperationError,
    SemanticSignalValue, SemanticStore, TimelineError, TrackDefinition, TrackId, TrackTiming,
    TrackValues, Transform2D,
};

use super::super::SemanticExecutionIndex;
use super::affine::{transform_is_finite, SemanticAffineAnimationField};

/// One affine channel prepared for an animation declaration that has not committed yet.
///
/// This carries only execution-derived identity. Semantic animation identity is allocated by
/// the authoritative store commit after every execution mutation has passed preflight.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSemanticAffineTrack {
    pub target: SemanticNodeId,
    pub execution_object_id: ObjectId,
    pub property: Property,
    pub semantic_property: SemanticObjectProperty,
    pub completion_value: SemanticSignalValue,
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

/// Lower one not-yet-committed affine declaration without reserving semantic identity.
///
/// The result is inert until the session publishes it together with the semantic declaration.
/// Reads are limited to the source, target state, their execution mapping, and the source's
/// current effective transform.
pub fn lower_prepared_semantic_transform_to<F>(
    store: &SemanticStore,
    index: &SemanticExecutionIndex,
    target: SemanticNodeId,
    target_state: SemanticNodeId,
    declaration_options: AnimationOptions,
    play_options: AnimationOptions,
    start_time: f64,
    mut effective_transform: F,
) -> Result<PreparedSemanticTransformToProjection, PreparedSemanticTransformToError>
where
    F: FnMut(ObjectId) -> Option<Transform2D>,
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
    let options =
        resolve_animation_options(AnimationDefaults::MANIM, declaration_options, play_options)
            .map_err(PreparedSemanticTransformToError::Options)?;
    validate_prepared_static_payload(source, target_object, options)?;

    let from = effective_transform(execution_object_id).ok_or(
        PreparedSemanticTransformToError::MissingEffectiveTransform {
            target,
            execution_object_id,
        },
    )?;
    if !transform_is_finite(from) {
        return Err(PreparedSemanticTransformToError::InvalidEffectiveTransform(
            target,
        ));
    }
    let to = lower_prepared_target_transform(target_object)?;
    let timing = TrackTiming::new(start_time, options.run_time, options.rate_func);
    let time_map = CompositionTimeMap::default();
    let mut tracks = Vec::with_capacity(3);
    push_prepared_property_if_changed(
        source,
        target,
        execution_object_id,
        SemanticObjectProperty::Translation,
        Property::Position,
        TrackValues::Vec2 {
            from: from.translation,
            to: to.translation,
        },
        SemanticSignalValue::Vec3(target_object.transform.translation),
        from.translation != to.translation,
        timing,
        &time_map,
        &mut tracks,
    )?;
    push_prepared_property_if_changed(
        source,
        target,
        execution_object_id,
        SemanticObjectProperty::RotationZ,
        Property::Rotation,
        TrackValues::Scalar {
            from: from.rotation,
            to: to.rotation,
        },
        SemanticSignalValue::Scalar(target_object.transform.rotation_z),
        from.rotation != to.rotation,
        timing,
        &time_map,
        &mut tracks,
    )?;
    push_prepared_property_if_changed(
        source,
        target,
        execution_object_id,
        SemanticObjectProperty::Scale,
        Property::Scale,
        TrackValues::Vec2 {
            from: from.scale,
            to: to.scale,
        },
        SemanticSignalValue::Vec3(target_object.transform.scale),
        from.scale != to.scale,
        timing,
        &time_map,
        &mut tracks,
    )?;
    Ok(PreparedSemanticTransformToProjection {
        run_time: options.run_time,
        tracks,
    })
}

fn validate_prepared_static_payload(
    source: &noon_core::SemanticObjectState,
    target: &noon_core::SemanticObjectState,
    options: ResolvedAnimationOptions,
) -> Result<(), PreparedSemanticTransformToError> {
    if options.remover || options.introducer {
        return Err(PreparedSemanticTransformToError::UnsupportedLifecycle {
            remover: options.remover,
            introducer: options.introducer,
        });
    }
    if source.content != target.content {
        return Err(PreparedSemanticTransformToError::UnsupportedContentChange);
    }
    if source.style != target.style {
        return Err(PreparedSemanticTransformToError::UnsupportedStyleChange);
    }
    if source.z_index() != target.z_index() {
        return Err(PreparedSemanticTransformToError::UnsupportedPainterOrderChange);
    }
    if source.signal_bindings() != target.signal_bindings() {
        return Err(PreparedSemanticTransformToError::UnsupportedBindingChange);
    }
    if source.transform.translation.z != target.transform.translation.z {
        return Err(PreparedSemanticTransformToError::UnsupportedDepthChange(
            SemanticAffineAnimationField::Translation,
        ));
    }
    if source.transform.scale.z != target.transform.scale.z {
        return Err(PreparedSemanticTransformToError::UnsupportedDepthChange(
            SemanticAffineAnimationField::Scale,
        ));
    }
    Ok(())
}

fn lower_prepared_target_transform(
    state: &noon_core::SemanticObjectState,
) -> Result<Transform2D, PreparedSemanticTransformToError> {
    let translation = state
        .transform
        .translation
        .lower_xy_f32()
        .map_err(
            |error| PreparedSemanticTransformToError::InvalidTargetValue {
                field: SemanticAffineAnimationField::Translation,
                error,
            },
        )?;
    let scale = state.transform.scale.lower_xy_f32().map_err(|error| {
        PreparedSemanticTransformToError::InvalidTargetValue {
            field: SemanticAffineAnimationField::Scale,
            error,
        }
    })?;
    let rotation = state.transform.rotation_z;
    if !rotation.is_finite() || rotation.abs() > f32::MAX as f64 {
        return Err(PreparedSemanticTransformToError::TargetValueOutOfRange(
            SemanticAffineAnimationField::RotationZ,
        ));
    }
    Ok(Transform2D {
        translation,
        rotation: rotation as f32,
        scale,
    })
}

#[allow(clippy::too_many_arguments)]
fn push_prepared_property_if_changed(
    source: &noon_core::SemanticObjectState,
    target: SemanticNodeId,
    execution_object_id: ObjectId,
    semantic_property: SemanticObjectProperty,
    property: Property,
    values: TrackValues,
    completion_value: SemanticSignalValue,
    changed: bool,
    timing: TrackTiming,
    time_map: &CompositionTimeMap,
    tracks: &mut Vec<PreparedSemanticAffineTrack>,
) -> Result<(), PreparedSemanticTransformToError> {
    if !changed {
        return Ok(());
    }
    if source
        .signal_bindings()
        .iter()
        .any(|binding| binding.property() == semantic_property)
    {
        return Err(PreparedSemanticTransformToError::ReactiveDriverConflict(
            semantic_property,
        ));
    }
    tracks.push(PreparedSemanticAffineTrack {
        target,
        execution_object_id,
        property,
        semantic_property,
        completion_value,
        values,
        timing,
        time_map: time_map.clone(),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use noon_core::{RateFunction, SemanticObjectState, SemanticVec3, StoredGeometry, Vec2};

    use super::*;

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
            AnimationOptions::new(),
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
            3.0,
            |_| {
                reads += 1;
                Some(Transform2D {
                    translation: Vec2::new(1.0, 0.0),
                    rotation: 0.0,
                    scale: Vec2::new(1.0, 1.0),
                })
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
                AnimationOptions::new(),
                0.0,
                |_| Some(Transform2D::default()),
            ),
            Err(PreparedSemanticTransformToError::UnsupportedContentChange)
        );
    }
}
