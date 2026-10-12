use super::*;
use crate::{SemanticObjectState, StoredGeometry};

fn object(store: &mut SemanticStore) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }))
}

fn family(store: &mut SemanticStore, members: &[SemanticNodeId]) -> SemanticNodeId {
    let root = store.insert_family();
    for &member in members {
        store.add_member(root, member).unwrap();
    }
    root
}

fn edit(
    store: &mut SemanticStore,
    root: SemanticNodeId,
    request: SemanticSceneMembershipRequest<'_>,
) {
    plan_semantic_scene_membership(store, root, request)
        .unwrap()
        .apply(store)
        .unwrap();
}

fn assert_lists(
    store: &SemanticStore,
    root: SemanticNodeId,
    display: &[SemanticNodeId],
    foreground: &[SemanticNodeId],
) {
    let node = store.node(root).unwrap();
    assert_eq!(node.members(), display);
    assert_eq!(node.foreground_members(), foreground);
}

#[test]
fn foreground_add_is_atomic_ordered_and_persists_across_ordinary_adds() {
    let mut store = SemanticStore::new();
    let a = object(&mut store);
    let b = object(&mut store);
    let c = object(&mut store);
    let root = family(&mut store, &[a, b]);
    let before = store.scene_revision();
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[a, c]),
    );
    assert_lists(&store, root, &[b, a, c], &[a, c]);
    assert_eq!(store.scene_revision(), before.checked_next().unwrap());
    edit(&mut store, root, SemanticSceneMembershipRequest::Add(&[b]));
    assert_lists(&store, root, &[b, a, c], &[a, c]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[a]),
    );
    assert_lists(&store, root, &[b, c, a], &[c, a]);
    let revision = store.scene_revision();
    let result = plan_semantic_scene_membership(
        &store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[a]),
    )
    .unwrap()
    .apply(&mut store)
    .unwrap();
    assert!(result.impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn removing_foreground_only_keeps_display_and_family_identity() {
    let mut store = SemanticStore::new();
    let a = object(&mut store);
    let b = object(&mut store);
    let later = object(&mut store);
    let group = family(&mut store, &[a, b]);
    let root = family(&mut store, &[]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[group]),
    );
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::RemoveForeground(&[a]),
    );
    assert_lists(&store, root, &[group], &[b]);
    assert_eq!(store.node(group).unwrap().members(), &[a, b]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Add(&[later]),
    );
    assert_lists(&store, root, &[a, later, b], &[b]);
    // A group removal also demotes descendants already promoted out of that group.
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::RemoveForeground(&[group]),
    );
    assert_lists(&store, root, &[a, later, b], &[]);
}

#[test]
fn nested_removal_promotes_only_survivors_without_resurrection() {
    let mut store = SemanticStore::new();
    let a = object(&mut store);
    let b = object(&mut store);
    let c = object(&mut store);
    let later = object(&mut store);
    let inner = family(&mut store, &[a, b]);
    let outer = family(&mut store, &[inner, c]);
    let root = family(&mut store, &[]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[outer]),
    );
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Remove(&[a]),
    );
    assert_lists(&store, root, &[b, c], &[b, c]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Add(&[later]),
    );
    assert_lists(&store, root, &[later, b, c], &[b, c]);
    assert_eq!(store.node(inner).unwrap().members(), &[a, b]);
    assert_eq!(store.node(outer).unwrap().members(), &[inner, c]);
    assert!(!semantic_scene_root_contains(&store, root, a).unwrap());
}

#[test]
fn back_and_clear_retire_foreground_status_in_the_same_transaction() {
    let mut store = SemanticStore::new();
    let a = object(&mut store);
    let b = object(&mut store);
    let c = object(&mut store);
    let root = family(&mut store, &[]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[a, b]),
    );
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::BringToBack(&[b]),
    );
    assert_lists(&store, root, &[b, a], &[a]);
    edit(&mut store, root, SemanticSceneMembershipRequest::Add(&[c]));
    assert_lists(&store, root, &[b, c, a], &[a]);
    edit(&mut store, root, SemanticSceneMembershipRequest::Clear);
    assert_lists(&store, root, &[], &[]);
    edit(&mut store, root, SemanticSceneMembershipRequest::Add(&[c]));
    assert_lists(&store, root, &[c], &[]);
}

