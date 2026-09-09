use std::{cell::RefCell, rc::Rc};

use noon::integration::{SemanticMutationTransaction, SemanticStore};
use noon::{ExecutionSession, LiveSession};
use noon_compile::semantic_execution_object_id;
use noon_core::{
    plan_semantic_scene_membership, SemanticNodeId, SemanticObjectState,
    SemanticSceneMembershipRequest, StoredGeometry,
};

fn object(store: &mut SemanticStore) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }))
}

fn family(store: &mut SemanticStore, members: &[SemanticNodeId]) -> SemanticNodeId {
    let family = store.insert_family();
    for &member in members {
        store.add_member(family, member).unwrap();
    }
    family
}

fn assert_order(store: &SemanticStore, root: SemanticNodeId, session: &ExecutionSession) {
    let actual = session
        .painter_order()
        .iter()
        .map(|&index| session.frame().objects[index as usize].id)
        .collect::<Vec<_>>();
    let expected = store
        .ordered_leaf_nodes(root)
        .unwrap()
        .into_iter()
        .map(semantic_execution_object_id)
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn shared_remove_preserves_promoted_empty_anchors() {
    let mut store = SemanticStore::new();
    let [a, b, c, right] = std::array::from_fn(|_| object(&mut store));
    let empty = family(&mut store, &[]);
    let nested = family(&mut store, &[a, b, empty, c]);
    let root = family(&mut store, &[nested, right]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let transaction =
        plan_semantic_scene_membership(&store, root, SemanticSceneMembershipRequest::Remove(&[a]))
            .unwrap();
    let owner = Rc::new(RefCell::new(store));
    LiveSession::new(&owner, root, &mut session)
        .apply(transaction)
        .unwrap();
    assert_order(&owner.borrow(), root, &session);
}

#[test]
fn shared_remove_skips_removed_successor_of_empty_anchor() {
    let mut store = SemanticStore::new();
    let [a, b, removed, right] = std::array::from_fn(|_| object(&mut store));
    let nested = family(&mut store, &[a, b]);
    let empty = family(&mut store, &[]);
    let root = family(&mut store, &[nested, empty, removed, right]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let transaction = plan_semantic_scene_membership(
        &store,
        root,
        SemanticSceneMembershipRequest::Remove(&[a, removed]),
    )
    .unwrap();
    let owner = Rc::new(RefCell::new(store));
    LiveSession::new(&owner, root, &mut session)
        .apply(transaction)
        .unwrap();
    assert_order(&owner.borrow(), root, &session);
}

#[test]
fn compound_reorder_uses_staged_empty_anchor_successor() {
    let mut store = SemanticStore::new();
    let [a, b, c] = std::array::from_fn(|_| object(&mut store));
    let empty = family(&mut store, &[]);
    let root = family(&mut store, &[empty, a, b, c]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let owner = Rc::new(RefCell::new(store));
    let mut transaction = SemanticMutationTransaction::new();
    transaction.reorder_member(root, a, Some(c));
    transaction.reorder_member(root, c, Some(empty));
    LiveSession::new(&owner, root, &mut session)
        .apply(transaction)
        .unwrap();
    assert_order(&owner.borrow(), root, &session);
}

#[test]
fn terminal_removal_of_reordered_leaf_never_half_publishes() {
    let mut store = SemanticStore::new();
    let [a, b, c] = std::array::from_fn(|_| object(&mut store));
    let root = family(&mut store, &[a, b, c]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.take_frame_changes();
    let before = session.publication_context();
    let frame = session.frame().clone();
    let owner = Rc::new(RefCell::new(store));
    let mut transaction = SemanticMutationTransaction::new();
    transaction.reorder_member(root, c, Some(a));
    transaction.remove_node(c);
    match LiveSession::new(&owner, root, &mut session).apply(transaction) {
        Ok(_) => assert_order(&owner.borrow(), root, &session),
        Err(_) => {
            assert_eq!(session.publication_context(), before);
            assert_eq!(owner.borrow().scene_revision(), before.scene_revision());
            assert_eq!(owner.borrow().node(root).unwrap().members(), &[a, b, c]);
            assert_eq!(session.frame(), &frame);
            assert!(session.take_frame_changes().is_empty());
        }
    }
}
