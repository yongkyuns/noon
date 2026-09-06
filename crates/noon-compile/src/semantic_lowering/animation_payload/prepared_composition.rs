use std::collections::{hash_map::Entry, HashMap};

use noon_core::{
    validate_track_definition, AnimationOptions, ObjectId, PreparedSemanticMutationTransaction,
    Property, RateFunction, SemanticFadeDirection, SemanticLoweringError, SemanticObjectProperty,
    SemanticTransactionNodeRef, SemanticTransactionReadError, TimelineError, TrackDefinition,
    TrackId, TrackTiming, TrackValues,
};

use super::super::{
    lower_prepared_semantic_animation_schedule, PreparedSemanticAnimationScheduleError,
    PreparedSemanticScheduledAnimationPayload, SemanticExecutionIndex, SemanticExecutionValueError,
};
use super::affine::{
    driver_key, lower_affine_lifecycle_channels, lower_transform_channels, validate_affine_payload,
    AffinePayloadIssue, EffectiveAnimationProperties, SemanticAnimationCompletion,
};

use super::transform_payload::SemanticAffineAnimationField;

/// One execution track lowered from an animation declaration that still has transaction-local
/// semantic references.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSemanticAnimationTrack {
    pub animation: SemanticTransactionNodeRef,
    pub target: SemanticTransactionNodeRef,
    pub execution_object_id: ObjectId,
    pub property: Property,
    pub completion: SemanticAnimationCompletion,
    pub values: TrackValues,
    pub timing: TrackTiming,
    pub time_map: noon_core::CompositionTimeMap,
}

impl PreparedSemanticAnimationTrack {
    /// Attach execution-local identity after all semantic and execution preflight succeeds.
    pub fn with_track_id(&self, id: TrackId) -> Result<TrackDefinition, TimelineError> {
        let track = TrackDefinition {
            id,
            object: self.execution_object_id,
            property: self.property,
            values: self.values.clone(),
            timing: self.timing,
            time_map: self.time_map.clone(),
        };
        validate_track_definition(&track)?;
        Ok(track)
    }
}

/// Candidate-sized compiler projection for one prepared animation graph.
///
/// The result owns only lowered execution values and transaction-local references. The caller can
/// preflight execution publication, commit the held semantic transaction once, and resolve every
/// reference through that single transaction result.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSemanticAnimationActivation {
    root: SemanticTransactionNodeRef,
    start_time: f64,
    run_time: f64,
    tracks: Vec<PreparedSemanticAnimationTrack>,
}

impl PreparedSemanticAnimationActivation {
    pub const fn root(&self) -> SemanticTransactionNodeRef {
        self.root
    }

    pub const fn start_time(&self) -> f64 {
        self.start_time
    }

    pub const fn run_time(&self) -> f64 {
        self.run_time
    }

    pub fn tracks(&self) -> &[PreparedSemanticAnimationTrack] {
        &self.tracks
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedSemanticAnimationLoweringError {
    Schedule(PreparedSemanticAnimationScheduleError),
    Target {
        animation: SemanticTransactionNodeRef,
        node: SemanticTransactionNodeRef,
        error: SemanticTransactionReadError,
    },
    MissingEffectiveProperties {
        animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
        execution_object_id: ObjectId,
    },
    InvalidEffectiveTransform {
        animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
    },
    InvalidEffectiveStyle {
        animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
    },
    InvalidEffectiveAppearance {
        animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
        appearance: f32,
    },
    UnsupportedFadeComposition {
        animation: SemanticTransactionNodeRef,
    },
    UnsupportedFadeOptions {
        animation: SemanticTransactionNodeRef,
    },
    UnsupportedCreateComposition {
        animation: SemanticTransactionNodeRef,
    },
    UnsupportedCreateOptions {
        animation: SemanticTransactionNodeRef,
    },
    UnsupportedAffineLifecycleComposition {
        animation: SemanticTransactionNodeRef,
    },
    UnsupportedAffineLifecycleOptions {
        animation: SemanticTransactionNodeRef,
    },
    UnsupportedContentChange {
        animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
        target_state: SemanticTransactionNodeRef,
    },
    UnsupportedPointCorrespondence {
        animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
        target_state: SemanticTransactionNodeRef,
    },
    UnsupportedStyleChange {
        animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
        target_state: SemanticTransactionNodeRef,
    },
    UnsupportedPainterOrderChange {
        animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
        target_state: SemanticTransactionNodeRef,
    },
    UnsupportedBindingChange {
        animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
        target_state: SemanticTransactionNodeRef,
    },
    UnsupportedDepthChange {
        animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
        target_state: SemanticTransactionNodeRef,
        field: SemanticAffineAnimationField,
    },
    UnsupportedLifecycle {
        animation: SemanticTransactionNodeRef,
        remover: bool,
        introducer: bool,
    },
    ReactiveDriverConflict {
        animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
        property: SemanticObjectProperty,
    },
    MultipleDrivers {
        first_animation: SemanticTransactionNodeRef,
        next_animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
        property: SemanticObjectProperty,
    },
    InvalidTargetValue {
        animation: SemanticTransactionNodeRef,
        target_state: SemanticTransactionNodeRef,
        field: SemanticAffineAnimationField,
        error: SemanticLoweringError,
    },
    TargetValueOutOfRange {
        animation: SemanticTransactionNodeRef,
        target_state: SemanticTransactionNodeRef,
        field: SemanticAffineAnimationField,
    },
    InvalidTargetStyle {
        animation: SemanticTransactionNodeRef,
        target_state: SemanticTransactionNodeRef,
        error: SemanticExecutionValueError,
    },
}

impl std::fmt::Display for PreparedSemanticAnimationLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "prepared semantic animation lowering failed: {self:?}"
        )
    }
}

