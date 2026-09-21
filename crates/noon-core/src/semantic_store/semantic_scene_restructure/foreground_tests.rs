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
