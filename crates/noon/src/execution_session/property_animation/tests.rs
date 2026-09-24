use super::*;
use crate::{integration::PointerFillOutcome, LiveSessionError, Mobject, RateFunction, Scene};
use noon_core::{
    Color, NativeInputModifiers, NativePointerId, NativePointerInput, NativePointerInputKind, Vec2,
};

fn fixture() -> (Scene, Mobject, ExecutionSession) {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.5).unwrap();
    target.set_fill(0.0, 0.5, 1.0, 0.7).unwrap();
    target.set_translation(2.0, 1.0).unwrap();
    scene.add(&target).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_renderer_publication();
    (scene, target, session)
}

fn start(
    scene: &Scene,
    target: &Mobject,
    session: &mut ExecutionSession,
) -> PropertyAnimationToken {
    scene
        .live(session)
        .start_indicate_effect(
            target,
            IndicateOptions::default(),
            AnimationOptions::new().run_time(1.0),
        )
        .unwrap()
        .unwrap()
}

#[test]
fn independent_indicate_matches_ordinary_semantic_animation_at_every_sample() {
    let (scene, target, mut session) = fixture();
    let (reference_scene, reference_target, mut reference) = fixture();
    session.advance_to(3.0).unwrap();
    reference.advance_to(3.0).unwrap();
    let before = session.publication_context();
    let nodes = scene.integration_store().borrow().len();
    let token = start(&scene, &target, &mut session);
    let segment = reference_scene
        .live(&mut reference)
        .declare_and_activate_indicate(
            &reference_target,
            IndicateOptions::default(),
            AnimationOptions::new().run_time(1.0),
        )
        .unwrap();
    for step in 0..=20 {
        let elapsed = f64::from(step) / 20.0;
        if step != 0 {
            session.advance_property_animations_by(0.05).unwrap();
        }
        reference
            .advance_segment_to(segment, 3.0 + elapsed)
            .unwrap();
        let actual = scene.live(&mut session).effective(&target).unwrap();
        let expected = reference_scene
            .live(&mut reference)
            .effective(&reference_target)
            .unwrap();
        assert!((actual.transform.scale.x - expected.transform.scale.x).abs() < 1e-5);
        assert_eq!(actual.transform.translation, expected.transform.translation);
        let a = actual.style.fill.unwrap();
        let b = expected.style.fill.unwrap();
        assert!((a.red - b.red).abs() < 1e-5);
        assert!((a.green - b.green).abs() < 1e-5);
        assert!((a.blue - b.blue).abs() < 1e-5);
        assert_eq!(a.alpha, b.alpha);
        assert_eq!(session.frame().time, 3.0);
    }
    assert_eq!(session.property_animation_elapsed(token), None);
    assert_eq!(
        session.publication_context().scene_revision(),
        before.scene_revision()
    );
    assert_eq!(
        session.publication_context().execution_revision(),
        before.execution_revision()
    );
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert!(session.pending_segment_token().is_none());
    assert!(!session.has_replay_timeline_work());
}

#[test]
fn activation_recaptures_effective_center_and_does_not_accumulate_semantic_nodes() {
    let (scene, target, mut session) = fixture();
    scene
        .live(&mut session)
        .set_translation(&target, 5.0, -2.0)
        .unwrap();
    let nodes = scene.integration_store().borrow().len();
    let revision = session.publication_context().scene_revision();
    for _ in 0..40 {
        start(&scene, &target, &mut session);
        session.advance_property_animations_by(0.5).unwrap();
        let state = scene.live(&mut session).effective(&target).unwrap();
        assert_eq!(state.transform.translation, Vec2::new(5.0, -2.0));
        assert_eq!(state.transform.scale, Vec2::new(1.2, 1.2));
        session.advance_property_animations_by(0.5).unwrap();
        assert!(!session.has_property_animations());
    }
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(session.publication_context().scene_revision(), revision);
    assert_eq!(session.frame().time, 0.0);
}

