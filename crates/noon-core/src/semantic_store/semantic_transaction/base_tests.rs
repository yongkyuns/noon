use super::*;
use crate::{
    SemanticObjectState, SemanticSignalBinding, SemanticSignalExpr, SemanticVec3, StoredGeometry,
};

fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
}

fn input_value(store: &SemanticStore, signal: SemanticNodeId) -> SemanticSignalValue {
    let SemanticSignalSource::Input(value) = store.semantic_signal_state(signal).unwrap().source()
    else {
        panic!("expected input signal")
    };
    value.clone()
}

fn property_value(
    store: &SemanticStore,
    object: SemanticNodeId,
    property: SemanticObjectProperty,
) -> SemanticSignalValue {
    object_property_value(
        store.semantic_object_state_checked(object).unwrap(),
        property,
    )
}

#[test]
fn multiple_signal_values_commit_after_complete_preflight() {
    let mut store = SemanticStore::new();
    let scalar = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let vector = store
        .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
        .unwrap();
    let before_len = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(scalar, 2.5_f64)
        .set_signal(vector, SemanticVec3::new(4.0, 5.0, 6.0));

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        input_value(&store, scalar),
        SemanticSignalValue::Scalar(2.5)
    );
    assert_eq!(
        input_value(&store, vector),
        SemanticSignalValue::Vec3(SemanticVec3::new(4.0, 5.0, 6.0))
    );
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::SignalValue { signal: scalar },
            SemanticMutationImpact::SignalValue { signal: vector },
        ]
    );
}

#[test]
fn mixed_signal_and_properties_commit_atomically_and_count_unique_slots() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let target = object(&mut store, 2.0);
    let translation = SemanticVec3::new(4.0, -2.0, 7.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 3.0_f64)
        .set_property(target, SemanticObjectProperty::Translation, translation)
        .set_property(target, SemanticObjectProperty::ObjectOpacity, 0.4_f64);

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        input_value(&store, signal),
        SemanticSignalValue::Scalar(3.0)
    );
    assert_eq!(
        property_value(&store, target, SemanticObjectProperty::Translation),
        SemanticSignalValue::Vec3(translation)
    );
    assert_eq!(
        property_value(&store, target, SemanticObjectProperty::ObjectOpacity),
        SemanticSignalValue::Scalar(0.4)
    );
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::SignalValue { signal },
            SemanticMutationImpact::ObjectProperty {
                object: target,
                property: SemanticObjectProperty::Translation,
            },
            SemanticMutationImpact::ObjectProperty {
                object: target,
                property: SemanticObjectProperty::ObjectOpacity,
            },
        ]
    );
}

