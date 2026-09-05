use super::*;
use crate::{SemanticNodeResidency, StoredGeometry};

fn object_state(radius: f32) -> SemanticObjectState {
    SemanticObjectState::new(StoredGeometry::Circle { radius })
}

#[test]
fn create_mutate_attach_commits_atomically_and_resolves_real_identity() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let before_len = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    let pending = transaction.create_node(SemanticNodeCreation::object(object_state(1.0)));
    transaction
        .set_pending_property(pending, SemanticObjectProperty::ObjectOpacity, 0.4_f64)
        .add_member_ref(family, pending);

    assert_eq!(store.len(), before_len);
    assert!(transaction
        .pending_object_state(pending)
        .is_some_and(|state| state.style.object_opacity == 0.4));

    let result = transaction.apply(&mut store).unwrap();
    let object = result.committed_node(pending).unwrap();
    assert_eq!(store.len(), before_len + 1);
    assert_eq!(
        store
            .semantic_object_state_checked(object)
            .unwrap()
            .style
            .object_opacity,
        0.4
    );
    assert!(store.node(object).unwrap().parents().contains(&family));
    assert_eq!(
        store.node(object).unwrap().residency(),
        SemanticNodeResidency::Detached
    );
    assert_eq!(result.committed_nodes(), &[(pending, object)]);
    assert_eq!(store.last_mutation_stats().slots_written, 2);
}

#[test]
fn two_pending_nodes_can_form_valid_family_edge() {
    let mut store = SemanticStore::new();
    let before_len = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    let family = transaction.create_node(SemanticNodeCreation::family());
    let object = transaction.create_node(SemanticNodeCreation::object(object_state(2.0)));
    transaction.add_member_ref(family, object);

    let result = transaction.apply(&mut store).unwrap();
    let family = result.committed_node(family).unwrap();
    let object = result.committed_node(object).unwrap();
    assert_eq!(store.len(), before_len + 2);
    assert_eq!(store.node(family).unwrap().members(), vec![object]);
    assert!(store.node(object).unwrap().parents().contains(&family));
}

#[test]
fn pending_family_cycle_fails_before_identity_allocation() {
    let mut store = SemanticStore::new();
    let before_len = store.len();
    let before_capacity = store.slot_capacity();
    let mut transaction = SemanticMutationTransaction::new();
    let first = transaction.create_node(SemanticNodeCreation::family());
    let second = transaction.create_node(SemanticNodeCreation::family());
    transaction
        .add_member_ref(first, second)
        .add_member_ref(second, first);

    assert!(matches!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::PendingFamily {
            error: SemanticPendingFamilyError::Cycle { .. },
            ..
        })
    ));
    assert_eq!(store.len(), before_len);
    assert_eq!(store.slot_capacity(), before_capacity);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn token_from_another_transaction_is_rejected_before_allocation() {
    let mut source = SemanticMutationTransaction::new();
    let foreign = source.create_node(SemanticNodeCreation::object(object_state(1.0)));
    let mut store = SemanticStore::new();
    let before_len = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_pending_property(foreign, SemanticObjectProperty::ObjectOpacity, 0.2_f64);

    assert!(matches!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::PendingNode {
            token,
            error: SemanticPendingNodeError::UnknownToken,
            ..
        }) if token == foreign
    ));
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn pending_read_overlay_is_private_and_reflects_read_after_write() {
    let store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let pending = transaction.create_node(SemanticNodeCreation::object(object_state(1.0)));
    transaction.set_pending_property(pending, SemanticObjectProperty::RotationZ, 0.75_f64);
    let staged = transaction.pending_object_state(pending).unwrap();
    assert_eq!(staged.transform.rotation_z, 0.75);
    assert!(store.is_empty());
}

#[test]
fn pending_creation_work_stays_local_with_large_unrelated_store() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        store.insert_semantic_object(object_state(index as f32 + 1.0));
    }
    let before_len = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    let pending = transaction.create_node(SemanticNodeCreation::object(object_state(0.25)));
    transaction.set_pending_property(pending, SemanticObjectProperty::ObjectOpacity, 0.5_f64);
    let result = transaction.apply(&mut store).unwrap();
    assert!(result.committed_node(pending).is_some());
    assert_eq!(store.len(), before_len + 1);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}
