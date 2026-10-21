use super::*;
use crate::{Color, SemanticPaint};

fn content() -> SemanticObjectContent {
    StoredGeometry::Rectangle {
        size: crate::Vec2::new(2.0, 3.0),
    }
    .into()
}

#[test]
fn family_content_keeps_root_identity_members_and_one_canonical_style() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let child = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 0.2,
    }));
    store.add_member(root, child).unwrap();
    let mut style = SemanticStyle::manim_family_root(Color::RED);
    style.background_stroke_width = 0.2;
    style.background_stroke_opacity = 0.3;
    let mut setup = SemanticMutationTransaction::new();
    setup
        .set_family_style(root, style.clone())
        .set_z_index(root, 7.0);
    setup.apply(&mut store).unwrap();
    let count = store.len();
    let mut tx = SemanticMutationTransaction::new();
    tx.replace_content(root, content());
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(prepared.object_state(root).unwrap().style, style);
    assert_eq!(prepared.proposed_object_state(root).unwrap().z_index(), 7.0);
    prepared.commit();
    let node = store.semantic_family_checked(root).unwrap();
    assert_eq!(node.members(), vec![child]);
    assert!(matches!(
        node.kind(),
        SemanticNodeKind::Family(crate::SemanticFamilyState::ContentOwning { .. })
    ));
    assert_eq!(
        node.family_style(),
        Some(&node.semantic_object_state().unwrap().style)
    );
    assert_eq!(node.family_background_stroke_width(), Some(0.2));
    assert_eq!(store.ordered_leaf_nodes(root).unwrap(), vec![root, child]);
    assert_eq!(store.len(), count);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn pending_family_materialization_observes_staged_metadata_and_retains_kind() {
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let root = tx.create_node(SemanticNodeCreation::family());
    let style = SemanticStyle::manim_family_root(Color::BLUE);
    tx.set_family_style(root, style.clone())
        .set_z_index(root, 3.0)
        .replace_content(root, content());
    let prepared = tx.prepare(&mut store).unwrap();
    let state = prepared.proposed_object_state(root).unwrap();
    assert_eq!(state.style, style);
    assert_eq!(state.z_index(), 3.0);
    let id = prepared.commit().resolve(root).unwrap();
    assert_eq!(store.semantic_object_state_checked(id).unwrap(), &state);
    assert!(store.semantic_family_checked(id).is_ok());
}

#[test]
fn materialized_family_style_and_object_style_compose_in_transaction_order() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let mut tx = SemanticMutationTransaction::new();
    tx.replace_content(root, content());
    tx.apply(&mut store).unwrap();
    for family_first in [true, false] {
        let family_style = SemanticStyle::manim_family_root(Color::BLUE);
        let object_style = SemanticStyle::manim_family_root(Color::RED);
        let mut tx = SemanticMutationTransaction::new();
        if family_first {
            tx.set_family_style(root, family_style.clone())
                .replace_style(root, object_style.clone());
        } else {
            tx.replace_style(root, object_style.clone())
                .set_family_style(root, family_style.clone());
        }
        let prepared = tx.prepare(&mut store).unwrap();
        let wanted = if family_first {
            object_style
        } else {
            family_style
        };
        assert_eq!(prepared.family_style(root).unwrap(), wanted);
        assert_eq!(
            prepared.family_color(root).unwrap(),
            wanted.family_color().unwrap()
        );
        assert_eq!(prepared.object_state(root).unwrap().style, wanted);
        assert_eq!(prepared.proposed_object_state(root).unwrap().style, wanted);
        prepared.commit();
        assert_eq!(store.node(root).unwrap().family_style(), Some(&wanted));
        assert_eq!(
            store.semantic_object_state_checked(root).unwrap().style,
            wanted
        );
    }
}

#[test]
fn failed_first_family_content_leaves_the_entire_root_unrendered() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let before = store.node(root).unwrap().clone();
    let revision = store.scene_revision();
    let invalid = SemanticStyle {
        stroke_width: f64::NAN,
        ..SemanticStyle::default()
    };
    let mut tx = SemanticMutationTransaction::new();
    tx.replace_content(root, content())
        .replace_style(root, invalid);
    assert!(tx.apply(&mut store).is_err());
    assert_eq!(store.node(root).unwrap(), &before);
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn family_background_stroke_overrides_share_the_prepared_and_committed_style() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    for materialize_first in [false, true] {
        let mut tx = SemanticMutationTransaction::new();
        if materialize_first {
            tx.replace_content(root, content());
        }
        tx.set_family_style_with_background_stroke_update(
            root,
            SemanticStyle::manim_family_root(Color::GREEN),
            Some(Color::RED),
            Some(0.8),
            Some(0.4),
        );
        if !materialize_first {
            tx.replace_content(root, content());
        }
        let prepared = tx.prepare(&mut store).unwrap();
        let style = prepared.family_style(root).unwrap();
        assert_eq!(
            style.background_stroke,
            Some(SemanticPaint::Solid(Color::RED))
        );
        assert_eq!(style.background_stroke_width, 0.8);
        assert_eq!(style.background_stroke_opacity, 0.4);
        assert_eq!(prepared.proposed_object_state(root).unwrap().style, style);
        prepared.commit();
        assert_eq!(
            store.semantic_object_state_checked(root).unwrap().style,
            style
        );
    }
}

