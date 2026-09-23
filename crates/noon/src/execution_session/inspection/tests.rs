use super::*;
use crate::{AnimationOptions, IndicateOptions, RateFunction, Scene};
use crate::integration::{NativePointerInputToken, PointerFillOutcome};
use noon_core::{NativeEventSource, NativeInputModifiers, NativePointerId, NativePointerInput, NativePointerInputKind, NativeStateSource, ReactiveValue, SemanticMutationTransaction, SemanticNativeInputSource, SemanticNodeCreation, SemanticNodeId, SemanticSignalValue, SemanticStore};

const POINTER: NativePointerId = NativePointerId { source: 7, pointer: 3 };
const SURFACE: u64 = 11;
const SIZE: Vec2 = Vec2::new(800.0, 400.0);
const CENTER: Vec2 = Vec2::new(400.0, 200.0);

fn fixture() -> (Scene, crate::Mobject, ExecutionSession) {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.3).unwrap();
    circle.set_fill(0.0, 0.5, 1.0, 1.0).unwrap();
    circle.set_translation(2.0, 0.0).unwrap();
    scene.add(&circle).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.enable_pointer_fill_selection(4.0).unwrap();
    session.configure_native_pointer_input(POINTER, SURFACE).unwrap();
    session.take_frame_changes();
    (scene, circle, session)
}

fn displayed(session: &ExecutionSession) -> PointerFrameSnapshot {
    let view = session.inspection_pointer_view(SURFACE, SIZE).unwrap();
    session.capture_pointer_frame(view).unwrap()
}

fn zoom(session: &mut ExecutionSession, anchor: Vec2, factor: f64) -> Result<bool, InspectionNavigationError> {
    let frame = displayed(session);
    session.zoom_inspection_view(&frame, frame.view(), anchor, factor)
}

fn input(token: &NativePointerInputToken, sequence: u64, kind: NativePointerInputKind) -> NativePointerInput {
    NativePointerInput::new(sequence, token.pointer(), token.context(), NativeInputModifiers::default(), kind)
}

fn click_edge(session: &mut ExecutionSession, sequence: u64, surface: Vec2, press: bool) -> super::super::NativePointerInputPublication {
    let frame = displayed(session);
    let token = frame.input_token(session, frame.view()).unwrap();
    let position = frame.position(surface).unwrap();
    let kind = if press { NativePointerInputKind::Press { position, button: 0 } } else { NativePointerInputKind::Release { position, button: 0 } };
    session.submit_native_pointer_input(&token, input(&token, sequence, kind)).unwrap()
}

#[test]
fn shared_view_projects_the_same_anchor_in_both_zoom_directions() {
    for factor in [0.125, 0.73, 1.4, 8.0] {
        let (_, _, mut session) = fixture();
        let before = displayed(&session);
        let point = Vec2::new(540.0, 120.0);
        let anchor = before.position(point).unwrap().scene();
        assert_eq!(zoom(&mut session, point, factor), Ok(true));
        let after = displayed(&session);
        let projected = after.position(point).unwrap().scene();
        assert!((projected.x - anchor.x).abs() < 1e-5);
        assert!((projected.y - anchor.y).abs() < 1e-5);
        assert_eq!(after.view().camera(), session.inspection_camera().unwrap());
        assert_eq!(after.inspection_revision(), 1);
        assert_eq!(before.validate_current(&session, after.view()), Err(PointerFrameError::ViewChanged));
        assert_eq!(session.camera().unwrap(), Camera2DState::default());
    }
}

#[test]
fn zoom_out_and_back_does_not_revive_an_old_displayed_snapshot() {
    let (_, _, mut session) = fixture();
    let old = displayed(&session);
    let publication = session.publication_context();
    zoom(&mut session, CENTER, 0.5).unwrap();
    zoom(&mut session, CENTER, 2.0).unwrap();
    let new = displayed(&session);
    assert_eq!(new.view(), old.view());
    assert_eq!(new.publication(), publication);
    assert_eq!(new.inspection_revision(), 2);
    assert_ne!(old, new);
    assert_eq!(old.validate_current(&session, new.view()), Err(PointerFrameError::ViewChanged));
}