#[test]
fn replacement_preserves_foreground_position_and_never_revives_source() {
    let mut store = SemanticStore::new();
    let a = object(&mut store);
    let b = object(&mut store);
    let new = object(&mut store);
    let later = object(&mut store);
    let group = family(&mut store, &[a, b]);
    let root = family(&mut store, &[]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[group]),
    );
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Replace { old: a, new },
    );
    assert_lists(&store, root, &[new, b], &[new, b]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Add(&[later]),
    );
    assert_lists(&store, root, &[later, new, b], &[new, b]);
    assert_eq!(store.node(group).unwrap().members(), &[a, b]);
}

#[test]
fn failed_or_abandoned_preparation_changes_neither_list() {
    let mut store = SemanticStore::new();
    let a = object(&mut store);
    let b = object(&mut store);
    let root = family(&mut store, &[a]);
    let revision = store.scene_revision();
    let counters = store.last_mutation_stats();
    assert!(plan_semantic_scene_membership(
        &store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[b, b])
    )
    .is_err());
    assert_eq!(store.last_mutation_stats(), counters);
    let transaction = plan_semantic_scene_membership(
        &store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[b]),
    )
    .unwrap();
    {
        let _prepared = transaction.prepare(&mut store).unwrap();
    }
    assert_lists(&store, root, &[a], &[]);
    let mut transaction = plan_semantic_scene_membership(
        &store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[b]),
    )
    .unwrap();
    transaction.add_member(b, root); // invalid family edge, after valid foreground planning
    assert!(transaction.prepare(&mut store).is_err());
    assert_lists(&store, root, &[a], &[]);
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats(), counters);
}

#[test]
fn stale_and_self_targets_fail_without_declaration_or_display_changes() {
    let mut store = SemanticStore::new();
    let stale = object(&mut store);
    let root = family(&mut store, &[]);
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(stale);
    remove.apply(&mut store).unwrap();
    assert!(plan_semantic_scene_membership(
        &store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[stale])
    )
    .is_err());
    let transaction = plan_semantic_scene_membership(
        &store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[root]),
    );
    assert!(transaction.is_err() || transaction.unwrap().prepare(&mut store).is_err());
    assert_lists(&store, root, &[], &[]);
}

#[test]
fn local_foreground_reorder_does_not_plan_unrelated_display_members() {
    let mut store = SemanticStore::new();
    let a = object(&mut store);
    let root = family(&mut store, &[a]);
    for _ in 0..2_000 {
        let other = object(&mut store);
        store.add_member(root, other).unwrap();
    }
    let transaction = plan_semantic_scene_membership(
        &store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[a]),
    )
    .unwrap();
    // One existing root reorder plus one declaration edit; no unrelated membership edits.
    assert_eq!(transaction.mutations().len(), 2);
    transaction.apply(&mut store).unwrap();
    assert_eq!(store.node(root).unwrap().foreground_members(), &[a]);
    assert_eq!(store.node(root).unwrap().members().last(), Some(&a));
    assert_eq!(store.node(root).unwrap().member_count(), 2_001);
}

#[test]
fn replacing_partially_demoted_family_retires_descendant_declarations() {
    let mut store = SemanticStore::new();
    let a = object(&mut store);
    let b = object(&mut store);
    let new = object(&mut store);
    let later = object(&mut store);
    let group = family(&mut store, &[a, b]);
    let root = family(&mut store, &[]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[group]),
    );
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::RemoveForeground(&[b]),
    );
    assert_lists(&store, root, &[group], &[a]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Replace { old: group, new },
    );
    assert_lists(&store, root, &[new], &[]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Add(&[later]),
    );
    assert_lists(&store, root, &[new, later], &[]);
    assert!(!semantic_scene_root_contains(&store, root, a).unwrap());
    assert_eq!(store.node(group).unwrap().members(), &[a, b]);
}

