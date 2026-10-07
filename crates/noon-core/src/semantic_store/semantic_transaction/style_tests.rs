use super::*;
use crate::{SemanticObjectState, SemanticPaint, StoredGeometry, StrokeCap, StrokeJoin, Style};

fn object(store: &mut SemanticStore) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }))
}

fn replacement_style() -> SemanticStyle {
    let mut style = SemanticStyle::from_compact(Style {
        stroke_join: StrokeJoin::Bevel,
        stroke_cap: StrokeCap::Square,
        ..Style::default()
    });
    style.fill = Some(SemanticPaint::Solid(crate::Color::RED));
    style.stroke_width = 3.5;
    style
}

fn world_path_state(store: &mut SemanticStore, cairo: bool) -> SemanticObjectState {
    let handle = store
        .insert_geometry_path(
            crate::VectorPath::new()
                .move_to(crate::Vec2::ZERO)
                .line_to(crate::Vec2::new(1.0, 0.0)),
        )
        .unwrap();
    let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
    state.transform = crate::SemanticWorldTransform3D::IDENTITY.into();
    state.style.stroke = Some(SemanticPaint::Solid(crate::Color::WHITE));
    state.style.stroke_width = 0.1;
    if cairo {
        state.set_spatial_material(crate::SemanticSpatialMaterial::CairoPath);
        state
            .set_cairo_path_appearance(crate::SemanticCairoPathAppearance::default())
            .unwrap();
    }
    state
}

#[test]
fn world_path_partial_paint_rejects_the_complete_transaction_and_allows_disabled_paint() {
    for cairo in [false, true] {
        let mut store = SemanticStore::new();
        let state = world_path_state(&mut store, cairo);
        let target = store.insert_semantic_object(state.clone());
        let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let revision = store.scene_revision();
        let resources = store.geometry_resources().stats();
        for property in [
            SemanticObjectProperty::FillOpacity,
            SemanticObjectProperty::StrokeOpacity,
            SemanticObjectProperty::ObjectOpacity,
        ] {
            let mut transaction = SemanticMutationTransaction::new();
            transaction
                .set_signal(signal, 2.0_f64)
                .set_property(target, property, 0.5_f64);
            assert_eq!(
                transaction.apply(&mut store),
                Err(
                    SemanticMutationTransactionError::UnsupportedWorldPathStyle {
                        object: target.into(),
                    }
                )
            );
            assert_eq!(store.semantic_object_state_checked(target).unwrap(), &state);
            assert_eq!(store.scene_revision(), revision);
            assert_eq!(store.geometry_resources().stats(), resources);
            assert_eq!(store.last_mutation_stats().slots_written, 0);
            assert_eq!(
                store.semantic_signal_state(signal).unwrap().source(),
                &SemanticSignalSource::Input(1.0_f64.into())
            );
        }
        let mut style = state.style.clone();
        style.fill = Some(SemanticPaint::Solid(crate::Color {
            alpha: 0.5,
            ..crate::Color::WHITE
        }));
        let mut replace = SemanticMutationTransaction::new();
        replace.replace_style(target, style);
        assert!(matches!(
            replace.apply(&mut store),
            Err(SemanticMutationTransactionError::UnsupportedWorldPathStyle { .. })
        ));
        let mut disable = SemanticMutationTransaction::new();
        disable
            .set_property(target, SemanticObjectProperty::FillOpacity, 0.0_f64)
            .set_property(target, SemanticObjectProperty::StrokeOpacity, 0.0_f64);
        disable.apply(&mut store).unwrap();
    }
}

#[test]
fn world_path_creation_uses_final_style_and_preserves_overlay_transparency() {
    let mut store = SemanticStore::new();
    let mut state = world_path_state(&mut store, true);
    state.style.fill_opacity = 0.5;
    let revision = store.scene_revision();
    let nodes = store.len();
    let mut invalid = SemanticMutationTransaction::new();
    let target = invalid.create_node(SemanticNodeCreation::object(state.clone()));
    assert_eq!(
        invalid.apply(&mut store),
        Err(
            SemanticMutationTransactionError::UnsupportedWorldPathStyle {
                object: target.into(),
            }
        )
    );
    assert_eq!(store.len(), nodes);
    assert_eq!(store.scene_revision(), revision);

    let mut valid = SemanticMutationTransaction::new();
    let target = valid.create_node(SemanticNodeCreation::object(state));
    valid.set_property(target, SemanticObjectProperty::FillOpacity, 1.0_f64);
    valid.apply(&mut store).unwrap();

    let mut overlay = world_path_state(&mut store, false);
    overlay
        .set_spatial_composition_domain(crate::SemanticSpatialCompositionDomain::FixedOrientation)
        .unwrap();
    overlay.style.fill_opacity = 0.5;
    let mut transaction = SemanticMutationTransaction::new();
    transaction.create_node(SemanticNodeCreation::object(overlay));
    transaction.apply(&mut store).unwrap();

    let planar = object(&mut store);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(planar, SemanticObjectProperty::FillOpacity, 0.5_f64);
    transaction.apply(&mut store).unwrap();
}