#[test]
fn unrelated_segment_survives_effect_activation_progress_and_completion() {
    let mut scene = Scene::new();
    let moving = scene.circle(0.4).unwrap();
    let indicated = scene.circle(0.5).unwrap();
    scene.add(&moving).unwrap();
    scene.add(&indicated).unwrap();
    let mut destination = moving.target_editor().unwrap();
    destination.set_translation(4.0, 0.0).unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(
            &moving,
            &destination,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    session.advance_segment_to(segment, 0.5).unwrap();
    let pending = session.pending_segment_token();
    let token = start(&scene, &indicated, &mut session);
    session.advance_property_animations_by(0.5).unwrap();
    assert_eq!(session.pending_segment_token(), pending);
    assert_eq!(session.frame().time, 0.5);
    assert_eq!(
        scene
            .live(&mut session)
            .effective(&moving)
            .unwrap()
            .transform
            .translation
            .x,
        1.0
    );
    // The separate authored mutation guard remains in force.
    assert!(scene
        .live(&mut session)
        .set_translation(&moving, 9.0, 0.0)
        .is_err());
    session.advance_segment_to(segment, 1.0).unwrap();
    assert_eq!(session.property_animation_elapsed(token), Some(0.5));
    session.advance_property_animations_by(0.5).unwrap();
    session.advance_segment_to(segment, 2.0).unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(
        scene
            .live(&mut session)
            .effective(&moving)
            .unwrap()
            .transform
            .translation
            .x,
        4.0
    );
    assert!(session.pending_segment_token().is_none());
}

#[test]
fn translation_on_the_same_target_is_independent_of_centered_scale_and_paint() {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.5).unwrap();
    target.set_translation(2.0, 1.0).unwrap();
    scene.add(&target).unwrap();
    let mut destination = target.target_editor().unwrap();
    destination.set_translation(4.0, 1.0).unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(
            &target,
            &destination,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    start(&scene, &target, &mut session);
    session.advance_property_animations_by(0.5).unwrap();
    session.advance_segment_to(segment, 1.0).unwrap();
    let state = scene.live(&mut session).effective(&target).unwrap();
    assert_eq!(state.transform.translation, Vec2::new(3.0, 1.0));
    assert_eq!(state.transform.scale, Vec2::new(1.2, 1.2));
    session.advance_property_animations_by(0.5).unwrap();
    let state = scene.live(&mut session).effective(&target).unwrap();
    assert_eq!(state.transform.translation, Vec2::new(3.0, 1.0));
    assert_eq!(state.transform.scale, Vec2::ONE);
}

#[test]
fn competing_activation_is_atomic_and_retrigger_after_release_is_allowed() {
    let (scene, target, mut session) = fixture();
    let token = start(&scene, &target, &mut session);
    session.advance_property_animations_by(0.5).unwrap();
    let frame = session.frame().clone();
    let context = session.publication_context();
    let next_track = session.next_activation_track_id;
    assert!(matches!(
        scene.live(&mut session).start_indicate_effect(
            &target,
            IndicateOptions::default(),
            AnimationOptions::new().run_time(1.0)
        ),
        Err(LiveSessionError::Activation(
            ExecutionSessionAnimationError::PropertyAnimation(
                PropertyAnimationError::ChannelBusy { .. }
            )
        ))
    ));
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), context);
    assert_eq!(session.next_activation_track_id, next_track);
    assert_eq!(session.property_animation_elapsed(token), Some(0.5));
    session.cancel_property_animation(token).unwrap();
    assert_ne!(start(&scene, &target, &mut session), token);
}

#[test]
fn invalid_options_foreign_store_and_stale_revision_never_activate_or_publish() {
    let (scene, target, mut session) = fixture();
    let before = session.publication_context();
    let count = scene.integration_store().borrow().len();
    for options in [
        AnimationOptions::new().run_time(-1.0),
        AnimationOptions::new().rate_func(RateFunction::Linear),
    ] {
        assert!(scene
            .live(&mut session)
            .start_indicate_effect(&target, IndicateOptions::default(), options)
            .is_err());
    }
    let foreign = Scene::new();
    assert!(session
        .start_indicate_effect(
            &mut foreign.integration_store().borrow_mut(),
            target.node_id(),
            SemanticVec3::ZERO,
            IndicateOptions::default(),
            AnimationOptions::new()
        )
        .is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(scene.integration_store().borrow().len(), count);
    assert!(!session.has_property_animations());
    let mut edit = target.clone();
    edit.set_translation(7.0, 0.0).unwrap();
    assert!(scene
        .live(&mut session)
        .start_indicate_effect(&target, IndicateOptions::default(), AnimationOptions::new())
        .is_err());
    assert_eq!(session.publication_context(), before);
}

#[test]
fn no_op_indication_and_invalid_deltas_do_not_create_work_or_consume_identity() {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.5).unwrap();
    target.set_fill(1.0, 1.0, 0.0, 1.0).unwrap();
    target.set_stroke_color(1.0, 1.0, 0.0, 1.0).unwrap();
    scene.add(&target).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_renderer_publication();
    let before = session.publication_context();
    let next = session.next_activation_track_id;
    assert_eq!(
        scene
            .live(&mut session)
            .start_indicate_effect(
                &target,
                IndicateOptions::new(1.0, Color::rgba(1.0, 1.0, 0.0, 1.0)),
                AnimationOptions::new().run_time(1.0)
            )
            .unwrap(),
        None
    );
    for delta in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(session.advance_property_animations_by(delta).is_err());
    }
    session.advance_property_animations_by(0.0).unwrap();
    assert_eq!(session.publication_context(), before);
    assert_eq!(session.next_activation_track_id, next);
    assert!(session.wake_state().is_quiescent());
}