#[test]
fn replacing_nested_family_keeps_only_unaffected_foreground_branches() {
    let mut store = SemanticStore::new();
    let [a, b, c, new, later] = std::array::from_fn(|_| object(&mut store));
    let inner = family(&mut store, &[a, b]);
    let outer = family(&mut store, &[inner, c]);
    let root = family(&mut store, &[]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[outer]),
    );
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::RemoveForeground(&[b]),
    );
    assert_lists(&store, root, &[outer], &[a, c]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Replace { old: inner, new },
    );
    assert_lists(&store, root, &[new, c], &[c]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Add(&[later]),
    );
    assert_lists(&store, root, &[new, later, c], &[c]);
    assert_eq!(store.node(inner).unwrap().members(), &[a, b]);
    assert_eq!(store.node(outer).unwrap().members(), &[inner, c]);
}

#[test]
fn replacing_family_preserves_declarations_that_survive_in_target() {
    for family_target in [false, true] {
        let mut store = SemanticStore::new();
        let [a, b, c, later] = std::array::from_fn(|_| object(&mut store));
        let group = family(&mut store, &[a, b]);
        let new = if family_target {
            family(&mut store, &[a, c])
        } else {
            a
        };
        let root = family(&mut store, &[]);
        edit(
            &mut store,
            root,
            SemanticSceneMembershipRequest::AddForeground(&[group]),
        );
        edit(
            &mut store,
            root,
            SemanticSceneMembershipRequest::RemoveForeground(&[b]),
        );
        edit(
            &mut store,
            root,
            SemanticSceneMembershipRequest::Replace { old: group, new },
        );
        assert_lists(&store, root, &[new], &[a]);
        edit(
            &mut store,
            root,
            SemanticSceneMembershipRequest::Add(&[later]),
        );
        let expected = if family_target {
            vec![c, later, a]
        } else {
            vec![later, a]
        };
        assert_lists(&store, root, &expected, &[a]);
        assert!(!semantic_scene_root_contains(&store, root, b).unwrap());
    }
}

#[test]
fn replacing_declared_family_prunes_independent_descendants_at_source_slot() {
    let mut store = SemanticStore::new();
    let [a, b, new, later] = std::array::from_fn(|_| object(&mut store));
    let group = family(&mut store, &[a, b]);
    let root = family(&mut store, &[]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[group]),
    );
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::RemoveForeground(&[b]),
    );
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[group]),
    );
    assert_lists(&store, root, &[a, group], &[a, group]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Replace { old: group, new },
    );
    assert_lists(&store, root, &[a, new], &[new]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Add(&[later]),
    );
    assert_lists(&store, root, &[a, later, new], &[new]);
}

#[test]
fn replacing_nonforeground_source_preserves_existing_foreground_target() {
    let mut store = SemanticStore::new();
    let [a, front, later] = std::array::from_fn(|_| object(&mut store));
    let root = family(&mut store, &[a]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[front]),
    );
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Replace { old: a, new: front },
    );
    assert_lists(&store, root, &[front], &[front]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Add(&[later]),
    );
    assert_lists(&store, root, &[later, front], &[front]);
}

#[test]
fn replacement_foreground_cleanup_is_atomic_when_preparation_fails() {
    let mut store = SemanticStore::new();
    let [a, b, new] = std::array::from_fn(|_| object(&mut store));
    let group = family(&mut store, &[a, b]);
    let root = family(&mut store, &[]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[group]),
    );
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::RemoveForeground(&[b]),
    );
    let revision = store.scene_revision();
    let counters = store.last_mutation_stats();
    let mut transaction = plan_semantic_scene_membership(
        &store,
        root,
        SemanticSceneMembershipRequest::Replace { old: group, new },
    )
    .unwrap();
    transaction.add_member(new, root);
    assert!(transaction.prepare(&mut store).is_err());
    assert_lists(&store, root, &[group], &[a]);
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats(), counters);
}

