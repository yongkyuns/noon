use super::*;
use crate::{SemanticObjectState, SemanticPaint, StoredGeometry, StrokeCap, StrokeJoin, Style};

fn object(store: &mut SemanticStore) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }))
}

fn replacement_style() -> SemanticStyle {
    let mut style = SemanticStyle::from_legacy(Style {
        stroke_join: StrokeJoin::Bevel,
        stroke_cap: StrokeCap::Square,
        ..Style::default()
    });
    style.fill = Some(SemanticPaint::Solid(crate::Color::RED));
    style.stroke_width = 3.5;
    style
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
            .rotation_z,
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
            .rotation_z,
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