#[test]
fn callback_pending_and_termination_cannot_be_bypassed_by_effect_ticks() {
    let (scene, target, mut session) = fixture();
    let effect = start(&scene, &target, &mut session);
    let dirty_before_barrier = session.wake_state().frame_pending();
    let overlay = session
        .begin_required_callback_phase(0.0, [target.node_id()])
        .unwrap();
    let callback = session.pending_callback_token().unwrap();
    assert!(!session.wake_state().property_animation_pending());
    assert_eq!(session.wake_state().frame_pending(), dirty_before_barrier);
    let before = session.publication_context();
    assert!(session.advance_property_animations_by(0.5).is_err());
    assert!(session.cancel_property_animation(effect).is_err());
    assert!(scene
        .live(&mut session)
        .start_indicate_effect(&target, IndicateOptions::default(), AnimationOptions::new())
        .is_err());
    assert_eq!(session.property_animation_elapsed(effect), Some(0.0));
    assert_eq!(session.publication_context(), before);
    drop(overlay);
    session.fail_required_callback_phase(callback).unwrap();
    assert!(matches!(
        session.advance_property_animations_by(0.5),
        Err(ExecutionSessionAnimationError::CallbackTerminated)
    ));
    session.take_renderer_publication();
    assert!(session.wake_state().is_quiescent());
    assert_eq!(session.property_animation_elapsed(effect), Some(0.0));
}

#[test]
fn sealed_replay_rejects_effects_without_discarding_history() {
    let (scene, target, mut session) = fixture();
    session
        .begin_replay_retention(noon_runtime::ReplayLimits::default())
        .unwrap();
    session.advance_to(1.0).unwrap();
    session.seal_replay().unwrap();
    let before = session.publication_context();
    assert!(matches!(
        scene.live(&mut session).start_indicate_effect(
            &target,
            IndicateOptions::default(),
            AnimationOptions::new()
        ),
        Err(LiveSessionError::Activation(
            ExecutionSessionAnimationError::PropertyAnimation(PropertyAnimationError::ReplaySealed)
        ))
    ));
    assert_eq!(session.publication_context(), before);
    assert!(session.replay_is_sealed());
    assert!(!session.has_property_animations());
}

#[test]
fn removal_retires_effect_and_old_runtime_tokens_cannot_restore_a_target() {
    let (scene, target, mut session) = fixture();
    let effect = start(&scene, &target, &mut session);
    let mut clone = session.clone();
    assert!(clone.cancel_property_animation(effect).is_err());
    session.advance_property_animations_by(0.5).unwrap();
    scene.live(&mut session).remove(&target).unwrap();
    assert_eq!(session.property_animation_elapsed(effect), None);
    let before = session.publication_context();
    assert!(session.cancel_property_animation(effect).is_err());
    session.advance_property_animations_by(1.0).unwrap();
    assert_eq!(session.publication_context(), before);
    assert!(!session.semantic_object_is_reachable(target.node_id()));
}

#[test]
fn enlarged_effect_is_pickable_through_inspection_camera_and_old_frames_stay_stale() {
    let (scene, target, mut session) = fixture();
    let size = Vec2::new(800.0, 400.0);
    let view = session.inspection_pointer_view(1, size).unwrap();
    let before = session.capture_pointer_frame(view).unwrap();
    start(&scene, &target, &mut session);
    assert!(before.validate_current(&session, view).is_err());
    session.advance_property_animations_by(0.5).unwrap();
    let view = session.inspection_pointer_view(1, size).unwrap();
    let midpoint = session.capture_pointer_frame(view).unwrap();
    session
        .zoom_inspection_view(&midpoint, view, Vec2::new(400.0, 200.0), 0.5)
        .unwrap();
    session
        .configure_native_pointer_input(
            NativePointerId {
                source: 1,
                pointer: 1,
            },
            1,
        )
        .unwrap();
    let view = session.inspection_pointer_view(1, size).unwrap();
    let displayed = session.capture_pointer_frame(view).unwrap();
    let token = displayed.input_token(&session, view).unwrap();
    // World (2.55, 1): outside the original radius, inside the indicated radius.
    let point = Vec2::new(655.0, 100.0);
    let input = NativePointerInput::new(
        1,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        NativePointerInputKind::Move(displayed.position(point).unwrap()),
    );
    let pick = session
        .pick_native_pointer_fill(&token, input, |_| true)
        .unwrap();
    assert_eq!(pick.outcome(), PointerFillOutcome::Hit(target.node_id()));
    assert_eq!(pick.precise_tests(), 1);
    session.advance_property_animations_by(0.5).unwrap();
    assert!(displayed.validate_current(&session, view).is_err());
    assert_eq!(session.inspection_camera().unwrap().height, 4.0);
    let restored = session.capture_pointer_frame(view).unwrap();
    let token = restored.input_token(&session, view).unwrap();
    let input = NativePointerInput::new(
        2,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        NativePointerInputKind::Move(restored.position(point).unwrap()),
    );
    assert_eq!(
        session
            .pick_native_pointer_fill(&token, input, |_| true)
            .unwrap()
            .outcome(),
        PointerFillOutcome::Miss
    );
}

