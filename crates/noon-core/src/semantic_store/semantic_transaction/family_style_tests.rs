use super::*;
use crate::{Color, SemanticPaint};

#[test]
fn family_style_is_a_separate_atomic_root_slot() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let leaf = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let leaf_before = store.semantic_object_state_checked(leaf).unwrap().clone();
    let style = SemanticStyle::manim_family_root(Color::BLUE);
    let revision = store.scene_revision();

    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_style(root, style.clone());
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(prepared.family_style(root).unwrap(), style);
    assert_eq!(prepared.object_updates().count(), 0);
    let result = prepared.commit();

    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::FamilyStyle { node: root }]
    );
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(store.node(root).unwrap().family_style(), Some(&style));
    assert_eq!(
        store.semantic_object_state_checked(leaf).unwrap(),
        &leaf_before
    );
}

#[test]
fn pending_family_style_is_visible_and_invalid_style_rolls_back() {
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let root = tx.create_node(SemanticNodeCreation::family());
    let style = SemanticStyle::manim_family_root(Color::RED);
    tx.set_family_style(root, style.clone());
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(prepared.family_style(root).unwrap(), style);
    let id = prepared.commit().resolve(root).unwrap();
    assert_eq!(store.node(id).unwrap().family_style(), Some(&style));

    let revision = store.scene_revision();
    let original = store.node(id).unwrap().family_style().unwrap().clone();
    let mut invalid = original.clone();
    invalid.fill_opacity = f64::NAN;
    let mut tx = SemanticMutationTransaction::new();
    tx.set_z_index(id, 4.0).set_family_style(id, invalid);
    assert!(tx.apply(&mut store).is_err());
    assert_eq!(store.node(id).unwrap().presentation().unwrap().z_index, 0.0);
    assert_eq!(store.node(id).unwrap().family_style(), Some(&original));
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn family_color_updates_the_root_vector_paint_color() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let mut differing = SemanticStyle::manim_family_root(Color::BLUE);
    differing.fill_opacity = 0.4;
    differing.stroke = Some(SemanticPaint::Solid(Color::RED));
    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_style(root, differing);
    tx.apply(&mut store).unwrap();
    assert_eq!(store.node(root).unwrap().family_color(), Some(Color::BLUE));

    let revision = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_color(root, Color::BLUE);
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(
        prepared.family_style(root).unwrap().stroke,
        Some(SemanticPaint::Solid(Color::BLUE))
    );
    let result = prepared.commit();
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::FamilyColor { node: root }]
    );
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
    let style = store.node(root).unwrap().family_style().unwrap();
    assert_eq!(style.fill, Some(SemanticPaint::Solid(Color::BLUE)));
    assert_eq!(style.stroke, Some(SemanticPaint::Solid(Color::BLUE)));
    assert_eq!(style.fill_opacity, 0.4);
    assert_eq!(style.stroke_width, 0.04);
}

#[test]
fn family_color_and_style_mutations_compose_in_transaction_order() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let mut style = SemanticStyle::manim_family_root(Color::RED);
    style.fill_opacity = 0.4;
    style.stroke_opacity = 0.7;

    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_style(root, style.clone())
        .set_family_color(root, Color::GREEN);
    tx.apply(&mut store).unwrap();
    let stored = store.node(root).unwrap().family_style().unwrap();
    assert_eq!(stored.fill, Some(SemanticPaint::Solid(Color::GREEN)));
    assert_eq!(stored.stroke, Some(SemanticPaint::Solid(Color::GREEN)));
    assert_eq!(stored.fill_opacity, 0.4);
    assert_eq!(stored.stroke_opacity, 0.7);
    assert_eq!(store.node(root).unwrap().family_color(), Some(Color::GREEN));

    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_color(root, Color::GREEN)
        .set_family_style(root, style.clone());
    tx.apply(&mut store).unwrap();
    assert_eq!(store.node(root).unwrap().family_style(), Some(&style));
    assert_eq!(store.node(root).unwrap().family_color(), Some(Color::RED));
}

#[test]
fn duplicate_family_style_writes_and_nonfamily_targets_are_rejected() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let leaf = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let original = store.node(root).unwrap().family_style().unwrap().clone();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_style(root, SemanticStyle::manim_family_root(Color::RED))
        .set_family_style(root, SemanticStyle::manim_family_root(Color::BLUE));
    assert!(matches!(
        tx.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateFamilyStyle { .. })
    ));
    assert_eq!(store.node(root).unwrap().family_style(), Some(&original));

    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_style(leaf, SemanticStyle::default());
    assert!(tx.apply(&mut store).is_err());
    assert_eq!(store.node(root).unwrap().family_style(), Some(&original));
}
