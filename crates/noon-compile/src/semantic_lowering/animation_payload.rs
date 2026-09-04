use noon_core::{
    validate_track_definition, Property, SemanticAnimationError, SemanticAnimationIntent,
    SemanticLoweringError, SemanticNodeId, SemanticObjectProperty, SemanticSceneOperationError,
    SemanticStore, TimelineError, TrackDefinition, TrackId, TrackValues,
};

use super::SemanticScheduledAnimationLeaf;

/// Semantic state outside the first supported translation-only `TransformTo` payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticTransformPayloadField {
    Content,
    Scale,
    RotationZ,
    Style,
    ZIndex,
    TranslationZ,
    TranslationSignalBinding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticTransformPayloadEndpoint {
    Source,
    TargetState,
}

/// Failure while lowering one already-scheduled semantic `TransformTo` leaf into
/// the existing execution timeline vocabulary.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticAnimationPayloadLoweringError {
    Animation(SemanticAnimationError),
    Object {
        animation: SemanticNodeId,
        node: SemanticNodeId,
        error: SemanticSceneOperationError,
    },
    ScheduleMismatch {
        animation: SemanticNodeId,
    },
    UnsupportedStateChange {
        animation: SemanticNodeId,
        field: SemanticTransformPayloadField,
    },
    UnsupportedLifecycle {
        animation: SemanticNodeId,
        remover: bool,
        introducer: bool,
    },
    TranslationLowering {
        animation: SemanticNodeId,
        endpoint: SemanticTransformPayloadEndpoint,
        error: SemanticLoweringError,
    },
    InvalidTrack {
        animation: SemanticNodeId,
        error: TimelineError,
    },
}

