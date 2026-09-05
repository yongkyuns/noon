use std::collections::{hash_map::Entry, HashMap};

use noon_core::{
    validate_track_definition, ObjectId, Property, ResolvedAnimationOptions,
    SemanticAnimationError, SemanticAnimationIntent, SemanticLoweringError, SemanticNodeId,
    SemanticObjectProperty, SemanticSceneOperationError, SemanticSignalValue, SemanticStore,
    TimelineError, TrackDefinition, TrackId, TrackValues, Transform2D,
};

use super::super::{SemanticAnimationScheduleProjection, SemanticScheduledAnimationLeaf};

/// One existing execution-timeline channel lowered from an activated semantic animation.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticAffineAnimationTrack {
    pub animation: SemanticNodeId,
    pub target: SemanticNodeId,
    pub execution_object_id: ObjectId,
    pub property: Property,
    pub semantic_property: SemanticObjectProperty,
    pub completion_value: SemanticSignalValue,
    pub values: TrackValues,
    pub timing: noon_core::TrackTiming,
    pub time_map: noon_core::CompositionTimeMap,
}

impl SemanticAffineAnimationTrack {
    /// Attach execution-local activation identity at the existing timeline boundary.
    pub fn with_track_id(
        &self,
        id: TrackId,
    ) -> Result<TrackDefinition, SemanticAffineAnimationTrackError> {
        let track = TrackDefinition {
            id,
            object: self.execution_object_id,
            property: self.property,
            values: self.values.clone(),
            timing: self.timing,
            time_map: self.time_map.clone(),
        };
        validate_track_definition(&track).map_err(|error| {
            SemanticAffineAnimationTrackError::InvalidTrack {
                animation: self.animation,
                error,
            }
        })?;
        Ok(track)
    }
}

/// Activation-level affine continuation of the canonical semantic animation payload seam.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticAffineAnimationTrackProjection {
    tracks: Vec<SemanticAffineAnimationTrack>,
}

