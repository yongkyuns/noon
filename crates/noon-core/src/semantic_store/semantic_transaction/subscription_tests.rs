use super::*;
use crate::{SemanticObjectState, SemanticVec3, StoredGeometry};

fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
}

fn scalar_input(store: &SemanticStore, signal: SemanticNodeId) -> f64 {
    let SemanticSignalSource::Input(SemanticSignalValue::Scalar(value)) =
        store.semantic_signal_state(signal).unwrap().source()
    else {
        panic!("expected scalar input signal")
    };
    *value
}

#[test]
fn mixed_value_property_and_subscription_changes_commit_together() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
    let target = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 0.75_f64)
        .set_property(target, SemanticObjectProperty::RotationZ, 0.5_f64)
        .change_subscription(target, SemanticObjectProperty::ObjectOpacity, Some(signal));

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(scalar_input(&store, signal), 0.75);
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .transform
            .rotation_z,
        0.5
    );
    assert_eq!(
        store.semantic_object_signal_bindings(target).unwrap(),
        &[SemanticSignalBinding::new(
            signal,
            SemanticObjectProperty::ObjectOpacity,
        )]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::SignalValue { signal },
            SemanticMutationImpact::ObjectProperty {
                object: target,
                property: SemanticObjectProperty::RotationZ,
            },
            SemanticMutationImpact::Subscription {
                object: target,
                property: SemanticObjectProperty::ObjectOpacity,
            },
        ]
    );
}

#[test]
fn invalid_late_subscription_rolls_back_earlier_changes() {
    let mut store = SemanticStore::new();
    let scalar = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let vector = store
        .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
        .unwrap();
    let target = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(scalar, 2.0_f64)
        .set_property(target, SemanticObjectProperty::RotationZ, 0.5_f64)
        .change_subscription(target, SemanticObjectProperty::ObjectOpacity, Some(vector));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::SubscriptionTypeMismatch {
            index: 2,
            object: target,
            property: SemanticObjectProperty::ObjectOpacity,
            signal: vector,
            expected: SemanticSignalValueKind::Scalar,
            actual: SemanticSignalValueKind::Vec3,
        })
    );
    assert_eq!(scalar_input(&store, scalar), 1.0);
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .transform
            .rotation_z,
        0.0
    );
    assert!(store
        .semantic_object_signal_bindings(target)
        .unwrap()
        .is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn rebind_preserves_existing_subscription_order() {
    let mut store = SemanticStore::new();
    let first = store.insert_semantic_input_signal(0.25_f64).unwrap();
    let replacement = store.insert_semantic_input_signal(0.75_f64).unwrap();
    let second = store.insert_semantic_input_signal(2.0_f64).unwrap();
    let target = object(&mut store, 1.0);
    store
        .bind_semantic_signal(first, target, SemanticObjectProperty::ObjectOpacity)
        .unwrap();
    store
        .bind_semantic_signal(second, target, SemanticObjectProperty::StrokeWidth)
        .unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.change_subscription(
        target,
        SemanticObjectProperty::ObjectOpacity,
        Some(replacement),
    );
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        store.semantic_object_signal_bindings(target).unwrap(),
        &[
            SemanticSignalBinding::new(replacement, SemanticObjectProperty::ObjectOpacity),
            SemanticSignalBinding::new(second, SemanticObjectProperty::StrokeWidth),
        ]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::Subscription {
            object: target,
            property: SemanticObjectProperty::ObjectOpacity,
        }]
    );
}

#[test]
fn unbind_cleans_stale_source_and_missing_unbind_is_a_noop() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
    let target = object(&mut store, 1.0);
    store
        .bind_semantic_signal(signal, target, SemanticObjectProperty::ObjectOpacity)
        .unwrap();
    store.remove_node(signal).unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.change_subscription(target, SemanticObjectProperty::ObjectOpacity, None);
    let result = transaction.apply(&mut store).unwrap();
    assert!(store
        .semantic_object_signal_bindings(target)
        .unwrap()
        .is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(result.impacts().len(), 1);

    let mut noop = SemanticMutationTransaction::new();
    noop.change_subscription(target, SemanticObjectProperty::ObjectOpacity, None);
    let result = noop.apply(&mut store).unwrap();
    assert!(result.impacts().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn duplicate_subscription_is_rejected_but_base_property_change_is_independent() {
    let mut store = SemanticStore::new();
    let first = store.insert_semantic_input_signal(0.25_f64).unwrap();
    let second = store.insert_semantic_input_signal(0.75_f64).unwrap();
    let target = object(&mut store, 1.0);
    let mut duplicate = SemanticMutationTransaction::new();
    duplicate
        .change_subscription(target, SemanticObjectProperty::ObjectOpacity, Some(first))
        .change_subscription(target, SemanticObjectProperty::ObjectOpacity, Some(second));

    assert_eq!(
        duplicate.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateSubscription {
            index: 1,
            object: target,
            property: SemanticObjectProperty::ObjectOpacity,
        })
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let mut mixed = SemanticMutationTransaction::new();
    mixed
        .set_property(target, SemanticObjectProperty::ObjectOpacity, 0.4_f64)
        .change_subscription(target, SemanticObjectProperty::ObjectOpacity, Some(first));
    let result = mixed.apply(&mut store).unwrap();
    assert_eq!(result.impacts().len(), 2);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn stale_subscription_source_fails_closed_before_mutation() {
    let mut store = SemanticStore::new();
    let stale = store.insert_semantic_input_signal(0.5_f64).unwrap();
    store.remove_node(stale).unwrap();
    let replacement = object(&mut store, 2.0);
    assert_eq!(stale.slot(), replacement.slot());
    assert_ne!(stale.generation(), replacement.generation());
    let target = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.change_subscription(target, SemanticObjectProperty::ObjectOpacity, Some(stale));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Signal {
            index: 0,
            error: SemanticSignalError::UnknownSignal(stale),
        })
    );
    assert!(store
        .semantic_object_signal_bindings(target)
        .unwrap()
        .is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn multiple_subscription_changes_on_one_object_write_one_slot() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let opacity = store.insert_semantic_input_signal(0.5_f64).unwrap();
    let width = store.insert_semantic_input_signal(2.0_f64).unwrap();
    let target = object(&mut store, 0.5);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .change_subscription(target, SemanticObjectProperty::ObjectOpacity, Some(opacity))
        .change_subscription(target, SemanticObjectProperty::StrokeWidth, Some(width));

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(result.impacts().len(), 2);
}
