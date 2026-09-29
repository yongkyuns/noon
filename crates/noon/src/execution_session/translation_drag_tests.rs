use noon_core::{
    NativeInputModifiers, NativePointerCancellation, NativePointerId, NativePointerInput,
    NativePointerInputKind, NativePointerPosition, SemanticMutationTransaction,
    SemanticObjectProperty, SemanticObjectState, SemanticStore, SemanticVec3, StoredGeometry, Vec2,
};

use super::{
    ExecutionSession, ExecutionSessionPublicationError, NativePointerInputToken,
    TranslationDragError,
};

const POINTER: NativePointerId = NativePointerId {
    source: 91,
    pointer: 7,
};

fn circle(x: f64) -> SemanticObjectState {
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    state.transform.translation = SemanticVec3::new(x, 0.0, 0.0);
    state
}

fn fixture() -> (
    SemanticStore,
    noon_core::SemanticNodeId,
    noon_core::SemanticNodeId,
    ExecutionSession,
) {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let target = store.insert_semantic_object(circle(0.0));
    let unrelated = store.insert_semantic_object(circle(10.0));
    store.add_semantic_family_member(root, target).unwrap();
    store.add_semantic_family_member(root, unrelated).unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.configure_native_pointer_input(POINTER, 1).unwrap();
    session.set_translation_drag_targets([target]).unwrap();
    session.take_frame_changes();
    (store, target, unrelated, session)
}

fn position(x: f32) -> NativePointerPosition {
    NativePointerPosition::new(Vec2::new(x, 0.0), Vec2::new(x * 10.0, 100.0)).unwrap()
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
    session: &mut ExecutionSession,
    store: &mut SemanticStore,
    sequence: u64,
    kind: NativePointerInputKind,
) -> Result<super::TranslationDragReceipt, TranslationDragError> {
    let (token, input) = input(session, sequence, kind);
    session.submit_translation_drag_input(store, &token, input)
}

fn translation(session: &ExecutionSession, node: noon_core::SemanticNodeId) -> Vec2 {
    let object = session.execution_object_id(node).unwrap();
    let index = session
        .frame()
        .objects
        .iter()
        .position(|row| row.id == object)
        .unwrap();
    session.frame().objects[index].transform.translation
}

#[test]
fn drag_is_scoped_to_target_commits_once_and_is_undoable() {
    let (mut store, target, unrelated, mut session) = fixture();
    submit(
        &mut session,
        &mut store,
        1,
        NativePointerInputKind::Press {
            position: position(0.0),
            button: 0,
        },
    )
    .unwrap();
    assert!(session.translation_drag_active());
    assert!(session.wake_state().is_quiescent());
    let before_move = session.publication_context();

    submit(
        &mut session,
        &mut store,
        2,
        NativePointerInputKind::Move(position(3.0)),
    )
    .unwrap();
    assert_eq!(translation(&session, target), Vec2::new(3.0, 0.0));
    assert_eq!(translation(&session, unrelated), Vec2::new(10.0, 0.0));
    assert_eq!(session.take_frame_changes().object_indices().len(), 1);
    let after_move = session.publication_context();
    assert_eq!(after_move.scene_revision(), before_move.scene_revision());
    assert_eq!(
        after_move.execution_revision(),
        before_move.execution_revision()
    );
    assert_eq!(
        after_move.frame_epoch(),
        before_move.frame_epoch().checked_next().unwrap(),
        "the native occurrence and drag write share one frame publication"
    );
    assert!(session.wake_state().is_quiescent());

    let receipt = submit(
        &mut session,
        &mut store,
        3,
        NativePointerInputKind::Release {
            position: position(3.0),
            button: 0,
        },
    )
    .unwrap();
    assert!(!session.translation_drag_active());
    assert_eq!(translation(&session, target), Vec2::new(3.0, 0.0));
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .transform
            .translation,
        SemanticVec3::new(3.0, 0.0, 0.0)
    );
    let undo = receipt.undo.unwrap();
    undo.undo(&mut session, &mut store).unwrap();
    assert_eq!(translation(&session, target), Vec2::ZERO);
}

