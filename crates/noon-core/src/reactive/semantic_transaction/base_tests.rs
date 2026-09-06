use super::*;
use crate::{
    SemanticObjectState, SemanticSignalBinding, SemanticSignalExpr, SemanticVec3, StoredGeometry,
};

fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
}

fn input_value(store: &SemanticStore, signal: SemanticNodeId) -> SemanticSignalValue {
    let SemanticSignalSource::Input(value) = store.semantic_signal_state(signal).unwrap().source()
    else {
        panic!("expected input signal")
    };
    value.clone()
}

fn property_value(
    store: &SemanticStore,
    object: SemanticNodeId,
    property: SemanticObjectProperty,
) -> SemanticSignalValue {
    object_property_value(
        store.semantic_object_state_checked(object).unwrap(),
        property,
    )
}

#[test]
fn multiple_signal_values_commit_after_complete_preflight() {
    let mut store = SemanticStore::new();
    let scalar = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let vector = store
        .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
        .unwrap();
    let before_len = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(scalar, 2.5_f64)
        .set_signal(vector, SemanticVec3::new(4.0, 5.0, 6.0));

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        input_value(&store, scalar),
        SemanticSignalValue::Scalar(2.5)
    );
    assert_eq!(
        input_value(&store, vector),
        SemanticSignalValue::Vec3(SemanticVec3::new(4.0, 5.0, 6.0))
    );
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::SignalValue { signal: scalar },
            SemanticMutationImpact::SignalValue { signal: vector },
        ]
    );
}

#[test]
fn mixed_signal_and_properties_commit_atomically_and_count_unique_slots() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let target = object(&mut store, 2.0);
    let translation = SemanticVec3::new(4.0, -2.0, 7.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 3.0_f64)
        .set_property(target, SemanticObjectProperty::Translation, translation)
        .set_property(target, SemanticObjectProperty::ObjectOpacity, 0.4_f64);

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        input_value(&store, signal),
        SemanticSignalValue::Scalar(3.0)
    );
    assert_eq!(
        property_value(&store, target, SemanticObjectProperty::Translation),
        SemanticSignalValue::Vec3(translation)
    );
    assert_eq!(
        property_value(&store, target, SemanticObjectProperty::ObjectOpacity),
        SemanticSignalValue::Scalar(0.4)
    );
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::SignalValue { signal },
            SemanticMutationImpact::ObjectProperty {
                object: target,
                property: SemanticObjectProperty::Translation,
            },
            SemanticMutationImpact::ObjectProperty {
                object: target,
                property: SemanticObjectProperty::ObjectOpacity,
            },
        ]
    );
}

