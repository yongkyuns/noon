use super::*;
use crate::integration::{NativePointerInputPublication, NativePointerInputToken};
use noon_core::{
    NativeEventSource, NativeInputModifiers, NativePointerCancellation, NativePointerId,
    NativePointerInput, NativePointerInputKind, NativePointerPosition, SemanticMutationTransaction,
    SemanticNativeInputSource, SemanticNodeCreation, SemanticObjectProperty, SemanticObjectState,
    SemanticSignalExpr, SemanticSignalValue, SemanticVec3, StoredGeometry, Vec2,
};

const POINTER: NativePointerId = NativePointerId {
    source: 71,
    pointer: 5,
};

fn object(x: f64, control: Option<PlaybackControl>) -> SemanticObjectState {
    let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
        size: Vec2::new(2.0, 1.0),
    });
    state.transform.translation.x = x;
    state.set_click_action(control.map(SemanticClickAction::Playback));
    state
}

fn attach(
    store: &mut SemanticStore,
    root: SemanticNodeId,
    state: SemanticObjectState,
) -> SemanticNodeId {
    let node = store.insert_semantic_object(state);
    store.add_semantic_family_member(root, node).unwrap();
    node
}

fn fixture() -> (
    SemanticStore,
    SemanticNodeId,
    SemanticNodeId,
    ExecutionSession,
) {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let target = attach(
        &mut store,
        root,
        object(0.0, Some(PlaybackControl::NextSection)),
    );
    for index in 0..1_000 {
        attach(
            &mut store,
            root,
            object(20.0 + 3.0 * f64::from(index), None),
        );
    }
    let session = session(&store, root);
    (store, root, target, session)
}

fn session(store: &SemanticStore, root: SemanticNodeId) -> ExecutionSession {
    let mut session = ExecutionSession::from_semantic_root(store, root).unwrap();
    session.configure_native_pointer_input(POINTER, 1).unwrap();
    session.take_frame_changes();
    session
}

fn position(x: f32, pixel_x: f32) -> NativePointerPosition {
    NativePointerPosition::new(Vec2::new(x, 0.0), Vec2::new(pixel_x, 100.0)).unwrap()
}

fn press(x: f32) -> NativePointerInputKind {
    NativePointerInputKind::Press {
        position: position(x, 100.0),
        button: 0,
    }
}

fn release(x: f32) -> NativePointerInputKind {
    NativePointerInputKind::Release {
        position: position(x, 100.0),
        button: 0,
    }
}

fn input(
    session: &ExecutionSession,
    sequence: u64,
    kind: NativePointerInputKind,
) -> (NativePointerInputToken, NativePointerInput) {
    let token = session.native_pointer_input_token().unwrap();
    let input = NativePointerInput::new(
        sequence,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        kind,
    );
    (token, input)
}

fn submit(
    store: &mut SemanticStore,
    session: &mut ExecutionSession,
    sequence: u64,
    kind: NativePointerInputKind,
) -> NativePointerInputPublication {
    let (token, input) = input(session, sequence, kind);
    session
        .submit_native_pointer_input(store, &token, input)
        .unwrap()
}

fn click(
    store: &mut SemanticStore,
    session: &mut ExecutionSession,
    sequence: u64,
    x: f32,
) -> NativePointerInputPublication {
    assert_eq!(
        submit(store, session, sequence, press(x)).playback_control(),
        None
    );
    submit(store, session, sequence + 1, release(x))
}

#[test]
fn playback_control_is_a_local_committed_request_without_time_or_authored_mutation() {
    let (mut store, _, target, mut session) = fixture();
    let revision = store.scene_revision();
    let frame = session.frame().clone();
    let publication = session.publication_context();
    assert!(session.has_playback_controls());
    assert!(session.has_native_pointer_subscribers());
    let receipt = click(&mut store, &mut session, 1, 0.0);
    assert_eq!(
        receipt.playback_control(),
        Some(PlaybackControl::NextSection)
    );
    assert_eq!(receipt.selection_click().unwrap().target(), Some(target));
    assert_eq!(receipt.publication(), publication);
    assert_eq!(session.last_native_event_sequence, Some(2));
    assert_eq!(session.frame(), &frame);
    assert_eq!(store.scene_revision(), revision);
    assert!(!session.interactions_active());
    assert!(session.take_frame_changes().is_empty());
    let query = receipt.selection_query().unwrap();
    assert_eq!(query.spatial_stats().candidates_tested, 1);
    assert_eq!(query.spatial_stats().full_scan_fallbacks, 0);
    assert_eq!(
        submit(&mut store, &mut session, 3, release(0.0)).playback_control(),
        None
    );
}