#[test]
fn stale_release_is_rejected_without_losing_the_lease_or_authored_value() {
    let (mut store, target, _, mut session) = fixture();
    submit(
        &mut session,
        &mut store,
        1,
        NativePointerInputKind::Press {
            position: position(0.0),
            button: 0,
        },
    )
    .unwrap();
    let (stale, release) = input(
        &session,
        3,
        NativePointerInputKind::Release {
            position: position(2.0),
            button: 0,
        },
    );
    submit(
        &mut session,
        &mut store,
        2,
        NativePointerInputKind::Move(position(2.0)),
    )
    .unwrap();
    let after_move = session.publication_context();
    assert!(matches!(
        session.submit_translation_drag_input(&mut store, &stale, release),
        Err(TranslationDragError::Input(_))
    ));
    assert_eq!(session.publication_context(), after_move);
    assert!(session.translation_drag_active());
    assert_eq!(translation(&session, target), Vec2::new(2.0, 0.0));
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .transform
            .translation,
        SemanticVec3::ZERO
    );
}

#[test]
fn cancellation_discards_the_effective_lease_without_authoring_a_value() {
    let (mut store, target, _, mut session) = fixture();
    submit(
        &mut session,
        &mut store,
        1,
        NativePointerInputKind::Press {
            position: position(0.0),
            button: 0,
        },
    )
    .unwrap();
    submit(
        &mut session,
        &mut store,
        2,
        NativePointerInputKind::Move(position(4.0)),
    )
    .unwrap();
    let before_cancel = session.publication_context();
    submit(
        &mut session,
        &mut store,
        3,
        NativePointerInputKind::Cancel(NativePointerCancellation::CaptureLost),
    )
    .unwrap();
    assert!(!session.translation_drag_active());
    let after_cancel = session.publication_context();
    assert_eq!(
        after_cancel.scene_revision(),
        before_cancel.scene_revision()
    );
    assert_eq!(
        after_cancel.execution_revision(),
        before_cancel.execution_revision()
    );
    assert_eq!(
        after_cancel.frame_epoch(),
        before_cancel.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(translation(&session, target), Vec2::ZERO);
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .transform
            .translation,
        SemanticVec3::ZERO
    );
}

#[test]
fn active_drag_rejects_source_edits_until_cancelled() {
    let (mut store, target, _, mut session) = fixture();
    submit(
        &mut session,
        &mut store,
        1,
        NativePointerInputKind::Press {
            position: position(0.0),
            button: 0,
        },
    )
    .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(
        target,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(9.0, 0.0, 0.0),
    );
    assert_eq!(
        session.apply_semantic_transaction(&mut store, transaction),
        Err(ExecutionSessionPublicationError::TranslationDragActive)
    );
    session.cancel_translation_drag().unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(
        target,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(9.0, 0.0, 0.0),
    );
    session
        .apply_semantic_transaction(&mut store, transaction)
        .unwrap();
    assert_eq!(translation(&session, target), Vec2::new(9.0, 0.0));
}

#[test]
fn translation_signal_driver_rejects_press_without_acknowledging_it() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let target = store.insert_semantic_object(circle(0.0));
    store.add_semantic_family_member(root, target).unwrap();
    let signal = store
        .insert_semantic_input_signal(SemanticVec3::ZERO)
        .unwrap();
    store
        .bind_semantic_signal(signal, target, SemanticObjectProperty::Translation)
        .unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.configure_native_pointer_input(POINTER, 1).unwrap();
    session.set_translation_drag_targets([target]).unwrap();
    let (token, press) = input(
        &session,
        1,
        NativePointerInputKind::Press {
            position: position(0.0),
            button: 0,
        },
    );
    assert_eq!(
        session.submit_translation_drag_input(&mut store, &token, press),
        Err(TranslationDragError::DriverConflict)
    );
    assert!(!session.translation_drag_active());
    // The same sequence remains admissible through the ordinary ingress because
    // rejected acquisition did not acknowledge it.
    session.submit_native_pointer_input(&token, press).unwrap();
}

