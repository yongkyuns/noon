//! An ordinary object's own content and ordered children share one semantic ID.
use crate::{ExecutionSession, Scene, SemanticNodeId};
use noon_core::{
    SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectState, StoredGeometry,
};

fn painter_ids(session: &ExecutionSession) -> Vec<noon_core::ObjectId> {
    session
        .painter_order()
        .iter()
        .map(|&row| session.frame().objects[row as usize].id)
        .collect()
}

fn assert_order(session: &ExecutionSession, nodes: &[SemanticNodeId]) {
    assert_eq!(
        painter_ids(session),
        nodes
            .iter()
            .copied()
            .map(noon_compile::semantic_execution_object_id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn initial_projection_emits_own_content_then_descendants_once() {
    let mut scene = Scene::new();
    let parent = scene.square(2.0).unwrap();
    let child = scene.circle(1.0).unwrap();
    let parent_state = parent.state().unwrap();
    let tail = scene.square(1.0).unwrap();
    let nested = scene.family(&[(&child).into(), (&tail).into()]).unwrap();
    let store = scene.integration_store().clone();
    let mut members = SemanticMutationTransaction::new();
    members
        .add_member(parent.node_id(), nested.node_id())
        .add_member(parent.node_id(), child.node_id());
    members.apply(&mut store.borrow_mut()).unwrap();
    scene
        .add_many(&[(&parent).into(), (&child).into()])
        .unwrap();
    let session = scene.execution_session().unwrap();
    assert_order(
        &session,
        &[parent.node_id(), child.node_id(), tail.node_id()],
    );
    assert_eq!(parent.state().unwrap(), parent_state);
    assert_eq!(
        store
            .borrow()
            .node(parent.node_id())
            .unwrap()
            .member_count(),
        2
    );
}

#[test]
fn live_subtree_admission_reorder_and_alias_removal_keep_own_content() {
    let mut scene = Scene::new();
    let parent = scene.square(2.0).unwrap();
    let child = scene.circle(1.0).unwrap();
    let grandchild = scene.square(0.5).unwrap();
    let tail = scene.circle(0.5).unwrap();
    let nested = scene
        .family(&[(&child).into(), (&grandchild).into()])
        .unwrap();
    let store = scene.integration_store().clone();
    let mut members = SemanticMutationTransaction::new();
    members
        .add_member(parent.node_id(), nested.node_id())
        .add_member(parent.node_id(), child.node_id());
    members.apply(&mut store.borrow_mut()).unwrap();
    scene.add(&tail).unwrap();
    let mut session = scene.execution_session().unwrap();
    let state = parent.state().unwrap();
    let mut attach = SemanticMutationTransaction::new();
    attach.add_member(scene.root(), parent.node_id());
    session
        .apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), attach)
        .unwrap();
    assert_order(
        &session,
        &[
            tail.node_id(),
            parent.node_id(),
            child.node_id(),
            grandchild.node_id(),
        ],
    );

    let mut reorder = SemanticMutationTransaction::new();
    reorder.reorder_member(scene.root(), parent.node_id(), Some(tail.node_id()));
    session
        .apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), reorder)
        .unwrap();
    assert_order(
        &session,
        &[
            parent.node_id(),
            child.node_id(),
            grandchild.node_id(),
            tail.node_id(),
        ],
    );

    let mut detach = SemanticMutationTransaction::new();
    detach
        .add_member(scene.root(), child.node_id())
        .remove_member(scene.root(), parent.node_id());
    session
        .apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), detach)
        .unwrap();
    assert_order(&session, &[tail.node_id(), child.node_id()]);
    assert_eq!(parent.state().unwrap(), state);
    assert_eq!(
        store
            .borrow()
            .node(parent.node_id())
            .unwrap()
            .member_count(),
        2
    );

    let mut reattach = SemanticMutationTransaction::new();
    reattach.add_member(scene.root(), parent.node_id());
    session
        .apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), reattach)
        .unwrap();
    assert_order(
        &session,
        &[
            tail.node_id(),
            child.node_id(),
            parent.node_id(),
            grandchild.node_id(),
        ],
    );
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(parent.node_id());
    session
        .apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), remove)
        .unwrap();
    assert_order(&session, &[tail.node_id(), child.node_id()]);
    assert!(store.borrow().node(parent.node_id()).is_none());
    assert!(store.borrow().node(child.node_id()).is_some());
}

#[test]
fn pending_point_owning_root_and_child_publish_in_one_revision() {
    let scene = Scene::new();
    let mut session = scene.execution_session().unwrap();
    let store = scene.integration_store().clone();
    let revision = scene.revision();
    let state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    let mut transaction = SemanticMutationTransaction::new();
    let parent = transaction.create_node(SemanticNodeCreation::object(state.clone()));
    let child = transaction.create_node(SemanticNodeCreation::object(state));
    transaction
        .add_member(parent, child)
        .add_member(scene.root(), parent);
    let result = session
        .apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), transaction)
        .unwrap();
    let parent = result.resolve(parent).unwrap();
    let child = result.resolve(child).unwrap();
    assert_order(&session, &[parent, child]);
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!(store.borrow().node(child).unwrap().parents(), &[parent]);
}

#[test]
fn rejected_cycle_keeps_frame_membership_and_publication_coherent() {
    let mut scene = Scene::new();
    let parent = scene.square(2.0).unwrap();
    let child = scene.circle(1.0).unwrap();
    scene.add(&parent).unwrap();
    let mut session = scene.execution_session().unwrap();
    let store = scene.integration_store().clone();
    let frame = session.frame().clone();
    let publication = session.publication_context();
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_member(parent.node_id(), child.node_id())
        .add_member(child.node_id(), parent.node_id());
    assert!(session
        .apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), transaction)
        .is_err());
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(
        store
            .borrow()
            .node(parent.node_id())
            .unwrap()
            .member_count(),
        0
    );
    let mut valid = SemanticMutationTransaction::new();
    valid.add_member(parent.node_id(), child.node_id());
    session
        .apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), valid)
        .unwrap();
    assert_order(&session, &[parent.node_id(), child.node_id()]);
}

#[test]
fn direct_native_and_wasm_example_uses_the_normal_session() {
    let session = crate::example_scenes::object_membership::session().unwrap();
    assert_eq!(session.painter_order().len(), 3);
}
