use super::*;
use crate::Color;

#[test]
fn family_color_stages_without_leaf_paint_and_commits_one_slot() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let leaf = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let leaf_before = store.semantic_object_state_checked(leaf).unwrap().clone();
    let revision = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_color(root, Color::RED);
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(prepared.family_color(root).unwrap(), Color::RED);
    assert_eq!(prepared.object_updates().count(), 0);
    let result = prepared.commit();
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::FamilyColor { node: root }]
    );
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(store.node(root).unwrap().family_color(), Some(Color::RED));
    assert_eq!(
        store.semantic_object_state_checked(leaf).unwrap(),
        &leaf_before
    );
}

#[test]
fn invalid_color_and_nonfamily_target_rollback_every_write() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let leaf = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let revision = store.scene_revision();
    for color in [
        Color::rgba(f32::NAN, 0.0, 0.0, 1.0),
        Color::rgba(0.0, 0.0, 0.0, f32::INFINITY),
    ] {
        let mut tx = SemanticMutationTransaction::new();
        tx.set_z_index(root, 9.0).set_family_color(root, color);
        assert!(tx.apply(&mut store).is_err());
        assert_eq!(
            store.node(root).unwrap().presentation().unwrap().z_index,
            0.0
        );
        assert_eq!(store.node(root).unwrap().family_color(), Some(Color::WHITE));
        assert_eq!(store.scene_revision(), revision);
    }
    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_color(root, Color::RED)
        .set_family_color(leaf, Color::BLUE);
    assert!(tx.apply(&mut store).is_err());
    assert_eq!(store.node(root).unwrap().family_color(), Some(Color::WHITE));
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn pending_family_colors_are_visible_and_removed_values_do_not_publish() {
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let root = tx.create_node(SemanticNodeCreation::family());
    tx.set_family_color(root, Color::BLUE);
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(prepared.family_color(root).unwrap(), Color::BLUE);
    let result = prepared.commit();
    let id = result.resolve(root).unwrap();
    assert_eq!(store.node(id).unwrap().family_color(), Some(Color::BLUE));
    let revision = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    let removed = tx.create_node(SemanticNodeCreation::family());
    tx.set_family_color(removed, Color::RED)
        .remove_node(removed);
    let prepared = tx.prepare(&mut store).unwrap();
    assert!(matches!(
        prepared.family_color(removed),
        Err(SemanticTransactionReadError::RemovedPendingNode(_))
    ));
    assert!(prepared.commit().impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn duplicate_color_writes_and_unchanged_color_have_no_partial_publication() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let revision = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_color(root, Color::RED)
        .set_family_color(root, Color::BLUE);
    assert!(matches!(
        tx.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateFamilyColor { .. })
    ));
    assert_eq!(store.node(root).unwrap().family_color(), Some(Color::WHITE));
    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_color(root, Color::WHITE);
    assert!(tx.apply(&mut store).unwrap().impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
}
