//! Foreground declaration ownership only; ordering policy is a separate planner.
use crate::{
    SemanticMutationImpact, SemanticMutationTransaction as Transaction,
    SemanticMutationTransactionError as Error, SemanticNodeCreation, SemanticNodeId,
    SemanticObjectState, SemanticStore, SemanticTransactionNodeRef, SemanticTransactionReadError,
    StoredGeometry,
};

fn object(store: &mut SemanticStore) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }))
}

fn declare(store: &mut SemanticStore, root: SemanticNodeId, members: &[SemanticNodeId]) {
    let mut tx = Transaction::new();
    tx.set_foreground_members(root, members.iter().copied());
    tx.apply(store).unwrap();
}

#[test]
fn pending_scope_and_members_resolve_in_one_atomic_commit() {
    let mut store = SemanticStore::new();
    let revision = store.scene_revision();
    let mut tx = Transaction::new();
    let root = tx.create_node(SemanticNodeCreation::family());
    let child = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let group = tx.create_node(SemanticNodeCreation::family());
    tx.set_foreground_members(root, [child, group]);
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(
        prepared.foreground_members(root).unwrap(),
        [child.into(), group.into()]
    );
    assert_eq!(prepared.store().scene_revision(), revision);
    let result = prepared.commit();
    let root = result.resolve(root).unwrap();
    let child = result.resolve(child).unwrap();
    let group = result.resolve(group).unwrap();
    assert_eq!(
        store.node(root).unwrap().foreground_members(),
        &[child, group]
    );
    assert!(store.node(root).unwrap().members().is_empty());
    assert!(store.node(child).unwrap().parents().is_empty());
    assert!(store.node(group).unwrap().parents().is_empty());
    assert_eq!(
        result.impacts().last(),
        Some(&SemanticMutationImpact::ForegroundMembers { scope: root })
    );
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
}

#[test]
fn unchanged_declaration_is_exact_noop_but_order_is_significant() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let a = object(&mut store);
    let b = object(&mut store);
    store.add_member(root, a).unwrap();
    store.add_member(root, b).unwrap();
    declare(&mut store, root, &[a, b]);
    let revision = store.scene_revision();
    let mut same = Transaction::new();
    same.set_foreground_members(root, [a, b]);
    let prepared = same.prepare(&mut store).unwrap();
    assert_eq!(prepared.candidate_mutations().count(), 0);
    assert_eq!(prepared.proposed_scene_revision(), revision);
    assert!(prepared.commit().impacts().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
    declare(&mut store, root, &[b, a]);
    assert_eq!(store.node(root).unwrap().foreground_members(), &[b, a]);
    assert_eq!(store.node(root).unwrap().members(), &[a, b]);
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn failed_batch_preserves_existing_declarations_and_allocator() {
    for mode in 0..6 {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let a = object(&mut store);
        let b = object(&mut store);
        let invalid_signal = store.insert_semantic_input_signal(0.0_f64).unwrap();
        declare(&mut store, root, &[a]);
        let expected_next = store.preview_node_allocations().next().unwrap();
        let revision = store.scene_revision();
        let stats = store.last_mutation_stats();
        let mut tx = Transaction::new();
        tx.create_node(SemanticNodeCreation::family());
        match mode {
            0 => {
                tx.set_foreground_members(root, [a, a]);
            }
            1 => {
                tx.set_foreground_members(root, [root]);
            }
            2 => {
                tx.set_foreground_members(root, [SemanticNodeId::new(999_999, 0)]);
            }
            3 => {
                tx.set_foreground_members(root, [invalid_signal]);
            }
            4 => {
                tx.set_foreground_members(a, [b]);
            }
            _ => {
                tx.set_foreground_members(root, [b])
                    .set_foreground_members(root, [a]);
            }
        }
        assert!(tx.prepare(&mut store).is_err(), "mode={mode}");
        assert_eq!(store.node(root).unwrap().foreground_members(), &[a]);
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.last_mutation_stats(), stats);
        assert_eq!(store.preview_node_allocations().next(), Some(expected_next));
    }
}

#[test]
fn abandoned_preparation_does_not_publish_new_identities_or_references() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let a = object(&mut store);
    declare(&mut store, root, &[a]);
    let revision = store.scene_revision();
    let stats = store.last_mutation_stats();
    let next = store.preview_node_allocations().next();
    let mut tx = Transaction::new();
    let pending = tx.create_node(SemanticNodeCreation::family());
    tx.set_foreground_members(root, [pending]);
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(prepared.foreground_members(root).unwrap(), [pending.into()]);
    assert_eq!(
        prepared.store().node(root).unwrap().foreground_members(),
        &[a]
    );
    drop(prepared);
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats(), stats);
    assert_eq!(store.preview_node_allocations().next(), next);
    assert_eq!(store.node(root).unwrap().foreground_members(), &[a]);
}

#[test]
fn declarations_cannot_refer_to_existing_or_pending_nodes_removed_in_the_batch() {
    for pending in [false, true] {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let a = object(&mut store);
        let revision = store.scene_revision();
        let mut tx = Transaction::new();
        let removed: SemanticTransactionNodeRef = if pending {
            tx.create_node(SemanticNodeCreation::family()).into()
        } else {
            a.into()
        };
        tx.set_foreground_members(root, [removed])
            .remove_node(removed);
        assert!(matches!(
            tx.apply(&mut store),
            Err(Error::ForegroundUsesRemovedNode { .. })
        ));
        assert_eq!(store.scene_revision(), revision);
        assert!(store.node(root).unwrap().foreground_members().is_empty());
        assert!(store.node(a).is_some());
    }
}