impl std::error::Error for PreparedSemanticAnimationLoweringError {}

/// Lower one prepared animation graph through the canonical schedule and shared payload paths.
///
/// Effective properties are captured at most once per affected execution object. The function
/// reads only staged animation/object dependencies and does not allocate semantic or execution
/// identity, mutate the prepared store, or publish runtime state.
pub fn lower_prepared_semantic_animation_composition<F>(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    index: &SemanticExecutionIndex,
    root: impl Into<SemanticTransactionNodeRef>,
    start_time: f64,
    play_options: AnimationOptions,
    mut effective_properties: F,
) -> Result<PreparedSemanticAnimationActivation, PreparedSemanticAnimationLoweringError>
where
    F: FnMut(ObjectId) -> Option<EffectiveAnimationProperties>,
{
    let root = root.into();
    let schedule =
        lower_prepared_semantic_animation_schedule(prepared, index, root, start_time, play_options)
            .map_err(PreparedSemanticAnimationLoweringError::Schedule)?;
    let mut captures = HashMap::<ObjectId, EffectiveAnimationProperties>::new();
    let mut driven = HashMap::<(u64, u8), SemanticTransactionNodeRef>::new();
    let mut tracks = Vec::new();

    for leaf in schedule.leaves() {
        let source = prepared.object_state(leaf.target).map_err(|error| {
            PreparedSemanticAnimationLoweringError::Target {
                animation: leaf.animation,
                node: leaf.target,
                error,
            }
        })?;
        let channel = match leaf.payload {
            PreparedSemanticScheduledAnimationPayload::TransformTo {
                target_state,
                interpolation,
            } => {
                let target = prepared.object_state(target_state).map_err(|error| {
                    PreparedSemanticAnimationLoweringError::Target {
                        animation: leaf.animation,
                        node: target_state,
                        error,
                    }
                })?;
                validate_affine_payload(source, target, leaf.options)
                    .map_err(|issue| prepared_payload_error(leaf, target_state, issue))?;
                let from = capture_effective(
                    leaf,
                    source,
                    index,
                    &mut captures,
                    &mut effective_properties,
                )?;
                let channels = lower_transform_channels(source, target, from, interpolation)
                    .map_err(|issue| prepared_payload_error(leaf, target_state, issue))?;
                for channel in channels {
                    push_prepared_channel(leaf, channel, &mut driven, &mut tracks)?;
                }
                continue;
            }
            PreparedSemanticScheduledAnimationPayload::Rotate { angle } => {
                if leaf.options.lag_ratio != 0.0
                    || leaf.options.path_arc != 0.0
                    || leaf.options.remover
                    || leaf.options.introducer
                    || leaf.options.reverse_rate_function
                {
                    return Err(
                        PreparedSemanticAnimationLoweringError::UnsupportedLifecycle {
                            animation: leaf.animation,
                            remover: leaf.options.remover,
                            introducer: leaf.options.introducer,
                        },
                    );
                }
                let from = capture_effective(
                    leaf,
                    source,
                    index,
                    &mut captures,
                    &mut effective_properties,
                )?;
                super::affine::lower_rotation_channel(source, from, angle)
                    .map_err(|issue| prepared_payload_error(leaf, leaf.target, issue))?
            }
            PreparedSemanticScheduledAnimationPayload::Fade { direction } => {
                if leaf.options.lag_ratio != 0.0
                    || leaf.options.path_arc != 0.0
                    || leaf.options.reverse_rate_function
                {
                    return Err(
                        PreparedSemanticAnimationLoweringError::UnsupportedFadeOptions {
                            animation: leaf.animation,
                        },
                    );
                }
                if !source.signal_bindings().is_empty() {
                    return Err(
                        PreparedSemanticAnimationLoweringError::ReactiveDriverConflict {
                            animation: leaf.animation,
                            target: leaf.target,
                            property: SemanticObjectProperty::Presence,
                        },
                    );
                }
                let from = match direction {
                    SemanticFadeDirection::In => 0.0,
                    SemanticFadeDirection::Out => {
                        capture_effective(
                            leaf,
                            source,
                            index,
                            &mut captures,
                            &mut effective_properties,
                        )?
                        .appearance
                    }
                };
                if !from.is_finite() || !(0.0..=1.0).contains(&from) {
                    return Err(
                        PreparedSemanticAnimationLoweringError::InvalidEffectiveAppearance {
                            animation: leaf.animation,
                            target: leaf.target,
                            appearance: from,
                        },
                    );
                }
                let to = match direction {
                    SemanticFadeDirection::In => 1.0,
                    SemanticFadeDirection::Out => 0.0,
                };
                super::affine::LoweredAffineChannel {
                    property: Property::Appearance,
                    conflict_property: SemanticObjectProperty::Presence,
                    completion: SemanticAnimationCompletion::Fade { direction },
                    values: TrackValues::Scalar { from, to },
                }
            }
            PreparedSemanticScheduledAnimationPayload::AffineLifecycle {
                direction,
                endpoint,
            } => {
                if leaf.options.lag_ratio != 0.0
                    || leaf.options.path_arc != 0.0
                    || leaf.options.reverse_rate_function
                {
                    return Err(
                        PreparedSemanticAnimationLoweringError::UnsupportedAffineLifecycleOptions {
                            animation: leaf.animation,
                        },
                    );
                }
                let from = capture_effective(
                    leaf,
                    source,
                    index,
                    &mut captures,
                    &mut effective_properties,
                )?;
                let channels = lower_affine_lifecycle_channels(source, from, direction, endpoint)
                    .map_err(|issue| prepared_payload_error(leaf, leaf.target, issue))?;
                for channel in channels {
                    push_prepared_channel(leaf, channel, &mut driven, &mut tracks)?;
                }
                continue;
            }
            PreparedSemanticScheduledAnimationPayload::Create => {
                if leaf.options.lag_ratio != 0.0
                    || leaf.options.path_arc != 0.0
                    || (leaf.options.reverse_rate_function
                        && !matches!(
                            leaf.options.rate_func,
                            RateFunction::Linear | RateFunction::Smooth
                        ))
                {
                    return Err(
                        PreparedSemanticAnimationLoweringError::UnsupportedCreateOptions {
                            animation: leaf.animation,
                        },
                    );
                }
                if !source.signal_bindings().is_empty() {
                    return Err(
                        PreparedSemanticAnimationLoweringError::ReactiveDriverConflict {
                            animation: leaf.animation,
                            target: leaf.target,
                            property: SemanticObjectProperty::Presence,
                        },
                    );
                }
                let remove = leaf.options.remover;
                // Reversal controls reveal direction; removal independently controls membership.
                let reverse = leaf.options.reverse_rate_function;
                super::affine::LoweredAffineChannel {
                    property: Property::Reveal,
                    conflict_property: SemanticObjectProperty::Presence,
                    completion: SemanticAnimationCompletion::RevealLifecycle { remove },
                    values: TrackValues::Scalar {
                        from: if reverse { 1.0 } else { 0.0 },
                        to: if reverse { 0.0 } else { 1.0 },
                    },
                }
            }
            PreparedSemanticScheduledAnimationPayload::Add => {
                if leaf.options.lag_ratio != 0.0
                    || leaf.options.path_arc != 0.0
                    || leaf.options.remover
                    || leaf.options.reverse_rate_function
                {
                    return Err(
                        PreparedSemanticAnimationLoweringError::UnsupportedLifecycle {
                            animation: leaf.animation,
                            remover: leaf.options.remover,
                            introducer: leaf.options.introducer,
                        },
                    );
                }
                if !source.signal_bindings().is_empty() {
                    return Err(
                        PreparedSemanticAnimationLoweringError::ReactiveDriverConflict {
                            animation: leaf.animation,
                            target: leaf.target,
                            property: SemanticObjectProperty::Presence,
                        },
                    );
                }
                super::affine::LoweredAffineChannel {
                    property: Property::Presence,
                    conflict_property: SemanticObjectProperty::Presence,
                    completion: SemanticAnimationCompletion::Release,
                    values: TrackValues::Bool {
                        from: false,
                        to: true,
                    },
                }
            }
        };
        push_prepared_channel(leaf, channel, &mut driven, &mut tracks)?;
    }

    Ok(PreparedSemanticAnimationActivation {
        root,
        start_time: schedule.start_time(),
        run_time: schedule.run_time(),
        tracks,
    })
}

