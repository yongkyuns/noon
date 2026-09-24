use super::*;
use crate::{SemanticPointerClickAction, SemanticVec3, YELLOW};

fn state() -> SemanticObjectState {
    SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 })
}

#[test]
fn pending_click_binding_is_staged_once_and_self_targeted_after_resolution() {
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let target = tx.create_node(SemanticNodeCreation::object(state()));
    let action = Some(SemanticPointerClickAction::default());
    tx.set_pointer_click_action(target, action);
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(
        prepared
            .object_state(target)
            .unwrap()
            .pointer_click_action(),
        action
    );
    assert_eq!(prepared.store().len(), 0);
    let result = prepared.commit();
    let target = result.resolve(target).unwrap();
    assert_eq!(
        store
            .semantic_object_state_checked(target)
            .unwrap()
            .pointer_click_action(),
        action
    );
    assert!(result
        .impacts()
        .contains(&SemanticMutationImpact::PointerClickAction { object: target }));
}

#[test]
fn binding_metadata_is_local_and_identical_replacement_is_a_noop() {
    let mut store = SemanticStore::new();
    let target = store.insert_semantic_object(state());
    for _ in 0..9999 {
        store.insert_semantic_object(state());
    }
    let before = store.semantic_object_state_checked(target).unwrap().clone();
    let action = Some(SemanticPointerClickAction::default());
    let mut tx = SemanticMutationTransaction::new();
    tx.set_pointer_click_action(target, action);
    tx.apply(&mut store).unwrap();
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    let after = store.semantic_object_state_checked(target).unwrap();
    assert_eq!(after.transform, before.transform);
    assert_eq!(after.style, before.style);
    assert_eq!(after.presentation(), before.presentation());
    let revision = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_pointer_click_action(target, action);
    assert!(tx.apply(&mut store).unwrap().impacts().is_empty());
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn invalid_and_duplicate_click_declarations_rollback_other_edits() {
    let mut store = SemanticStore::new();
    let target = store.insert_semantic_object(state());
    let before = store.semantic_object_state_checked(target).unwrap().clone();
    let revision = store.scene_revision();
    for (scale, color, duration) in [
        (f64::NAN, YELLOW, 1.0),
        (-0.1, YELLOW, 1.0),
        (f64::MAX, YELLOW, 1.0),
        (1.2, YELLOW, 0.0),
        (1.2, YELLOW, -1.0),
        (1.2, YELLOW, f64::INFINITY),
        (1.2, crate::Color::rgba(f32::NAN, 0.0, 0.0, 1.0), 1.0),
    ] {
        let mut tx = SemanticMutationTransaction::new();
        tx.set_property(
            target,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(3.0, 0.0, 0.0),
        );
        tx.set_pointer_click_action(
            target,
            Some(SemanticPointerClickAction::indicate(scale, color, duration)),
        );
        assert!(matches!(
            tx.apply(&mut store),
            Err(SemanticMutationTransactionError::InvalidPointerClickAction { .. })
        ));
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(
            store.semantic_object_state_checked(target).unwrap(),
            &before
        );
    }
    let mut tx = SemanticMutationTransaction::new();
    tx.set_pointer_click_action(target, Some(SemanticPointerClickAction::default()));
    tx.set_pointer_click_action(target, None);
    assert!(matches!(
        tx.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicatePointerClickAction { .. })
    ));
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn visual_replacement_preserves_receiver_binding_and_copy_keeps_self_action() {
    let mut receiver = state();
    receiver.set_pointer_click_action(Some(SemanticPointerClickAction::default()));
    let mut target = state();
    target.transform.translation = SemanticVec3::new(4.0, 0.0, 0.0);
    target.set_pointer_click_action(Some(SemanticPointerClickAction::indicate(
        2.0,
        crate::RED,
        2.0,
    )));
    let replaced = receiver.with_visual_state_from(&target);
    assert_eq!(
        replaced.pointer_click_action(),
        receiver.pointer_click_action()
    );
    assert_eq!(replaced.transform, target.transform);
    assert_eq!(
        receiver.clone().pointer_click_action(),
        receiver.pointer_click_action()
    );
}

#[test]
fn removed_generations_cannot_receive_a_binding() {
    let mut store = SemanticStore::new();
    let node = store.insert_semantic_object(state());
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(node);
    tx.apply(&mut store).unwrap();
    let replacement = store.insert_semantic_object(state());
    assert_ne!(replacement, node);
    let revision = store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_pointer_click_action(node, Some(SemanticPointerClickAction::default()));
    assert!(tx.apply(&mut store).is_err());
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(
        store
            .semantic_object_state_checked(replacement)
            .unwrap()
            .pointer_click_action(),
        None
    );
}