#[test]
fn invalid_late_value_prevents_earlier_valid_mutation() {
    let mut store = SemanticStore::new();
    let first = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let second = store.insert_semantic_input_signal(2.0_f64).unwrap();
    let first_before = input_value(&store, first);
    let second_before = input_value(&store, second);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(first, 10.0_f64)
        .set_signal(second, f64::NAN);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Signal {
            index: 1,
            error: SemanticSignalError::NonFiniteValue,
        })
    );
    assert_eq!(input_value(&store, first), first_before);
    assert_eq!(input_value(&store, second), second_before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn invalid_late_property_prevents_earlier_signal_and_property_mutation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let target = object(&mut store, 2.0);
    let signal_before = input_value(&store, signal);
    let translation_before = property_value(&store, target, SemanticObjectProperty::Translation);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 5.0_f64)
        .set_property(
            target,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(1.0, 2.0, 3.0),
        )
        .set_property(target, SemanticObjectProperty::StrokeWidth, f64::NAN);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::NonFinitePropertyValue {
            index: 2,
            object: target,
            property: SemanticObjectProperty::StrokeWidth,
        })
    );
    assert_eq!(input_value(&store, signal), signal_before);
    assert_eq!(
        property_value(&store, target, SemanticObjectProperty::Translation),
        translation_before
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn stale_late_target_prevents_earlier_valid_mutation() {
    let mut store = SemanticStore::new();
    let first = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let stale = store.insert_semantic_input_signal(2.0_f64).unwrap();
    store.remove_node(stale).unwrap();
    let replacement = object(&mut store, 3.0);
    assert_eq!(stale.slot(), replacement.slot());
    assert_ne!(stale.generation(), replacement.generation());
    let first_before = input_value(&store, first);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(first, 10.0_f64)
        .set_signal(stale, 20.0_f64);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Signal {
            index: 1,
            error: SemanticSignalError::UnknownSignal(stale),
        })
    );
    assert_eq!(input_value(&store, first), first_before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn stale_property_target_prevents_earlier_valid_mutation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let stale = object(&mut store, 2.0);
    store.remove_node(stale).unwrap();
    let replacement = object(&mut store, 3.0);
    assert_eq!(stale.slot(), replacement.slot());
    assert_ne!(stale.generation(), replacement.generation());
    let signal_before = input_value(&store, signal);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_signal(signal, 2.0_f64).set_property(
        stale,
        SemanticObjectProperty::RotationZ,
        0.5_f64,
    );

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::Object {
            index: 1,
            error: SemanticSceneOperationError::UnknownNode(stale),
        })
    );
    assert_eq!(input_value(&store, signal), signal_before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn duplicate_target_is_rejected_before_mutation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let before = input_value(&store, signal);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .set_signal(signal, 3.0_f64);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateTarget {
            index: 1,
            target: signal,
        })
    );
    assert_eq!(input_value(&store, signal), before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn duplicate_property_is_rejected_but_distinct_properties_share_one_object() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let mut duplicate = SemanticMutationTransaction::new();
    duplicate
        .set_property(target, SemanticObjectProperty::RotationZ, 0.5_f64)
        .set_property(target, SemanticObjectProperty::RotationZ, 1.0_f64);

    assert_eq!(
        duplicate.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateProperty {
            index: 1,
            object: target,
            property: SemanticObjectProperty::RotationZ,
        })
    );
    assert_eq!(
        property_value(&store, target, SemanticObjectProperty::RotationZ),
        SemanticSignalValue::Scalar(0.0)
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let mut distinct = SemanticMutationTransaction::new();
    distinct
        .set_property(target, SemanticObjectProperty::RotationZ, 0.5_f64)
        .set_property(target, SemanticObjectProperty::StrokeWidth, 3.0_f64);
    let result = distinct.apply(&mut store).unwrap();
    assert_eq!(result.impacts().len(), 2);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn type_mismatch_and_derived_signal_targets_fail_atomically() {
    let mut store = SemanticStore::new();
    let scalar = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let derived = store
        .insert_semantic_derived_signal(SemanticSignalExpr::signal(scalar))
        .unwrap();
    let scalar_before = input_value(&store, scalar);

    let mut mismatch = SemanticMutationTransaction::new();
    mismatch.set_signal(scalar, SemanticVec3::new(1.0, 2.0, 3.0));
    assert_eq!(
        mismatch.apply(&mut store),
        Err(SemanticMutationTransactionError::SignalTypeMismatch {
            index: 0,
            signal: scalar,
            expected: SemanticSignalValueKind::Scalar,
            actual: SemanticSignalValueKind::Vec3,
        })
    );
    assert_eq!(input_value(&store, scalar), scalar_before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let mut derived_target = SemanticMutationTransaction::new();
    derived_target.set_signal(derived, 4.0_f64);
    assert_eq!(
        derived_target.apply(&mut store),
        Err(SemanticMutationTransactionError::NotInputSignal {
            index: 0,
            signal: derived,
        })
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn property_type_and_target_kind_are_validated_before_mutation() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let family = store.insert_family();
    let mut mismatch = SemanticMutationTransaction::new();
    mismatch.set_property(target, SemanticObjectProperty::Scale, 2.0_f64);

    assert_eq!(
        mismatch.apply(&mut store),
        Err(SemanticMutationTransactionError::PropertyTypeMismatch {
            index: 0,
            object: target,
            property: SemanticObjectProperty::Scale,
            expected: SemanticSignalValueKind::Vec3,
            actual: SemanticSignalValueKind::Scalar,
        })
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let mut wrong_target = SemanticMutationTransaction::new();
    wrong_target.set_property(family, SemanticObjectProperty::RotationZ, 1.0_f64);
    assert_eq!(
        wrong_target.apply(&mut store),
        Err(SemanticMutationTransactionError::Object {
            index: 0,
            error: SemanticSceneOperationError::NotSemanticObject(family),
        })
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn presence_property_requires_a_typed_signal_binding() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(target, SemanticObjectProperty::Presence, false);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::UnsupportedPropertyWrite {
            index: 0,
            object: target.into(),
            property: SemanticObjectProperty::Presence,
        })
    );
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn unchanged_signal_is_a_noop_with_no_impact() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(true).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_signal(signal, true);

    let result = transaction.apply(&mut store).unwrap();

    assert!(result.impacts().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
    assert_eq!(input_value(&store, signal), SemanticSignalValue::Bool(true));
}

#[test]
fn unchanged_property_is_a_noop_with_no_impact() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(
        target,
        SemanticObjectProperty::Translation,
        SemanticVec3::ZERO,
    );

    let result = transaction.apply(&mut store).unwrap();

    assert!(result.impacts().is_empty());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn set_property_preserves_signal_binding_declarations() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
    let target = object(&mut store, 1.0);
    store
        .bind_semantic_signal(signal, target, SemanticObjectProperty::ObjectOpacity)
        .unwrap();
    let binding = SemanticSignalBinding::new(signal, SemanticObjectProperty::ObjectOpacity);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(target, SemanticObjectProperty::ObjectOpacity, 0.25_f64);

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        property_value(&store, target, SemanticObjectProperty::ObjectOpacity),
        SemanticSignalValue::Scalar(0.25)
    );
    assert_eq!(
        store.semantic_object_signal_bindings(target).unwrap(),
        &[binding]
    );
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::ObjectProperty {
            object: target,
            property: SemanticObjectProperty::ObjectOpacity,
        }]
    );
}

