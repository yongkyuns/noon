use super::*;
use noon_core::{
    Color, GeometryId, GeometryRef, ObjectContentRef, SemanticClickIndicate,
    SemanticMutationTransaction, SemanticObjectState, SemanticStore, StoredGeometry,
};

fn fixture() -> (
    SemanticStore,
    ExecutionSession,
    SemanticNodeId,
    SemanticNodeId,
    SemanticNodeId,
) {
    let mut store = SemanticStore::new();
    let keep = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let retired = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 2.0,
    }));
    let root = store.insert_family();
    store.add_member(root, keep).unwrap();
    store.add_member(root, retired).unwrap();
    store.attach_to_scene(root).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();
    (store, session, root, keep, retired)
}

fn retire(
    store: &mut SemanticStore,
    session: &mut ExecutionSession,
    root: SemanticNodeId,
    retired: SemanticNodeId,
) {
    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_member(root, retired);
    session
        .apply_semantic_transaction(store, transaction)
        .unwrap();
}

#[test]
fn maintenance_compacts_live_session_and_invalidates_derived_context() {
    let (mut store, mut session, root, keep, retired) = fixture();
    retire(&mut store, &mut session, root, retired);
    let before = session.publication_context();
    let identity = session.runtime_identity();
    let effective = session
        .effective_semantic_object(&store, keep)
        .unwrap()
        .object
        .clone();

    let stats = session.reclaim_retired_object_slots().unwrap();
    assert_eq!(stats.compiled.object_slots_reclaimed, 1);
    assert_eq!(session.runtime_identity(), identity);
    assert_eq!(session.frame().objects.len(), 1);
    assert_eq!(
        session
            .effective_semantic_object(&store, keep)
            .unwrap()
            .object,
        &effective
    );
    assert_eq!(
        session.publication_context().scene_revision(),
        before.scene_revision()
    );
    assert_eq!(
        session.publication_context().execution_revision(),
        before.execution_revision().checked_next().unwrap()
    );
    assert_eq!(
        session.publication_context().frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
}

#[test]
fn maintenance_rejects_pending_callback_before_touching_tombstones() {
    let (mut store, mut session, root, keep, retired) = fixture();
    retire(&mut store, &mut session, root, retired);
    let _overlay = session.begin_required_callback_phase(1.0, [keep]).unwrap();
    assert_eq!(
        session.reclaim_retired_object_slots(),
        Err(ExecutionSessionMaintenanceError::RequiredCallbackPending)
    );
    assert_eq!(session.frame().objects.len(), 2);
}

#[test]
fn callback_content_failure_keeps_phase_retryable_and_success_publishes_once() {
    let (_store, mut session, _root, keep, _retired) = fixture();
    let batch = session
        .begin_required_callback_phase(1.0, [keep])
        .unwrap()
        .finish();
    let before = session.publication_context();
    assert!(matches!(
        session.commit_required_callback_phase_with_content(
            batch.clone(),
            keep,
            ObjectContentRef::Geometry(GeometryRef::External(GeometryId::new(777))),
            None,
            None,
        ),
        Err(ExecutionSessionCallbackError::Content(_))
    ));
    assert_eq!(session.pending_callback_token(), Some(batch.token()));
    assert_eq!(session.publication_context(), before);
    assert_eq!(session.frame().time, 0.0);

    let lease = session
        .commit_required_callback_phase_with_content(
            batch,
            keep,
            ObjectContentRef::Geometry(GeometryRef::rectangle(2.0, 3.0)),
            None,
            None,
        )
        .unwrap();
    assert_eq!(session.pending_callback_token(), None);
    assert_eq!(session.frame().time, 1.0);
    assert_eq!(
        session.publication_context().frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(session.effective_content_lease(keep), Some(lease));
}

#[test]
fn maintenance_rejects_active_interaction_before_touching_tombstones() {
    let (mut store, mut session, root, keep, retired) = fixture();
    retire(&mut store, &mut session, root, retired);
    let object = session.execution_object_id(keep).unwrap();
    let effect = session
        .runtime
        .prepare_click_indicate(object, SemanticClickIndicate::new(1.2, Color::YELLOW, 1.0))
        .unwrap()
        .unwrap();
    session.runtime.start_transient_animation(effect).unwrap();
    assert_eq!(
        session.reclaim_retired_object_slots(),
        Err(ExecutionSessionMaintenanceError::InteractionActive)
    );
    assert_eq!(session.frame().objects.len(), 2);
}