#[test]
fn no_op_and_saturated_zoom_preserve_bindings_and_revisions() {
    let (_, _, mut session) = fixture();
    let token = session.native_pointer_input_token().unwrap();
    assert_eq!(zoom(&mut session, CENTER, 1.0), Ok(false));
    assert_eq!(session.inspection_view_revision(), 0);
    assert_eq!(session.native_pointer_input_token().unwrap(), token);
    zoom(&mut session, CENTER, f64::MAX).unwrap();
    let token = session.configure_native_pointer_input(POINTER, SURFACE).unwrap();
    let revision = session.inspection_view_revision();
    assert_eq!(zoom(&mut session, Vec2::new(300.0, 100.0), 2.0), Ok(false));
    assert_eq!(session.inspection_view_revision(), revision);
    assert_eq!(session.native_pointer_input_token().unwrap(), token);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn invalid_and_foreign_requests_leave_view_frame_and_gesture_unchanged() {
    let (_, _, mut session) = fixture();
    click_edge(&mut session, 1, Vec2::new(500.0, 200.0), true);
    let state = session.frame().clone();
    let gesture = session.pointer_selection;
    let token = session.native_pointer_input_token().unwrap();
    for factor in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(zoom(&mut session, CENTER, factor).is_err());
    }
    assert!(zoom(&mut session, Vec2::new(f32::NAN, 0.0), 0.5).is_err());
    let (_, _, other) = fixture();
    let foreign = displayed(&other);
    assert!(session.zoom_inspection_view(&foreign, foreign.view(), CENTER, 0.5).is_err());
    assert_eq!(session.frame(), &state);
    assert_eq!(session.pointer_selection, gesture);
    assert_eq!(session.native_pointer_input_token().unwrap(), token);
    assert_eq!(session.inspection_view_revision(), 0);
    assert_eq!(session.last_native_event_sequence, Some(1));
}

