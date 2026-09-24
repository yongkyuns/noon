use super::*;
use noon_compile::{CompiledObject, CompiledScene, ExecutionMutationTransaction};
use noon_core::{
    Color, CompositionTimeMap, GeometryRef, RateFunction, Style, TrackId, TrackTiming, TrackValues,
    Transform2D, Vec2,
};

fn instance(count: usize, tracks: &[TrackDefinition]) -> SceneInstance {
    SceneInstance::new(
        CompiledScene::compile_objects(
            (1..=count)
                .map(|id| {
                    CompiledObject::new(
                        ObjectId::new(id as u64),
                        GeometryRef::circle(1.0),
                        Transform2D::IDENTITY,
                        Style {
                            fill: Some(Color::BLUE),
                            ..Style::default()
                        },
                    )
                })
                .collect(),
            tracks,
        )
        .unwrap(),
    )
}

fn scale(object: u64) -> TrackDefinition {
    TrackDefinition {
        id: TrackId::new(1),
        object: ObjectId::new(object),
        property: Property::Scale,
        values: TrackValues::Vec2 {
            from: Vec2::ONE,
            to: Vec2::new(1.2, 1.2),
        },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::ThereAndBack),
        time_map: CompositionTimeMap::identity(),
    }
}

fn moving(object: u64, property: Property, values: TrackValues) -> TrackDefinition {
    TrackDefinition {
        id: TrackId::new(10 + property as u64),
        object: ObjectId::new(object),
        property,
        values,
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    }
}

#[test]
fn independent_time_midpoint_exact_release_and_retrigger() {
    let mut runtime = instance(1, &[]);
    runtime.take_frame_changes();
    let original = runtime.frame().clone();
    let context = runtime.publication_context();
    let token = runtime
        .start_restoring_property_animation(&[scale(1)])
        .unwrap();
    assert_eq!(runtime.frame(), &original);
    assert!(runtime.take_frame_changes().is_empty());
    assert!(runtime.wake_state().property_animation_pending());
    assert!(!runtime.wake_state().is_quiescent());
    assert_eq!(
        runtime.wake_state().timeline(),
        crate::TimelineWakeState::Quiescent
    );
    runtime.advance_property_animations_by(0.5).unwrap();
    assert_eq!(runtime.frame().time, 0.0);
    assert_eq!(
        runtime.frame().objects[0].transform.scale,
        Vec2::new(1.2, 1.2)
    );
    assert_eq!(runtime.property_animation_elapsed(token), Some(0.5));
    runtime.advance_property_animations_by(0.5).unwrap();
    assert_eq!(runtime.frame(), &original);
    assert_eq!(runtime.property_animation_elapsed(token), None);
    runtime.take_frame_changes();
    assert!(runtime.wake_state().is_quiescent());
    let released = runtime.publication_context();
    assert_eq!(released.scene_revision(), context.scene_revision());
    assert_eq!(released.execution_revision(), context.execution_revision());
    runtime.advance_property_animations_by(1000.0).unwrap();
    assert_eq!(
        runtime.publication_context(),
        released,
        "idle must not poll or publish"
    );
    let second = runtime
        .start_restoring_property_animation(&[scale(1)])
        .unwrap();
    assert_ne!(second, token);
    runtime.advance_property_animations_by(0.5).unwrap();
    assert_eq!(
        runtime.frame().objects[0].transform.scale,
        Vec2::new(1.2, 1.2)
    );
}