#[test]
fn family_metadata_and_point_properties_have_one_prepared_state() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let mut tx = SemanticMutationTransaction::new();
    tx.replace_content(root, content());
    tx.apply(&mut store).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_family_style(root, SemanticStyle::manim_family_root(Color::RED))
        .set_property(root, SemanticObjectProperty::FillOpacity, 0.6)
        .set_z_index(root, 8.0)
        .set_sheen_direction(root, crate::SemanticSheenDirection::DEFAULT);
    let prepared = tx.prepare(&mut store).unwrap();
    let state = prepared.proposed_object_state(root).unwrap();
    assert_eq!(state.style.fill_opacity, 0.6);
    assert_eq!(prepared.family_style(root).unwrap(), state.style);
    assert_eq!(state.z_index(), 8.0);
    prepared.commit();
    assert_eq!(store.semantic_object_state_checked(root).unwrap(), &state);
}

#[test]
fn owned_family_creation_preserves_full_state_and_predicted_insertion_order() {
    let mut store = SemanticStore::new();
    let mut state = SemanticObjectState::new(content());
    state.transform.translation = crate::SemanticVec3::new(2., 3., 0.);
    state.style = SemanticStyle::manim_family_root(Color::BLUE);
    state.set_z_index(7.0);
    let mut tx = SemanticMutationTransaction::new();
    tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        content(),
    )));
    let root = tx.create_node(SemanticNodeCreation::family_with_content(state.clone()));
    tx.create_node(SemanticNodeCreation::family());
    let final_object = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        content(),
    )));
    let prepared = tx.prepare(&mut store).unwrap();
    let proposed = prepared.proposed_object_state(root).unwrap();
    assert_eq!(proposed.insertion_order(), 1);
    assert_eq!(
        prepared
            .proposed_object_state(final_object)
            .unwrap()
            .insertion_order(),
        2
    );
    assert_eq!(prepared.z_index(root).unwrap(), 7.0);
    assert_eq!(proposed.style, state.style);
    assert_eq!(proposed.transform, state.transform);
    let root = prepared.commit().resolve(root).unwrap();
    assert!(store.semantic_family_checked(root).is_ok());
    assert_eq!(
        store.semantic_object_state_checked(root).unwrap(),
        &proposed
    );
}

#[test]
fn owned_family_creation_reserves_capacity_and_invalid_content_is_atomic() {
    let mut store = SemanticStore::new();
    store.set_next_insertion_order_for_test(u64::MAX);
    let mut tx = SemanticMutationTransaction::new();
    tx.create_node(SemanticNodeCreation::family_with_content(
        SemanticObjectState::new(content()),
    ));
    assert!(matches!(
        tx.prepare(&mut store),
        Err(SemanticMutationTransactionError::InsertionOrderExhausted)
    ));
    assert_eq!(store.len(), 0);

    let mut store = SemanticStore::new();
    let mut state = SemanticObjectState::new(content());
    state.style.stroke_width = f64::NAN;
    let mut tx = SemanticMutationTransaction::new();
    tx.create_node(SemanticNodeCreation::family_with_content(state));
    assert!(tx.apply(&mut store).is_err());
    assert_eq!(store.len(), 0);
}

#[test]
fn owned_family_creation_and_prepared_paint_removal_preserve_independent_fallback() {
    for painted in [false, true] {
        let mut store = SemanticStore::new();
        let mut state = SemanticObjectState::new(content());
        state.style = SemanticStyle::manim_family_root(Color::BLUE);
        if !painted {
            state.style.fill = None;
            state.style.stroke = None;
        }
        let mut tx = SemanticMutationTransaction::new();
        let root = tx.create_node(SemanticNodeCreation::family_with_content_and_fallback(
            state.clone(),
            Color::RED,
        ));
        let prepared = tx.prepare(&mut store).unwrap();
        let selected = if painted { Color::BLUE } else { Color::RED };
        assert_eq!(prepared.family_color(root).unwrap(), selected);
        assert_eq!(prepared.object_state(root).unwrap().style, state.style);
        let root = prepared.commit().resolve(root).unwrap();
        assert_eq!(store.node(root).unwrap().family_color(), Some(selected));
        assert_eq!(
            store.node(root).unwrap().family_fallback_color(),
            Some(Color::RED)
        );

        let mut empty_paint = state.style;
        empty_paint.fill = None;
        empty_paint.stroke = None;
        let mut tx = SemanticMutationTransaction::new();
        tx.replace_style(root, empty_paint);
        let prepared = tx.prepare(&mut store).unwrap();
        assert_eq!(prepared.family_color(root).unwrap(), Color::RED);
        prepared.commit();
        assert_eq!(store.node(root).unwrap().family_color(), Some(Color::RED));
    }
}