#[test]
fn revision_exhaustion_is_atomic_but_an_exact_no_op_still_succeeds() {
    let (_, _, mut session) = fixture();
    session.inspection.revision = u64::MAX;
    let token = session.native_pointer_input_token().unwrap();
    assert_eq!(zoom(&mut session, CENTER, 0.5), Err(InspectionNavigationError::RevisionExhausted));
    assert_eq!(zoom(&mut session, CENTER, 1.0), Ok(false));
    assert_eq!(session.inspection_view_revision(), u64::MAX);
    assert_eq!(session.native_pointer_input_token().unwrap(), token);
    assert_eq!(session.inspection_camera().unwrap(), session.camera().unwrap());
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn precise_picking_uses_composed_view_and_rejects_old_uncomposed_view() {
    let (_, circle, mut session) = fixture();
    let original = displayed(&session);
    zoom(&mut session, CENTER, 0.5).unwrap();
    assert_eq!(session.capture_pointer_frame(original.view()), Err(PointerFrameError::CameraMismatch));
    session.configure_native_pointer_input(POINTER, SURFACE).unwrap();
    let frame = displayed(&session);
    let token = frame.input_token(&session, frame.view()).unwrap();
    for (surface, expected) in [(Vec2::new(600.0, 200.0), PointerFillOutcome::Hit(circle.node_id())), (Vec2::new(500.0, 200.0), PointerFillOutcome::Miss)] {
        let event = input(&token, 1, NativePointerInputKind::Move(frame.position(surface).unwrap()));
        let pick = session.pick_native_pointer_fill(&token, event, |_| true).unwrap();
        assert_eq!(pick.outcome(), expected);
        assert!(pick.precise_tests() <= 1);
    }
}

#[test]
fn zoom_cancels_a_press_and_old_or_fresh_releases_cannot_synthesize_a_click() {
    let (_, _, mut session) = fixture();
    click_edge(&mut session, 1, Vec2::new(500.0, 200.0), true);
    let old_frame = displayed(&session);
    let old_token = session.native_pointer_input_token().unwrap();
    zoom(&mut session, CENTER, 0.5).unwrap();
    let release = input(&old_token, 2, NativePointerInputKind::Release { position: old_frame.position(Vec2::new(500.0, 200.0)).unwrap(), button: 0 });
    assert!(session.submit_native_pointer_input(&old_token, release).is_err());
    assert_eq!(session.last_native_event_sequence, Some(1));
    session.configure_native_pointer_input(POINTER, SURFACE).unwrap();
    assert!(click_edge(&mut session, 2, Vec2::new(600.0, 200.0), false).selection_click().is_none());
    click_edge(&mut session, 3, Vec2::new(600.0, 200.0), true);
    assert!(click_edge(&mut session, 4, Vec2::new(600.0, 200.0), false).selection_click().unwrap().target().is_some());
}

#[test]
fn reset_increments_revision_and_old_identity_view_does_not_revive() {
    let (_, _, mut session) = fixture();
    let original = displayed(&session);
    assert_eq!(session.reset_inspection_view(), Ok(false));
    zoom(&mut session, Vec2::new(500.0, 100.0), 0.5).unwrap();
    assert_eq!(session.reset_inspection_view(), Ok(true));
    assert_eq!(session.inspection_view_revision(), 2);
    assert_eq!(session.inspection_camera().unwrap(), session.camera().unwrap());
    assert_eq!(original.validate_current(&session, original.view()), Err(PointerFrameError::ViewChanged));
}

#[test]
fn seeks_and_clones_preserve_adjustment_but_new_sessions_start_at_identity() {
    let (scene, _, mut session) = fixture();
    zoom(&mut session, CENTER, 0.5).unwrap();
    let before = displayed(&session);
    session.seek(1.0).unwrap();
    session.seek(0.0).unwrap();
    assert_eq!(session.inspection_camera().unwrap(), before.view().camera());
    assert_eq!(session.inspection_view_revision(), 1);
    assert!(before.validate_current(&session, before.view()).is_err());
    let clone = session.clone();
    assert_eq!(clone.inspection_camera().unwrap(), session.inspection_camera().unwrap());
    let fresh_frame = displayed(&session);
    assert!(fresh_frame.validate_current(&clone, fresh_frame.view()).is_err());
    let fresh = scene.execution_session().unwrap();
    assert_eq!(fresh.inspection_camera().unwrap(), Camera2DState::default());
    assert_eq!(fresh.inspection_view_revision(), 0);
}

#[test]
fn required_callback_barrier_is_not_bypassed_even_by_unit_zoom() {
    let (_, circle, mut session) = fixture();
    let frame = displayed(&session);
    let token = session.native_pointer_input_token().unwrap();
    let overlay = session.begin_required_callback_phase(0.0, [circle.node_id()]).unwrap();
    let before = session.frame().clone();
    for factor in [0.5, 1.0] {
        assert_eq!(session.zoom_inspection_view(&frame, frame.view(), CENTER, factor), Err(InspectionNavigationError::Frame(PointerFrameError::Input(ExecutionSessionInputError::RequiredCallbackPending))));
    }
    assert_eq!(session.reset_inspection_view(), Err(InspectionNavigationError::Input(ExecutionSessionInputError::RequiredCallbackPending)));
    assert_eq!(session.inspection_view_revision(), 0);
    assert_eq!(session.native_pointer_input_token().unwrap(), token);
    assert_eq!(session.frame(), &before);
    session.commit_required_callback_phase(overlay.finish()).unwrap();
    assert_eq!(zoom(&mut session, CENTER, 0.5), Ok(true));
}

#[test]
fn navigation_during_real_indicate_preserves_midpoint_completion_and_authored_guard() {
    let (scene, circle, mut session) = fixture();
    let original = scene.live(&mut session).effective(&circle).unwrap();
    let segment = scene.live(&mut session).declare_and_activate_indicate(&circle, IndicateOptions::default(), AnimationOptions::new().run_time(1.0)).unwrap();
    scene.live(&mut session).advance_segment_to(segment, 0.5).unwrap();
    let midpoint = scene.live(&mut session).effective(&circle).unwrap();
    assert!(midpoint.transform.scale.x > original.transform.scale.x);
    assert_ne!(midpoint.style, original.style);
    let state = session.frame().clone();
    let publication = session.publication_context();
    assert!(scene.live(&mut session).set_translation(&circle, 9.0, 0.0).is_err());
    zoom(&mut session, CENTER, 0.5).unwrap();
    assert!(scene.live(&mut session).set_translation(&circle, 9.0, 0.0).is_err());
    assert_eq!(session.frame(), &state);
    assert_eq!(session.publication_context(), publication);
    scene.live(&mut session).advance_segment_to(segment, segment.end_time()).unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    let restored = scene.live(&mut session).effective(&circle).unwrap();
    assert_eq!(restored.transform, original.transform);
    assert_eq!(restored.style, original.style);
    assert_eq!(session.inspection_camera().unwrap().height, 4.0);
}

#[test]
fn navigation_composes_with_real_authored_camera_animation_and_completion() {
    let mut scene = Scene::new();
    let camera = scene.camera_frame().unwrap();
    let mut target = camera.target_editor().unwrap();
    target.set_translation(4.0, 2.0).unwrap();
    target.set_scale(2.0, 2.0).unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).declare_and_activate_transform_to(&camera, &target, AnimationOptions::new().run_time(1.0).rate_func(RateFunction::Linear)).unwrap();
    zoom(&mut session, CENTER, 0.5).unwrap();
    for time in [0.25, 0.5, 1.0] {
        scene.live(&mut session).advance_segment_to(segment, time).unwrap();
        let raw = session.camera().unwrap();
        let view = displayed(&session).view().camera();
        assert_eq!(view.center, raw.center);
        assert_eq!(view.height, raw.height * 0.5);
    }
    let endpoint = session.inspection_camera().unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(session.inspection_camera().unwrap(), endpoint);
    assert_eq!(session.camera().unwrap().center, Vec2::new(4.0, 2.0));
}