#[test]
fn concurrent_authored_motion_keeps_its_time_and_unowned_fields() {
    let mut runtime = instance(
        2,
        &[
            moving(
                1,
                Property::Position,
                TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::new(8.0, 4.0),
                },
            ),
            moving(
                1,
                Property::Opacity,
                TrackValues::Scalar { from: 1.0, to: 0.4 },
            ),
        ],
    );
    let token = runtime
        .start_restoring_property_animation(&[scale(1)])
        .unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    runtime.advance_to(1.0).unwrap();
    assert_eq!(runtime.property_animation_elapsed(token), Some(0.5));
    assert_eq!(
        runtime.frame().objects[0].transform.translation,
        Vec2::new(4.0, 2.0)
    );
    assert_eq!(
        runtime.frame().objects[0].transform.scale,
        Vec2::new(1.2, 1.2)
    );
    assert!((runtime.frame().objects[0].style.opacity - 0.7).abs() < 1e-6);
    runtime.advance_property_animations_by(0.5).unwrap();
    assert_eq!(runtime.frame().time, 1.0);
    assert_eq!(
        runtime.frame().objects[0].transform.translation,
        Vec2::new(4.0, 2.0)
    );
    assert_eq!(runtime.frame().objects[0].transform.scale, Vec2::ONE);
    assert!((runtime.frame().objects[0].style.opacity - 0.7).abs() < 1e-6);
}

#[test]
fn duplicate_claim_is_atomic_but_disjoint_effects_coexist() {
    let mut runtime = instance(2, &[]);
    let one = runtime
        .start_restoring_property_animation(&[scale(1)])
        .unwrap();
    let context = runtime.publication_context();
    assert!(matches!(
        runtime.start_restoring_property_animation(&[scale(1)]),
        Err(PropertyAnimationError::ChannelBusy {
            property: Property::Scale,
            ..
        })
    ));
    assert_eq!(runtime.publication_context(), context);
    let two = runtime
        .start_restoring_property_animation(&[scale(2)])
        .unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    runtime.cancel_property_animation(one).unwrap();
    assert_eq!(runtime.frame().objects[0].transform.scale, Vec2::ONE);
    assert_eq!(
        runtime.frame().objects[1].transform.scale,
        Vec2::new(1.2, 1.2)
    );
    assert_eq!(runtime.property_animation_elapsed(two), Some(0.5));
}

#[test]
fn authored_edit_on_another_object_preserves_effect_and_target_edit_retires_it() {
    let mut runtime = instance(2, &[]);
    let token = runtime
        .start_restoring_property_animation(&[scale(1)])
        .unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    let transform = Transform2D {
        translation: Vec2::new(4.0, 2.0),
        ..Transform2D::IDENTITY
    };
    runtime
        .apply_execution_patch(&ExecutionPatch::SetTransform {
            object: ObjectId::new(2),
            transform,
        })
        .unwrap();
    assert_eq!(runtime.property_animation_elapsed(token), Some(0.5));
    let style = Style {
        fill: Some(Color::RED),
        ..Style::default()
    };
    runtime
        .apply_execution_patch(&ExecutionPatch::SetStyle {
            object: ObjectId::new(1),
            style,
        })
        .unwrap();
    assert_eq!(runtime.property_animation_elapsed(token), None);
    runtime.advance_property_animations_by(1.0).unwrap();
    assert_eq!(runtime.frame().objects[0].transform.scale, Vec2::ONE);
    assert_eq!(runtime.frame().objects[0].style, style);
    assert_eq!(runtime.frame().objects[1].transform, transform);
}

#[test]
fn failed_authored_transaction_does_not_retire_or_partially_restore() {
    let mut runtime = instance(1, &[]);
    let token = runtime
        .start_restoring_property_animation(&[scale(1)])
        .unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    let before = runtime.frame().clone();
    let context = runtime.publication_context();
    let transaction = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::SetStyle {
            object: ObjectId::new(1),
            style: Style {
                fill: Some(Color::RED),
                ..Style::default()
            },
        },
        ExecutionPatch::RemoveObject(ObjectId::new(999)),
    ]);
    assert!(runtime.apply_execution_transaction(&transaction).is_err());
    assert_eq!(runtime.frame(), &before);
    assert_eq!(runtime.publication_context(), context);
    assert_eq!(runtime.property_animation_elapsed(token), Some(0.5));
}