impl SemanticAffineAnimationTrackProjection {
    pub fn tracks(&self) -> &[SemanticAffineAnimationTrack] {
        &self.tracks
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticAffineAnimationField {
    Translation,
    Scale,
    RotationZ,
}

impl std::fmt::Display for SemanticAffineAnimationField {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Translation => "translation",
            Self::Scale => "scale",
            Self::RotationZ => "rotation_z",
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticAffineAnimationTrackError {
    Animation(SemanticAnimationError),
    Target {
        animation: SemanticNodeId,
        node: SemanticNodeId,
        error: SemanticSceneOperationError,
    },
    ScheduleMismatch {
        animation: SemanticNodeId,
    },
    MissingEffectiveTransform {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        execution_object_id: ObjectId,
    },
    InvalidEffectiveTransform {
        animation: SemanticNodeId,
        target: SemanticNodeId,
    },
    UnsupportedContentChange {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        target_state: SemanticNodeId,
    },
    UnsupportedStyleChange {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        target_state: SemanticNodeId,
    },
    UnsupportedPainterOrderChange {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        target_state: SemanticNodeId,
    },
    UnsupportedBindingChange {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        target_state: SemanticNodeId,
    },
    UnsupportedDepthChange {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        target_state: SemanticNodeId,
        field: SemanticAffineAnimationField,
    },
    UnsupportedLifecycle {
        animation: SemanticNodeId,
        remover: bool,
        introducer: bool,
    },
    ReactiveDriverConflict {
        animation: SemanticNodeId,
        target: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    MultipleDrivers {
        first_animation: SemanticNodeId,
        next_animation: SemanticNodeId,
        target: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    InvalidTargetValue {
        animation: SemanticNodeId,
        target_state: SemanticNodeId,
        field: SemanticAffineAnimationField,
        error: SemanticLoweringError,
    },
    TargetValueOutOfRange {
        animation: SemanticNodeId,
        target_state: SemanticNodeId,
        field: SemanticAffineAnimationField,
    },
    InvalidTrack {
        animation: SemanticNodeId,
        error: TimelineError,
    },
}

impl std::fmt::Display for SemanticAffineAnimationTrackError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Animation(error) => error.fmt(formatter),
            Self::Target {
                animation,
                node,
                error,
            } => write!(
                formatter,
                "semantic animation {}:{} cannot read object {}:{}: {error}",
                animation.slot(),
                animation.generation(),
                node.slot(),
                node.generation()
            ),
            Self::ScheduleMismatch { animation } => write!(
                formatter,
                "scheduled semantic animation {}:{} no longer matches its authored TransformTo declaration",
                animation.slot(),
                animation.generation()
            ),
            Self::MissingEffectiveTransform {
                animation,
                target,
                execution_object_id,
            } => write!(
                formatter,
                "semantic animation {}:{} cannot capture effective transform for target {}:{} / execution object {}",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation(),
                execution_object_id.get()
            ),
            Self::InvalidEffectiveTransform { animation, target } => write!(
                formatter,
                "semantic animation {}:{} received a non-finite effective transform for target {}:{}",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation()
            ),
            Self::UnsupportedContentChange {
                animation,
                target,
                target_state,
            } => write!(
                formatter,
                "semantic animation {}:{} changes content from target {}:{} to target-state {}:{} before non-affine TransformTo payload lowering is available",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::UnsupportedStyleChange {
                animation,
                target,
                target_state,
            } => write!(
                formatter,
                "semantic animation {}:{} changes style from target {}:{} to target-state {}:{} before style animation payload lowering is available",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::UnsupportedPainterOrderChange {
                animation,
                target,
                target_state,
            } => write!(
                formatter,
                "semantic animation {}:{} changes z/painter order from target {}:{} to target-state {}:{}",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::UnsupportedBindingChange {
                animation,
                target,
                target_state,
            } => write!(
                formatter,
                "semantic animation {}:{} changes native-reactive binding topology from target {}:{} to target-state {}:{}",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::UnsupportedDepthChange {
                animation,
                target,
                target_state,
                field,
            } => write!(
                formatter,
                "semantic animation {}:{} changes {field} depth between target {}:{} and target-state {}:{} before the current 2D execution domain can represent it",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::UnsupportedLifecycle {
                animation,
                remover,
                introducer,
            } => write!(
                formatter,
                "semantic animation {}:{} cannot lower remover={remover} introducer={introducer} through affine-only execution tracks",
                animation.slot(),
                animation.generation()
            ),
            Self::ReactiveDriverConflict {
                animation,
                target,
                property,
            } => write!(
                formatter,
                "semantic animation {}:{} and a native-reactive binding both drive {property:?} on target {}:{} before driver arbitration is available",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation()
            ),
            Self::MultipleDrivers {
                first_animation,
                next_animation,
                target,
                property,
            } => write!(
                formatter,
                "semantic animations {}:{} and {}:{} both drive {property:?} on target {}:{} in one activation before composition driver chaining/arbitration is defined",
                first_animation.slot(),
                first_animation.generation(),
                next_animation.slot(),
                next_animation.generation(),
                target.slot(),
                target.generation()
            ),
            Self::InvalidTargetValue {
                animation,
                target_state,
                field,
                error,
            } => write!(
                formatter,
                "semantic animation {}:{} target-state {}:{} has invalid {field}: {error}",
                animation.slot(),
                animation.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::TargetValueOutOfRange {
                animation,
                target_state,
                field,
            } => write!(
                formatter,
                "semantic animation {}:{} target-state {}:{} {field} cannot lower to the current f32 execution domain",
                animation.slot(),
                animation.generation(),
                target_state.slot(),
                target_state.generation()
            ),
            Self::InvalidTrack { animation, error } => write!(
                formatter,
                "semantic animation {}:{} produced an invalid execution track: {error}",
                animation.slot(),
                animation.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticAffineAnimationTrackError {}

/// Lower the affine payload of one already-resolved semantic animation activation.
///
/// `effective_transform` is supplied by the execution/session owner and sampled at
/// most once per execution object. The compiler therefore never substitutes authored
/// base state for activation-time effective state and does not own the session barrier.
///
/// This path fails closed for unsupported non-affine state, lifecycle semantics,
/// reactive/timeline driver conflicts, stale scheduled leaves, and multiple drivers
/// of one target/property until the corresponding shared policies exist.
pub fn lower_semantic_affine_animation_tracks<F>(
    store: &SemanticStore,
    schedule: &SemanticAnimationScheduleProjection,
    mut effective_transform: F,
) -> Result<SemanticAffineAnimationTrackProjection, SemanticAffineAnimationTrackError>
where
    F: FnMut(ObjectId) -> Option<Transform2D>,
{
    let mut captures = HashMap::<ObjectId, Transform2D>::new();
    let mut driven = HashMap::<(u64, u8), SemanticNodeId>::new();
    let mut tracks = Vec::new();

    for leaf in schedule.leaves() {
        validate_leaf_matches_declaration(store, leaf)?;
        let source = object_state(store, leaf, leaf.target)?;
        let target = object_state(store, leaf, leaf.target_state)?;
        validate_affine_payload(source, target, leaf.options)
            .map_err(|issue| existing_payload_error(leaf, issue))?;
        let from = if let Some(captured) = captures.get(&leaf.execution_object_id).copied() {
            captured
        } else {
            let captured = effective_transform(leaf.execution_object_id).ok_or(
                SemanticAffineAnimationTrackError::MissingEffectiveTransform {
                    animation: leaf.animation,
                    target: leaf.target,
                    execution_object_id: leaf.execution_object_id,
                },
            )?;
            captures.insert(leaf.execution_object_id, captured);
            captured
        };
        let channels = lower_affine_channels(source, target, from)
            .map_err(|issue| existing_payload_error(leaf, issue))?;
        for channel in channels {
            match driven.entry(driver_key(
                leaf.execution_object_id,
                channel.semantic_property,
            )) {
                Entry::Occupied(entry) => {
                    return Err(SemanticAffineAnimationTrackError::MultipleDrivers {
                        first_animation: *entry.get(),
                        next_animation: leaf.animation,
                        target: leaf.target,
                        property: channel.semantic_property,
                    });
                }
                Entry::Vacant(entry) => {
                    entry.insert(leaf.animation);
                }
            }
            tracks.push(SemanticAffineAnimationTrack {
                animation: leaf.animation,
                target: leaf.target,
                execution_object_id: leaf.execution_object_id,
                property: channel.property,
                semantic_property: channel.semantic_property,
                completion_value: channel.completion_value,
                values: channel.values,
                timing: leaf.timing,
                time_map: leaf.time_map.clone(),
            });
        }
    }

    Ok(SemanticAffineAnimationTrackProjection { tracks })
}

fn validate_leaf_matches_declaration(
    store: &SemanticStore,
    leaf: &SemanticScheduledAnimationLeaf,
) -> Result<(), SemanticAffineAnimationTrackError> {
    let animation = store
        .semantic_animation_state(leaf.animation)
        .map_err(SemanticAffineAnimationTrackError::Animation)?;
    match animation.intent() {
        SemanticAnimationIntent::TransformTo {
            target,
            target_state,
        } if *target == leaf.target && *target_state == leaf.target_state => Ok(()),
        _ => Err(SemanticAffineAnimationTrackError::ScheduleMismatch {
            animation: leaf.animation,
        }),
    }
}

fn object_state<'a>(
    store: &'a SemanticStore,
    leaf: &SemanticScheduledAnimationLeaf,
    node: SemanticNodeId,
) -> Result<&'a noon_core::SemanticObjectState, SemanticAffineAnimationTrackError> {
    store.semantic_object_state_checked(node).map_err(|error| {
        SemanticAffineAnimationTrackError::Target {
            animation: leaf.animation,
            node,
            error,
        }
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LoweredAffineChannel {
    pub property: Property,
    pub semantic_property: SemanticObjectProperty,
    pub completion_value: SemanticSignalValue,
    pub values: TrackValues,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum AffinePayloadIssue {
    InvalidEffectiveTransform,
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
}

pub(super) fn validate_affine_payload(
    source: &noon_core::SemanticObjectState,
    target: &noon_core::SemanticObjectState,
    options: ResolvedAnimationOptions,
) -> Result<(), AffinePayloadIssue> {
    if options.remover || options.introducer {
        return Err(AffinePayloadIssue::UnsupportedLifecycle {
            remover: options.remover,
            introducer: options.introducer,
        });
    }
    if source.content != target.content {
        return Err(AffinePayloadIssue::UnsupportedContentChange);
    }
    if source.style != target.style {
        return Err(AffinePayloadIssue::UnsupportedStyleChange);
    }
    if source.z_index() != target.z_index() {
        return Err(AffinePayloadIssue::UnsupportedPainterOrderChange);
    }
    if source.signal_bindings() != target.signal_bindings() {
        return Err(AffinePayloadIssue::UnsupportedBindingChange);
    }
    if source.transform.translation.z != target.transform.translation.z {
        return Err(AffinePayloadIssue::UnsupportedDepthChange(
            SemanticAffineAnimationField::Translation,
        ));
    }
    if source.transform.scale.z != target.transform.scale.z {
        return Err(AffinePayloadIssue::UnsupportedDepthChange(
            SemanticAffineAnimationField::Scale,
        ));
    }
    Ok(())
}

pub(super) fn lower_affine_channels(
    source: &noon_core::SemanticObjectState,
    target: &noon_core::SemanticObjectState,
    from: Transform2D,
) -> Result<Vec<LoweredAffineChannel>, AffinePayloadIssue> {
    if !transform_is_finite(from) {
        return Err(AffinePayloadIssue::InvalidEffectiveTransform);
    }
    let translation = target
        .transform
        .translation
        .lower_xy_f32()
        .map_err(|error| AffinePayloadIssue::InvalidTargetValue {
            field: SemanticAffineAnimationField::Translation,
            error,
        })?;
    let scale = target.transform.scale.lower_xy_f32().map_err(|error| {
        AffinePayloadIssue::InvalidTargetValue {
            field: SemanticAffineAnimationField::Scale,
            error,
        }
    })?;
    let rotation = target.transform.rotation_z;
    if !rotation.is_finite() || rotation.abs() > f32::MAX as f64 {
        return Err(AffinePayloadIssue::TargetValueOutOfRange(
            SemanticAffineAnimationField::RotationZ,
        ));
    }
    let to = Transform2D {
        translation,
        rotation: rotation as f32,
        scale,
    };
    let mut channels = Vec::with_capacity(3);
    push_affine_channel(
        source,
        SemanticObjectProperty::Translation,
        Property::Position,
        TrackValues::Vec2 {
            from: from.translation,
            to: to.translation,
        },
        SemanticSignalValue::Vec3(target.transform.translation),
        from.translation != to.translation,
        &mut channels,
    )?;
    push_affine_channel(
        source,
        SemanticObjectProperty::RotationZ,
        Property::Rotation,
        TrackValues::Scalar {
            from: from.rotation,
            to: to.rotation,
        },
        SemanticSignalValue::Scalar(target.transform.rotation_z),
        from.rotation != to.rotation,
        &mut channels,
    )?;
    push_affine_channel(
        source,
        SemanticObjectProperty::Scale,
        Property::Scale,
        TrackValues::Vec2 {
            from: from.scale,
            to: to.scale,
        },
        SemanticSignalValue::Vec3(target.transform.scale),
        from.scale != to.scale,
        &mut channels,
    )?;
    Ok(channels)
}

#[allow(clippy::too_many_arguments)]
fn push_affine_channel(
    source: &noon_core::SemanticObjectState,
    semantic_property: SemanticObjectProperty,
    property: Property,
    values: TrackValues,
    completion_value: SemanticSignalValue,
    changed: bool,
    channels: &mut Vec<LoweredAffineChannel>,
) -> Result<(), AffinePayloadIssue> {
    if !changed {
        return Ok(());
    }
    if source
        .signal_bindings()
        .iter()
        .any(|binding| binding.property() == semantic_property)
    {
        return Err(AffinePayloadIssue::ReactiveDriverConflict(
            semantic_property,
        ));
    }
    channels.push(LoweredAffineChannel {
        property,
        semantic_property,
        completion_value,
        values,
    });
    Ok(())
}

fn existing_payload_error(
    leaf: &SemanticScheduledAnimationLeaf,
    issue: AffinePayloadIssue,
) -> SemanticAffineAnimationTrackError {
    match issue {
        AffinePayloadIssue::InvalidEffectiveTransform => {
            SemanticAffineAnimationTrackError::InvalidEffectiveTransform {
                animation: leaf.animation,
                target: leaf.target,
            }
        }
        AffinePayloadIssue::UnsupportedContentChange => {
            SemanticAffineAnimationTrackError::UnsupportedContentChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state: leaf.target_state,
            }
        }
        AffinePayloadIssue::UnsupportedStyleChange => {
            SemanticAffineAnimationTrackError::UnsupportedStyleChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state: leaf.target_state,
            }
        }
        AffinePayloadIssue::UnsupportedPainterOrderChange => {
            SemanticAffineAnimationTrackError::UnsupportedPainterOrderChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state: leaf.target_state,
            }
        }
        AffinePayloadIssue::UnsupportedBindingChange => {
            SemanticAffineAnimationTrackError::UnsupportedBindingChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state: leaf.target_state,
            }
        }
        AffinePayloadIssue::UnsupportedDepthChange(field) => {
            SemanticAffineAnimationTrackError::UnsupportedDepthChange {
                animation: leaf.animation,
                target: leaf.target,
                target_state: leaf.target_state,
                field,
            }
        }
        AffinePayloadIssue::UnsupportedLifecycle {
            remover,
            introducer,
        } => SemanticAffineAnimationTrackError::UnsupportedLifecycle {
            animation: leaf.animation,
            remover,
            introducer,
        },
        AffinePayloadIssue::ReactiveDriverConflict(property) => {
            SemanticAffineAnimationTrackError::ReactiveDriverConflict {
                animation: leaf.animation,
                target: leaf.target,
                property,
            }
        }
        AffinePayloadIssue::InvalidTargetValue { field, error } => {
            SemanticAffineAnimationTrackError::InvalidTargetValue {
                animation: leaf.animation,
                target_state: leaf.target_state,
                field,
                error,
            }
        }
        AffinePayloadIssue::TargetValueOutOfRange(field) => {
            SemanticAffineAnimationTrackError::TargetValueOutOfRange {
                animation: leaf.animation,
                target_state: leaf.target_state,
                field,
            }
        }
    }
}

fn driver_key(object: ObjectId, property: SemanticObjectProperty) -> (u64, u8) {
    let slot = match property {
        SemanticObjectProperty::Translation => 0,
        SemanticObjectProperty::RotationZ => 1,
        SemanticObjectProperty::Scale => 2,
        _ => unreachable!("affine payload lowering only registers affine drivers"),
    };
    (object.get(), slot)
}

pub(super) fn transform_is_finite(transform: Transform2D) -> bool {
    transform.translation.x.is_finite()
        && transform.translation.y.is_finite()
        && transform.rotation.is_finite()
        && transform.scale.x.is_finite()
        && transform.scale.y.is_finite()
}

#[cfg(test)]
mod tests {
    use noon_core::{AnimationOptions, SemanticObjectState, SemanticVec3, StoredGeometry, Vec2};

    use super::*;
    use crate::{lower_semantic_animation_schedule, SemanticExecutionIndex};

    fn visible_object(store: &mut SemanticStore) -> SemanticNodeId {
        let id = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 1.0,
        }));
        store.attach_to_scene(id).unwrap();
        id
    }

    fn index(store: &SemanticStore) -> SemanticExecutionIndex {
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(store).unwrap();
        index
    }

    fn schedule(
        store: &SemanticStore,
        index: &SemanticExecutionIndex,
        animation: SemanticNodeId,
    ) -> SemanticAnimationScheduleProjection {
        lower_semantic_animation_schedule(store, index, animation, 4.0, AnimationOptions::new())
            .unwrap()
    }

    #[test]
    fn lowers_affine_target_from_effective_activation_state() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let mut target_state = store.semantic_object_state_checked(target).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(10.0, -3.0, 0.0);
        target_state.transform.rotation_z = 1.25;
        target_state.transform.scale = SemanticVec3::new(2.0, 0.5, 1.0);
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = index(&store);
        let projection = lower_semantic_affine_animation_tracks(
            &store,
            &schedule(&store, &index, animation),
            |_| {
                Some(Transform2D {
                    translation: Vec2::new(5.0, 7.0),
                    rotation: 0.25,
                    scale: Vec2::new(1.5, 1.5),
                })
            },
        )
        .unwrap();

        assert_eq!(projection.len(), 3);
        assert_eq!(projection.tracks()[0].property, Property::Position);
        assert_eq!(
            projection.tracks()[0].values,
            TrackValues::Vec2 {
                from: Vec2::new(5.0, 7.0),
                to: Vec2::new(10.0, -3.0),
            }
        );
        assert_eq!(
            projection.tracks()[1].values,
            TrackValues::Scalar {
                from: 0.25,
                to: 1.25,
            }
        );
        assert_eq!(
            projection.tracks()[2].values,
            TrackValues::Vec2 {
                from: Vec2::new(1.5, 1.5),
                to: Vec2::new(2.0, 0.5),
            }
        );
        assert_eq!(
            projection.tracks()[0]
                .with_track_id(TrackId::new(42))
                .unwrap()
                .id,
            TrackId::new(42)
        );
    }

    #[test]
    fn captures_each_execution_object_once_per_activation() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);

        let mut rotation_state = store.semantic_object_state_checked(target).unwrap().clone();
        rotation_state.transform.rotation_z = 0.5;
        let rotation_state = store.insert_semantic_object(rotation_state);
        let rotation = store
            .insert_semantic_transform_animation(target, rotation_state, AnimationOptions::new())
            .unwrap();

        let mut scale_state = store.semantic_object_state_checked(target).unwrap().clone();
        scale_state.transform.scale = SemanticVec3::new(2.0, 2.0, 1.0);
        let scale_state = store.insert_semantic_object(scale_state);
        let scale = store
            .insert_semantic_transform_animation(target, scale_state, AnimationOptions::new())
            .unwrap();
        let root = store
            .insert_semantic_sequence_animation(&[rotation, scale], AnimationOptions::new())
            .unwrap();

        let index = index(&store);
        let calls = std::cell::Cell::new(0);
        let projection =
            lower_semantic_affine_animation_tracks(&store, &schedule(&store, &index, root), |_| {
                calls.set(calls.get() + 1);
                Some(Transform2D::default())
            })
            .unwrap();

        assert_eq!(calls.get(), 1);
        assert_eq!(projection.len(), 2);
    }

    #[test]
    fn stale_scheduled_leaf_is_rejected_against_authored_declaration() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let mut target_state = store.semantic_object_state_checked(target).unwrap().clone();
        target_state.transform.translation.x = 2.0;
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = index(&store);
        let mut leaf = schedule(&store, &index, animation).leaves()[0].clone();
        leaf.target_state = target;

        assert_eq!(
            validate_leaf_matches_declaration(&store, &leaf),
            Err(SemanticAffineAnimationTrackError::ScheduleMismatch { animation })
        );
    }

    #[test]
    fn duplicate_property_drivers_fail_closed_with_keyed_detection() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let mut first_state = store.semantic_object_state_checked(target).unwrap().clone();
        first_state.transform.translation.x = 2.0;
        let first_state = store.insert_semantic_object(first_state);
        let first = store
            .insert_semantic_transform_animation(target, first_state, AnimationOptions::new())
            .unwrap();
        let mut second_state = store.semantic_object_state_checked(target).unwrap().clone();
        second_state.transform.translation.x = 3.0;
        let second_state = store.insert_semantic_object(second_state);
        let second = store
            .insert_semantic_transform_animation(target, second_state, AnimationOptions::new())
            .unwrap();
        let root = store
            .insert_semantic_sequence_animation(&[first, second], AnimationOptions::new())
            .unwrap();
        let index = index(&store);

        assert_eq!(
            lower_semantic_affine_animation_tracks(&store, &schedule(&store, &index, root), |_| {
                Some(Transform2D::default())
            },),
            Err(SemanticAffineAnimationTrackError::MultipleDrivers {
                first_animation: first,
                next_animation: second,
                target,
                property: SemanticObjectProperty::Translation,
            })
        );
    }

    #[test]
    fn reactive_affine_driver_conflict_fails_closed() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let signal = store
            .insert_semantic_input_signal(SemanticVec3::ZERO)
            .unwrap();
        store
            .bind_semantic_signal(signal, target, SemanticObjectProperty::Translation)
            .unwrap();
        let mut target_state = store.semantic_object_state_checked(target).unwrap().clone();
        target_state.transform.translation.x = 4.0;
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = index(&store);

        assert_eq!(
            lower_semantic_affine_animation_tracks(
                &store,
                &schedule(&store, &index, animation),
                |_| Some(Transform2D::default()),
            ),
            Err(SemanticAffineAnimationTrackError::ReactiveDriverConflict {
                animation,
                target,
                property: SemanticObjectProperty::Translation,
            })
        );
    }

    #[test]
    fn missing_effective_capture_never_falls_back_to_authored_state() {
        let mut store = SemanticStore::new();
        let target = visible_object(&mut store);
        let mut target_state = store.semantic_object_state_checked(target).unwrap().clone();
        target_state.transform.translation.x = 2.0;
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = index(&store);
        let execution_object_id = index.execution_object_id(target).unwrap();

        assert_eq!(
            lower_semantic_affine_animation_tracks(
                &store,
                &schedule(&store, &index, animation),
                |_| None,
            ),
            Err(
                SemanticAffineAnimationTrackError::MissingEffectiveTransform {
                    animation,
                    target,
                    execution_object_id,
                }
            )
        );
    }
}