#[test]
fn family_setters_update_fallback_even_when_visible_paint_already_matches() {
    for replace_style in [false, true] {
        let mut store = SemanticStore::new();
        let mut state = SemanticObjectState::new(content());
        state.style = SemanticStyle::manim_family_root(Color::BLUE);
        let mut tx = SemanticMutationTransaction::new();
        let root = tx.create_node(SemanticNodeCreation::family_with_content_and_fallback(
            state.clone(),
            Color::RED,
        ));
        let root = tx.apply(&mut store).unwrap().resolve(root).unwrap();
        let revision = store.scene_revision();
        let mut tx = SemanticMutationTransaction::new();
        if replace_style {
            tx.set_family_style(root, state.style.clone());
        } else {
            tx.set_family_color(root, Color::BLUE);
        }
        tx.apply(&mut store).unwrap();
        assert_ne!(store.scene_revision(), revision);
        assert_eq!(
            store.semantic_object_state_checked(root).unwrap().style,
            state.style
        );
        assert_eq!(
            store.node(root).unwrap().family_fallback_color(),
            Some(Color::BLUE)
        );
    }
}

#[test]
fn invalid_family_fallback_rejects_before_admitting_any_node() {
    for creation in [
        SemanticNodeCreation::family_with_content_and_fallback(
            SemanticObjectState::new(content()),
            Color::rgba(f32::NAN, 0.0, 0.0, 1.0),
        ),
        SemanticNodeCreation::Family {
            state: None,
            fallback_color: Some(Color::RED),
            source_identity: None,
        },
    ] {
        let mut store = SemanticStore::new();
        let revision = store.scene_revision();
        let mut tx = SemanticMutationTransaction::new();
        tx.create_node(creation);
        assert!(tx.apply(&mut store).is_err());
        assert_eq!(store.len(), 0);
        assert_eq!(store.scene_revision(), revision);
    }
}

#[test]
fn family_drawing_eligibility_survives_materialization_in_either_transaction_order() {
    for content_first in [false, true] {
        let mut store = SemanticStore::new();
        let mut tx = SemanticMutationTransaction::new();
        let root = tx.create_node(SemanticNodeCreation::family());
        if content_first {
            tx.replace_content(root, content());
        }
        tx.set_draw_enabled(root, false);
        if !content_first {
            tx.replace_content(root, content());
        }
        let prepared = tx.prepare(&mut store).unwrap();
        assert!(!prepared.draw_enabled(root).unwrap());
        let wanted = prepared.proposed_object_state(root).unwrap();
        assert!(!wanted.draw_enabled());
        assert_eq!(wanted.style, SemanticStyle::manim_family_root(Color::WHITE));
        let root = prepared.commit().resolve(root).unwrap();
        assert_eq!(store.semantic_object_state_checked(root).unwrap(), &wanted);
        assert!(
            !store
                .node(root)
                .unwrap()
                .presentation()
                .unwrap()
                .draw_enabled
        );
    }
}

#[test]
fn unmaterialized_family_drawing_edit_is_local_atomic_and_idempotent() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let before = store.node(root).unwrap().clone();
    let mut invalid = SemanticMutationTransaction::new();
    invalid
        .set_draw_enabled(root, false)
        .set_z_index(root, f64::NAN);
    assert!(invalid.apply(&mut store).is_err());
    assert_eq!(store.node(root).unwrap(), &before);
    let mut tx = SemanticMutationTransaction::new();
    tx.set_draw_enabled(root, false);
    let prepared = tx.prepare(&mut store).unwrap();
    assert!(!prepared.draw_enabled(root).unwrap());
    assert!(prepared.object_state(root).is_err());
    prepared.commit();
    assert_eq!(store.len(), 1);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    let revision = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_draw_enabled(root, false);
    assert!(tx.apply(&mut store).unwrap().impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn duplicate_drawing_edits_reject_existing_and_pending_roots_without_commit() {
    for pending in [false, true] {
        let mut store = SemanticStore::new();
        let existing = store.insert_family();
        let before = store.node(existing).unwrap().clone();
        let mut tx = SemanticMutationTransaction::new();
        let root: SemanticTransactionNodeRef = if pending {
            tx.create_node(SemanticNodeCreation::family()).into()
        } else {
            existing.into()
        };
        tx.set_draw_enabled(root, false)
            .set_draw_enabled(root, true);
        assert!(tx.apply(&mut store).is_err());
        assert_eq!(store.node(existing).unwrap(), &before);
        assert_eq!(store.len(), 1);
    }
}
