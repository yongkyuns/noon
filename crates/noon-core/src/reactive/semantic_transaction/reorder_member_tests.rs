use super::*;
use crate::{SemanticObjectState, StoredGeometry};

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
fn reorder_member_changes_only_family_order_and_emits_one_impact() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let first = object(&mut store, 1.0);
    let second = object(&mut store, 2.0);
    let third = object(&mut store, 3.0);
    store.add_member(family, first).unwrap();
    store.add_member(family, second).unwrap();
    store.add_member(family, third).unwrap();
    let parents_before = store.node(third).unwrap().parents().to_vec();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.reorder_member(family, third, Some(first));
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        store.node(family).unwrap().members(),
        vec![third, first, second]
    );
    assert_eq!(store.node(third).unwrap().parents(), parents_before);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::FamilyMemberReordered {
            family,
            member: third,
            before: Some(first),
        }]
    );
}

#[test]
fn reorder_member_supports_tail_and_noop_positions() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let first = object(&mut store, 1.0);
    let second = object(&mut store, 2.0);
    let third = object(&mut store, 3.0);
    store.add_member(family, first).unwrap();
    store.add_member(family, second).unwrap();
    store.add_member(family, third).unwrap();

    let mut already_before = SemanticMutationTransaction::new();
    already_before.reorder_member(family, first, Some(second));
    let result = already_before.apply(&mut store).unwrap();
    assert!(result.impacts().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let mut already_tail = SemanticMutationTransaction::new();
    already_tail.reorder_member(family, third, None);
    let result = already_tail.apply(&mut store).unwrap();
    assert!(result.impacts().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let mut move_tail = SemanticMutationTransaction::new();
    move_tail.reorder_member(family, first, None);
    let result = move_tail.apply(&mut store).unwrap();
    assert_eq!(
        store.node(family).unwrap().members(),
        vec![second, third, first]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(result.impacts().len(), 1);
}

#[test]
fn add_then_reorder_is_preflighted_and_committed_in_transaction_order() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let first = object(&mut store, 1.0);
    let second = object(&mut store, 2.0);
    let added = object(&mut store, 3.0);
    store.add_member(family, first).unwrap();
    store.add_member(family, second).unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_member(family, added)
        .reorder_member(family, added, Some(first));
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        store.node(family).unwrap().members(),
        vec![added, first, second]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::FamilyMemberAdded {
                family,
                member: added,
            },
            SemanticMutationImpact::FamilyMemberReordered {
                family,
                member: added,
                before: Some(first),
            },
        ]
    );
}

#[test]
fn invalid_late_reorder_rolls_back_earlier_mutations() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let family = store.insert_family();
    let member = object(&mut store, 1.0);
    let non_member = object(&mut store, 2.0);
    store.add_member(family, member).unwrap();
    let before = store.node(family).unwrap().members();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .reorder_member(family, member, Some(non_member));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Family {
            index: 1,
            error: SemanticSceneOperationError::Store(SemanticStoreError::NotFamilyMember {
                family,
                member: non_member,
            }),
        })
    );
    assert_eq!(scalar_input(&store, signal), 1.0);
    assert_eq!(store.node(family).unwrap().members(), before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn duplicate_reorder_for_one_member_is_rejected_before_commit() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let first = object(&mut store, 1.0);
    let second = object(&mut store, 2.0);
    let third = object(&mut store, 3.0);
    store.add_member(family, first).unwrap();
    store.add_member(family, second).unwrap();
    store.add_member(family, third).unwrap();
    let before = store.node(family).unwrap().members();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .reorder_member(family, third, Some(first))
        .reorder_member(family, third, Some(second));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateFamilyOrder {
            index: 1,
            family,
            member: third
        })
    );
    assert_eq!(store.node(family).unwrap().members(), before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn reorder_rejects_member_or_anchor_removed_by_same_transaction() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let first = object(&mut store, 1.0);
    let second = object(&mut store, 2.0);
    store.add_member(family, first).unwrap();
    store.add_member(family, second).unwrap();

    let mut member_removed = SemanticMutationTransaction::new();
    member_removed
        .reorder_member(family, first, None)
        .remove_node(first);
    assert_eq!(
        member_removed.apply(&mut store),
        Err(
            SemanticMutationTransactionError::FamilyOrderUsesRemovedNode {
                index: 0,
                family,
                node: first,
            }
        )
    );

    let mut anchor_removed = SemanticMutationTransaction::new();
    anchor_removed
        .reorder_member(family, second, Some(first))
        .remove_node(first);
    assert_eq!(
        anchor_removed.apply(&mut store),
        Err(
            SemanticMutationTransactionError::FamilyOrderUsesRemovedNode {
                index: 0,
                family,
                node: first,
            }
        )
    );
    assert_eq!(store.node(family).unwrap().members(), vec![first, second]);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn reorder_writes_one_slot_with_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let family = store.insert_family();
    let first = object(&mut store, 0.25);
    let second = object(&mut store, 0.5);
    let third = object(&mut store, 0.75);
    store.add_member(family, first).unwrap();
    store.add_member(family, second).unwrap();
    store.add_member(family, third).unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.reorder_member(family, third, Some(first));
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        store.node(family).unwrap().members(),
        vec![third, first, second]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(result.impacts().len(), 1);
}