#[test]
fn retirement_and_same_time_seek_cannot_resurrect_a_driver() {
    let mut runtime = instance(1, &[]);
    let old = runtime
        .start_restoring_property_animation(&[scale(1)])
        .unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(1)))
        .unwrap();
    runtime.advance_property_animations_by(1.0).unwrap();
    assert!(runtime.property_animation_elapsed(old).is_none());
    assert!(!runtime.frame().presences[0]);
    let compiled = CompiledObject::new(
        ObjectId::new(1),
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    );
    runtime
        .apply_execution_patch(&ExecutionPatch::CreateObject(compiled))
        .unwrap();
    let new = runtime
        .start_restoring_property_animation(&[scale(1)])
        .unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    let context = runtime.publication_context();
    runtime.seek(0.0).unwrap();
    assert_ne!(runtime.publication_context(), context);
    assert!(runtime.property_animation_elapsed(new).is_none());
    assert_eq!(runtime.frame().objects[0].transform.scale, Vec2::ONE);
    assert_ne!(old, new);
}

#[test]
fn opaque_tokens_and_prepared_frames_reject_replaced_lifetimes() {
    let mut runtime = instance(1, &[]);
    let old_prepared = runtime.prepare_advance_to(0.0).unwrap();
    let old_batch = runtime.prepare_effective_property_batch(&[]).unwrap();
    let token = runtime
        .start_restoring_property_animation(&[scale(1)])
        .unwrap();
    assert!(matches!(
        runtime.commit_prepared_frame(old_prepared, old_batch),
        Err(PreparedFrameCommitError::StalePublication { .. })
    ));
    let mut cloned = runtime.clone();
    assert!(matches!(
        cloned.cancel_property_animation(token),
        Err(PropertyAnimationError::ForeignToken)
    ));
    runtime.advance_property_animations_by(0.5).unwrap();
    let prepared = runtime.prepare_advance_to(0.0).unwrap();
    let batch = runtime.prepare_effective_property_batch(&[]).unwrap();
    runtime.cancel_property_animation(token).unwrap();
    assert!(matches!(
        runtime.commit_prepared_frame(prepared, batch),
        Err(PreparedFrameCommitError::StalePublication { .. })
    ));
}

#[test]
fn competing_effective_writes_reject_atomically_and_unowned_fields_work() {
    let mut runtime = instance(1, &[]);
    let token = runtime
        .start_restoring_property_animation(&[scale(1)])
        .unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    let phase = runtime.prepare_advance_to(0.0).unwrap();
    let batch = runtime
        .prepare_effective_property_batch(&[EffectivePropertyWrite::Scale {
            object: ObjectId::new(1),
            scale: Vec2::ONE,
        }])
        .unwrap();
    let before = runtime.frame().clone();
    assert!(matches!(
        runtime.commit_prepared_frame(phase, batch),
        Err(PreparedFrameCommitError::PropertyAnimationConflict { .. })
    ));
    assert_eq!(runtime.frame(), &before);
    let phase = runtime.prepare_advance_to(0.0).unwrap();
    let batch = runtime
        .prepare_effective_property_batch(&[EffectivePropertyWrite::Translation {
            object: ObjectId::new(1),
            translation: Vec2::new(4.0, 2.0),
        }])
        .unwrap();
    runtime.commit_prepared_frame(phase, batch).unwrap();
    runtime.cancel_property_animation(token).unwrap();
    assert_eq!(
        runtime.frame().objects[0].transform.translation,
        Vec2::new(4.0, 2.0)
    );
    assert_eq!(runtime.frame().objects[0].transform.scale, Vec2::ONE);
}

#[test]
fn invalid_time_and_enormous_finite_delta_are_bounded_and_atomic() {
    let mut runtime = instance(1, &[]);
    let token = runtime
        .start_restoring_property_animation(&[scale(1)])
        .unwrap();
    let before = runtime.frame().clone();
    let context = runtime.publication_context();
    for delta in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(runtime.advance_property_animations_by(delta).is_err());
        assert_eq!(runtime.frame(), &before);
        assert_eq!(runtime.publication_context(), context);
        assert_eq!(runtime.property_animation_elapsed(token), Some(0.0));
    }
    runtime.advance_property_animations_by(f64::MAX).unwrap();
    assert!(runtime.property_animation_elapsed(token).is_none());
    assert_eq!(runtime.frame(), &before);
}

