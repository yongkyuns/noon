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
