use super::*;

#[test]
fn pending_signal_and_scope_publish_once_and_read_through_prepared_view() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    let signal = transaction.create_node(SemanticNodeCreation::input_signal(1.25_f64).unwrap());
    transaction.scope_signal(root, signal);

    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(prepared.scoped_signals(root).unwrap(), vec![signal.into()]);
    let result = prepared.commit();
    let signal = result.resolve(signal).unwrap();

    assert_eq!(store.semantic_scoped_signals(root).unwrap(), &[signal]);
    assert_eq!(
        store.semantic_input_scalar_value_at(signal, 0.0).unwrap(),
        1.25
    );
    assert_eq!(store.scene_revision().get(), revision.get() + 1);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::NodeAdded { node: signal },
            SemanticMutationImpact::SignalScoped {
                scope: root,
                signal
            },
        ]
    );
}

#[test]
fn signal_scope_is_idempotent_and_rejects_foreign_pending_identity() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let signal = store.insert_semantic_input_signal(0.0_f64).unwrap();
    let mut first = SemanticMutationTransaction::new();
    first.scope_signal(root, signal);
    first.apply(&mut store).unwrap();
    let revision = store.scene_revision();

    let mut duplicate = SemanticMutationTransaction::new();
    duplicate
        .scope_signal(root, signal)
        .scope_signal(root, signal);
    let result = duplicate.apply(&mut store).unwrap();
    assert!(result.impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);

    let mut owner = SemanticMutationTransaction::new();
    let foreign = owner.create_node(SemanticNodeCreation::input_signal(2.0_f64).unwrap());
    let mut misuse = SemanticMutationTransaction::new();
    misuse.scope_signal(root, foreign);
    assert!(matches!(
        misuse.apply(&mut store),
        Err(SemanticMutationTransactionError::PendingNodeFromDifferentTransaction { .. })
    ));
}

#[test]
fn removing_signal_cleans_only_its_indexed_scope_edges() {
    let mut store = SemanticStore::new();
    let first = store.insert_family();
    let second = store.insert_family();
    let signal = store.insert_semantic_input_signal(0.0_f64).unwrap();
    let other = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let mut scope = SemanticMutationTransaction::new();
    scope
        .scope_signal(first, signal)
        .scope_signal(first, other)
        .scope_signal(second, signal);
    scope.apply(&mut store).unwrap();

    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(signal);
    remove.apply(&mut store).unwrap();

    assert_eq!(store.semantic_scoped_signals(first).unwrap(), &[other]);
    assert!(store.semantic_scoped_signals(second).unwrap().is_empty());
    assert!(store.semantic_signal_state(other).is_ok());
}

#[test]
fn signal_scope_publication_writes_only_the_selected_root() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let selected = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let mut populate = SemanticMutationTransaction::new();
    for value in 0..512 {
        let unrelated = store
            .insert_semantic_input_signal(f64::from(value))
            .unwrap();
        populate.scope_signal(root, unrelated);
    }
    populate.apply(&mut store).unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.scope_signal(root, selected);
    transaction.apply(&mut store).unwrap();
    assert_eq!(store.last_mutation_stats().slots_written, 1);

    let revision = store.scene_revision();
    let mut duplicate = SemanticMutationTransaction::new();
    duplicate.scope_signal(root, selected);
    duplicate.apply(&mut store).unwrap();
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}
