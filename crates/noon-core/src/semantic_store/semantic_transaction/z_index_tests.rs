use super::*;

fn object(store: &mut SemanticStore) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }))
}

#[test]
fn z_index_stages_objects_and_family_roots_with_one_local_publication() {
    let mut store = SemanticStore::new();
    let leaf = object(&mut store);
    let family = store.insert_family();
    let before = store.semantic_object_state_checked(leaf).unwrap().clone();
    let revision = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_z_index(leaf, 2.5).set_z_index(family, -3.25);
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(prepared.z_index(leaf).unwrap(), 2.5);
    assert_eq!(prepared.z_index(family).unwrap(), -3.25);
    assert_eq!(prepared.object_state(leaf).unwrap().z_index(), 2.5);
    assert_eq!(prepared.object_updates().count(), 1);
    let result = prepared.commit();
    assert_eq!(result.impacts().len(), 2);
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    let after = store.semantic_object_state_checked(leaf).unwrap();
    assert_eq!(after.transform, before.transform);
    assert_eq!(after.content, before.content);
    assert_eq!(
        after.presentation().insertion_order,
        before.presentation().insertion_order
    );
    assert_eq!(
        store.node(family).unwrap().presentation().unwrap().z_index,
        -3.25
    );
}

#[test]
fn invalid_and_duplicate_priorities_rollback_the_whole_batch() {
    let mut store = SemanticStore::new();
    let leaf = object(&mut store);
    let family = store.insert_family();
    let revision = store.scene_revision();
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut tx = SemanticMutationTransaction::new();
        tx.set_z_index(leaf, 8.0).set_z_index(family, invalid);
        assert!(matches!(
            tx.apply(&mut store),
            Err(SemanticMutationTransactionError::NonFiniteZIndex { .. })
        ));
        assert_eq!(
            store.semantic_object_state_checked(leaf).unwrap().z_index(),
            0.0
        );
        assert_eq!(store.scene_revision(), revision);
    }
    let mut tx = SemanticMutationTransaction::new();
    tx.set_z_index(leaf, 1.0).set_z_index(leaf, 2.0);
    assert!(matches!(
        tx.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateZIndex { .. })
    ));
    let mut tx = SemanticMutationTransaction::new();
    tx.set_z_index(leaf, -0.0).set_z_index(family, 0.0);
    assert!(tx.apply(&mut store).unwrap().impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn pending_priorities_are_readable_and_discarded_with_removed_nodes() {
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let family = tx.create_node(SemanticNodeCreation::family());
    let leaf = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    tx.set_z_index(family, 1.25).set_z_index(leaf, -2.5);
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(prepared.z_index(family).unwrap(), 1.25);
    assert_eq!(prepared.z_index(leaf).unwrap(), -2.5);
    prepared.commit();
    let revision = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    let removed = tx.create_node(SemanticNodeCreation::family());
    tx.set_z_index(removed, 7.5).remove_node(removed);
    let prepared = tx.prepare(&mut store).unwrap();
    assert!(matches!(
        prepared.z_index(removed),
        Err(SemanticTransactionReadError::RemovedPendingNode(_))
    ));
    assert!(prepared.commit().impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
}