#[test]
fn foreign_pending_tokens_and_non_authoring_pending_nodes_are_rejected() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let mut other = Transaction::new();
    let foreign = other.create_node(SemanticNodeCreation::family());
    let mut tx = Transaction::new();
    tx.set_foreground_members(root, [foreign]);
    assert!(matches!(
        tx.apply(&mut store),
        Err(Error::PendingNodeFromDifferentTransaction { .. })
    ));
    let mut tx = Transaction::new();
    let signal = tx.create_node(SemanticNodeCreation::input_signal(1.0_f64).unwrap());
    tx.set_foreground_members(root, [signal]);
    assert!(matches!(
        tx.apply(&mut store),
        Err(Error::PendingNodeKindMismatch { .. })
    ));
    assert!(store.node(root).unwrap().foreground_members().is_empty());
}

#[test]
fn deletion_removes_only_indexed_soft_references_and_preserves_order() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let alias = store.insert_family();
    let a = object(&mut store);
    let removed = object(&mut store);
    let b = object(&mut store);
    declare(&mut store, root, &[a, removed, b]);
    declare(&mut store, alias, &[removed]);
    let untouched: Vec<_> = (0..512)
        .map(|_| {
            let scope = store.insert_family();
            declare(&mut store, scope, &[a, b]);
            (scope, store.node(scope).unwrap().clone())
        })
        .collect();
    let mut tx = Transaction::new();
    tx.remove_node(removed);
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(
        prepared.foreground_members(root).unwrap(),
        [a.into(), b.into()]
    );
    assert!(prepared.foreground_members(alias).unwrap().is_empty());
    assert_eq!(
        prepared.store().node(root).unwrap().foreground_members(),
        &[a, removed, b]
    );
    let result = prepared.commit();
    assert!(store.node(removed).is_none());
    assert_eq!(store.node(root).unwrap().foreground_members(), &[a, b]);
    assert!(store.node(alias).unwrap().foreground_members().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 3);
    assert_eq!(result.impacts().len(), 3);
    for scope in [root, alias] {
        assert!(result
            .impacts()
            .contains(&SemanticMutationImpact::ForegroundMembers { scope }));
    }
    for (scope, before) in untouched {
        assert_eq!(store.node(scope), Some(&before));
    }
    let reused = object(&mut store);
    assert_eq!(reused.slot(), removed.slot());
    assert_ne!(reused, removed);
    assert!(!store
        .node(root)
        .unwrap()
        .foreground_members()
        .contains(&reused));
}

#[test]
fn replacing_declarations_unregisters_old_reverse_references() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let a = object(&mut store);
    let b = object(&mut store);
    declare(&mut store, root, &[a]);
    declare(&mut store, root, &[b]);
    let mut tx = Transaction::new();
    tx.remove_node(a);
    let result = tx.apply(&mut store).unwrap();
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::NodeRemoved { node: a }]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(store.node(root).unwrap().foreground_members(), &[b]);
    declare(&mut store, root, &[]);
    let mut tx = Transaction::new();
    tx.remove_node(b);
    assert_eq!(
        tx.apply(&mut store).unwrap().impacts(),
        &[SemanticMutationImpact::NodeRemoved { node: b }]
    );
}

#[test]
fn deleting_scope_does_not_delete_targets_or_attach_refs_to_reused_scope_slot() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let target = object(&mut store);
    declare(&mut store, root, &[target]);
    let mut tx = Transaction::new();
    tx.remove_node(root);
    tx.apply(&mut store).unwrap();
    assert!(store.node(target).is_some());
    let reused = store.insert_family();
    assert_eq!(reused.slot(), root.slot());
    let mut tx = Transaction::new();
    tx.remove_node(target);
    let result = tx.apply(&mut store).unwrap();
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::NodeRemoved { node: target }]
    );
    assert!(store.node(reused).unwrap().foreground_members().is_empty());
}

#[test]
fn prepared_reads_reject_wrong_kind_removed_and_foreign_scope() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let target = object(&mut store);
    let mut foreign = Transaction::new();
    let foreign_root = foreign.create_node(SemanticNodeCreation::family());
    let mut tx = Transaction::new();
    let pending_target = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    tx.remove_node(root);
    let prepared = tx.prepare(&mut store).unwrap();
    assert!(matches!(
        prepared.foreground_members(root),
        Err(SemanticTransactionReadError::RemovedExistingNode(_))
    ));
    assert!(matches!(
        prepared.foreground_members(target),
        Err(SemanticTransactionReadError::NotFamily(_))
    ));
    assert!(matches!(
        prepared.foreground_members(pending_target),
        Err(SemanticTransactionReadError::NotFamily(_))
    ));
    assert!(matches!(
        prepared.foreground_members(foreign_root),
        Err(SemanticTransactionReadError::PendingNodeFromDifferentTransaction(_))
    ));
}

#[test]
fn metadata_and_display_membership_compose_but_are_not_the_same_relation() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let target = object(&mut store);
    let mut tx = Transaction::new();
    tx.add_member(root, target)
        .set_foreground_members(root, [target]);
    tx.apply(&mut store).unwrap();
    assert_eq!(store.node(root).unwrap().members(), &[target]);
    assert_eq!(store.node(target).unwrap().parents(), &[root]);
    declare(&mut store, root, &[]);
    assert_eq!(store.node(root).unwrap().members(), &[target]);
    assert_eq!(store.node(target).unwrap().parents(), &[root]);
}