#[test]
fn transaction_writes_only_changed_signal_slots_with_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let first = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let second = store.insert_semantic_input_signal(2.0_f64).unwrap();
    let unchanged = store.insert_semantic_input_signal(3.0_f64).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(first, 10.0_f64)
        .set_signal(second, 20.0_f64)
        .set_signal(unchanged, 3.0_f64);

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(result.impacts().len(), 2);
}

#[test]
fn property_transaction_writes_only_target_slot_with_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let target = object(&mut store, 0.5);
    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_property(target, SemanticObjectProperty::RotationZ, 0.75_f64)
        .set_property(target, SemanticObjectProperty::StrokeWidth, 4.0_f64);

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(result.impacts().len(), 2);
}

#[test]
fn complete_spatial_transform_publishes_atomically() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let transform = crate::SemanticTransform {
        translation: SemanticVec3::new(2.0, 3.0, 4.0),
        scale: SemanticVec3::new(1.0, 2.0, 3.0),
        orientation: crate::SemanticOrientation::Spatial(
            crate::SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.75)
                .expect("valid rotation"),
        ),
    };
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_object_transform(target, transform);

    let result = transaction.apply(&mut store).expect("valid transform");

    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .transform,
        transform
    );
    assert_eq!(result.impacts().len(), 1);
}

#[test]
fn invalid_complete_transform_is_rejected_without_publication() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let before = store.semantic_object_state_checked(target).unwrap().clone();
    let invalid = crate::SemanticTransform {
        translation: SemanticVec3::new(f64::NAN, 0.0, 0.0),
        ..crate::SemanticTransform::default()
    };
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_object_transform(target, invalid);

    assert!(matches!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidObjectTransform { index: 0, object }) if object == target.into()
    ));
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap(),
        &before
    );
}

#[test]
fn spatial_composition_domain_publishes_as_one_object_mutation() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_spatial_composition_domain(
        target,
        crate::SemanticSpatialCompositionDomain::FixedFrame,
    );

    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .spatial_composition_domain(),
        crate::SemanticSpatialCompositionDomain::FixedFrame
    );
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::SpatialCompositionDomain { object: target }]
    );
}

