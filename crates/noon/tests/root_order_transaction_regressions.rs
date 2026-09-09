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

#[test]
fn provisional_empty_anchor_preserves_runtime_order_and_identity() {
    use noon_core::SemanticNodeCreation;
    let mut store = SemanticStore::new();
    let [source, tail] = std::array::from_fn(|_| object(&mut store));
    let root = family(&mut store, &[source, tail]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.take_frame_changes();
    let before = session.publication_context();
    let frame = session.frame().clone();
    let slot = session.execution_slot_for_frame_index(0);
    let owner = Rc::new(RefCell::new(store));
    let mut transaction = SemanticMutationTransaction::new();
    let empty = transaction.create_node(SemanticNodeCreation::family());
    transaction.add_member(root, empty);
    transaction.reorder_member(root, empty, Some(tail));
    transaction.reorder_member_ref(root, source, Some(empty.into()));
    LiveSession::new(&owner, root, &mut session)
        .apply(transaction)
        .unwrap();
    assert_order(&owner.borrow(), root, &session);
    assert_eq!(session.frame().objects, frame.objects);
    assert_eq!(session.execution_slot_for_frame_index(0), slot);
    let after = session.publication_context();
    assert_eq!(
        after.scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );
    assert_eq!(after.execution_revision(), before.execution_revision());
    assert_eq!(
        after.frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn provisional_object_and_nested_family_keep_their_staged_position() {
    use noon_core::SemanticNodeCreation;
    for nested in [false, true] {
        let mut store = SemanticStore::new();
        let [source, tail] = std::array::from_fn(|_| object(&mut store));
        let root = family(&mut store, &[source, tail]);
        let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
        let owner = Rc::new(RefCell::new(store));
        let mut tx = SemanticMutationTransaction::new();
        let leaf = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
            StoredGeometry::Circle { radius: 1.0 },
        )));
        let anchor = if nested {
            let inner = tx.create_node(SemanticNodeCreation::family());
            let outer = tx.create_node(SemanticNodeCreation::family());
            tx.add_member(inner, leaf);
            tx.add_member(outer, inner);
            outer
        } else {
            leaf
        };
        tx.add_member(root, anchor);
        tx.reorder_member(root, anchor, Some(tail));
        tx.reorder_member_ref(root, source, Some(anchor.into()));
        LiveSession::new(&owner, root, &mut session)
            .apply(tx)
            .unwrap();
        assert_order(&owner.borrow(), root, &session);
    }
}

#[test]
fn nested_alias_append_and_reorder_use_first_occurrence_once() {
    for append in [false, true] {
        let mut store = SemanticStore::new();
        let [shared, other, anchor] = std::array::from_fn(|_| object(&mut store));
        let duplicate = family(&mut store, &[shared]);
        let moved = family(&mut store, &[shared, other, duplicate]);
        let members = if append {
            vec![anchor]
        } else {
            vec![anchor, moved]
        };
        let root = family(&mut store, &members);
        let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
        let owner = Rc::new(RefCell::new(store));
        let mut tx = SemanticMutationTransaction::new();
        if append {
            tx.add_member(root, moved);
        } else {
            tx.reorder_member(root, moved, Some(anchor));
        }
        LiveSession::new(&owner, root, &mut session)
            .apply(tx)
            .unwrap();
        assert_order(&owner.borrow(), root, &session);
    }
}

#[test]
fn rejected_provisional_order_preserves_state_then_reuses_unpublished_identity() {
    use noon_core::{SemanticNodeCreation, SemanticObjectProperty};
    let mut store = SemanticStore::new();
    let [source, tail] = std::array::from_fn(|_| object(&mut store));
    let root = family(&mut store, &[source, tail]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.take_frame_changes();
    let before = session.publication_context();
    let frame = session.frame().clone();
    let count = store.len();
    let owner = Rc::new(RefCell::new(store));
    for invalid in [true, false] {
        let mut tx = SemanticMutationTransaction::new();
        let leaf = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
            StoredGeometry::Circle { radius: 1.0 },
        )));
        tx.add_member(root, leaf);
        tx.reorder_member(root, leaf, Some(tail));
        if invalid {
            tx.set_property(leaf, SemanticObjectProperty::RotationZ, f64::MAX);
        }
        let result = LiveSession::new(&owner, root, &mut session).apply(tx);
        if invalid {
            assert!(matches!(
                result,
                Err(noon::LiveSessionError::Publication(
                    noon::ExecutionSessionPublicationError::Lowering(_)
                ))
            ));
            assert_eq!(owner.borrow().len(), count);
            assert_eq!(
                owner.borrow().node(root).unwrap().members(),
                vec![source, tail]
            );
            assert_eq!(owner.borrow().scene_revision(), before.scene_revision());
            assert_eq!(session.publication_context(), before);
            assert_eq!(session.frame(), &frame);
            assert!(session.take_frame_changes().is_empty());
        } else {
            let result = result.unwrap();
            let id = result.resolve(leaf).unwrap();
            assert_eq!(id.slot() as usize, count);
            assert_eq!(id.generation(), 0);
            assert_order(&owner.borrow(), root, &session);
        }
    }
}