#[test]
fn lifecycle_membership_combines_removals_and_foreground_aware_admission() {
    let mut store = SemanticStore::new();
    let back = object(&mut store);
    let retired_leaf = object(&mut store);
    let retired = family(&mut store, &[retired_leaf]);
    let sibling = object(&mut store);
    let group = family(&mut store, &[retired, sibling]);
    let front = object(&mut store);
    let added = object(&mut store);
    let root = family(&mut store, &[back]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[group, front]),
    );
    let revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    stage_semantic_scene_lifecycle_membership(&store, root, &[retired], &[added], &mut transaction)
        .unwrap();
    assert_lists(&store, root, &[back, group, front], &[group, front]);
    // Preparing and dropping the complete change cannot demote persistence early.
    drop(transaction.prepare(&mut store).unwrap());
    assert_eq!(store.scene_revision(), revision);
    assert_lists(&store, root, &[back, group, front], &[group, front]);
    let mut transaction = SemanticMutationTransaction::new();
    stage_semantic_scene_lifecycle_membership(&store, root, &[retired], &[added], &mut transaction)
        .unwrap();
    transaction.apply(&mut store).unwrap();
    assert_lists(
        &store,
        root,
        &[back, added, sibling, front],
        &[sibling, front],
    );
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
    assert_eq!(store.node(group).unwrap().members(), &[retired, sibling]);
}

#[test]
fn lifecycle_membership_removal_without_admission_does_not_reorder_survivors() {
    let mut store = SemanticStore::new();
    let retired = object(&mut store);
    let front = object(&mut store);
    let ordinary = object(&mut store);
    let root = family(&mut store, &[retired, front, ordinary]);
    let mut declaration = SemanticMutationTransaction::new();
    declaration.set_foreground_members(root, [retired, front]);
    declaration.apply(&mut store).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    stage_semantic_scene_lifecycle_membership(&store, root, &[retired], &[], &mut transaction)
        .unwrap();
    transaction.apply(&mut store).unwrap();
    assert_lists(&store, root, &[front, ordinary], &[front]);
}

#[test]
fn lifecycle_membership_admission_reuses_an_existing_target_edge() {
    let mut store = SemanticStore::new();
    let source = object(&mut store);
    let target = object(&mut store);
    let front = object(&mut store);
    let root = family(&mut store, &[source, target]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[front]),
    );
    let mut transaction = SemanticMutationTransaction::new();
    stage_semantic_scene_lifecycle_membership(&store, root, &[source], &[target], &mut transaction)
        .unwrap();
    transaction.apply(&mut store).unwrap();
    assert_lists(&store, root, &[target, front], &[front]);
}

#[test]
fn lifecycle_membership_keeps_unrelated_roots_out_of_the_plan() {
    let mut store = SemanticStore::new();
    let source = object(&mut store);
    let target = object(&mut store);
    let front = object(&mut store);
    let unrelated = (0..2_000).map(|_| object(&mut store)).collect::<Vec<_>>();
    let root = family(&mut store, &unrelated);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::Add(&[source]),
    );
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[front]),
    );
    let mut transaction = SemanticMutationTransaction::new();
    stage_semantic_scene_lifecycle_membership(&store, root, &[source], &[target], &mut transaction)
        .unwrap();
    // Remove source, add target, and place target/front; no unrelated root edit.
    assert_eq!(transaction.mutations().len(), 4);
    transaction.apply(&mut store).unwrap();
    let expected = unrelated
        .iter()
        .copied()
        .chain([target, front])
        .collect::<Vec<_>>();
    assert_lists(&store, root, &expected, &[front]);
}