fn capture_effective<F>(
    leaf: &super::super::PreparedSemanticScheduledAnimationLeaf,
    source: &noon_core::SemanticObjectState,
    index: &SemanticExecutionIndex,
    captures: &mut HashMap<ObjectId, EffectiveAnimationProperties>,
    effective_properties: &mut F,
) -> Result<EffectiveAnimationProperties, PreparedSemanticAnimationLoweringError>
where
    F: FnMut(ObjectId) -> Option<EffectiveAnimationProperties>,
{
    if let Some(captured) = captures.get(&leaf.execution_object_id).copied() {
        return Ok(captured);
    }
    let captured = if let Some(captured) = effective_properties(leaf.execution_object_id) {
        captured
    } else if leaf
        .target
        .existing()
        .and_then(|node| index.execution_object_id(node))
        .is_none()
    {
        EffectiveAnimationProperties {
            transform: super::super::projection::lower_semantic_transform_value(source).map_err(
                |_| PreparedSemanticAnimationLoweringError::MissingEffectiveProperties {
                    animation: leaf.animation,
                    target: leaf.target,
                    execution_object_id: leaf.execution_object_id,
                },
            )?,
            style: super::super::projection::lower_semantic_style_value(source).map_err(|_| {
                PreparedSemanticAnimationLoweringError::MissingEffectiveProperties {
                    animation: leaf.animation,
                    target: leaf.target,
                    execution_object_id: leaf.execution_object_id,
                }
            })?,
            appearance: 1.0,
        }
    } else {
        return Err(
            PreparedSemanticAnimationLoweringError::MissingEffectiveProperties {
                animation: leaf.animation,
                target: leaf.target,
                execution_object_id: leaf.execution_object_id,
            },
        );
    };
    captures.insert(leaf.execution_object_id, captured);
    Ok(captured)
}

