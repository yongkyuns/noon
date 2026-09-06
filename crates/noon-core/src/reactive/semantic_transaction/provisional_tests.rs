use super::*;
use crate::{AnimationOptions, SemanticAnimationIntent, SemanticNodeResidency, SemanticVec3};

fn object_state(radius: f32) -> SemanticObjectState {
    SemanticObjectState::new(StoredGeometry::Circle { radius })
}

#[test]
fn pending_object_can_be_mutated_read_attached_and_resolved_atomically() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    let object = transaction.create_node(SemanticNodeCreation::object(object_state(1.0)));
    let translation = SemanticVec3::new(2.0, 3.0, 4.0);
    transaction
        .set_property(object, SemanticObjectProperty::Translation, translation)
        .add_member(family, object);

    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(
        prepared.object_state(object).unwrap().transform.translation,
        translation
    );
    assert_eq!(
        prepared.family_members(family).unwrap(),
        [SemanticTransactionNodeRef::Pending(object)]
    );
    assert_eq!(prepared.store().len(), 1);

    let result = prepared.commit();
    let committed = result.resolve(object).unwrap();
    assert_eq!(
        store
            .semantic_object_state_checked(committed)
            .unwrap()
            .transform
            .translation,
        translation
    );
    assert_eq!(store.node(family).unwrap().members(), [committed]);
    assert_eq!(
        store.node(committed).unwrap().residency(),
        SemanticNodeResidency::Detached
    );
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::NodeAdded { node: committed },
            SemanticMutationImpact::ObjectProperty {
                object: committed,
                property: SemanticObjectProperty::Translation,
            },
            SemanticMutationImpact::FamilyMemberAdded {
                family,
                member: committed,
            },
        ]
    );
}

#[test]
fn multiple_pending_nodes_support_ordered_edges_and_reject_cycles_before_allocation() {
    let mut store = SemanticStore::new();
    let before_capacity = store.slot_capacity();
    let before_revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    let outer = transaction.create_node(SemanticNodeCreation::family());
    let inner = transaction.create_node(SemanticNodeCreation::family());
    let leaf = transaction.create_node(SemanticNodeCreation::object(object_state(1.0)));
    transaction
        .add_member(outer, inner)
        .add_member(inner, leaf)
        .add_member(inner, outer);

    let error = match transaction.prepare(&mut store) {
        Ok(_) => panic!("cycle must fail preflight"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        SemanticMutationTransactionError::PendingFamilyCycle {
            index: 5,
            family: inner.into(),
            member: outer.into(),
        }
    );
    assert_eq!(store.len(), 0);
    assert_eq!(store.slot_capacity(), before_capacity);
    assert_eq!(store.scene_revision(), before_revision);
}

#[test]
fn pending_family_members_have_a_staged_and_committed_order() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let family = transaction.create_node(SemanticNodeCreation::family());
    let first = transaction.create_node(SemanticNodeCreation::object(object_state(1.0)));
    let second = transaction.create_node(SemanticNodeCreation::object(object_state(2.0)));
    transaction
        .add_member(family, first)
        .add_member(family, second)
        .reorder_member_ref(family, second, Some(first.into()));

    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(
        prepared.family_members(family).unwrap(),
        [second.into(), first.into()]
    );
    let result = prepared.commit();
    let family = result.resolve(family).unwrap();
    let first = result.resolve(first).unwrap();
    let second = result.resolve(second).unwrap();
    assert_eq!(store.node(family).unwrap().members(), [second, first]);
}

#[test]
fn animation_can_reference_two_pending_objects() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let target = transaction.create_node(SemanticNodeCreation::object(object_state(1.0)));
    let target_state = transaction.create_node(SemanticNodeCreation::object(object_state(2.0)));
    transaction.add_transform_animation(target, target_state, AnimationOptions::default());

    let result = transaction.apply(&mut store).unwrap();
    let target = result.resolve(target).unwrap();
    let target_state = result.resolve(target_state).unwrap();
    let SemanticMutationImpact::AnimationAdded { animation } = result.impacts()[2] else {
        panic!("expected committed animation identity")
    };
    assert_eq!(
        store.semantic_animation_state(animation).unwrap().intent(),
        &SemanticAnimationIntent::TransformTo {
            target,
            target_state,
            interpolation: SemanticTransformInterpolation::Affine,
        }
    );
}