#[test]
fn invalid_camera_composition_domain_rolls_back_transaction() {
    let mut store = SemanticStore::new();
    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    camera.set_role(crate::SemanticObjectRole::Camera3D);
    camera
        .set_camera_projection(Some(crate::SemanticProjection3D::Perspective {
            vertical_fov_radians: 1.0,
            near: 0.1,
            far: 100.0,
        }))
        .unwrap();
    let target = store.insert_semantic_object(camera);
    let before = store.semantic_object_state_checked(target).unwrap().clone();
    let revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_spatial_composition_domain(
        target,
        crate::SemanticSpatialCompositionDomain::FixedOrientation,
    );

    assert!(transaction.apply(&mut store).is_err());
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap(),
        &before
    );
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn fixed_orientation_anchor_accepts_self_or_containing_family() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let family = store.insert_family();
    store.add_member(family, target).unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_spatial_composition_domain_with_anchor(
        target,
        crate::SemanticSpatialCompositionDomain::FixedOrientation,
        Some(family),
    );
    transaction.apply(&mut store).unwrap();
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .spatial_anchor_family(),
        Some(family)
    );

    let mut self_anchor = SemanticMutationTransaction::new();
    self_anchor.set_spatial_composition_domain_with_anchor(
        target,
        crate::SemanticSpatialCompositionDomain::FixedOrientation,
        Some(target),
    );
    self_anchor.apply(&mut store).unwrap();
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .spatial_anchor_family(),
        Some(target)
    );
}

#[test]
fn invalid_fixed_orientation_anchor_rolls_back_and_rejects_unrelated_or_stale_roots() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let unrelated_family = store.insert_family();
    let before = store.semantic_object_state_checked(target).unwrap().clone();
    let revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_spatial_composition_domain_with_anchor(
        target,
        crate::SemanticSpatialCompositionDomain::FixedOrientation,
        Some(unrelated_family),
    );
    assert!(transaction.apply(&mut store).is_err());
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap(),
        &before
    );
    assert_eq!(store.scene_revision(), revision);

    let stale_root = store.insert_family();
    store.remove_node(stale_root).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_spatial_composition_domain_with_anchor(
        target,
        crate::SemanticSpatialCompositionDomain::FixedOrientation,
        Some(stale_root),
    );
    assert!(transaction.apply(&mut store).is_err());
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap(),
        &before
    );
}

#[test]
fn fixed_orientation_anchor_can_reference_a_family_created_in_the_same_transaction() {
    let mut store = SemanticStore::new();
    let before_len = store.len();
    let before_revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    let family = transaction.create_node(SemanticNodeCreation::family());
    let alias = transaction.create_node(SemanticNodeCreation::family());
    let child = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    transaction
        .set_spatial_composition_domain_with_anchor_ref(
            child,
            crate::SemanticSpatialCompositionDomain::FixedOrientation,
            Some(family.into()),
        )
        .add_member(family, alias)
        .add_member(alias, child);

    let result = transaction.apply(&mut store).unwrap();
    let family_id = result.resolve(family).unwrap();
    let alias_id = result.resolve(alias).unwrap();
    let child_id = result.resolve(child).unwrap();
    assert_eq!(
        store.node(family_id).unwrap().first_member(),
        Some(alias_id)
    );
    assert_eq!(store.node(alias_id).unwrap().first_member(), Some(child_id));
    assert_eq!(store.len(), before_len + 3);
    assert_eq!(
        store
            .semantic_object_state_checked(child_id)
            .unwrap()
            .spatial_anchor_family(),
        Some(family_id)
    );
    assert_eq!(
        store.scene_revision(),
        before_revision.checked_next().unwrap()
    );
}

#[test]
fn prepared_object_updates_resolve_pending_spatial_anchor_before_publication() {
    let mut store = SemanticStore::new();
    let child = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    let family = transaction.create_node(SemanticNodeCreation::family());
    transaction
        .add_member(family, child)
        .set_spatial_composition_domain_with_anchor_ref(
            child,
            crate::SemanticSpatialCompositionDomain::FixedOrientation,
            Some(family.into()),
        );

    let prepared = transaction.prepare(&mut store).unwrap();
    let updates = prepared.object_updates().collect::<Vec<_>>();
    assert_eq!(updates.len(), 1);
    let proposed_anchor = updates[0].1.spatial_anchor_family().unwrap();
    let result = prepared.commit();
    assert_eq!(result.resolve(family), Some(proposed_anchor));
    assert_eq!(
        store
            .semantic_object_state_checked(child)
            .unwrap()
            .spatial_anchor_family(),
        Some(proposed_anchor)
    );
}