#[test]
fn replace_style_changes_only_style_and_publishes_once() {
    let mut store = SemanticStore::new();
    let target = object(&mut store);
    let before = store.semantic_object_state_checked(target).unwrap().clone();
    let before_revision = store.scene_revision();
    let replacement = replacement_style();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.replace_style(target, replacement.clone());
    let result = transaction.apply(&mut store).unwrap();

    let after = store.semantic_object_state_checked(target).unwrap();
    assert_eq!(after.style, replacement);
    assert_eq!(after.content, before.content);
    assert_eq!(after.transform, before.transform);
    assert_eq!(after.presentation(), before.presentation());
    assert_eq!(after.signal_bindings(), before.signal_bindings());
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(
        store.scene_revision(),
        before_revision.checked_next().unwrap()
    );
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::ObjectStyle { object: target }]
    );
}

#[test]
fn unchanged_style_is_a_noop_and_invalid_style_fails_atomically() {
    let mut store = SemanticStore::new();
    let target = object(&mut store);
    let original = store
        .semantic_object_state_checked(target)
        .unwrap()
        .style
        .clone();
    let original_revision = store.scene_revision();

    let mut unchanged = SemanticMutationTransaction::new();
    unchanged.replace_style(target, original.clone());
    let result = unchanged.apply(&mut store).unwrap();
    assert!(result.impacts().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
    assert_eq!(store.scene_revision(), original_revision);

    let other = object(&mut store);
    let mut invalid = replacement_style();
    invalid.stroke_width = f64::NAN;
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_property(other, SemanticObjectProperty::RotationZ, 0.5_f64)
        .replace_style(target, invalid);
    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidStyle {
            index: 1,
            object: target,
        })
    );
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap().style,
        original
    );
    assert_eq!(
        store
            .semantic_object_state_checked(other)
            .unwrap()
            .transform
            .planar_rotation()
            .unwrap(),
        0.0
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let mut invalid_paint = replacement_style();
    invalid_paint.fill = Some(SemanticPaint::Solid(crate::Color {
        red: f32::NAN,
        ..crate::Color::RED
    }));
    let mut paint_transaction = SemanticMutationTransaction::new();
    paint_transaction
        .set_property(other, SemanticObjectProperty::RotationZ, 0.5_f64)
        .replace_style(target, invalid_paint);
    assert_eq!(
        paint_transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidStyle {
            index: 1,
            object: target,
        })
    );
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap().style,
        original
    );
    assert_eq!(
        store
            .semantic_object_state_checked(other)
            .unwrap()
            .transform
            .planar_rotation()
            .unwrap(),
        0.0
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn full_style_and_scalar_style_writes_conflict_in_either_order() {
    let mut store = SemanticStore::new();
    let target = object(&mut store);

    let mut replacement_first = SemanticMutationTransaction::new();
    replacement_first
        .replace_style(target, replacement_style())
        .set_property(target, SemanticObjectProperty::StrokeWidth, 2.0_f64);
    assert_eq!(
        replacement_first.apply(&mut store),
        Err(SemanticMutationTransactionError::ConflictingStyleMutation {
            index: 1,
            object: target,
        })
    );

    let mut property_first = SemanticMutationTransaction::new();
    property_first
        .set_property(target, SemanticObjectProperty::FillOpacity, 0.5_f64)
        .replace_style(target, replacement_style());
    assert_eq!(
        property_first.apply(&mut store),
        Err(SemanticMutationTransactionError::ConflictingStyleMutation {
            index: 1,
            object: target,
        })
    );

    let mut distinct_properties = SemanticMutationTransaction::new();
    distinct_properties
        .set_property(target, SemanticObjectProperty::FillOpacity, 0.5_f64)
        .set_property(target, SemanticObjectProperty::StrokeWidth, 2.0_f64);
    assert!(distinct_properties.apply(&mut store).is_ok());
}

#[test]
fn duplicate_style_and_non_object_target_fail_before_commit() {
    let mut store = SemanticStore::new();
    let target = object(&mut store);
    let mut duplicate = SemanticMutationTransaction::new();
    duplicate
        .replace_style(target, replacement_style())
        .replace_style(target, SemanticStyle::default());
    assert_eq!(
        duplicate.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateStyle {
            index: 1,
            object: target,
        })
    );

    let family = store.insert_family();
    let mut wrong_target = SemanticMutationTransaction::new();
    wrong_target.replace_style(family, replacement_style());
    assert_eq!(
        wrong_target.apply(&mut store),
        Err(SemanticMutationTransactionError::Object {
            index: 0,
            error: SemanticSceneOperationError::NotSemanticObject(family),
        })
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}
