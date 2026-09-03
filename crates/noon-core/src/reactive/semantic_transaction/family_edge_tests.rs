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
fn family_edges_share_the_atomic_transaction_and_impact_order() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
    let target = object(&mut store, 1.0);
    let family = store.insert_family();
    let first = object(&mut store, 2.0);
    let second = object(&mut store, 3.0);
    store.add_member(family, first).unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 0.75_f64)
        .set_property(
            target,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(1.0, 2.0, 3.0),
        )
        .remove_member(family, first)
        .add_member(family, second);

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(scalar_input(&store, signal), 0.75);
    assert_eq!(store.node(family).unwrap().members(), vec![second]);
    assert!(store.node(first).unwrap().parents().is_empty());
    assert_eq!(store.node(second).unwrap().parents(), &[family]);
    assert_eq!(store.last_mutation_stats().slots_written, 5);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::SignalValue { signal },
            SemanticMutationImpact::ObjectProperty {
                object: target,
                property: SemanticObjectProperty::Translation,
            },
            SemanticMutationImpact::FamilyMemberRemoved {
                family,
                member: first,
            },
            SemanticMutationImpact::FamilyMemberAdded {
                family,
                member: second,
            },
        ]
    );
}

#[test]
fn late_family_cycle_rolls_back_earlier_value_and_edge_changes() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let outer = store.insert_family();
    let inner = store.insert_family();
    let leaf = object(&mut store, 1.0);
    store.add_member(outer, inner).unwrap();
    let signal_before = scalar_input(&store, signal);

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .add_member(inner, leaf)
        .add_member(inner, outer);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Family {
            index: 2,
            error: SemanticSceneOperationError::Store(SemanticStoreError::FamilyCycle {
                family: inner,
                member: outer,
            }),
        })
    );
    assert_eq!(scalar_input(&store, signal), signal_before);
    assert!(store.node(inner).unwrap().members().is_empty());
    assert_eq!(store.node(outer).unwrap().members(), vec![inner]);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn family_preflight_detects_cycle_created_only_by_multiple_pending_adds() {
    let mut store = SemanticStore::new();
    let first = store.insert_family();
    let second = store.insert_family();
    let third = store.insert_family();
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_member(first, second)
        .add_member(second, third)
        .add_member(third, first);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Family {
            index: 2,
            error: SemanticSceneOperationError::Store(SemanticStoreError::FamilyCycle {
                family: third,
                member: first,
            }),
        })
    );
    assert!(store.node(first).unwrap().members().is_empty());
    assert!(store.node(second).unwrap().members().is_empty());
    assert!(store.node(third).unwrap().members().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn pending_removal_is_visible_to_later_cycle_validation() {
    let mut store = SemanticStore::new();
    let first = store.insert_family();
    let second = store.insert_family();
    let third = store.insert_family();
    store.add_member(first, second).unwrap();
    store.add_member(second, third).unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .remove_member(first, second)
        .add_member(third, first);

    let result = transaction.apply(&mut store).unwrap();

    assert!(store.node(first).unwrap().members().is_empty());
    assert_eq!(store.node(second).unwrap().members(), vec![third]);
    assert_eq!(store.node(third).unwrap().members(), vec![first]);
    assert_eq!(result.impacts().len(), 2);
    assert_eq!(store.last_mutation_stats().slots_written, 3);
}

#[test]
fn repeated_family_edge_mutation_is_rejected_before_commit() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let member = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_member(family, member)
        .remove_member(family, member);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateFamilyEdge {
            index: 1,
            family,
            member,
        })
    );
    assert!(store.node(family).unwrap().members().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn stale_family_member_identity_fails_closed_and_rolls_back() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let family = store.insert_family();
    let stale = object(&mut store, 1.0);
    store.remove_node(stale).unwrap();
    let replacement = object(&mut store, 2.0);
    assert_eq!(stale.slot(), replacement.slot());
    assert_ne!(stale.generation(), replacement.generation());

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .add_member(family, stale);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Family {
            index: 1,
            error: SemanticSceneOperationError::UnknownNode(stale),
        })
    );
    assert_eq!(scalar_input(&store, signal), 1.0);
    assert!(store.node(family).unwrap().members().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn family_edge_rejects_non_target_authoring_members() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let identity_only = store.insert_authoring_object();

    for member in [signal, identity_only] {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_member(family, member);
        assert_eq!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::Family {
                index: 0,
                error: SemanticSceneOperationError::NotSemanticAuthoringNode(member),
            })
        );
        assert!(store.node(family).unwrap().members().is_empty());
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }
}

#[test]
fn existing_add_and_absent_remove_are_noops() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let existing = object(&mut store, 1.0);
    let absent = object(&mut store, 2.0);
    store.add_member(family, existing).unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_member(family, existing)
        .remove_member(family, absent);

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(store.node(family).unwrap().members(), vec![existing]);
    assert!(result.impacts().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn family_edge_transaction_writes_only_affected_slots_with_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let family = store.insert_family();
    let removed = object(&mut store, 1.0);
    let added = object(&mut store, 2.0);
    store.add_member(family, removed).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .remove_member(family, removed)
        .add_member(family, added);

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(store.node(family).unwrap().members(), vec![added]);
    assert_eq!(store.last_mutation_stats().slots_written, 3);
    assert_eq!(result.impacts().len(), 2);
}
