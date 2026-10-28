use super::*;
use crate::{Color, SemanticPaint};

#[test]
fn copied_family_opacity_intent_is_atomic_weak_and_tracks_derived_cardinality() {
    let mut store = SemanticStore::new();
    let source = store.insert_family();
    let target = store.insert_family();
    let style = store.node(source).unwrap().family_style().unwrap().clone();
    let mut copy = SemanticMutationTransaction::new();
    copy.set_copied_family_style_with_background_stroke_update(
        target,
        source,
        style.clone(),
        None,
        None,
        None,
    );
    copy.apply(&mut store).unwrap();
    let baseline = store
        .node(target)
        .unwrap()
        .family_opacity_copy_provenance()
        .unwrap();
    assert_eq!(baseline.source(), source);
    assert_eq!(baseline.copied_opacity(), 1.0);
    assert!(!baseline.writes_object_opacity());
    assert!(!store.has_composed_source_declarations());
    let revision = store.scene_revision();
    let write = || {
        let mut tx = SemanticMutationTransaction::new();
        tx.set_family_style_with_explicit_object_opacity(target, style.clone());
        tx
    };
    {
        let prepared = write().prepare(&mut store).unwrap();
        assert!(prepared
            .family_opacity_copy_provenance(target)
            .unwrap()
            .unwrap()
            .writes_object_opacity());
        assert_eq!(prepared.store().scene_revision(), revision);
        assert!(!prepared.store().has_composed_source_declarations());
    }
    assert_eq!(
        store.node(target).unwrap().family_opacity_copy_provenance(),
        Some(baseline)
    );
    let mut invalid = write();
    let mut bad_style = style.clone();
    bad_style.object_opacity = f64::NAN;
    invalid.set_family_style(source, bad_style);
    assert!(invalid.apply(&mut store).is_err());
    assert_eq!(store.scene_revision(), revision);
    assert!(!store.has_composed_source_declarations());
    assert_eq!(
        store.node(target).unwrap().family_opacity_copy_provenance(),
        Some(baseline)
    );
    write().apply(&mut store).unwrap();
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert!(store.has_composed_source_declarations());
    assert!(store.clone().has_composed_source_declarations());
    let committed = store.scene_revision();
    let mut repeat = SemanticMutationTransaction::new();
    repeat.set_family_style_with_explicit_object_opacity(target, style);
    assert!(repeat.apply(&mut store).unwrap().impacts().is_empty());
    assert_eq!(store.scene_revision(), committed);
    let mut materialized = store.clone();
    let mut content = SemanticMutationTransaction::new();
    content.replace_content(target, StoredGeometry::Circle { radius: 1.0 });
    content.apply(&mut materialized).unwrap();
    assert!(!materialized.has_composed_source_declarations());
    assert!(materialized
        .node(target)
        .unwrap()
        .family_opacity_copy_provenance()
        .is_none());
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(source);
    remove.apply(&mut store).unwrap();
    assert!(
        store.node(target).is_some(),
        "copied provenance does not own the source lifetime"
    );
    let fresh = store.insert_family();
    assert_eq!(fresh.slot(), source.slot());
    assert_ne!(
        fresh, source,
        "a reused slot is a different source generation"
    );
    assert_ne!(
        store
            .node(target)
            .unwrap()
            .family_opacity_copy_provenance()
            .unwrap()
            .source(),
        fresh
    );
    let mut retire = SemanticMutationTransaction::new();
    retire.remove_node(target);
    retire.apply(&mut store).unwrap();
    assert!(!store.has_composed_source_declarations());
}

#[test]
fn partial_family_opacity_cardinality_tracks_style_clone_materialization_and_retirement() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    assert!(!store.has_composed_source_declarations());
    let set = |store: &mut SemanticStore, opacity: f64| {
        let mut style = store.node(family).unwrap().family_style().unwrap().clone();
        style.object_opacity = opacity;
        let mut tx = SemanticMutationTransaction::new();
        tx.set_family_style(family, style);
        tx.apply(store).unwrap();
    };
    set(&mut store, 0.5);
    assert!(store.has_composed_source_declarations());
    assert!(store.clone().has_composed_source_declarations());
    let mut materialized = store.clone();
    let mut content = SemanticMutationTransaction::new();
    content.replace_content(family, StoredGeometry::Circle { radius: 1.0 });
    content.apply(&mut materialized).unwrap();
    assert!(!materialized.has_composed_source_declarations());
    set(&mut store, 1.0);
    assert!(!store.has_composed_source_declarations());
    set(&mut store, 0.0);
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(family);
    remove.apply(&mut store).unwrap();
    assert!(!store.has_composed_source_declarations());
}

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