fn push_prepared_channel(
    leaf: &super::super::PreparedSemanticScheduledAnimationLeaf,
    channel: super::affine::LoweredAffineChannel,
    driven: &mut HashMap<(u64, u8), SemanticTransactionNodeRef>,
    tracks: &mut Vec<PreparedSemanticAnimationTrack>,
) -> Result<(), PreparedSemanticAnimationLoweringError> {
    let umbrella_conflict = super::affine::transform_driver_conflict(
        driven,
        leaf.execution_object_id,
        channel.property,
        leaf.animation,
    );
    if let Some(first_animation) = umbrella_conflict {
        return Err(PreparedSemanticAnimationLoweringError::MultipleDrivers {
            first_animation,
            next_animation: leaf.animation,
            target: leaf.target,
            property: channel.conflict_property,
        });
    }
    match driven.entry(driver_key(leaf.execution_object_id, channel.property)) {
        Entry::Occupied(entry) => {
            return Err(PreparedSemanticAnimationLoweringError::MultipleDrivers {
                first_animation: *entry.get(),
                next_animation: leaf.animation,
                target: leaf.target,
                property: channel.conflict_property,
            });
        }
        Entry::Vacant(entry) => {
            entry.insert(leaf.animation);
        }
    }
    tracks.push(PreparedSemanticAnimationTrack {
        animation: leaf.animation,
        target: leaf.target,
        execution_object_id: leaf.execution_object_id,
        property: channel.property,
        completion: channel.completion,
        values: channel.values,
        timing: leaf.timing,
        time_map: leaf.time_map.clone(),
    });
    Ok(())
}