#[test]
fn invalid_late_value_prevents_earlier_valid_mutation() {
    let mut store = SemanticStore::new();
    let first = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let second = store.insert_semantic_input_signal(2.0_f64).unwrap();
    let first_before = input_value(&store, first);
    let second_before = input_value(&store, second);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(first, 10.0_f64)
        .set_signal(second, f64::NAN);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Signal {
            index: 1,
            error: SemanticSignalError::NonFiniteValue,
        })
    );
    assert_eq!(input_value(&store, first), first_before);
    assert_eq!(input_value(&store, second), second_before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn invalid_late_property_prevents_earlier_signal_and_property_mutation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let target = object(&mut store, 2.0);
    let signal_before = input_value(&store, signal);
    let translation_before = property_value(&store, target, SemanticObjectProperty::Translation);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 5.0_f64)
        .set_property(
            target,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(1.0, 2.0, 3.0),
        )
        .set_property(target, SemanticObjectProperty::StrokeWidth, f64::NAN);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::NonFinitePropertyValue {
            index: 2,
            object: target,
            property: SemanticObjectProperty::StrokeWidth,
        })
    );
    assert_eq!(input_value(&store, signal), signal_before);
    assert_eq!(
        property_value(&store, target, SemanticObjectProperty::Translation),
        translation_before
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn stale_late_target_prevents_earlier_valid_mutation() {
    let mut store = SemanticStore::new();
    let first = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let stale = store.insert_semantic_input_signal(2.0_f64).unwrap();
    store.remove_node(stale).unwrap();
    let replacement = object(&mut store, 3.0);
    assert_eq!(stale.slot(), replacement.slot());
    assert_ne!(stale.generation(), replacement.generation());
    let first_before = input_value(&store, first);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(first, 10.0_f64)
        .set_signal(stale, 20.0_f64);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Signal {
            index: 1,
            error: SemanticSignalError::UnknownSignal(stale),
        })
    );
    assert_eq!(input_value(&store, first), first_before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn stale_property_target_prevents_earlier_valid_mutation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let stale = object(&mut store, 2.0);
    store.remove_node(stale).unwrap();
    let replacement = object(&mut store, 3.0);
    assert_eq!(stale.slot(), replacement.slot());
    assert_ne!(stale.generation(), replacement.generation());
    let signal_before = input_value(&store, signal);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_signal(signal, 2.0_f64).set_property(
        stale,
        SemanticObjectProperty::RotationZ,
        0.5_f64,
    );

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Object {
            index: 1,
            error: SemanticSceneOperationError::UnknownNode(stale),
        })
    );
    assert_eq!(input_value(&store, signal), signal_before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn duplicate_target_is_rejected_before_mutation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let before = input_value(&store, signal);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .set_signal(signal, 3.0_f64);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateTarget {
            index: 1,
            target: signal,
        })
    );
    assert_eq!(input_value(&store, signal), before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn duplicate_property_is_rejected_but_distinct_properties_share_one_object() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let mut duplicate = SemanticMutationTransaction::new();
    duplicate
        .set_property(target, SemanticObjectProperty::RotationZ, 0.5_f64)
        .set_property(target, SemanticObjectProperty::RotationZ, 1.0_f64);

    assert_eq!(
        duplicate.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateProperty {
            index: 1,
            object: target,
            property: SemanticObjectProperty::RotationZ,
        })
    );
    assert_eq!(
        property_value(&store, target, SemanticObjectProperty::RotationZ),
        SemanticSignalValue::Scalar(0.0)
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let mut distinct = SemanticMutationTransaction::new();
    distinct
        .set_property(target, SemanticObjectProperty::RotationZ, 0.5_f64)
        .set_property(target, SemanticObjectProperty::StrokeWidth, 3.0_f64);
    let result = distinct.apply(&mut store).unwrap();
    assert_eq!(result.impacts().len(), 2);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn type_mismatch_and_derived_signal_targets_fail_atomically() {
    let mut store = SemanticStore::new();
    let scalar = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let derived = store
        .insert_semantic_derived_signal(SemanticSignalExpr::signal(scalar))
        .unwrap();
    let scalar_before = input_value(&store, scalar);

    let mut mismatch = SemanticMutationTransaction::new();
    mismatch.set_signal(scalar, SemanticVec3::new(1.0, 2.0, 3.0));
    assert_eq!(
        mismatch.apply(&mut store),
        Err(SemanticMutationTransactionError::SignalTypeMismatch {
            index: 0,
            signal: scalar,
            expected: SemanticSignalValueKind::Scalar,
            actual: SemanticSignalValueKind::Vec3,
        })
    );
    assert_eq!(input_value(&store, scalar), scalar_before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let mut derived_target = SemanticMutationTransaction::new();
    derived_target.set_signal(derived, 4.0_f64);
    assert_eq!(
        derived_target.apply(&mut store),
        Err(SemanticMutationTransactionError::NotInputSignal {
            index: 0,
            signal: derived,
        })
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn property_type_and_target_kind_are_validated_before_mutation() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let family = store.insert_family();
    let mut mismatch = SemanticMutationTransaction::new();
    mismatch.set_property(target, SemanticObjectProperty::Scale, 2.0_f64);

    assert_eq!(
        mismatch.apply(&mut store),
        Err(SemanticMutationTransactionError::PropertyTypeMismatch {
            index: 0,
            object: target,
            property: SemanticObjectProperty::Scale,
            expected: SemanticSignalValueKind::Vec3,
            actual: SemanticSignalValueKind::Scalar,
        })
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let mut wrong_target = SemanticMutationTransaction::new();
    wrong_target.set_property(family, SemanticObjectProperty::RotationZ, 1.0_f64);
    assert_eq!(
        wrong_target.apply(&mut store),
        Err(SemanticMutationTransactionError::Object {
            index: 0,
            error: SemanticSceneOperationError::NotSemanticObject(family),
        })
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn presence_property_requires_a_typed_signal_binding() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(target, SemanticObjectProperty::Presence, false);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::UnsupportedPropertyWrite {
            index: 0,
            object: target.into(),
            property: SemanticObjectProperty::Presence,
        })
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn unchanged_signal_is_a_noop_with_no_impact() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(true).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_signal(signal, true);

    let result = transaction.apply(&mut store).unwrap();

    assert!(result.impacts().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
    assert_eq!(input_value(&store, signal), SemanticSignalValue::Bool(true));
}

#[test]
fn unchanged_property_is_a_noop_with_no_impact() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(
        target,
        SemanticObjectProperty::Translation,
        SemanticVec3::ZERO,
    );

    let result = transaction.apply(&mut store).unwrap();

    assert!(result.impacts().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn set_property_preserves_signal_binding_declarations() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
    let target = object(&mut store, 1.0);
    store
        .bind_semantic_signal(signal, target, SemanticObjectProperty::ObjectOpacity)
        .unwrap();
    let binding = SemanticSignalBinding::new(signal, SemanticObjectProperty::ObjectOpacity);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(target, SemanticObjectProperty::ObjectOpacity, 0.25_f64);

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        property_value(&store, target, SemanticObjectProperty::ObjectOpacity),
        SemanticSignalValue::Scalar(0.25)
    );
    assert_eq!(
        store.semantic_object_signal_bindings(target).unwrap(),
        &[binding]
    );
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::ObjectProperty {
            object: target,
            property: SemanticObjectProperty::ObjectOpacity,
        }]
    );
}

#[test]
fn transaction_writes_only_changed_signal_slots_with_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let first = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let second = store.insert_semantic_input_signal(2.0_f64).unwrap();
    let unchanged = store.insert_semantic_input_signal(3.0_f64).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(first, 10.0_f64)
        .set_signal(second, 20.0_f64)
        .set_signal(unchanged, 3.0_f64);

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(result.impacts().len(), 2);
}

#[test]
fn property_transaction_writes_only_target_slot_with_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let target = object(&mut store, 0.5);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_property(target, SemanticObjectProperty::RotationZ, 0.75_f64)
        .set_property(target, SemanticObjectProperty::StrokeWidth, 4.0_f64);

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(result.impacts().len(), 2);
}