#[test]
fn live_root_must_belong_to_the_execution_domain_before_publication() {
    use noon::{ExecutionSessionPublicationError, LiveSessionError};
    use noon_core::{SemanticNodeCreation, SemanticObjectProperty, SemanticVec3};

    for scene_owned in [false, true] {
        for nested_alias in [false, true] {
            for pending_dirty in [false, true] {
                let mut store = SemanticStore::new();
                let a = object(&mut store);
                let b = object(&mut store);
                let wrong_root = family(&mut store, &[a, b]);
                let root = family(&mut store, &[a, b]);
                if nested_alias {
                    store.add_member(root, wrong_root).unwrap();
                }
                let mut session = if scene_owned {
                    store.attach_to_scene(root).unwrap();
                    ExecutionSession::from_semantic_store(&store).unwrap()
                } else {
                    ExecutionSession::from_semantic_root(&store, root).unwrap()
                };
                let owner = Rc::new(RefCell::new(store));
                session.take_frame_changes();
                if pending_dirty {
                    let mut tx = SemanticMutationTransaction::new();
                    tx.set_property(
                        a,
                        SemanticObjectProperty::Translation,
                        SemanticVec3::new(3.0, 0.0, 0.0),
                    );
                    LiveSession::new(&owner, root, &mut session)
                        .apply(tx)
                        .unwrap();
                }
                let context = session.publication_context();
                let frame = session.frame().clone();
                let order = session.painter_order().to_vec();
                let slots = (0..frame.objects.len())
                    .map(|row| session.execution_slot_for_frame_index(row))
                    .collect::<Vec<_>>();
                let count = owner.borrow().len();
                let root_members = owner.borrow().node(root).unwrap().members();
                let mutation_stats = owner.borrow().last_mutation_stats();
                let authored_b = owner
                    .borrow()
                    .semantic_object_state_checked(b)
                    .unwrap()
                    .clone();

                // Equal leaf membership (even a reachable alias family) does not
                // make this the root that defines the retained execution domain.
                let mut tx = SemanticMutationTransaction::new();
                tx.reorder_member(root, b, Some(a));
                tx.set_property(b, SemanticObjectProperty::RotationZ, 0.5);
                tx.create_node(SemanticNodeCreation::family());
                assert!(matches!(
                    LiveSession::new(&owner, wrong_root, &mut session).apply(tx),
                    Err(LiveSessionError::Publication(
                        ExecutionSessionPublicationError::UnknownObject(rejected)
                    )) if rejected == wrong_root
                ));

                assert_eq!(session.publication_context(), context);
                assert_eq!(owner.borrow().scene_revision(), context.scene_revision());
                assert_eq!(owner.borrow().len(), count);
                assert_eq!(owner.borrow().last_mutation_stats(), mutation_stats);
                assert_eq!(owner.borrow().node(root).unwrap().members(), root_members);
                assert_eq!(owner.borrow().node(wrong_root).unwrap().members(), [a, b]);
                assert_eq!(
                    owner.borrow().node(root).unwrap().compare_members(a, b),
                    Some(std::cmp::Ordering::Less)
                );
                assert_eq!(
                    owner.borrow().semantic_object_state_checked(b).unwrap(),
                    &authored_b
                );
                assert_eq!(session.frame(), &frame);
                assert_eq!(session.painter_order(), order);
                for (row, slot) in slots.iter().enumerate() {
                    assert_eq!(session.execution_slot_for_frame_index(row), *slot);
                }
                let changes = session.take_frame_changes();
                assert_eq!(changes.painter_order_range(), None);
                if pending_dirty {
                    assert_eq!(changes.object_indices(), &[0]);
                } else {
                    assert!(changes.is_empty());
                }

                let mut valid = SemanticMutationTransaction::new();
                valid.reorder_member(root, b, Some(a));
                LiveSession::new(&owner, root, &mut session)
                    .apply(valid)
                    .unwrap();
                assert_order(&owner.borrow(), root, &session);
                assert_eq!(session.frame(), &frame);
                let after = session.publication_context();
                assert_eq!(
                    after.scene_revision(),
                    context.scene_revision().checked_next().unwrap()
                );
                assert_eq!(
                    after.execution_revision(),
                    context.execution_revision().checked_next().unwrap()
                );
                assert_eq!(
                    after.frame_epoch(),
                    context.frame_epoch().checked_next().unwrap()
                );
            }
        }
    }
}