fn native_signal(store: &mut SemanticStore, root: SemanticNodeId, source: SemanticNativeInputSource, initial: SemanticSignalValue) -> SemanticNodeId {
    let mut transaction = SemanticMutationTransaction::new();
    let pending = transaction.create_node(SemanticNodeCreation::native_input_signal(initial, source).unwrap());
    transaction.scope_signal(root, pending);
    transaction.apply(store).unwrap().resolve(pending).unwrap()
}

#[test]
fn cancellation_clears_held_native_buttons_without_fabricating_release_events() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let button = native_signal(&mut store, root, SemanticNativeInputSource::State(NativeStateSource::PointerButton { button: 0 }), SemanticSignalValue::Bool(false));
    let up = native_signal(&mut store, root, SemanticNativeInputSource::Event(NativeEventSource::PointerUp { button: 0 }), SemanticSignalValue::Scalar(0.0));
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.configure_native_pointer_input(POINTER, SURFACE).unwrap();
    click_edge(&mut session, 1, CENTER, true);
    assert_eq!(session.effective_signal_value(button), Some(&ReactiveValue::Bool(true)));
    let revision = session.publication_context().scene_revision();
    zoom(&mut session, CENTER, 0.5).unwrap();
    assert_eq!(session.effective_signal_value(button), Some(&ReactiveValue::Bool(false)));
    assert_eq!(session.effective_signal_value(up), Some(&ReactiveValue::Scalar(0.0)));
    assert_eq!(session.last_native_event_sequence, Some(1));
    assert_eq!(session.frame().time, 0.0);
    assert_eq!(session.publication_context().scene_revision(), revision);
}

#[test]
fn sealed_replay_allows_view_only_navigation_but_rejects_button_mutation_atomically() {
    let (_, _, mut session) = fixture();
    session.begin_replay_retention(noon_runtime::ReplayLimits::default()).unwrap();
    session.seal_replay().unwrap();
    let stats = session.replay_stats();
    zoom(&mut session, CENTER, 0.5).unwrap();
    assert!(session.replay_is_sealed());
    assert_eq!(session.replay_stats(), stats);
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let button = native_signal(&mut store, root, SemanticNativeInputSource::State(NativeStateSource::PointerButton { button: 0 }), SemanticSignalValue::Bool(true));
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.begin_replay_retention(noon_runtime::ReplayLimits::default()).unwrap();
    session.seal_replay().unwrap();
    let before = session.frame().clone();
    let publication = session.publication_context();
    assert_eq!(zoom(&mut session, CENTER, 0.5), Err(InspectionNavigationError::Input(ExecutionSessionInputError::Evaluation(noon_runtime::EvaluationError::ReplaySealed))));
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.effective_signal_value(button), Some(&ReactiveValue::Bool(true)));
    assert_eq!(session.inspection_view_revision(), 0);
}

#[test]
fn repeated_navigation_has_no_scene_dirtiness_or_resource_and_timeline_growth() {
    let mut scene = Scene::new();
    for _ in 0..10_000 { let circle = scene.circle(0.2).unwrap(); scene.add(&circle).unwrap(); }
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    let publication = session.publication_context();
    let allocation = session.frame().objects.as_ptr();
    let spatial = session.last_spatial_update_stats();
    for _ in 0..32 {
        zoom(&mut session, CENTER, 0.5).unwrap();
        zoom(&mut session, CENTER, 2.0).unwrap();
    }
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.frame().objects.as_ptr(), allocation);
    assert_eq!(session.last_spatial_update_stats(), spatial);
    assert!(!session.has_replay_timeline_work());
    assert!(session.take_frame_changes().is_empty());
    assert!(session.wake_state().is_quiescent());
}