#[test]
fn playback_uses_painter_correct_pick_and_background_does_not_dispatch() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    attach(
        &mut store,
        root,
        object(0.0, Some(PlaybackControl::PreviousSection)),
    );
    let front = attach(
        &mut store,
        root,
        object(0.0, Some(PlaybackControl::Restart)),
    );
    attach(
        &mut store,
        root,
        object(4.0, Some(PlaybackControl::NextSection)),
    );
    let occluder = attach(&mut store, root, object(4.0, None));
    let mut session = session(&store, root);
    let receipt = click(&mut store, &mut session, 1, 0.0);
    assert_eq!(receipt.selection_click().unwrap().target(), Some(front));
    assert_eq!(receipt.playback_control(), Some(PlaybackControl::Restart));
    let blocked = click(&mut store, &mut session, 3, 4.0);
    assert_eq!(blocked.selection_click().unwrap().target(), Some(occluder));
    assert_eq!(blocked.playback_control(), None);
    assert_eq!(
        click(&mut store, &mut session, 5, -4.0).playback_control(),
        None
    );
}

#[test]
fn drag_cancel_and_seek_do_not_emit_or_retain_playback_requests() {
    let (mut store, _, _, mut session) = fixture();
    submit(&mut store, &mut session, 1, press(0.0));
    submit(
        &mut store,
        &mut session,
        2,
        NativePointerInputKind::Move(position(0.0, 105.0)),
    );
    assert_eq!(
        submit(&mut store, &mut session, 3, release(0.0)).playback_control(),
        None
    );
    submit(&mut store, &mut session, 4, press(0.0));
    submit(
        &mut store,
        &mut session,
        5,
        NativePointerInputKind::Cancel(NativePointerCancellation::FocusLost),
    );
    assert_eq!(
        submit(&mut store, &mut session, 6, release(0.0)).playback_control(),
        None
    );
    submit(&mut store, &mut session, 7, press(0.0));
    session.seek_with_store(&mut store, 1.0).unwrap();
    assert_eq!(
        submit(&mut store, &mut session, 8, release(0.0)).playback_control(),
        None
    );
    assert_eq!(
        click(&mut store, &mut session, 9, 0.0).playback_control(),
        Some(PlaybackControl::NextSection)
    );
    assert_eq!(session.frame().time, 1.0);
}

#[test]
fn stale_release_does_not_dispatch_or_acknowledge_the_occurrence() {
    let (mut store, _, _, mut session) = fixture();
    submit(&mut store, &mut session, 1, press(0.0));
    let (stale, release) = input(&session, 2, release(0.0));
    session.seek_with_store(&mut store, 1.0).unwrap();
    let frame = session.frame().clone();
    assert!(matches!(
        session.submit_native_pointer_input(&mut store, &stale, release),
        Err(ExecutionSessionInputError::StalePointerPublication { .. })
    ));
    assert_eq!(session.last_native_event_sequence, Some(1));
    assert_eq!(session.frame(), &frame);
}

#[test]
fn failed_reactive_release_has_no_request_and_the_same_sequence_can_retry() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let target = attach(
        &mut store,
        root,
        object(0.0, Some(PlaybackControl::ToggleAudioMuted)),
    );
    let driven = attach(&mut store, root, object(4.0, None));
    let mut declaration = SemanticMutationTransaction::new();
    let events = declaration.create_node(
        SemanticNodeCreation::native_input_signal(
            SemanticSignalValue::Scalar(0.0),
            SemanticNativeInputSource::Event(NativeEventSource::PointerUp { button: 0 }),
        )
        .unwrap(),
    );
    declaration.scope_signal(root, events);
    let events = declaration
        .apply(&mut store)
        .unwrap()
        .resolve(events)
        .unwrap();
    let gain = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let scaled = store
        .insert_semantic_derived_signal(SemanticSignalExpr::Mul(
            Box::new(SemanticSignalExpr::signal(events)),
            Box::new(SemanticSignalExpr::signal(gain)),
        ))
        .unwrap();
    let squared = store
        .insert_semantic_derived_signal(SemanticSignalExpr::Mul(
            Box::new(SemanticSignalExpr::signal(scaled)),
            Box::new(SemanticSignalExpr::signal(scaled)),
        ))
        .unwrap();
    store
        .bind_semantic_signal(squared, driven, SemanticObjectProperty::RotationZ)
        .unwrap();
    let mut session = session(&store, root);
    session.set_reactive_input(gain, 1.0e20_f32).unwrap();
    submit(&mut store, &mut session, 1, press(0.0));
    let before = session.pointer_selection;
    let publication = session.publication_context();
    let frame = session.frame().clone();
    let (token, release_input) = input(&session, 2, release(0.0));
    assert!(matches!(
        session.submit_native_pointer_input(&mut store, &token, release_input),
        Err(ExecutionSessionInputError::Evaluation(_))
    ));
    assert_eq!(session.pointer_selection, before);
    assert_eq!(session.last_native_event_sequence, Some(1));
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.frame(), &frame);
    session.set_reactive_input(gain, 1.0_f32).unwrap();
    let receipt = submit(&mut store, &mut session, 2, release(0.0));
    assert_eq!(receipt.selection_click().unwrap().target(), Some(target));
    assert_eq!(
        receipt.playback_control(),
        Some(PlaybackControl::ToggleAudioMuted)
    );
    assert_eq!(
        submit(&mut store, &mut session, 3, release(0.0)).playback_control(),
        None
    );
}