impl std::fmt::Display for SemanticAnimationPayloadLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Animation(error) => error.fmt(formatter),
            Self::Object {
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
            Self::UnsupportedStateChange { animation, field } => write!(
                formatter,
                "semantic animation {}:{} changes unsupported TransformTo payload field {field:?}",
                animation.slot(),
                animation.generation()
            ),
            Self::UnsupportedLifecycle {
                animation,
                remover,
                introducer,
            } => write!(
                formatter,
                "semantic animation {}:{} cannot lower remover={remover} introducer={introducer} lifecycle semantics into a position track",
                animation.slot(),
                animation.generation()
            ),
            Self::TranslationLowering {
                animation,
                endpoint,
                error,
            } => write!(
                formatter,
                "semantic animation {}:{} cannot lower {endpoint:?} translation: {error}",
                animation.slot(),
                animation.generation()
            ),
            Self::InvalidTrack { animation, error } => write!(
                formatter,
                "semantic animation {}:{} produced an invalid position track: {error}",
                animation.slot(),
                animation.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticAnimationPayloadLoweringError {}

/// Lower the first target-state payload slice: a `TransformTo` whose only
/// execution-visible state change is x/y translation.
///
/// `track_id` is supplied by the activation/execution owner. A semantic animation
/// declaration may be activated repeatedly, so declaration identity is not reused
/// as mutable execution-track identity and this compiler helper introduces no
/// second allocator.
///
/// All unsupported target-state differences fail closed. The output is the existing
/// `TrackDefinition` consumed by `CompiledScene`/`SceneInstance`; no alternate
/// animation representation or evaluator is introduced.
pub fn lower_semantic_transform_position_track(
    store: &SemanticStore,
    leaf: &SemanticScheduledAnimationLeaf,
    track_id: TrackId,
) -> Result<TrackDefinition, SemanticAnimationPayloadLoweringError> {
    let animation = store
        .semantic_animation_state(leaf.animation)
        .map_err(SemanticAnimationPayloadLoweringError::Animation)?;
    match animation.intent() {
        SemanticAnimationIntent::TransformTo {
            target,
            target_state,
        } if *target == leaf.target && *target_state == leaf.target_state => {}
        _ => {
            return Err(SemanticAnimationPayloadLoweringError::ScheduleMismatch {
                animation: leaf.animation,
            });
        }
    }

    if leaf.options.remover || leaf.options.introducer {
        return Err(
            SemanticAnimationPayloadLoweringError::UnsupportedLifecycle {
                animation: leaf.animation,
                remover: leaf.options.remover,
                introducer: leaf.options.introducer,
            },
        );
    }

    let source = store
        .semantic_object_state_checked(leaf.target)
        .map_err(|error| SemanticAnimationPayloadLoweringError::Object {
            animation: leaf.animation,
            node: leaf.target,
            error,
        })?;
    let target = store
        .semantic_object_state_checked(leaf.target_state)
        .map_err(|error| SemanticAnimationPayloadLoweringError::Object {
            animation: leaf.animation,
            node: leaf.target_state,
            error,
        })?;

    reject_if(
        source.content != target.content,
        leaf.animation,
        SemanticTransformPayloadField::Content,
    )?;
    reject_if(
        source.transform.scale != target.transform.scale,
        leaf.animation,
        SemanticTransformPayloadField::Scale,
    )?;
    reject_if(
        source.transform.rotation_z != target.transform.rotation_z,
        leaf.animation,
        SemanticTransformPayloadField::RotationZ,
    )?;
    reject_if(
        source.style != target.style,
        leaf.animation,
        SemanticTransformPayloadField::Style,
    )?;
    reject_if(
        source.z_index() != target.z_index(),
        leaf.animation,
        SemanticTransformPayloadField::ZIndex,
    )?;
    reject_if(
        source.transform.translation.z != target.transform.translation.z,
        leaf.animation,
        SemanticTransformPayloadField::TranslationZ,
    )?;
    reject_if(
        source
            .signal_bindings()
            .iter()
            .any(|binding| binding.property() == SemanticObjectProperty::Translation),
        leaf.animation,
        SemanticTransformPayloadField::TranslationSignalBinding,
    )?;

    let from = source
        .transform
        .translation
        .lower_xy_f32()
        .map_err(
            |error| SemanticAnimationPayloadLoweringError::TranslationLowering {
                animation: leaf.animation,
                endpoint: SemanticTransformPayloadEndpoint::Source,
                error,
            },
        )?;
    let to = target
        .transform
        .translation
        .lower_xy_f32()
        .map_err(
            |error| SemanticAnimationPayloadLoweringError::TranslationLowering {
                animation: leaf.animation,
                endpoint: SemanticTransformPayloadEndpoint::TargetState,
                error,
            },
        )?;

    let track = TrackDefinition {
        id: track_id,
        object: leaf.execution_object_id,
        property: Property::Position,
        values: TrackValues::Vec2 { from, to },
        timing: leaf.timing,
        time_map: leaf.time_map.clone(),
    };
    validate_track_definition(&track).map_err(|error| {
        SemanticAnimationPayloadLoweringError::InvalidTrack {
            animation: leaf.animation,
            error,
        }
    })?;
    Ok(track)
}

fn reject_if(
    condition: bool,
    animation: SemanticNodeId,
    field: SemanticTransformPayloadField,
) -> Result<(), SemanticAnimationPayloadLoweringError> {
    if condition {
        return Err(
            SemanticAnimationPayloadLoweringError::UnsupportedStateChange { animation, field },
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, RateFunction, SemanticObjectState, SemanticVec3, StoredGeometry, Vec2,
    };

    use super::*;
    use crate::{lower_semantic_animation_schedule, SemanticExecutionIndex};

    struct Fixture {
        store: SemanticStore,
        index: SemanticExecutionIndex,
        animation: SemanticNodeId,
        target: SemanticNodeId,
        target_state: SemanticNodeId,
    }

    fn fixture_from_states(
        source: SemanticObjectState,
        target_state: SemanticObjectState,
        options: AnimationOptions,
    ) -> Fixture {
        let mut store = SemanticStore::new();
        let target = store.insert_semantic_object(source);
        store.attach_to_scene(target).unwrap();
        let target_state = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, options)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        Fixture {
            store,
            index,
            animation,
            target,
            target_state,
        }
    }

    fn translation_fixture(from: SemanticVec3, to: SemanticVec3) -> Fixture {
        let mut source = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
        source.transform.translation = from;
        let mut target_state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
        target_state.transform.translation = to;
        fixture_from_states(
            source,
            target_state,
            AnimationOptions::new().rate_func(RateFunction::Linear),
        )
    }

    fn fixture_with_target_change(
        field: SemanticTransformPayloadField,
    ) -> (Fixture, SemanticTransformPayloadField) {
        let source = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
        let mut target_state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
        target_state.transform.translation = SemanticVec3::new(2.0, 0.0, 0.0);
        match field {
            SemanticTransformPayloadField::Content => {
                target_state.content = StoredGeometry::Circle { radius: 2.0 }.into();
            }
            SemanticTransformPayloadField::Scale => {
                target_state.transform.scale = SemanticVec3::new(2.0, 1.0, 1.0);
            }
            SemanticTransformPayloadField::RotationZ => {
                target_state.transform.rotation_z = 0.5;
            }
            SemanticTransformPayloadField::Style => {
                target_state.style.object_opacity = 0.5;
            }
            SemanticTransformPayloadField::ZIndex => target_state.set_z_index(4),
            SemanticTransformPayloadField::TranslationZ => {
                target_state.transform.translation.z = 3.0;
            }
            SemanticTransformPayloadField::TranslationSignalBinding => {
                unreachable!("translation signal binding is authored on the source after insertion")
            }
        }
        (
            fixture_from_states(
                source,
                target_state,
                AnimationOptions::new().rate_func(RateFunction::Linear),
            ),
            field,
        )
    }

    fn scheduled_leaf(fixture: &Fixture) -> SemanticScheduledAnimationLeaf {
        lower_semantic_animation_schedule(
            &fixture.store,
            &fixture.index,
            fixture.animation,
            2.0,
            AnimationOptions::new(),
        )
        .unwrap()
        .leaves()[0]
            .clone()
    }

    #[test]
    fn lowers_translation_only_target_state_into_existing_position_track() {
        let fixture = translation_fixture(
            SemanticVec3::new(1.0, -2.0, 0.0),
            SemanticVec3::new(5.0, 4.0, 0.0),
        );
        let leaf = scheduled_leaf(&fixture);
        let track =
            lower_semantic_transform_position_track(&fixture.store, &leaf, TrackId::new(41))
                .unwrap();

        assert_eq!(track.id, TrackId::new(41));
        assert_eq!(
            track.object,
            fixture.index.execution_object_id(fixture.target).unwrap()
        );
        assert_eq!(track.property, Property::Position);
        assert_eq!(
            track.values,
            TrackValues::Vec2 {
                from: Vec2::new(1.0, -2.0),
                to: Vec2::new(5.0, 4.0),
            }
        );
        assert_eq!(track.timing, leaf.timing);
        assert_eq!(track.time_map, leaf.time_map);
    }

    #[test]
    fn repeated_activation_uses_execution_owned_track_identity() {
        let fixture = translation_fixture(SemanticVec3::ZERO, SemanticVec3::new(2.0, 0.0, 0.0));
        let leaf = scheduled_leaf(&fixture);
        let first = lower_semantic_transform_position_track(&fixture.store, &leaf, TrackId::new(7))
            .unwrap();
        let second =
            lower_semantic_transform_position_track(&fixture.store, &leaf, TrackId::new(8))
                .unwrap();

        assert_eq!(first.id, TrackId::new(7));
        assert_eq!(second.id, TrackId::new(8));
        assert_eq!(first.values, second.values);
    }

    #[test]
    fn rejects_every_non_translation_target_state_change_in_this_slice() {
        for field in [
            SemanticTransformPayloadField::Content,
            SemanticTransformPayloadField::Scale,
            SemanticTransformPayloadField::RotationZ,
            SemanticTransformPayloadField::Style,
            SemanticTransformPayloadField::ZIndex,
            SemanticTransformPayloadField::TranslationZ,
        ] {
            let (fixture, field) = fixture_with_target_change(field);
            let leaf = scheduled_leaf(&fixture);
            assert_eq!(
                lower_semantic_transform_position_track(&fixture.store, &leaf, TrackId::new(1))
                    .unwrap_err(),
                SemanticAnimationPayloadLoweringError::UnsupportedStateChange {
                    animation: fixture.animation,
                    field,
                }
            );
        }
    }

    #[test]
    fn rejects_position_track_when_translation_is_native_reactive() {
        let mut fixture = translation_fixture(SemanticVec3::ZERO, SemanticVec3::new(2.0, 0.0, 0.0));
        let signal = fixture
            .store
            .insert_semantic_input_signal(SemanticVec3::ZERO)
            .unwrap();
        fixture
            .store
            .bind_semantic_signal(signal, fixture.target, SemanticObjectProperty::Translation)
            .unwrap();
        let leaf = scheduled_leaf(&fixture);

        assert_eq!(
            lower_semantic_transform_position_track(&fixture.store, &leaf, TrackId::new(1))
                .unwrap_err(),
            SemanticAnimationPayloadLoweringError::UnsupportedStateChange {
                animation: fixture.animation,
                field: SemanticTransformPayloadField::TranslationSignalBinding,
            }
        );
    }

    #[test]
    fn rejects_lifecycle_flags_until_lifecycle_payload_is_lowered() {
        let mut fixture = translation_fixture(SemanticVec3::ZERO, SemanticVec3::new(2.0, 0.0, 0.0));
        let animation = fixture
            .store
            .insert_semantic_transform_animation(
                fixture.target,
                fixture.target_state,
                AnimationOptions::new().remover(true),
            )
            .unwrap();
        fixture.animation = animation;
        let leaf = scheduled_leaf(&fixture);

        assert_eq!(
            lower_semantic_transform_position_track(&fixture.store, &leaf, TrackId::new(1))
                .unwrap_err(),
            SemanticAnimationPayloadLoweringError::UnsupportedLifecycle {
                animation,
                remover: true,
                introducer: false,
            }
        );
    }
}