#[test]
fn lifecycle_membership_rejects_stale_admission_without_staging_removal() {
    let mut store = SemanticStore::new();
    let source = object(&mut store);
    let stale = object(&mut store);
    let root = family(&mut store, &[source]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[source]),
    );
    let mut deletion = SemanticMutationTransaction::new();
    deletion.remove_node(stale);
    deletion.apply(&mut store).unwrap();
    let revision = store.scene_revision();
    let counters = store.last_mutation_stats();
    let mut transaction = SemanticMutationTransaction::new();
    assert!(stage_semantic_scene_lifecycle_membership(
        &store,
        root,
        &[source],
        &[stale],
        &mut transaction,
    )
    .is_err());
    assert!(transaction.mutations().is_empty());
    assert_lists(&store, root, &[source], &[source]);
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats(), counters);
}

#[test]
fn staged_admission_preserves_mixed_existing_and_pending_order() {
    use crate::{SemanticNodeCreation, SemanticObjectState, StoredGeometry};
    let mut store = SemanticStore::new();
    let a = object(&mut store);
    let b = object(&mut store);
    let front = object(&mut store);
    let root = family(&mut store, &[front]);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[front]),
    );
    let mut transaction = SemanticMutationTransaction::new();
    let fresh = || {
        SemanticNodeCreation::object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 1.0,
        }))
    };
    let p = transaction.create_node(fresh());
    let q = transaction.create_node(fresh());
    stage_semantic_scene_admission(
        &store,
        root,
        &[a.into(), p.into(), b.into(), q.into()],
        &mut transaction,
    )
    .unwrap();
    assert_lists(&store, root, &[front], &[front]);
    let result = transaction.apply(&mut store).unwrap();
    assert_lists(
        &store,
        root,
        &[
            a,
            result.resolve(p).unwrap(),
            b,
            result.resolve(q).unwrap(),
            front,
        ],
        &[front],
    );
}

#[test]
fn staged_admission_rejects_foreign_family_and_duplicate_pending_tokens_before_staging() {
    use crate::{SemanticNodeCreation, SemanticObjectState, StoredGeometry};
    let mut store = SemanticStore::new();
    let front = object(&mut store);
    let root = family(&mut store, &[front]);
    let mut foreign = SemanticMutationTransaction::new();
    let token = foreign.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let mut transaction = SemanticMutationTransaction::new();
    assert!(
        stage_semantic_scene_admission(&store, root, &[token.into()], &mut transaction).is_err()
    );
    assert!(transaction.is_empty());
    let group = transaction.create_node(SemanticNodeCreation::family());
    let count = transaction.mutations().len();
    assert!(
        stage_semantic_scene_admission(&store, root, &[group.into()], &mut transaction).is_err()
    );
    assert_eq!(transaction.mutations().len(), count);
    let leaf = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let count = transaction.mutations().len();
    assert!(stage_semantic_scene_admission(
        &store,
        root,
        &[leaf.into(), leaf.into()],
        &mut transaction
    )
    .is_err());
    assert_eq!(transaction.mutations().len(), count);
    assert_eq!(store.node(root).unwrap().members(), [front]);
}

#[test]
fn staged_admission_projects_nested_foreground_without_touching_unrelated_roots() {
    let mut store = SemanticStore::new();
    let a = object(&mut store);
    let b = object(&mut store);
    let introduced = object(&mut store);
    let group = family(&mut store, &[a, b]);
    let unrelated = (0..2_000).map(|_| object(&mut store)).collect::<Vec<_>>();
    let root = family(&mut store, &unrelated);
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::AddForeground(&[group]),
    );
    edit(
        &mut store,
        root,
        SemanticSceneMembershipRequest::RemoveForeground(&[a]),
    );
    let mut transaction = SemanticMutationTransaction::new();
    stage_semantic_scene_admission(&store, root, &[introduced.into()], &mut transaction).unwrap();
    assert!(transaction.mutations().len() <= 8);
    transaction.apply(&mut store).unwrap();
    let expected = unrelated
        .into_iter()
        .chain([a, introduced, b])
        .collect::<Vec<_>>();
    assert_lists(&store, root, &expected, &[b]);
    assert_eq!(store.node(group).unwrap().members(), [a, b]);
}
