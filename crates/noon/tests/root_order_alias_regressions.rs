use noon::integration::{SemanticMutationTransaction, SemanticStore};
use noon::{ExecutionSession, LiveSession};
use noon_compile::semantic_execution_object_id;
use noon_core::{SemanticNodeId, SemanticObjectState, StoredGeometry};
use std::{cell::RefCell, rc::Rc};
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
fn check(store: &SemanticStore, root: SemanticNodeId, session: &ExecutionSession) {
    let actual = session
        .painter_order()
        .iter()
        .map(|&row| session.frame().objects[row as usize].id)
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
fn raw_cross_root_alias_reorder_matches_first_occurrence() {
    let mut store = SemanticStore::new();
    let [shared, a, b] = std::array::from_fn(|_| object(&mut store));
    let left = family(&mut store, &[shared, a]);
    let right = family(&mut store, &[shared, b]);
    let root = family(&mut store, &[left, right]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let owner = Rc::new(RefCell::new(store));
    let mut transaction = SemanticMutationTransaction::new();
    transaction.reorder_member(root, left, None);
    LiveSession::new(&owner, root, &mut session)
        .apply(transaction)
        .unwrap();
    check(&owner.borrow(), root, &session);
}
#[test]
fn raw_append_with_nested_alias_matches_first_occurrence() {
    let mut store = SemanticStore::new();
    let [anchor, shared, other] = std::array::from_fn(|_| object(&mut store));
    let alias = family(&mut store, &[shared]);
    let group = family(&mut store, &[shared, other, alias]);
    let root = family(&mut store, &[anchor]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let owner = Rc::new(RefCell::new(store));
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_member(root, group);
    LiveSession::new(&owner, root, &mut session)
        .apply(transaction)
        .unwrap();
    check(&owner.borrow(), root, &session);
}

fn oracle(store: &SemanticStore, root: SemanticNodeId) -> Vec<noon_core::ObjectId> {
    fn visit(
        store: &SemanticStore,
        node: SemanticNodeId,
        seen: &mut std::collections::HashSet<SemanticNodeId>,
        out: &mut Vec<noon_core::ObjectId>,
    ) {
        if !seen.insert(node) {
            return;
        }
        let value = store.node(node).unwrap();
        if matches!(value.kind(), noon_core::SemanticNodeKind::AuthoringObject) {
            out.push(semantic_execution_object_id(node));
        } else {
            for member in value.members_iter() {
                visit(store, member, seen, out);
            }
        }
    }
    let mut out = Vec::new();
    visit(store, root, &mut std::collections::HashSet::new(), &mut out);
    out
}

fn painter(session: &ExecutionSession) -> Vec<noon_core::ObjectId> {
    session
        .painter_order()
        .iter()
        .map(|&row| session.frame().objects[row as usize].id)
        .collect()
}

fn permutation(index: usize) -> [usize; 5] {
    let mut rank = index;
    let mut pool = vec![0, 1, 2, 3, 4];
    std::array::from_fn(|i| {
        let block = (1..5 - i).product::<usize>();
        let element = pool.remove(rank / block);
        rank %= block;
        element
    })
}

#[test]
fn all_cross_root_alias_positions_match_independent_reference() {
    let mut checked = 0;
    for arrangement in 0..120 {
        for moved in 0..5 {
            for before in 0..=5 {
                let mut store = SemanticStore::new();
                let [s, a, b, c] = std::array::from_fn(|_| object(&mut store));
                let nested = family(&mut store, &[s, a]);
                let left = family(&mut store, &[nested, b]);
                let right = family(&mut store, &[b, s, c, nested]);
                let empty = family(&mut store, &[]);
                let members = [left, right, s, empty, c];
                let ordered = permutation(arrangement).map(|i| members[i]);
                let root = family(&mut store, &ordered);
                let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
                let frame = session.frame().clone();
                let slots = (0..frame.objects.len())
                    .map(|row| session.execution_slot_for_frame_index(row))
                    .collect::<Vec<_>>();
                let owner = Rc::new(RefCell::new(store));
                let mut tx = SemanticMutationTransaction::new();
                tx.reorder_member(root, ordered[moved], ordered.get(before).copied());
                LiveSession::new(&owner, root, &mut session)
                    .apply(tx)
                    .unwrap();
                assert_eq!(
                    painter(&session),
                    oracle(&owner.borrow(), root),
                    "arrangement={arrangement}, move={moved}, before={before}"
                );
                assert_eq!(session.frame(), &frame);
                for (row, slot) in slots.into_iter().enumerate() {
                    assert_eq!(session.execution_slot_for_frame_index(row), slot);
                }
                assert_eq!(session.last_patch_stats().full_seeks, 0);
                assert_eq!(session.last_patch_stats().full_group_rebuilds, 0);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 3600);
    println!("independent-reference alias reorder arrangements: {checked}");
}

#[test]
fn compound_alias_reorder_removal_and_readmission_preserve_net_order() {
    for deleted_node in [false, true] {
        let mut store = SemanticStore::new();
        let [shared, a, b, c] = std::array::from_fn(|_| object(&mut store));
        let left = family(&mut store, &[shared, a]);
        let right = family(&mut store, &[b, shared]);
        let third = family(&mut store, &[c, a]);
        let root = family(&mut store, &[left, right, third]);
        let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
        let runtime = session.runtime_identity();
        let owner = Rc::new(RefCell::new(store));
        let mut tx = SemanticMutationTransaction::new();
        tx.reorder_member(root, third, Some(left));
        tx.reorder_member(root, left, None);
        LiveSession::new(&owner, root, &mut session)
            .apply(tx)
            .unwrap();
        assert_eq!(painter(&session), oracle(&owner.borrow(), root));
        let mut tx = SemanticMutationTransaction::new();
        if deleted_node {
            tx.remove_node(right);
        } else {
            tx.remove_member(root, right);
        }
        LiveSession::new(&owner, root, &mut session)
            .apply(tx)
            .unwrap();
        assert_eq!(painter(&session), oracle(&owner.borrow(), root));
        let mut tx = SemanticMutationTransaction::new();
        tx.add_member(root, b);
        tx.reorder_member(root, b, Some(left));
        tx.reorder_member(root, left, Some(third));
        LiveSession::new(&owner, root, &mut session)
            .apply(tx)
            .unwrap();
        assert_eq!(painter(&session), oracle(&owner.borrow(), root));
        assert_eq!(session.runtime_identity(), runtime);
    }
}

#[test]
fn alias_transfer_revisions_rejection_recovery_and_seek_are_coherent() {
    use noon::{ExecutionSessionPublicationError, LiveSessionError};
    use noon_core::{SemanticObjectProperty, SemanticVec3};
    let mut store = SemanticStore::new();
    let [shared, a, b] = std::array::from_fn(|_| object(&mut store));
    let left = family(&mut store, &[shared, a]);
    let right = family(&mut store, &[shared, b]);
    let root = family(&mut store, &[left, right]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.take_frame_changes();
    let owner = Rc::new(RefCell::new(store));
    // Preserve already-pending renderer dirtiness through a rejected mixed batch.
    let mut tx = SemanticMutationTransaction::new();
    tx.set_property(
        a,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(3.0, 0.0, 0.0),
    );
    LiveSession::new(&owner, root, &mut session)
        .apply(tx)
        .unwrap();
    let before = session.publication_context();
    let frame = session.frame().clone();
    let before_order = painter(&session);
    let slot = session.execution_slot_for_frame_index(0);
    let mut invalid = SemanticMutationTransaction::new();
    invalid.reorder_member(root, left, None);
    invalid.set_property(b, SemanticObjectProperty::RotationZ, f64::MAX);
    assert!(matches!(
        LiveSession::new(&owner, root, &mut session).apply(invalid),
        Err(LiveSessionError::Publication(
            ExecutionSessionPublicationError::Lowering(_)
        ))
    ));
    assert_eq!(session.publication_context(), before);
    assert_eq!(owner.borrow().scene_revision(), before.scene_revision());
    assert_eq!(
        owner.borrow().node(root).unwrap().members(),
        vec![left, right]
    );
    assert_eq!(
        owner
            .borrow()
            .node(root)
            .unwrap()
            .compare_members(left, right),
        Some(std::cmp::Ordering::Less)
    );
    assert_eq!(session.frame(), &frame);
    assert_eq!(painter(&session), before_order);
    assert_eq!(session.execution_slot_for_frame_index(0), slot);
    let changes = session.take_frame_changes();
    assert_eq!(changes.object_indices(), &[1]);
    assert!(!changes.has_painter_order_change());
    let mut valid = SemanticMutationTransaction::new();
    valid.reorder_member(root, left, None);
    LiveSession::new(&owner, root, &mut session)
        .apply(valid)
        .unwrap();
    let after = session.publication_context();
    assert_eq!(
        after.scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );
    assert_eq!(
        after.execution_revision(),
        before.execution_revision().checked_next().unwrap()
    );
    assert_eq!(
        after.frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(session.execution_slot_for_frame_index(0), slot);
    assert_eq!(painter(&session), oracle(&owner.borrow(), root));
    session.take_frame_changes();
    let mut noop = SemanticMutationTransaction::new();
    noop.reorder_member(root, left, None);
    LiveSession::new(&owner, root, &mut session)
        .apply(noop)
        .unwrap();
    assert_eq!(session.publication_context(), after);
    assert!(session.take_frame_changes().is_empty());
    for time in [1.0, 2.0, 0.0] {
        session.seek(time).unwrap();
        assert_eq!(painter(&session), oracle(&owner.borrow(), root));
        assert_eq!(
            session.publication_context().scene_revision(),
            after.scene_revision()
        );
        assert_eq!(
            session.publication_context().execution_revision(),
            after.execution_revision()
        );
    }
}

#[test]
fn identical_alias_blocks_reorder_authored_only() {
    let mut store = SemanticStore::new();
    let [s, a] = std::array::from_fn(|_| object(&mut store));
    let left = family(&mut store, &[s, a]);
    let right = family(&mut store, &[s, a]);
    let root = family(&mut store, &[left, right]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.take_frame_changes();
    let before = session.publication_context();
    let owner = Rc::new(RefCell::new(store));
    let mut tx = SemanticMutationTransaction::new();
    tx.reorder_member(root, left, None);
    LiveSession::new(&owner, root, &mut session)
        .apply(tx)
        .unwrap();
    let after = session.publication_context();
    assert_eq!(
        after.scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );
    assert_eq!(
        after.frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(after.execution_revision(), before.execution_revision());
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn repeated_alias_membership_edits_match_reference_after_every_publication() {
    let mut store = SemanticStore::new();
    let objects: [SemanticNodeId; 6] = std::array::from_fn(|_| object(&mut store));
    let first = family(&mut store, &[objects[0], objects[1], objects[2]]);
    let second = family(&mut store, &[objects[2], objects[0], objects[3]]);
    let nested = family(&mut store, &[first, objects[4], second]);
    let last = family(&mut store, &[objects[5], first]);
    let empty = family(&mut store, &[]);
    let candidates = [
        first, second, nested, last, empty, objects[0], objects[1], objects[5],
    ];
    let root = family(&mut store, &[first, second, last]);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let owner = Rc::new(RefCell::new(store));
    let mut seed = 981827u64;
    for step in 0..4000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let target = candidates[(seed as usize >> 8) % candidates.len()];
        let mut tx = SemanticMutationTransaction::new();
        let members = owner.borrow().node(root).unwrap().members();
        match seed % 3 {
            0 => {
                tx.add_member(root, target);
            }
            1 => {
                tx.remove_member(root, target);
            }
            _ if members.contains(&target) => {
                let anchor = members
                    .get((seed >> 32) as usize % (members.len() + 1))
                    .copied();
                tx.reorder_member(root, target, anchor);
            }
            _ => {
                tx.add_member(root, target);
            }
        }
        LiveSession::new(&owner, root, &mut session)
            .apply(tx)
            .unwrap();
        assert_eq!(
            painter(&session),
            oracle(&owner.borrow(), root),
            "step={step}"
        );
        session.take_frame_changes();
    }
}
