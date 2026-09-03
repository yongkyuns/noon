use super::*;
use crate::{
    AnimationOptions, SemanticObjectState, SemanticSignalExpr, SemanticSignalSource, StoredGeometry,
};

fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
}

fn input_scalar(store: &SemanticStore, signal: SemanticNodeId) -> f64 {
    let SemanticSignalSource::Input(SemanticSignalValue::Scalar(value)) =
        store.semantic_signal_state(signal).unwrap().source()
    else {
        panic!("expected scalar input signal")
    };
    *value
}

#[test]
fn remove_node_unbinds_properties_and_cascades_invalid_derived_signals() {
    let mut store = SemanticStore::new();
    let source = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let derived = store
        .insert_semantic_derived_signal(SemanticSignalExpr::Add(
            Box::new(SemanticSignalExpr::signal(source)),
            Box::new(SemanticSignalExpr::scalar(2.0)),
        ))
        .unwrap();
    let target = object(&mut store, 1.0);
    store
        .bind_semantic_signal(derived, target, SemanticObjectProperty::ObjectOpacity)
        .unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_node(source);
    let result = transaction.apply(&mut store).unwrap();

    assert!(store.node(source).is_none());
    assert!(store.node(derived).is_none());
    assert!(store.node(target).is_some());
    assert!(store
        .semantic_object_signal_bindings(target)
        .unwrap()
        .is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 3);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::NodeRemoved { node: source },
            SemanticMutationImpact::NodeRemoved { node: derived },
            SemanticMutationImpact::Subscription {
                object: target,
                property: SemanticObjectProperty::ObjectOpacity,
            },
        ]
    );
}

#[test]
fn removing_animation_target_cascades_leaf_and_parent_composition_only() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let leaf = store
        .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
        .unwrap();

    let other_target = object(&mut store, 3.0);
    let other_target_state = object(&mut store, 4.0);
    let other_leaf = store
        .insert_semantic_transform_animation(
            other_target,
            other_target_state,
            AnimationOptions::new(),
        )
        .unwrap();
    let composition = store
        .insert_semantic_parallel_animation(&[leaf, other_leaf], AnimationOptions::new())
        .unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_node(target);
    let result = transaction.apply(&mut store).unwrap();

    assert!(store.node(target).is_none());
    assert!(store.node(leaf).is_none());
    assert!(store.node(composition).is_none());
    assert!(store.node(target_state).is_some());
    assert!(store.node(other_target).is_some());
    assert!(store.node(other_target_state).is_some());
    assert!(store.node(other_leaf).is_some());
    assert_eq!(store.last_mutation_stats().slots_written, 3);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::NodeRemoved { node: target },
            SemanticMutationImpact::NodeRemoved { node: leaf },
            SemanticMutationImpact::NodeRemoved { node: composition },
        ]
    );
}