#[test]
fn undo_rejects_a_later_authored_revision() {
    let (mut store, target, unrelated, mut session) = fixture();
    submit(
        &mut session,
        &mut store,
        1,
        NativePointerInputKind::Press {
            position: position(0.0),
            button: 0,
        },
    )
    .unwrap();
    submit(
        &mut session,
        &mut store,
        2,
        NativePointerInputKind::Move(position(2.0)),
    )
    .unwrap();
    let undo = submit(
        &mut session,
        &mut store,
        3,
        NativePointerInputKind::Release {
            position: position(2.0),
            button: 0,
        },
    )
    .unwrap()
    .undo
    .unwrap();
    let mut later = SemanticMutationTransaction::new();
    later.set_property(
        unrelated,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(12.0, 0.0, 0.0),
    );
    session
        .apply_semantic_transaction(&mut store, later)
        .unwrap();
    assert_eq!(
        undo.undo(&mut session, &mut store),
        Err(TranslationDragError::StaleUndo)
    );
    assert_eq!(translation(&session, target), Vec2::new(2.0, 0.0));
}

#[test]
fn release_uses_final_pointer_position_and_preserves_authored_z() {
    let (mut store, target, _, mut session) = fixture();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(
        target,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(0.0, 0.0, 0.25),
    );
    session
        .apply_semantic_transaction(&mut store, transaction)
        .unwrap();
    submit(
        &mut session,
        &mut store,
        1,
        NativePointerInputKind::Press {
            position: position(0.0),
            button: 0,
        },
    )
    .unwrap();
    submit(
        &mut session,
        &mut store,
        2,
        NativePointerInputKind::Move(position(2.0)),
    )
    .unwrap();
    let before = session.publication_context();
    let receipt = submit(
        &mut session,
        &mut store,
        3,
        NativePointerInputKind::Release {
            position: position(4.0),
            button: 0,
        },
    )
    .unwrap();
    assert_eq!(translation(&session, target), Vec2::new(4.0, 0.0));
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .transform
            .translation,
        SemanticVec3::new(4.0, 0.0, 0.25)
    );
    assert_eq!(
        session.publication_context().frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    receipt
        .undo
        .unwrap()
        .undo(&mut session, &mut store)
        .unwrap();
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .transform
            .translation
            .z,
        0.25
    );
}

#[test]
fn active_drag_blocks_undo_mapping_changes_compaction_and_callback_cancellation() {
    let (mut store, target, _, mut session) = fixture();
    submit(
        &mut session,
        &mut store,
        1,
        NativePointerInputKind::Press {
            position: position(0.0),
            button: 0,
        },
    )
    .unwrap();
    let undo = submit(
        &mut session,
        &mut store,
        2,
        NativePointerInputKind::Release {
            position: position(2.0),
            button: 0,
        },
    )
    .unwrap()
    .undo
    .unwrap();
    submit(
        &mut session,
        &mut store,
        3,
        NativePointerInputKind::Press {
            position: position(2.0),
            button: 0,
        },
    )
    .unwrap();
    submit(
        &mut session,
        &mut store,
        4,
        NativePointerInputKind::Move(position(3.0)),
    )
    .unwrap();
    let before = session.publication_context();
    assert_eq!(
        undo.undo(&mut session, &mut store),
        Err(TranslationDragError::DriverConflict)
    );
    assert!(session.configure_native_pointer_input(POINTER, 2).is_err());
    assert_eq!(
        session.reclaim_retired_object_slots(),
        Err(super::ExecutionSessionMaintenanceError::InteractionActive)
    );
    let overlay = session
        .begin_required_callback_phase(0.0, [target])
        .unwrap();
    assert_eq!(
        session.cancel_translation_drag(),
        Err(TranslationDragError::Input(
            super::ExecutionSessionInputError::RequiredCallbackPending
        ))
    );
    assert!(session.set_translation_drag_targets([]).is_err());
    assert!(session.translation_drag_active());
    assert_eq!(session.publication_context(), before);
    assert_eq!(translation(&session, target), Vec2::new(3.0, 0.0));
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    session.cancel_translation_drag().unwrap();
    assert_eq!(translation(&session, target), Vec2::new(2.0, 0.0));
    assert!(!session.translation_drag_active());
}

#[test]
fn uncaptured_drag_input_preserves_ordinary_click_actions() {
    let (mut store, _, unrelated, mut session) = fixture();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_click_indicate(
        unrelated,
        Some(noon_core::SemanticClickIndicate::new(
            1.2,
            noon_core::Color::YELLOW,
            0.5,
        )),
    );
    session
        .apply_semantic_transaction(&mut store, transaction)
        .unwrap();
    submit(
        &mut session,
        &mut store,
        1,
        NativePointerInputKind::Press {
            position: position(10.0),
            button: 0,
        },
    )
    .unwrap();
    assert!(!session.translation_drag_active());
    let receipt = submit(
        &mut session,
        &mut store,
        2,
        NativePointerInputKind::Release {
            position: position(10.0),
            button: 0,
        },
    )
    .unwrap();
    assert_eq!(
        receipt.input.selection_click().unwrap().target(),
        Some(unrelated)
    );
    assert!(session.runtime.interactions_active());
    assert!(receipt.undo.is_none());
}