#[test]
fn effect_publication_and_spatial_maintenance_remain_local_among_ten_thousand_objects() {
    let mut scene = Scene::new();
    let objects: Vec<_> = (0..10_000).map(|_| scene.circle(0.1).unwrap()).collect();
    scene
        .add_many(
            &objects
                .iter()
                .map(crate::MobjectTarget::Object)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_renderer_publication();
    start(&scene, &objects[5000], &mut session);
    session.advance_property_animations_by(0.5).unwrap();
    assert_eq!(session.take_frame_changes().object_indices().len(), 1);
    assert_eq!(session.runtime.last_stats().groups_evaluated, 0);
    assert_eq!(session.last_spatial_update_stats().full_rebuilds, 0);
    assert_eq!(session.last_spatial_update_stats().leaves_upserted, 1);
}

#[test]
fn callback_settlement_reopens_effect_demand_without_consuming_elapsed_time() {
    let (scene, target, mut session) = fixture();
    let effect = start(&scene, &target, &mut session);
    session.take_renderer_publication();
    assert!(session.wake_state().property_animation_pending());
    let overlay = session
        .begin_required_callback_phase(0.0, [target.node_id()])
        .unwrap();
    assert!(!session.wake_state().property_animation_pending());
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    assert!(session.wake_state().property_animation_pending());
    assert_eq!(session.property_animation_elapsed(effect), Some(0.0));
    session.advance_property_animations_by(0.5).unwrap();
    assert_eq!(session.property_animation_elapsed(effect), Some(0.5));
}

#[test]
fn configured_callbacks_reject_action_before_allocating_or_publishing() {
    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let mut registration = SemanticMutationTransaction::new();
    registration.add_updater(
        target.node_id(),
        noon_core::HostCallbackId::new(8),
        1.0,
        None,
    );
    registration
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.publication_context();
    assert!(matches!(
        scene.live(&mut session).start_indicate_effect(
            &target,
            IndicateOptions::default(),
            AnimationOptions::new()
        ),
        Err(LiveSessionError::Activation(
            ExecutionSessionAnimationError::EffectInput(
                super::super::ExecutionSessionInputError::RequiredCallbacksConfigured
            )
        ))
    ));
    assert_eq!(session.publication_context(), before);
    assert!(!session.has_property_animations());
    assert!(!session.wake_state().property_animation_pending());
}

#[test]
fn off_origin_geometry_uses_effective_layout_center_like_ordinary_indicate() {
    fn line_scene() -> (Scene, Mobject, ExecutionSession) {
        let mut scene = Scene::new();
        let mut line = scene.line((1.0, 0.0), (3.0, 2.0)).unwrap();
        line.set_translation(4.0, -1.0).unwrap();
        scene.add(&line).unwrap();
        let session = scene.execution_session().unwrap();
        (scene, line, session)
    }
    let (scene, target, mut session) = line_scene();
    let (reference_scene, reference_target, mut reference) = line_scene();
    start(&scene, &target, &mut session);
    let segment = reference_scene
        .live(&mut reference)
        .declare_and_activate_indicate(
            &reference_target,
            IndicateOptions::default(),
            AnimationOptions::new().run_time(1.0),
        )
        .unwrap();
    session.advance_property_animations_by(0.5).unwrap();
    reference.advance_segment_to(segment, 0.5).unwrap();
    let actual = scene.live(&mut session).effective(&target).unwrap();
    let expected = reference_scene
        .live(&mut reference)
        .effective(&reference_target)
        .unwrap();
    assert_eq!(actual.transform, expected.transform);
    assert_eq!(actual.style, expected.style);
    assert_ne!(actual.transform.translation, Vec2::new(4.0, -1.0));
}