#[test]
fn playback_binding_is_root_relative_and_does_not_use_a_recycled_generation() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let unrelated = store.insert_family();
    let old = attach(
        &mut store,
        unrelated,
        object(0.0, Some(PlaybackControl::Restart)),
    );
    assert!(!session(&store, root).has_playback_controls());
    let mut removal = SemanticMutationTransaction::new();
    removal.remove_node(old);
    removal.apply(&mut store).unwrap();
    let replacement = attach(&mut store, root, object(0.0, None));
    assert_eq!(replacement.slot(), old.slot());
    assert_ne!(replacement.generation(), old.generation());
    let mut session = session(&store, root);
    assert!(!session.has_playback_controls());
    assert_eq!(
        click(&mut store, &mut session, 1, 0.0).playback_control(),
        None
    );
}

#[test]
fn predeclared_control_plan_rejects_authored_edits_and_history_without_changes() {
    let (mut store, root, target, mut session) = fixture();
    let revision = store.scene_revision();
    let publication = session.publication_context();
    let mut change = SemanticMutationTransaction::new();
    change.set_property(
        target,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(3.0, 0.0, 0.0),
    );
    assert!(matches!(
        session.apply_semantic_transaction_at_root(&mut store, root, change),
        Err(super::super::ExecutionSessionPublicationError::PlaybackControlsExecutionImmutable)
    ));
    assert_eq!(
        session.begin_replay_retention(noon_runtime::ReplayLimits::default()),
        Err(noon_runtime::ReplayError::UnsupportedDomain)
    );
    assert!(!session.replay_scope_active());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(
        click(&mut store, &mut session, 1, 0.0).playback_control(),
        Some(PlaybackControl::NextSection)
    );
}

#[test]
fn live_control_addition_and_incoming_subtree_reject_atomically() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let target = attach(&mut store, root, object(0.0, None));
    let detached = store.insert_family();
    attach(
        &mut store,
        detached,
        object(4.0, Some(PlaybackControl::TogglePlayback)),
    );
    let mut session = session(&store, root);
    let revision = store.scene_revision();
    let publication = session.publication_context();
    let mut bind = SemanticMutationTransaction::new();
    bind.set_property(
        target,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(3.0, 0.0, 0.0),
    );
    bind.set_click_action(
        target,
        Some(SemanticClickAction::Playback(PlaybackControl::NextSection)),
    );
    assert!(matches!(
        session.apply_semantic_transaction_at_root(&mut store, root, bind),
        Err(super::super::ExecutionSessionPublicationError::Lowering(
            noon_compile::SemanticPublicationLoweringError::PlaybackControlUnavailable { .. }
        ))
    ));
    let mut attach = SemanticMutationTransaction::new();
    attach.add_member(root, detached);
    assert!(matches!(
        session.apply_semantic_transaction_at_root(&mut store, root, attach),
        Err(super::super::ExecutionSessionPublicationError::Lowering(
            noon_compile::SemanticPublicationLoweringError::PlaybackControlUnavailable { .. }
        ))
    ));
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .transform
            .translation
            .x,
        0.0
    );
    assert!(!session.has_playback_controls());
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn scene_facade_preserves_time_and_rejects_live_source_before_continuation() {
    struct MustNotRun;
    impl crate::LiveContinuation for MustNotRun {
        type Error = std::convert::Infallible;
        fn resume(
            &mut self,
            _: &mut crate::LiveSession<'_>,
        ) -> Result<crate::ContinuationStep, Self::Error> {
            panic!("unsupported live control profile must not run user continuation");
        }
    }
    let mut scene = crate::Scene::new();
    let target = scene.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let before = scene.time();
    scene
        .on_click_playback(&target, PlaybackControl::TogglePlayback)
        .unwrap();
    assert_eq!(scene.time(), before);
    assert!(scene.execution_session().unwrap().has_playback_controls());
    assert!(matches!(
        scene.into_live_program(MustNotRun),
        Err(
            noon_compile::SemanticExecutionLoweringError::PlaybackControlsExecutionUnavailable { .. }
        )
    ));
}
