use super::*;
use crate::{ExecutionSession, Scene, SemanticVec3};
use noon_core::{
    NativeInputModifiers, NativePointerId, NativePointerInputKind, NativePointerPosition,
    SemanticMutationTransaction, Vec2,
};

fn fixture() -> (Scene, Mobject, ExecutionSession) {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.5).unwrap();
    target.set_fill(0.0, 0.5, 1.0, 1.0).unwrap();
    target
        .set_pointer_click_action(Some(SemanticPointerClickAction::default()))
        .unwrap();
    scene.add(&target).unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .configure_native_pointer_input(
            NativePointerId {
                source: 8,
                pointer: 3,
            },
            1,
        )
        .unwrap();
    session.enable_pointer_fill_clicks(5.0).unwrap();
    session.take_renderer_publication();
    (scene, target, session)
}

fn event(
    session: &ExecutionSession,
    sequence: u64,
    x: f32,
    down: bool,
) -> (NativePointerInputToken, NativePointerInput) {
    let token = session.native_pointer_input_token().unwrap();
    let position =
        NativePointerPosition::new(Vec2::new(x, 0.0), Vec2::new(100.0 + x * 10.0, 100.0)).unwrap();
    let kind = if down {
        NativePointerInputKind::Press {
            position,
            button: 0,
        }
    } else {
        NativePointerInputKind::Release {
            position,
            button: 0,
        }
    };
    let input = NativePointerInput::new(
        sequence,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        kind,
    );
    (token, input)
}
fn click(
    scene: &Scene,
    session: &mut ExecutionSession,
    seq: u64,
    x: f32,
) -> PointerActionPublication {
    let (token, input) = event(session, seq, x, true);
    let result = scene
        .live(session)
        .submit_pointer_input_with_actions(&token, input)
        .unwrap();
    assert_eq!(result.action().unwrap(), PointerClickActionOutcome::None);
    let (token, input) = event(session, seq + 1, x, false);
    scene
        .live(session)
        .submit_pointer_input_with_actions(&token, input)
        .unwrap()
}
fn started(result: &PointerActionPublication) -> PropertyAnimationToken {
    let PointerClickActionOutcome::Started(token) = result.action().unwrap() else {
        panic!("expected a newly activated effect: {result:?}")
    };
    token
}

