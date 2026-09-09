//! Root-order acceptance coverage for #1292 / the compiler extraction in #1305.
//! Raw fixture construction stays at the explicit integration boundary. Every
//! live edit uses the existing typed LiveSession and atomic publication path.

use std::{cell::RefCell, collections::HashSet, rc::Rc};

use noon::integration::{SemanticMutationTransaction, SemanticNodeKind, SemanticStore};
use noon::{ExecutionSession, ExecutionSessionPublicationError, LiveSession, LiveSessionError};
use noon_compile::{
    semantic_execution_object_id, SemanticLoweringError, SemanticPublicationLoweringError,
};
use noon_core::{
    plan_semantic_scene_membership, ObjectId, SemanticNodeId, SemanticObjectProperty,
    SemanticObjectState, SemanticSceneMembershipRequest, SemanticStoreError, SemanticVec3,
    StoredGeometry,
};

type Owner = Rc<RefCell<SemanticStore>>;

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

fn session(store: SemanticStore, root: SemanticNodeId) -> (Owner, ExecutionSession) {
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.take_frame_changes();
    (Rc::new(RefCell::new(store)), session)
}

fn painter_order(session: &ExecutionSession) -> Vec<ObjectId> {
    session
        .painter_order()
        .iter()
        .map(|&row| session.frame().objects[row as usize].id)
        .collect()
}

// Deliberately simple full traversal used only as a test oracle. Production
// root-order preparation must not rebuild this whole-root projection.
fn reference_order(store: &SemanticStore, root: SemanticNodeId) -> Vec<ObjectId> {
    fn visit(
        store: &SemanticStore,
        id: SemanticNodeId,
        seen: &mut HashSet<SemanticNodeId>,
        result: &mut Vec<ObjectId>,
    ) {
        if !seen.insert(id) {
            return;
        }
        let node = store.node(id).unwrap();
        match node.kind() {
            SemanticNodeKind::AuthoringObject => result.push(semantic_execution_object_id(id)),
            SemanticNodeKind::Family => {
                for child in node.members_iter() {
                    visit(store, child, seen, result);
                }
            }
            SemanticNodeKind::Signal(_) | SemanticNodeKind::Animation(_) => {}
        }
    }
    let mut result = Vec::new();
    visit(store, root, &mut HashSet::new(), &mut result);
    result
}

fn assert_reference(owner: &Owner, session: &ExecutionSession, root: SemanticNodeId) {
    assert_eq!(
        painter_order(session),
        reference_order(&owner.borrow(), root)
    );
    assert_eq!(session.last_patch_stats().full_seeks, 0);
    assert_eq!(session.last_patch_stats().full_group_rebuilds, 0);
}

fn edit(
    owner: &Owner,
    session: &mut ExecutionSession,
    root: SemanticNodeId,
    request: SemanticSceneMembershipRequest<'_>,
) {
    let transaction = plan_semantic_scene_membership(&owner.borrow(), root, request).unwrap();
    LiveSession::new(owner, root, session)
        .apply(transaction)
        .unwrap();
    assert_reference(owner, session, root);
}

#[test]
fn all_single_root_reorders_match_reference_with_empty_and_nested_families() {
    for moved in 0..5 {
        for anchor in 0..=5 {
            let mut store = SemanticStore::new();
            let a = object(&mut store);
            let b = object(&mut store);
            let c = object(&mut store);
            let d = object(&mut store);
            let empty = family(&mut store, &[]);
            let group = family(&mut store, &[b, c]);
            let nested_empty = family(&mut store, &[empty]);
            let members = [a, empty, group, nested_empty, d];
            let root = family(&mut store, &members);
            let (owner, mut live) = session(store, root);
            let rows = live
                .frame()
                .objects
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>();
            let before = members.get(anchor).copied();
            let mut transaction = SemanticMutationTransaction::new();
            transaction.reorder_member(root, members[moved], before);
            LiveSession::new(&owner, root, &mut live)
                .apply(transaction)
                .unwrap();
            let mut expected = members.to_vec();
            if before != Some(members[moved]) {
                expected.remove(moved);
                let position = before
                    .and_then(|before| expected.iter().position(|node| *node == before))
                    .unwrap_or(expected.len());
                expected.insert(position, members[moved]);
            }
            assert_eq!(owner.borrow().node(root).unwrap().members(), expected);
            assert_eq!(
                painter_order(&live),
                reference_order(&owner.borrow(), root),
                "move {moved} before {anchor}"
            );
            assert_eq!(
                live.frame()
                    .objects
                    .iter()
                    .map(|row| row.id)
                    .collect::<Vec<_>>(),
                rows
            );
            assert_eq!(live.last_patch_stats().full_seeks, 0);
            assert_eq!(live.last_patch_stats().full_group_rebuilds, 0);
        }
    }
}

