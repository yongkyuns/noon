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
fn transaction_reorder_shares_atomic_impact_order_with_value_mutations() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
    let family = store.insert_family();
    let first = object(&mut store, 1.0);
    let second = object(&mut store, 2.0);
    let third = object(&mut store, 3.0);
    for member in [first, second, third] {
        store.add_semantic_family_member(family, member).unwrap();
    }

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 0.75_f64)
        .reorder_member(family, third, Some(first));
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(scalar_input(&store, signal), 0.75);
    assert_eq!(
        store.semantic_family_members_checked(family).unwrap(),
        vec![third, first, second]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::SignalValue { signal },
            SemanticMutationImpact::FamilyMemberReordered {
                family,
                member: third,
                before: Some(first),
            },
        ]
    );
}

#[test]
fn pending_added_anchor_is_visible_to_later_reorder_preflight() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let first = object(&mut store, 1.0);
    let second = object(&mut store, 2.0);
    let anchor = object(&mut store, 3.0);
    store.add_semantic_family_member(family, first).unwrap();
    store.add_semantic_family_member(family, second).unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_member(family, anchor)
        .reorder_member(family, first, Some(anchor));
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        store.semantic_family_members_checked(family).unwrap(),
        vec![second, first, anchor]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::FamilyMemberAdded {
                family,
                member: anchor,
            },
            SemanticMutationImpact::FamilyMemberReordered {
                family,
                member: first,
                before: Some(anchor),
            },
        ]
    );
}

#[test]
fn pending_removed_anchor_rejects_reorder_before_any_commit() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let first = object(&mut store, 1.0);
    let anchor = object(&mut store, 2.0);
    let third = object(&mut store, 3.0);
    for member in [first, anchor, third] {
        store.add_semantic_family_member(family, member).unwrap();
    }

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .remove_member(family, anchor)
        .reorder_member(family, third, Some(anchor));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Family {
            index: 1,
            error: SemanticSceneOperationError::NotSemanticFamilyMember {
                family,
                member: anchor,
            },
        })
    );
    assert_eq!(
        store.semantic_family_members_checked(family).unwrap(),
        vec![first, anchor, third]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn stale_reorder_identity_fails_closed_before_commit() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let stale = object(&mut store, 1.0);
    let survivor = object(&mut store, 2.0);
    store.add_semantic_family_member(family, stale).unwrap();
    store.add_semantic_family_member(family, survivor).unwrap();
    store.remove_node(stale).unwrap();
    let replacement = object(&mut store, 3.0);
    assert_eq!(stale.slot(), replacement.slot());
    assert_ne!(stale.generation(), replacement.generation());

    let mut transaction = SemanticMutationTransaction::new();
    transaction.reorder_member(family, stale, Some(survivor));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Family {
            index: 0,
            error: SemanticSceneOperationError::UnknownNode(stale),
        })
    );
    assert_eq!(
        store.semantic_family_members_checked(family).unwrap(),
        vec![survivor]
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn transaction_reorder_is_local_with_ten_thousand_family_members() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let members = (0..10_000)
        .map(|index| object(&mut store, index as f32 + 1.0))
        .collect::<Vec<_>>();
    for member in members.iter().copied() {
        store.add_semantic_family_member(family, member).unwrap();
    }
    let target = members[5_000];
    let anchor = members[10];

    let mut transaction = SemanticMutationTransaction::new();
    transaction.reorder_member(family, target, Some(anchor));
    let result = transaction.apply(&mut store).unwrap();

    let ordered = store.semantic_family_members_checked(family).unwrap();
    assert_eq!(ordered.len(), 10_000);
    assert_eq!(ordered[10], target);
    assert_eq!(ordered[11], anchor);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::FamilyMemberReordered {
            family,
            member: target,
            before: Some(anchor),
        }]
    );
}