fn prepared_payload_error(
    leaf: &super::super::PreparedSemanticScheduledAnimationLeaf,
    target_state: SemanticTransactionNodeRef,
    issue: AffinePayloadIssue,
) -> PreparedSemanticAnimationLoweringError {
    match issue {
        AffinePayloadIssue::InvalidEffectiveTransform => {
            PreparedSemanticAnimationLoweringError::InvalidEffectiveTransform {
                animation: leaf.animation,
                target: leaf.target,
            }
        }
        AffinePayloadIssue::InvalidEffectiveStyle => {
            PreparedSemanticAnimationLoweringError::InvalidEffectiveStyle {
                animation: leaf.animation,
                target: leaf.target,
            }
        }
        AffinePayloadIssue::UnsupportedContentChange => {
            PreparedSemanticAnimationLoweringError::UnsupportedContentChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state,
            }
        }
        AffinePayloadIssue::UnsupportedPointCorrespondence => {
            PreparedSemanticAnimationLoweringError::UnsupportedPointCorrespondence {
                animation: leaf.animation,
                target: leaf.target,
                target_state,
            }
        }
        AffinePayloadIssue::UnsupportedStyleChange => {
            PreparedSemanticAnimationLoweringError::UnsupportedStyleChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state,
            }
        }
        AffinePayloadIssue::UnsupportedPainterOrderChange => {
            PreparedSemanticAnimationLoweringError::UnsupportedPainterOrderChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state,
            }
        }
        AffinePayloadIssue::UnsupportedBindingChange => {
            PreparedSemanticAnimationLoweringError::UnsupportedBindingChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state,
            }
        }
        AffinePayloadIssue::UnsupportedDepthChange(field) => {
            PreparedSemanticAnimationLoweringError::UnsupportedDepthChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state,
                field,
            }
        }
        AffinePayloadIssue::UnsupportedLifecycle {
            remover,
            introducer,
        } => PreparedSemanticAnimationLoweringError::UnsupportedLifecycle {
            animation: leaf.animation,
            remover,
            introducer,
        },
        AffinePayloadIssue::ReactiveDriverConflict(property) => {
            PreparedSemanticAnimationLoweringError::ReactiveDriverConflict {
                animation: leaf.animation,
                target: leaf.target,
                property,
            }
        }
        AffinePayloadIssue::InvalidTargetValue { field, error } => {
            PreparedSemanticAnimationLoweringError::InvalidTargetValue {
                animation: leaf.animation,
                target_state,
                field,
                error,
            }
        }
        AffinePayloadIssue::TargetValueOutOfRange(field) => {
            PreparedSemanticAnimationLoweringError::TargetValueOutOfRange {
                animation: leaf.animation,
                target_state,
                field,
            }
        }
        AffinePayloadIssue::InvalidTargetStyle(error) => {
            PreparedSemanticAnimationLoweringError::InvalidTargetStyle {
                animation: leaf.animation,
                target_state,
                error,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        Color, RateFunction, SemanticAffineLifecycleDirection, SemanticAffineLifecycleEndpoint,
        SemanticAnimationCompositionKind, SemanticMutationTransaction,
        SemanticMutationTransactionResult, SemanticNodeCreation, SemanticObjectState, SemanticVec3,
        StoredGeometry, Transform2D, Vec2,
    };

    use super::*;
    use crate::{lower_semantic_affine_animation_tracks, lower_semantic_animation_schedule};

    fn visible_circle(store: &mut noon_core::SemanticStore) -> noon_core::SemanticNodeId {
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 0.5,
            }));
        store.attach_to_scene(object).unwrap();
        object
    }

    fn target_state(translation: SemanticVec3) -> SemanticObjectState {
        let mut target = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.5 });
        target.transform.translation = translation;
        target
    }

    fn rectangle_target() -> SemanticObjectState {
        let mut target = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(2.0, 2.0),
        });
        target.transform.translation = SemanticVec3::new(2.0, -1.0, 0.0);
        target.style.fill_opacity = 0.5;
        target
    }

    fn effective(translation: Vec2) -> EffectiveAnimationProperties {
        EffectiveAnimationProperties {
            transform: Transform2D {
                translation,
                ..Transform2D::IDENTITY
            },
            style: noon_core::Style::default(),
            appearance: 1.0,
        }
    }

    fn resolve(
        node: SemanticTransactionNodeRef,
        committed: &SemanticMutationTransactionResult,
    ) -> noon_core::SemanticNodeId {
        match node {
            SemanticTransactionNodeRef::Existing(node) => node,
            SemanticTransactionNodeRef::Pending(token) => committed.resolve(token).unwrap(),
        }
    }

    #[test]
    fn prepared_and_published_compositions_share_schedule_and_payload_lowering() {
        let mut store = noon_core::SemanticStore::new();
        let left = visible_circle(&mut store);
        let right = visible_circle(&mut store);
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();

        let mut transaction = SemanticMutationTransaction::new();
        let left_target = transaction.create_node(SemanticNodeCreation::object(target_state(
            SemanticVec3::new(-2.0, 1.0, 0.0),
        )));
        let right_target = transaction.create_node(SemanticNodeCreation::object(target_state(
            SemanticVec3::new(2.0, -1.0, 0.0),
        )));
        let left_animation = transaction.create_transform_animation(
            left,
            left_target,
            AnimationOptions::new().run_time(1.0),
        );
        let right_animation = transaction.create_transform_animation(
            right,
            right_target,
            AnimationOptions::new().run_time(2.0),
        );
        let root = transaction.create_animation_composition(
            SemanticAnimationCompositionKind::Sequence,
            [left_animation, right_animation],
            AnimationOptions::new().rate_func(RateFunction::Smooth),
        );
        let prepared = transaction.prepare(&mut store).unwrap();
        let mut captures = 0;
        let projection = lower_prepared_semantic_animation_composition(
            &prepared,
            &index,
            root,
            4.0,
            AnimationOptions::new().run_time(6.0),
            |object| {
                captures += 1;
                (object == index.execution_object_id(left).unwrap())
                    .then_some(effective(Vec2::new(0.5, 0.0)))
                    .or_else(|| {
                        (object == index.execution_object_id(right).unwrap())
                            .then_some(effective(Vec2::new(-0.5, 0.0)))
                    })
            },
        )
        .unwrap();

        assert_eq!(projection.root(), root.into());
        assert_eq!(projection.start_time(), 4.0);
        assert_eq!(projection.run_time(), 6.0);
        assert_eq!(captures, 2);
        assert_eq!(projection.tracks().len(), 2);
        assert!(projection
            .tracks()
            .iter()
            .all(|track| !track.time_map.is_identity()));

        let committed = prepared.commit();
        let published_root = committed.resolve(root).unwrap();
        let published_schedule = lower_semantic_animation_schedule(
            &store,
            &index,
            published_root,
            4.0,
            AnimationOptions::new().run_time(6.0),
        )
        .unwrap();
        let published =
            lower_semantic_affine_animation_tracks(&store, &published_schedule, |object| {
                (object == index.execution_object_id(left).unwrap())
                    .then_some(effective(Vec2::new(0.5, 0.0)))
                    .or_else(|| {
                        (object == index.execution_object_id(right).unwrap())
                            .then_some(effective(Vec2::new(-0.5, 0.0)))
                    })
            })
            .unwrap();

        assert_eq!(published.tracks().len(), projection.tracks().len());
        for (prepared, published) in projection.tracks().iter().zip(published.tracks()) {
            assert_eq!(resolve(prepared.animation, &committed), published.animation);
            assert_eq!(resolve(prepared.target, &committed), published.target);
            assert_eq!(prepared.execution_object_id, published.execution_object_id);
            assert_eq!(prepared.property, published.property);
            assert_eq!(prepared.completion, published.completion);
            assert_eq!(prepared.values, published.values);
            assert_eq!(prepared.timing, published.timing);
            assert_eq!(prepared.time_map, published.time_map);
        }
    }

    #[test]
    fn analytic_content_morph_lowers_to_prepared_geometry_and_scalar_channels() {
        let mut store = noon_core::SemanticStore::new();
        let source = visible_circle(&mut store);
        let source_style = crate::semantic_lowering::projection::lower_semantic_style_value(
            store.semantic_object_state_checked(source).unwrap(),
        )
        .unwrap();
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        let target = transaction.create_node(SemanticNodeCreation::object(rectangle_target()));
        let animation =
            transaction.create_transform_animation(source, target, AnimationOptions::new());
        let prepared = transaction.prepare(&mut store).unwrap();

        let activation = lower_prepared_semantic_animation_composition(
            &prepared,
            &index,
            animation,
            0.0,
            AnimationOptions::new().run_time(2.0),
            |_| {
                Some(EffectiveAnimationProperties {
                    style: source_style,
                    ..effective(Vec2::ZERO)
                })
            },
        )
        .unwrap();

        let track = activation
            .tracks()
            .iter()
            .find(|track| track.property == Property::Morph)
            .expect("content morph must include one prepared Morph track");
        assert!(matches!(
            &track.values,
            TrackValues::PreparedMorph { geometry: noon_core::GeometryRef::VectorPath(path), .. }
                if path.morph_target().is_some()
        ));
        assert!(matches!(
            track.completion,
            SemanticAnimationCompletion::ContentMorph {
                content: noon_core::SemanticObjectContent::Geometry(
                    StoredGeometry::Rectangle { .. }
                ),
                ..
            }
        ));
    }

    #[test]
    fn prepared_flat_parallel_create_uses_each_child_rate_once() {
        let mut store = noon_core::SemanticStore::new();
        let root = store.insert_family();
        let circle =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 0.4,
            }));
        let square =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Rectangle {
                size: Vec2::new(0.8, 0.8),
            }));
        let index = SemanticExecutionIndex::new();
        let child_options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Smooth);
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_member(root, circle);
        transaction.add_member(root, square);
        let left = transaction.create_create_animation(circle, child_options);
        let right = transaction.create_create_animation(square, child_options);
        let composition = transaction.create_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            [left, right],
            AnimationOptions::new(),
        );
        let prepared = transaction.prepare(&mut store).unwrap();

        let activation = lower_prepared_semantic_animation_composition(
            &prepared,
            &index,
            composition,
            0.0,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
            |_| None,
        )
        .unwrap();

        assert_eq!(activation.len(), 2);
        for track in activation.tracks() {
            assert_eq!(track.property, Property::Reveal);
            assert_eq!(track.timing.easing, RateFunction::Smooth);
            assert_eq!(track.time_map.steps.len(), 1);
            let step = track.time_map.steps[0];
            assert_eq!(step.start, 0.0);
            assert_eq!(step.duration, 1.0);
            assert_eq!(step.rate_func, RateFunction::Linear);
            // At a quarter of the parent interval, only the child applies Manim Smooth.
            assert_eq!(track.time_map.evaluate(0.25).alpha, 0.25);
            assert_eq!(
                track.timing.easing.evaluate(0.25),
                RateFunction::Smooth.evaluate(0.25)
            );
        }
    }

    #[test]
    fn reveal_direction_is_independent_of_cleanup() {
        for reverse in [false, true] {
            for remove in [false, true] {
                let mut store = noon_core::SemanticStore::new();
                let root = store.insert_family();
                let circle = store.insert_semantic_object(SemanticObjectState::new(
                    StoredGeometry::Circle { radius: 0.4 },
                ));
                let index = SemanticExecutionIndex::new();
                let mut transaction = SemanticMutationTransaction::new();
                transaction.add_member(root, circle);
                let animation = transaction.create_create_animation(
                    circle,
                    AnimationOptions::new()
                        .reverse_rate_function(reverse)
                        .remover(remove),
                );
                let prepared = transaction.prepare(&mut store).unwrap();
                let activation = lower_prepared_semantic_animation_composition(
                    &prepared,
                    &index,
                    animation,
                    0.0,
                    AnimationOptions::new(),
                    |_| None,
                )
                .unwrap();
                let track = &activation.tracks()[0];
                assert_eq!(
                    track.completion,
                    SemanticAnimationCompletion::RevealLifecycle { remove }
                );
                assert_eq!(
                    track.values,
                    TrackValues::Scalar {
                        from: if reverse { 1.0 } else { 0.0 },
                        to: if reverse { 0.0 } else { 1.0 },
                    }
                );
            }
        }
    }

    #[test]
    fn affine_lifecycle_uses_activation_relative_release_channels() {
        let mut store = noon_core::SemanticStore::new();
        let square =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Rectangle {
                size: Vec2::new(1.0, 1.0),
            }));
        store.attach_to_scene(square).unwrap();
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        let animation = transaction.create_affine_lifecycle_animation(
            square,
            SemanticAffineLifecycleDirection::RemoveTo,
            SemanticAffineLifecycleEndpoint {
                point: SemanticVec3::new(-2.0, 1.0, 0.0),
                rotation_offset: -std::f64::consts::FRAC_PI_2,
                point_color: Some(Color::RED),
            },
            AnimationOptions::new().run_time(1.0),
        );
        let prepared = transaction.prepare(&mut store).unwrap();
        let effective = EffectiveAnimationProperties {
            transform: Transform2D {
                translation: Vec2::new(3.0, 2.0),
                rotation: 0.25,
                scale: Vec2::new(2.0, 3.0),
            },
            style: noon_core::Style::default(),
            appearance: 1.0,
        };
        let activation = lower_prepared_semantic_animation_composition(
            &prepared,
            &index,
            animation,
            4.0,
            AnimationOptions::new(),
            |_| Some(effective),
        )
        .unwrap();

        assert_eq!(activation.start_time(), 4.0);
        assert!(activation
            .tracks()
            .iter()
            .all(|track| track.completion == SemanticAnimationCompletion::Release));
        assert!(activation.tracks().iter().any(|track| {
            track.property == Property::Position
                && track.values
                    == TrackValues::Vec2 {
                        from: effective.transform.translation,
                        to: Vec2::new(-2.0, 1.0),
                    }
        }));
        assert!(activation.tracks().iter().any(|track| {
            track.property == Property::Scale
                && track.values
                    == TrackValues::Vec2 {
                        from: effective.transform.scale,
                        to: Vec2::ZERO,
                    }
        }));
    }

    #[test]
    fn prepared_sequential_create_remains_unavailable_before_publication() {
        let mut store = noon_core::SemanticStore::new();
        let root = store.insert_family();
        let circle =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 0.4,
            }));
        let square =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Rectangle {
                size: Vec2::new(0.8, 0.8),
            }));
        let index = SemanticExecutionIndex::new();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_member(root, circle);
        transaction.add_member(root, square);
        let left = transaction.create_create_animation(circle, AnimationOptions::new());
        let right = transaction.create_create_animation(square, AnimationOptions::new());
        let composition = transaction.create_animation_composition(
            SemanticAnimationCompositionKind::Sequence,
            [left, right],
            AnimationOptions::new(),
        );
        let before = store.scene_revision();
        let prepared = transaction.prepare(&mut store).unwrap();

        assert!(matches!(
            lower_prepared_semantic_animation_composition(
                &prepared,
                &index,
                composition,
                0.0,
                AnimationOptions::new(),
                |_| None,
            ),
            Err(PreparedSemanticAnimationLoweringError::UnsupportedCreateComposition { .. })
        ));
        drop(prepared);
        assert_eq!(store.scene_revision(), before);
    }

    #[test]
    fn duplicate_prepared_driver_fails_without_semantic_publication() {
        let mut store = noon_core::SemanticStore::new();
        let source = visible_circle(&mut store);
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        let before_revision = store.scene_revision();
        let before_len = store.len();

        let mut transaction = SemanticMutationTransaction::new();
        let first_target = transaction.create_node(SemanticNodeCreation::object(target_state(
            SemanticVec3::new(1.0, 0.0, 0.0),
        )));
        let second_target = transaction.create_node(SemanticNodeCreation::object(target_state(
            SemanticVec3::new(2.0, 0.0, 0.0),
        )));
        let first =
            transaction.create_transform_animation(source, first_target, AnimationOptions::new());
        let second =
            transaction.create_transform_animation(source, second_target, AnimationOptions::new());
        let root = transaction.create_animation_composition(
            SemanticAnimationCompositionKind::Sequence,
            [first, second],
            AnimationOptions::new(),
        );
        let prepared = transaction.prepare(&mut store).unwrap();
        let mut captures = 0;
        let result = lower_prepared_semantic_animation_composition(
            &prepared,
            &index,
            root,
            0.0,
            AnimationOptions::new(),
            |_| {
                captures += 1;
                Some(effective(Vec2::ZERO))
            },
        );

        assert!(matches!(
            result,
            Err(PreparedSemanticAnimationLoweringError::MultipleDrivers {
                first_animation,
                next_animation,
                target: SemanticTransactionNodeRef::Existing(actual_target),
                property: SemanticObjectProperty::Translation,
            }) if first_animation == first.into()
                && next_animation == second.into()
                && actual_target == source
        ));
        assert_eq!(captures, 1);
        drop(prepared);
        assert_eq!(store.scene_revision(), before_revision);
        assert_eq!(store.len(), before_len);
    }

    #[test]
    fn prepared_single_leaf_captures_once_and_preserves_bound_style_domains() {
        for property in [
            SemanticObjectProperty::FillOpacity,
            SemanticObjectProperty::StrokeOpacity,
            SemanticObjectProperty::ObjectOpacity,
        ] {
            let mut store = noon_core::SemanticStore::new();
            let source = visible_circle(&mut store);
            let signal = store.insert_semantic_input_signal(0.65_f64).unwrap();
            store
                .bind_semantic_signal(signal, source, property)
                .unwrap();
            let mut target_state = store.semantic_object_state_checked(source).unwrap().clone();
            target_state.transform.translation = SemanticVec3::new(2.0, 0.0, 0.0);
            let mut index = SemanticExecutionIndex::new();
            index.lower_scene(&store).unwrap();

            let mut transaction = SemanticMutationTransaction::new();
            let target = transaction.create_node(SemanticNodeCreation::object(target_state));
            let animation =
                transaction.create_transform_animation(source, target, AnimationOptions::new());
            let prepared = transaction.prepare(&mut store).unwrap();
            let mut captures = 0;
            let mut current = effective(Vec2::ZERO);
            match property {
                SemanticObjectProperty::FillOpacity => {
                    current.style.fill.as_mut().unwrap().alpha = 0.65;
                }
                SemanticObjectProperty::StrokeOpacity => {
                    current.style.stroke = Some(noon_core::Color {
                        alpha: 0.65,
                        ..noon_core::Color::WHITE
                    });
                }
                SemanticObjectProperty::ObjectOpacity => current.style.opacity = 0.65,
                _ => unreachable!(),
            }
            let activation = lower_prepared_semantic_animation_composition(
                &prepared,
                &index,
                animation,
                0.0,
                AnimationOptions::new(),
                |_| {
                    captures += 1;
                    Some(current)
                },
            )
            .unwrap();

            assert_eq!(captures, 1, "each leaf captures its target exactly once");
            assert_eq!(activation.len(), 1);
            assert_eq!(activation.tracks()[0].property, Property::Position);
        }
    }

    #[test]
    fn unsupported_prepared_leaf_fails_before_effective_capture_or_commit() {
        let mut store = noon_core::SemanticStore::new();
        let source = visible_circle(&mut store);
        let mut target_state = store.semantic_object_state_checked(source).unwrap().clone();
        target_state.style.stroke_width = 2.0;
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        let before_revision = store.scene_revision();
        let before_len = store.len();

        let mut transaction = SemanticMutationTransaction::new();
        let target = transaction.create_node(SemanticNodeCreation::object(target_state));
        let animation =
            transaction.create_transform_animation(source, target, AnimationOptions::new());
        let prepared = transaction.prepare(&mut store).unwrap();
        let mut captures = 0;
        let result = lower_prepared_semantic_animation_composition(
            &prepared,
            &index,
            animation,
            0.0,
            AnimationOptions::new(),
            |_| {
                captures += 1;
                Some(effective(Vec2::ZERO))
            },
        );

        assert!(matches!(
            result,
            Err(PreparedSemanticAnimationLoweringError::UnsupportedStyleChange {
                animation: actual,
                ..
            }) if actual == animation.into()
        ));
        assert_eq!(captures, 0);
        drop(prepared);
        assert_eq!(store.scene_revision(), before_revision);
        assert_eq!(store.len(), before_len);
    }

    #[test]
    fn late_unsupported_leaf_does_not_allocate_pending_identities() {
        let mut store = noon_core::SemanticStore::new();
        let first_source = visible_circle(&mut store);
        let second_source = visible_circle(&mut store);
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        let before_revision = store.scene_revision();
        let before_len = store.len();

        let mut transaction = SemanticMutationTransaction::new();
        let valid_target = transaction.create_node(SemanticNodeCreation::object(target_state(
            SemanticVec3::new(1.0, 0.0, 0.0),
        )));
        let invalid_target = transaction.create_node(SemanticNodeCreation::object(
            SemanticObjectState::new(StoredGeometry::Rectangle {
                size: Vec2::new(1.0, 1.0),
            }),
        ));
        let valid = transaction.create_transform_animation(
            first_source,
            valid_target,
            AnimationOptions::new(),
        );
        let invalid = transaction.create_transform_animation(
            second_source,
            invalid_target,
            AnimationOptions::new(),
        );
        let root = transaction.create_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            [valid, invalid],
            AnimationOptions::new(),
        );
        let prepared = transaction.prepare(&mut store).unwrap();
        let result = lower_prepared_semantic_animation_composition(
            &prepared,
            &index,
            root,
            0.0,
            AnimationOptions::new(),
            |_| Some(effective(Vec2::ZERO)),
        );

        assert!(matches!(
            result,
            Err(PreparedSemanticAnimationLoweringError::UnsupportedContentChange {
                animation,
                ..
            }) if animation == invalid.into()
        ));
        drop(prepared);
        assert_eq!(store.scene_revision(), before_revision);
        assert_eq!(store.len(), before_len);
    }
}