#[test]
fn shared_membership_remove_replace_and_add_preserve_empty_boundary_order() {
    let mut store = SemanticStore::new();
    let [left, removed, survivor, right, x, y] = std::array::from_fn(|_| object(&mut store));
    let nested = family(&mut store, &[removed, survivor]);
    let empty = family(&mut store, &[]);
    let replacement = family(&mut store, &[x, y]);
    let root = family(&mut store, &[left, nested, empty, right]);
    let (owner, mut live) = session(store, root);
    let identity = live.runtime_identity();
    let left_slot = live.execution_slot_for_frame_index(0);

    edit(
        &owner,
        &mut live,
        root,
        SemanticSceneMembershipRequest::Remove(&[removed]),
    );
    assert_eq!(
        owner.borrow().node(nested).unwrap().members(),
        &[removed, survivor]
    );
    edit(
        &owner,
        &mut live,
        root,
        SemanticSceneMembershipRequest::Replace {
            old: survivor,
            new: replacement,
        },
    );
    edit(
        &owner,
        &mut live,
        root,
        SemanticSceneMembershipRequest::Add(&[y]),
    );
    edit(
        &owner,
        &mut live,
        root,
        SemanticSceneMembershipRequest::Remove(&[right]),
    );
    assert_eq!(live.runtime_identity(), identity);
    assert_eq!(live.execution_slot_for_frame_index(0), left_slot);
    edit(
        &owner,
        &mut live,
        root,
        SemanticSceneMembershipRequest::Clear,
    );
    assert!(painter_order(&live).is_empty());
    edit(
        &owner,
        &mut live,
        root,
        SemanticSceneMembershipRequest::Add(&[nested]),
    );
}