#[test]
fn accepted_click_activates_real_indicate_without_selection_or_authored_time() {
    let (scene, target, mut session) = fixture();
    let before = session.frame().clone();
    let revision = session.publication_context().scene_revision();
    let nodes = scene.integration_store().borrow().len();
    let publication = click(&scene, &mut session, 0, 0.0);
    let effect = started(&publication);
    assert!(publication.input().selection_click().is_some());
    assert!(!publication.input().selection_changed());
    assert_eq!(session.selected_pointer_target(), None);
    assert!(session.pointer_selection_highlight().is_none());
    assert_eq!(publication.current(), session.publication_context());
    assert_ne!(publication.current(), publication.input().publication());
    session.advance_property_animations_by(0.5).unwrap();
    assert_eq!(
        scene
            .live(&mut session)
            .effective(&target)
            .unwrap()
            .transform
            .scale,
        Vec2::new(1.2, 1.2)
    );
    assert_eq!(session.frame().time, before.time);
    assert_eq!(session.property_animation_elapsed(effect), Some(0.5));
    session.advance_property_animations_by(0.5).unwrap();
    assert_eq!(session.frame(), &before);
    assert!(!session.has_property_animations());
    assert_eq!(session.publication_context().scene_revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
}

#[test]
fn repeated_click_is_explicitly_busy_then_retriggers_after_release() {
    let (scene, _, mut session) = fixture();
    let first = started(&click(&scene, &mut session, 0, 0.0));
    session.advance_property_animations_by(0.4).unwrap();
    let context = session.publication_context();
    assert_eq!(
        click(&scene, &mut session, 2, 0.0).action().unwrap(),
        PointerClickActionOutcome::Busy
    );
    assert_eq!(session.property_animation_elapsed(first), Some(0.4));
    assert_eq!(session.publication_context(), context);
    session.advance_property_animations_by(0.6).unwrap();
    let second = started(&click(&scene, &mut session, 4, 0.0));
    assert_ne!(first, second);
    assert_eq!(session.property_animation_elapsed(second), Some(0.0));
}

#[test]
fn accepted_release_cannot_be_replayed_even_when_the_action_fails() {
    let (scene, _, mut session) = fixture();
    session
        .begin_replay_retention(noon_runtime::ReplayLimits::default())
        .unwrap();
    session.advance_to(1.0).unwrap();
    session.seal_replay().unwrap();
    let (token, input) = event(&session, 0, 0.0, true);
    scene
        .live(&mut session)
        .submit_pointer_input_with_actions(&token, input)
        .unwrap();
    let (token, input) = event(&session, 1, 0.0, false);
    let result = scene
        .live(&mut session)
        .submit_pointer_input_with_actions(&token, input)
        .unwrap();
    assert!(matches!(
        result.action(),
        Err(LiveSessionError::Activation(
            ExecutionSessionAnimationError::PropertyAnimation(PropertyAnimationError::ReplaySealed)
        ))
    ));
    assert_eq!(result.input().input(), input);
    assert!(scene
        .live(&mut session)
        .submit_pointer_input_with_actions(&token, input)
        .is_err());
    assert!(session.replay_is_sealed());
    assert!(!session.has_property_animations());
}

#[test]
fn background_unbound_and_moved_gestures_do_not_start_an_effect() {
    let (scene, target, mut session) = fixture();
    assert_eq!(
        click(&scene, &mut session, 0, 3.0).action().unwrap(),
        PointerClickActionOutcome::None
    );
    let (token, input) = event(&session, 2, 0.0, true);
    scene
        .live(&mut session)
        .submit_pointer_input_with_actions(&token, input)
        .unwrap();
    let (token, input) = event(&session, 3, 3.0, false);
    assert_eq!(
        scene
            .live(&mut session)
            .submit_pointer_input_with_actions(&token, input)
            .unwrap()
            .action()
            .unwrap(),
        PointerClickActionOutcome::None
    );
    scene
        .live(&mut session)
        .set_pointer_click_action(&target, None)
        .unwrap();
    assert_eq!(
        click(&scene, &mut session, 4, 0.0).action().unwrap(),
        PointerClickActionOutcome::None
    );
    assert!(!session.has_property_animations());
}

#[test]
fn binding_publication_is_metadata_only_and_does_not_retire_an_active_effect() {
    let (scene, target, mut session) = fixture();
    let effect = started(&click(&scene, &mut session, 0, 0.0));
    session.advance_property_animations_by(0.5).unwrap();
    session.take_renderer_publication();
    let before = session.frame().clone();
    scene
        .live(&mut session)
        .set_pointer_click_action(&target, None)
        .unwrap();
    assert_eq!(session.frame(), &before);
    assert_eq!(session.property_animation_elapsed(effect), Some(0.5));
    let changes = session.take_frame_changes();
    assert!(!changes.is_all());
    assert!(changes.object_indices().is_empty());
    session.advance_property_animations_by(0.5).unwrap();
    assert_eq!(
        scene
            .live(&mut session)
            .effective(&target)
            .unwrap()
            .transform
            .scale,
        Vec2::ONE
    );
    assert_eq!(
        click(&scene, &mut session, 2, 0.0).action().unwrap(),
        PointerClickActionOutcome::None
    );
}

#[test]
fn invalid_binding_is_atomic_with_other_authored_changes() {
    let (scene, target, mut session) = fixture();
    let before = session.publication_context();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(
        target.node_id(),
        noon_core::SemanticObjectProperty::Translation,
        noon_core::SemanticSignalValue::Vec3(SemanticVec3::new(3.0, 0.0, 0.0)),
    );
    transaction.set_pointer_click_action(
        target.node_id(),
        Some(SemanticPointerClickAction::indicate(
            1.2,
            noon_core::YELLOW,
            f64::NAN,
        )),
    );
    assert!(scene.live(&mut session).apply(transaction).is_err());
    assert_eq!(session.publication_context(), before);
    started(&click(&scene, &mut session, 0, 0.0));
}

#[test]
fn different_bound_objects_run_independently_and_clone_retains_recognition_mode() {
    let (scene, first, session) = fixture();
    let mut copy = first.copy_handle().unwrap();
    copy.set_translation(3.0, 0.0).unwrap();
    // copy_handle is raw authored allocation; explicitly construct a fresh session.
    drop(session);
    let mut scene = scene;
    scene.add(&copy).unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .configure_native_pointer_input(
            NativePointerId {
                source: 8,
                pointer: 3,
            },
            1,
        )
        .unwrap();
    session.enable_pointer_fill_clicks(5.0).unwrap();
    let a = started(&click(&scene, &mut session, 0, 0.0));
    let b = started(&click(&scene, &mut session, 2, 3.0));
    assert_ne!(a, b);
    session.advance_property_animations_by(0.5).unwrap();
    assert_eq!(
        scene
            .live(&mut session)
            .effective(&first)
            .unwrap()
            .transform
            .scale,
        Vec2::new(1.2, 1.2)
    );
    assert_eq!(
        scene
            .live(&mut session)
            .effective(&copy)
            .unwrap()
            .transform
            .scale,
        Vec2::new(1.2, 1.2)
    );
    session.advance_property_animations_by(0.5).unwrap();
    let mut cloned = session.clone();
    cloned
        .configure_native_pointer_input(
            NativePointerId {
                source: 9,
                pointer: 1,
            },
            1,
        )
        .unwrap();
    // A clone retains the last admitted sequence; changing its pointer is not a reset.
    started(&click(&scene, &mut cloned, 4, 0.0));
    assert!(cloned.pointer_selection_highlight().is_none());
}