#[test]
fn claimed_future_timeline_channel_is_rejected_without_global_exclusion() {
    let mut future = scale(1);
    future.timing.start_time = 10.0;
    let mut runtime = instance(2, &[future]);
    assert!(matches!(
        runtime.start_restoring_property_animation(&[scale(1)]),
        Err(PropertyAnimationError::ChannelBusy { .. })
    ));
    runtime
        .start_restoring_property_animation(&[scale(2)])
        .unwrap();
}

#[test]
fn sealed_replay_stays_read_only_and_live_effects_are_not_silently_recorded() {
    let mut runtime = instance(1, &[]);
    runtime
        .begin_replay_retention(crate::ReplayLimits::default())
        .unwrap();
    runtime.advance_to(1.0).unwrap();
    runtime.seal_replay().unwrap();
    let before = runtime.frame().clone();
    assert!(matches!(
        runtime.start_restoring_property_animation(&[scale(1)]),
        Err(PropertyAnimationError::ReplaySealed)
    ));
    assert_eq!(runtime.frame(), &before);
    assert!(runtime.replay_is_sealed());
    let mut live = instance(1, &[]);
    live.begin_replay_retention(crate::ReplayLimits::default())
        .unwrap();
    live.start_restoring_property_animation(&[scale(1)])
        .unwrap();
    assert_eq!(live.seal_replay(), Err(crate::ReplayError::UnrecordedInput));
}

#[test]
fn one_effect_among_ten_thousand_only_publishes_its_row() {
    let mut runtime = instance(10_000, &[]);
    runtime.take_frame_changes();
    runtime
        .start_restoring_property_animation(&[scale(5000)])
        .unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    let changes = runtime.take_frame_changes();
    assert_eq!(changes.object_indices(), &[4999]);
    assert_eq!(runtime.last_stats().groups_evaluated, 0);
    runtime.advance_property_animations_by(0.5).unwrap();
    assert_eq!(runtime.take_frame_changes().object_indices(), &[4999]);
}

#[test]
fn actual_semantic_indicate_uses_the_existing_lowerer_and_runtime_interpolator() {
    use noon_compile::{
        lower_semantic_affine_animation_tracks, lower_semantic_animation_schedule,
        lower_semantic_execution, EffectiveAnimationProperties, SemanticAnimationCompletion,
        SemanticExecutionIndex,
    };
    use noon_core::{
        AnimationOptions, SemanticObjectState, SemanticStore, SemanticVec3, StoredGeometry,
    };
    let mut store = SemanticStore::new();
    let target = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let animation = store
        .insert_semantic_indicate_animation(
            target,
            1.2,
            Color::YELLOW,
            SemanticVec3::ZERO,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::ThereAndBack),
        )
        .unwrap();
    store.attach_semantic_object(target).unwrap();
    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution(&store, &mut index).unwrap();
    let mut runtime = SceneInstance::from_semantic_execution(lowered);
    let object = index.execution_object_id(target).unwrap();
    let original = runtime.frame().clone();
    let schedule =
        lower_semantic_animation_schedule(&store, &index, animation, 0.0, AnimationOptions::new())
            .unwrap();
    let projected = lower_semantic_affine_animation_tracks(&store, &schedule, |id| {
        let row = runtime.effective_object(id)?;
        Some(EffectiveAnimationProperties {
            z_index: row.z_index,
            transform: row.transform,
            style: row.style,
            appearance: row.appearance,
            reveal: 1.0,
        })
    })
    .unwrap();
    assert!(!projected.is_empty());
    assert!(projected
        .tracks()
        .iter()
        .all(|track| track.completion == SemanticAnimationCompletion::Release));
    let tracks: Vec<_> = projected
        .tracks()
        .iter()
        .enumerate()
        .map(|(i, track)| track.with_track_id(TrackId::new(i as u64)).unwrap())
        .collect();
    let token = runtime.start_restoring_property_animation(&tracks).unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    assert_eq!(
        runtime.effective_object(object).unwrap().transform.scale,
        Vec2::new(1.2, 1.2)
    );
    assert_eq!(runtime.frame().time, 0.0);
    runtime.advance_property_animations_by(0.5).unwrap();
    assert_eq!(runtime.frame(), &original);
    assert!(runtime.property_animation_elapsed(token).is_none());
}