#[test]
fn configured_drag_interest_prevents_sealing_unrecorded_interaction_history() {
    let (_, target, _, mut session) = fixture();
    assert!(session.has_native_pointer_subscribers());
    session.begin_replay_retention(Default::default()).unwrap();
    assert_eq!(
        session.seal_replay(),
        Err(noon_runtime::ReplayError::UnsupportedDomain)
    );
    session.discard_replay_retention();
    session.set_translation_drag_targets([]).unwrap();
    assert!(!session.has_native_pointer_subscribers());
    session.begin_replay_retention(Default::default()).unwrap();
    session.set_translation_drag_targets([target]).unwrap();
    assert_eq!(
        session.seal_replay(),
        Err(noon_runtime::ReplayError::UnsupportedDomain)
    );
}

#[test]
fn sealed_history_rejects_drag_configuration_without_changing_policy_or_frame() {
    let (_, target, _, mut session) = fixture();
    session.set_translation_drag_targets([]).unwrap();
    session.begin_replay_retention(Default::default()).unwrap();
    session.seal_replay().unwrap();
    let before = session.publication_context();
    assert_eq!(
        session.set_translation_drag_targets([target]),
        Err(TranslationDragError::ReplaySealed)
    );
    assert_eq!(session.publication_context(), before);
    assert!(!session.has_native_pointer_subscribers());
    assert!(session.replay_is_sealed());
}

#[test]
fn drag_waits_for_pending_segment_before_acquiring_a_persistent_edit() {
    use noon_core::AnimationOptions;
    let (mut store, target, unrelated, mut session) = fixture();
    let endpoint = store.insert_semantic_object(circle(12.0));
    let animation = store
        .insert_semantic_transform_animation(unrelated, endpoint, AnimationOptions::new())
        .unwrap();
    let segment = session
        .activate_animation_segment(&store, animation, AnimationOptions::new().run_time(1.0))
        .unwrap();
    let before = session.publication_context();
    let token = session.native_pointer_input_token().unwrap();
    assert_eq!(
        submit(
            &mut session,
            &mut store,
            1,
            NativePointerInputKind::Press {
                position: position(0.0),
                button: 0,
            }
        ),
        Err(TranslationDragError::Publication(
            ExecutionSessionPublicationError::SegmentCompletionPending
        ))
    );
    assert_eq!(session.publication_context(), before);
    assert_eq!(session.native_pointer_input_token().unwrap(), token);
    assert!(!session.translation_drag_active());
    assert_eq!(translation(&session, target), Vec2::ZERO);
    session
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    session.complete_segment(&mut store, segment).unwrap();
    submit(
        &mut session,
        &mut store,
        1,
        NativePointerInputKind::Press {
            position: position(0.0),
            button: 0,
        },
    )
    .unwrap();
    assert!(session.translation_drag_active());
}

#[test]
fn active_drag_rejects_new_animation_without_installing_tracks() {
    use noon_core::AnimationOptions;
    let (mut store, target, _, mut session) = fixture();
    let endpoint = store.insert_semantic_object(circle(3.0));
    let animation = store
        .insert_semantic_transform_animation(target, endpoint, AnimationOptions::new())
        .unwrap();
    submit(
        &mut session,
        &mut store,
        1,
        NativePointerInputKind::Press {
            position: position(0.0),
            button: 0,
        },
    )
    .unwrap();
    let before = session.publication_context();
    assert_eq!(
        session.activate_animation_segment(&store, animation, AnimationOptions::new()),
        Err(super::ExecutionSessionAnimationError::AuthoredPublication(
            ExecutionSessionPublicationError::TranslationDragActive
        ))
    );
    assert_eq!(session.publication_context(), before);
    assert!(session.translation_drag_active());
    session.cancel_translation_drag().unwrap();
    session
        .activate_animation_segment(&store, animation, AnimationOptions::new())
        .unwrap();
}
