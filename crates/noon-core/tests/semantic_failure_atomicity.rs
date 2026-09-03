use noon_core::{SemanticStore, SemanticStoreError, SourceIdentity};

#[test]
fn rejected_family_cycle_leaves_semantic_graph_unchanged() {
    let mut store = SemanticStore::new();
    let outer = store.insert_family();
    let inner = store.insert_family();
    let leaf = store.insert_family();

    store.add_member(outer, inner).unwrap();
    store.add_member(inner, leaf).unwrap();

    let before_outer_members = store.node(outer).unwrap().members();
    let before_inner_members = store.node(inner).unwrap().members();
    let before_leaf_members = store.node(leaf).unwrap().members();
    let before_outer_parents = store.node(outer).unwrap().parents().to_vec();
    let before_inner_parents = store.node(inner).unwrap().parents().to_vec();
    let before_leaf_parents = store.node(leaf).unwrap().parents().to_vec();
    let before_len = store.len();
    let before_roots = store.scene_roots().collect::<Vec<_>>();

    assert!(matches!(
        store.add_member(leaf, outer),
        Err(SemanticStoreError::FamilyCycle { family, member })
            if family == leaf && member == outer
    ));

    assert_eq!(store.node(outer).unwrap().members(), before_outer_members);
    assert_eq!(store.node(inner).unwrap().members(), before_inner_members);
    assert_eq!(store.node(leaf).unwrap().members(), before_leaf_members);
    assert_eq!(store.node(outer).unwrap().parents(), before_outer_parents);
    assert_eq!(store.node(inner).unwrap().parents(), before_inner_parents);
    assert_eq!(store.node(leaf).unwrap().parents(), before_leaf_parents);
    assert_eq!(store.len(), before_len);
    assert_eq!(store.scene_roots().collect::<Vec<_>>(), before_roots);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
    assert!(store.last_mutation_stats().cycle_nodes_visited > 0);
}

#[test]
fn duplicate_source_identity_rejection_preserves_both_bindings() {
    let mut store = SemanticStore::new();
    let first = store.insert_family();
    let second = store.insert_family();
    let first_source = SourceIdentity::ExplicitKey("first".into());
    let second_source = SourceIdentity::ExplicitKey("second".into());

    store
        .set_source_identity(first, Some(first_source.clone()))
        .unwrap();
    store
        .set_source_identity(second, Some(second_source.clone()))
        .unwrap();

    let before_first = store.node(first).unwrap().source_identity().cloned();
    let before_second = store.node(second).unwrap().source_identity().cloned();
    let before_len = store.len();

    assert!(matches!(
        store.set_source_identity(second, Some(first_source.clone())),
        Err(SemanticStoreError::DuplicateSourceIdentity(source)) if source == first_source
    ));

    assert_eq!(
        store.node(first).unwrap().source_identity(),
        before_first.as_ref()
    );
    assert_eq!(
        store.node(second).unwrap().source_identity(),
        before_second.as_ref()
    );
    assert_eq!(store.node_for_source(&first_source), Some(first));
    assert_eq!(store.node_for_source(&second_source), Some(second));
    assert_eq!(store.len(), before_len);
}