#[test]
fn pending_tokens_are_rejected_by_other_transactions() {
    let mut store = SemanticStore::new();
    let mut owner = SemanticMutationTransaction::new();
    let pending = owner.create_node(SemanticNodeCreation::object(object_state(1.0)));
    let mut foreign = SemanticMutationTransaction::new();
    foreign.set_property(pending, SemanticObjectProperty::RotationZ, 0.5_f64);

    let error = match foreign.prepare(&mut store) {
        Ok(_) => panic!("foreign pending token must fail preflight"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        SemanticMutationTransactionError::PendingNodeFromDifferentTransaction {
            index: 0,
            token: pending,
        }
    );
    assert_eq!(store.len(), 0);
}

#[test]
fn pending_kind_errors_fail_without_publishing_identity() {
    let mut store = SemanticStore::new();
    let before_revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    let family = transaction.create_node(SemanticNodeCreation::family());
    transaction.set_property(family, SemanticObjectProperty::RotationZ, 0.5_f64);

    let error = match transaction.prepare(&mut store) {
        Ok(_) => panic!("pending family cannot accept object properties"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        SemanticMutationTransactionError::PendingNodeKindMismatch {
            index: 1,
            token: family,
            expected: SemanticPendingNodeKind::Object,
        }
    );
    assert_eq!(store.len(), 0);
    assert_eq!(store.scene_revision(), before_revision);
}

#[test]
fn terminal_pending_removal_cancels_creation_and_all_references() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let before_capacity = store.slot_capacity();
    let before_revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    let pending = transaction.create_node(SemanticNodeCreation::object(object_state(1.0)));
    transaction
        .set_property(pending, SemanticObjectProperty::RotationZ, 0.5_f64)
        .add_member(family, pending)
        .remove_node(pending);

    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(
        prepared.object_state(pending),
        Err(SemanticTransactionReadError::RemovedPendingNode(pending))
    );
    assert!(prepared.family_members(family).unwrap().is_empty());
    let result = prepared.commit();
    assert_eq!(result.resolve(pending), None);
    assert!(result.impacts().is_empty());
    assert_eq!(store.len(), 1);
    assert_eq!(store.slot_capacity(), before_capacity);
    assert_eq!(store.scene_revision(), before_revision);
    assert!(store.node(family).unwrap().members().is_empty());
}

#[test]
fn staged_reads_hide_existing_nodes_in_the_terminal_removal_closure() {
    let mut store = SemanticStore::new();
    let family = store.insert_family();
    let member = store.insert_semantic_object(object_state(1.0));
    store.add_member(family, member).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_node(member);

    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(
        prepared.object_state(member),
        Err(SemanticTransactionReadError::RemovedExistingNode(member))
    );
    assert!(prepared.family_members(family).unwrap().is_empty());
    prepared.commit();
    assert!(store.node(member).is_none());
}

#[test]
fn pending_removal_cancels_dependent_animation_but_keeps_unrelated_creation() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let removed_target = transaction.create_node(SemanticNodeCreation::object(object_state(1.0)));
    let surviving_target = transaction.create_node(SemanticNodeCreation::object(object_state(2.0)));
    transaction
        .add_transform_animation(
            removed_target,
            surviving_target,
            AnimationOptions::default(),
        )
        .remove_node(removed_target);

    let result = transaction.apply(&mut store).unwrap();
    assert_eq!(result.resolve(removed_target), None);
    let surviving_target = result.resolve(surviving_target).unwrap();
    assert_eq!(store.len(), 1);
    assert!(store.node(surviving_target).is_some());
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::NodeAdded {
            node: surviving_target,
        }]
    );
}

#[test]
fn pending_preflight_is_bounded_and_does_not_touch_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        store.insert_semantic_object(object_state(index as f32 + 1.0));
    }
    let before_len = store.len();
    let before_capacity = store.slot_capacity();
    let before_stats = store.last_mutation_stats();
    let mut transaction = SemanticMutationTransaction::new();
    let pending = transaction.create_node(SemanticNodeCreation::object(object_state(0.5)));
    transaction.set_property(
        pending,
        SemanticObjectProperty::Scale,
        SemanticVec3::new(2.0, 2.0, 1.0),
    );

    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(prepared.store().len(), before_len);
    assert_eq!(prepared.store().slot_capacity(), before_capacity);
    assert_eq!(prepared.store().last_mutation_stats(), before_stats);
    let result = prepared.commit();
    assert!(result.resolve(pending).is_some());
    assert_eq!(store.len(), before_len + 1);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}