#[test]
fn stale_remove_target_rolls_back_earlier_valid_mutation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let stale = object(&mut store, 1.0);
    store.remove_node(stale).unwrap();
    let replacement = object(&mut store, 2.0);
    assert_eq!(stale.slot(), replacement.slot());
    assert_ne!(stale.generation(), replacement.generation());

    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_signal(signal, 2.0_f64).remove_node(stale);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Node {
            index: 1,
            error: SemanticStoreError::UnknownNode(stale),
        })
    );
    assert_eq!(input_scalar(&store, signal), 1.0);
    assert!(store.node(replacement).is_some());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn structural_removals_are_terminal_and_preflighted_before_commit() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_node(target).set_signal(signal, 2.0_f64);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::MutationAfterRemove { index: 1 })
    );
    assert!(store.node(target).is_some());
    assert_eq!(input_scalar(&store, signal), 1.0);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn transaction_rejects_mutating_a_node_it_also_removes() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_property(target, SemanticObjectProperty::RotationZ, 0.5_f64)
        .remove_node(target);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::TargetRemoved { index: 0, target })
    );
    assert!(store.node(target).is_some());
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .transform
            .rotation_z,
        0.0
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn transaction_rejects_binding_a_signal_it_also_removes() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
    let target = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .change_subscription(target, SemanticObjectProperty::ObjectOpacity, Some(signal))
        .remove_node(signal);

    assert_eq!(
        transaction.apply(&mut store),
        Err(
            SemanticMutationTransactionError::SubscriptionUsesRemovedSignal {
                index: 0,
                object: target,
                property: SemanticObjectProperty::ObjectOpacity,
                signal,
            }
        )
    );
    assert!(store.node(signal).is_some());
    assert!(store
        .semantic_object_signal_bindings(target)
        .unwrap()
        .is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn transaction_rejects_binding_a_signal_removed_by_later_cascade() {
    let mut store = SemanticStore::new();
    let source = store.insert_semantic_input_signal(0.5_f64).unwrap();
    let derived = store
        .insert_semantic_derived_signal(SemanticSignalExpr::signal(source))
        .unwrap();
    let target = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .change_subscription(target, SemanticObjectProperty::ObjectOpacity, Some(derived))
        .remove_node(source);

    assert_eq!(
        transaction.apply(&mut store),
        Err(
            SemanticMutationTransactionError::SubscriptionUsesRemovedSignal {
                index: 0,
                object: target,
                property: SemanticObjectProperty::ObjectOpacity,
                signal: derived,
            }
        )
    );
    assert!(store.node(source).is_some());
    assert!(store.node(derived).is_some());
    assert!(store
        .semantic_object_signal_bindings(target)
        .unwrap()
        .is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn subscription_rebind_moves_reverse_reference_to_the_new_signal() {
    let mut store = SemanticStore::new();
    let first = store.insert_semantic_input_signal(0.25_f64).unwrap();
    let second = store.insert_semantic_input_signal(0.75_f64).unwrap();
    let target = object(&mut store, 1.0);
    store
        .bind_semantic_signal(first, target, SemanticObjectProperty::ObjectOpacity)
        .unwrap();

    let mut rebind = SemanticMutationTransaction::new();
    rebind.change_subscription(target, SemanticObjectProperty::ObjectOpacity, Some(second));
    rebind.apply(&mut store).unwrap();

    let mut remove_first = SemanticMutationTransaction::new();
    remove_first.remove_node(first);
    let first_result = remove_first.apply(&mut store).unwrap();
    assert_eq!(
        store.semantic_object_signal_bindings(target).unwrap()[0].signal(),
        second
    );
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(
        first_result.impacts(),
        &[SemanticMutationImpact::NodeRemoved { node: first }]
    );

    let mut remove_second = SemanticMutationTransaction::new();
    remove_second.remove_node(second);
    let second_result = remove_second.apply(&mut store).unwrap();
    assert!(store
        .semantic_object_signal_bindings(target)
        .unwrap()
        .is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(
        second_result.impacts(),
        &[
            SemanticMutationImpact::NodeRemoved { node: second },
            SemanticMutationImpact::Subscription {
                object: target,
                property: SemanticObjectProperty::ObjectOpacity,
            },
        ]
    );
}

#[test]
fn signal_source_rewire_moves_reverse_dependency_to_the_new_source() {
    let mut store = SemanticStore::new();
    let dependency = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let target = store.insert_semantic_input_signal(2.0_f64).unwrap();
    store
        .set_semantic_signal_source(
            target,
            SemanticSignalSource::Derived(SemanticSignalExpr::signal(dependency)),
        )
        .unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_node(dependency);
    let result = transaction.apply(&mut store).unwrap();

    assert!(store.node(dependency).is_none());
    assert!(store.node(target).is_none());
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::NodeRemoved { node: dependency },
            SemanticMutationImpact::NodeRemoved { node: target },
        ]
    );
}

#[test]
fn remove_node_cleanup_is_local_with_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let source = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let derived = store
        .insert_semantic_derived_signal(SemanticSignalExpr::signal(source))
        .unwrap();
    let target = object(&mut store, 0.5);
    store
        .bind_semantic_signal(derived, target, SemanticObjectProperty::ObjectOpacity)
        .unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_node(source);
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(store.last_mutation_stats().slots_written, 3);
    assert_eq!(result.impacts().len(), 3);
    assert_eq!(store.len(), 10_001);
}