#[test]
fn empty_only_reorder_is_authored_only_and_exact_noop_publishes_nothing() {
    let mut store = SemanticStore::new();
    let a = object(&mut store);
    let b = object(&mut store);
    let empty = family(&mut store, &[]);
    let root = family(&mut store, &[a, empty, b]);
    let (owner, mut live) = session(store, root);
    let before = live.publication_context();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.reorder_member(root, empty, None);
    LiveSession::new(&owner, root, &mut live)
        .apply(transaction)
        .unwrap();
    let after = live.publication_context();
    assert_eq!(
        after.scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );
    assert_eq!(after.execution_revision(), before.execution_revision());
    assert_eq!(
        after.frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert!(live.take_frame_changes().is_empty());
    let mut noop = SemanticMutationTransaction::new();
    noop.reorder_member(root, empty, None);
    LiveSession::new(&owner, root, &mut live)
        .apply(noop)
        .unwrap();
    assert_eq!(live.publication_context(), after);
    assert!(live.take_frame_changes().is_empty());
}

#[test]
fn invalid_explicit_root_rejects_before_publication_and_valid_root_recovers() {
    for wrong_kind in [false, true] {
        let mut store = SemanticStore::new();
        let a = object(&mut store);
        let b = object(&mut store);
        let root = family(&mut store, &[a, b]);
        let invalid = if wrong_kind {
            a
        } else {
            SemanticNodeId::new(root.slot(), root.generation() + 1)
        };
        let (owner, mut live) = session(store, root);
        let before = live.publication_context();
        let frame = live.frame().clone();
        let slot = live.execution_slot_for_frame_index(0);
        let mut transaction = SemanticMutationTransaction::new();
        transaction.reorder_member(root, b, Some(a));
        let error = LiveSession::new(&owner, invalid, &mut live)
            .apply(transaction)
            .unwrap_err();
        let expected = if wrong_kind {
            SemanticStoreError::NotFamily(invalid)
        } else {
            SemanticStoreError::UnknownNode(invalid)
        };
        assert!(matches!(error,
            LiveSessionError::Publication(ExecutionSessionPublicationError::Lowering(
                SemanticPublicationLoweringError::Value(SemanticLoweringError::Store(error))
            )) if error == expected));
        assert_eq!(live.publication_context(), before);
        assert_eq!(owner.borrow().scene_revision(), before.scene_revision());
        assert_eq!(owner.borrow().node(root).unwrap().members(), &[a, b]);
        assert_eq!(live.frame(), &frame);
        assert_eq!(live.execution_slot_for_frame_index(0), slot);
        assert!(live.take_frame_changes().is_empty());
        let mut valid = SemanticMutationTransaction::new();
        valid.reorder_member(root, b, Some(a));
        LiveSession::new(&owner, root, &mut live)
            .apply(valid)
            .unwrap();
        assert_reference(&owner, &live, root);
    }
}

#[test]
fn failed_mixed_order_and_value_preparation_preserves_existing_dirty_state() {
    let mut store = SemanticStore::new();
    let a = object(&mut store);
    let b = object(&mut store);
    let root = family(&mut store, &[a, b]);
    let (owner, mut live) = session(store, root);
    let mut first = SemanticMutationTransaction::new();
    first.set_property(
        a,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(2.0, 0.0, 0.0),
    );
    LiveSession::new(&owner, root, &mut live)
        .apply(first)
        .unwrap();
    let before = live.publication_context();
    let frame = live.frame().clone();
    let mut invalid = SemanticMutationTransaction::new();
    invalid.reorder_member(root, b, Some(a));
    invalid.set_property(b, SemanticObjectProperty::RotationZ, f64::MAX);
    assert!(matches!(
        LiveSession::new(&owner, root, &mut live).apply(invalid),
        Err(LiveSessionError::Publication(
            ExecutionSessionPublicationError::Lowering(_)
        ))
    ));
    assert_eq!(live.publication_context(), before);
    assert_eq!(owner.borrow().scene_revision(), before.scene_revision());
    assert_eq!(owner.borrow().node(root).unwrap().members(), &[a, b]);
    assert_eq!(live.frame(), &frame);
    let changes = live.take_frame_changes();
    assert_eq!(changes.object_indices(), &[0]);
    assert_eq!(changes.painter_order_range(), None);
    let mut valid = SemanticMutationTransaction::new();
    valid.reorder_member(root, b, Some(a));
    LiveSession::new(&owner, root, &mut live)
        .apply(valid)
        .unwrap();
    assert_reference(&owner, &live, root);
}

#[test]
fn alias_membership_edits_match_first_occurrence_projection_without_identity_churn() {
    let mut store = SemanticStore::new();
    let shared = object(&mut store);
    let a = object(&mut store);
    let b = object(&mut store);
    let left = family(&mut store, &[shared, a]);
    let right = family(&mut store, &[shared, b]);
    let root = family(&mut store, &[left, right]);
    let (owner, mut live) = session(store, root);
    assert_reference(&owner, &live, root);
    edit(
        &owner,
        &mut live,
        root,
        SemanticSceneMembershipRequest::Remove(&[shared]),
    );
    edit(
        &owner,
        &mut live,
        root,
        SemanticSceneMembershipRequest::Add(&[left]),
    );
    edit(
        &owner,
        &mut live,
        root,
        SemanticSceneMembershipRequest::Remove(&[left]),
    );
    edit(
        &owner,
        &mut live,
        root,
        SemanticSceneMembershipRequest::Add(&[right]),
    );
    assert_eq!(owner.borrow().node(left).unwrap().members(), &[shared, a]);
    assert_eq!(owner.borrow().node(right).unwrap().members(), &[shared, b]);
}

#[test]
fn ordered_publication_advances_revisions_once_and_survives_forward_and_backward_seek() {
    let mut store = SemanticStore::new();
    let [a, b, c] = std::array::from_fn(|_| object(&mut store));
    let root = family(&mut store, &[a, b, c]);
    let (owner, mut live) = session(store, root);
    let before = live.publication_context();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.reorder_member(root, c, Some(a));
    LiveSession::new(&owner, root, &mut live)
        .apply(transaction)
        .unwrap();
    let after = live.publication_context();
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
    let changes = live.take_frame_changes();
    assert_eq!(changes.painter_order_range(), Some(0..3));
    assert!(changes.object_indices().is_empty());
    for time in [1.0, 2.0, 0.0, 1.0] {
        live.seek(time).unwrap();
        assert_eq!(painter_order(&live), reference_order(&owner.borrow(), root));
        assert_eq!(
            live.publication_context().scene_revision(),
            after.scene_revision()
        );
        assert_eq!(
            live.publication_context().execution_revision(),
            after.execution_revision()
        );
    }
    let context = live.publication_context();
    let mut noop = SemanticMutationTransaction::new();
    noop.reorder_member(root, c, Some(a));
    LiveSession::new(&owner, root, &mut live)
        .apply(noop)
        .unwrap();
    assert_eq!(live.publication_context(), context);
}