#[test]
fn prepared_anchor_updates_follow_clear_and_reassign_last_write() {
    let mut store = SemanticStore::new();
    let child = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    let first_family = transaction.create_node(SemanticNodeCreation::family());
    let final_family = transaction.create_node(SemanticNodeCreation::family());
    transaction
        .add_member(first_family, child)
        .add_member(final_family, child)
        .set_spatial_composition_domain_with_anchor_ref(
            child,
            crate::SemanticSpatialCompositionDomain::FixedOrientation,
            Some(first_family.into()),
        )
        .set_spatial_composition_domain_with_anchor_ref(
            child,
            crate::SemanticSpatialCompositionDomain::FixedOrientation,
            None,
        )
        .set_spatial_composition_domain_with_anchor_ref(
            child,
            crate::SemanticSpatialCompositionDomain::FixedOrientation,
            Some(final_family.into()),
        );

    let prepared = transaction.prepare(&mut store).unwrap();
    let updates = prepared.object_updates().collect::<Vec<_>>();
    assert_eq!(updates.len(), 1);
    let proposed_anchor = updates[0].1.spatial_anchor_family().unwrap();
    let result = prepared.commit();
    assert_eq!(result.resolve(final_family), Some(proposed_anchor));
    assert_eq!(
        store
            .semantic_object_state_checked(child)
            .unwrap()
            .spatial_anchor_family(),
        Some(proposed_anchor)
    );
}

#[test]
fn invalid_pending_spatial_anchors_reject_without_publishing_any_nodes() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let before_state = store.semantic_object_state_checked(target).unwrap().clone();
    let before_len = store.len();
    let before_revision = store.scene_revision();

    let mut unrelated = SemanticMutationTransaction::new();
    let family = unrelated.create_node(SemanticNodeCreation::family());
    unrelated.set_spatial_composition_domain_with_anchor_ref(
        target,
        crate::SemanticSpatialCompositionDomain::FixedOrientation,
        Some(family.into()),
    );
    assert!(unrelated.apply(&mut store).is_err());

    let mut signal_anchor = SemanticMutationTransaction::new();
    let signal = signal_anchor.create_node(SemanticNodeCreation::input_signal(1.0_f64).unwrap());
    signal_anchor.set_spatial_composition_domain_with_anchor_ref(
        target,
        crate::SemanticSpatialCompositionDomain::FixedOrientation,
        Some(signal.into()),
    );
    assert!(signal_anchor.apply(&mut store).is_err());

    let mut foreign_owner = SemanticMutationTransaction::new();
    let foreign = foreign_owner.create_node(SemanticNodeCreation::family());
    let mut foreign_reference = SemanticMutationTransaction::new();
    foreign_reference.set_spatial_composition_domain_with_anchor_ref(
        target,
        crate::SemanticSpatialCompositionDomain::FixedOrientation,
        Some(foreign.into()),
    );
    assert!(foreign_reference.apply(&mut store).is_err());

    let mut removed = SemanticMutationTransaction::new();
    let removed_family = removed.create_node(SemanticNodeCreation::family());
    removed
        .add_member(removed_family, target)
        .set_spatial_composition_domain_with_anchor_ref(
            target,
            crate::SemanticSpatialCompositionDomain::FixedOrientation,
            Some(removed_family.into()),
        )
        .remove_node(removed_family);
    assert!(removed.apply(&mut store).is_err());
    assert_eq!(store.len(), before_len);
    assert_eq!(store.scene_revision(), before_revision);
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap(),
        &before_state
    );

    let mut removed_owner = SemanticMutationTransaction::new();
    let owner = removed_owner.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 2.0 },
    )));
    let owner_anchor = removed_owner.create_node(SemanticNodeCreation::family());
    removed_owner
        .add_member(owner_anchor, owner)
        .set_spatial_composition_domain_with_anchor_ref(
            owner,
            crate::SemanticSpatialCompositionDomain::FixedOrientation,
            Some(owner_anchor.into()),
        )
        .remove_node(owner);
    let result = removed_owner.apply(&mut store).unwrap();
    assert!(result.resolve(owner).is_none());
    assert!(result.resolve(owner_anchor).is_some());

    assert_eq!(store.len(), before_len + 1);
    assert_eq!(
        store.scene_revision(),
        before_revision.checked_next().unwrap()
    );
    assert_eq!(
        store.semantic_object_state_checked(target).unwrap(),
        &before_state
    );
}
