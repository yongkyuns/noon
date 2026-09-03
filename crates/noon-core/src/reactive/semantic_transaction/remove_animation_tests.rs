use super::*;
use crate::{AnimationOptions, SemanticObjectState, StoredGeometry};

fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
}

fn transform(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
    let target = object(store, radius);
    let target_state = object(store, radius + 0.5);
    store
        .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
        .unwrap()
}

fn scalar_input(store: &SemanticStore, signal: SemanticNodeId) -> f64 {
    let SemanticSignalSource::Input(SemanticSignalValue::Scalar(value)) =
        store.semantic_signal_state(signal).unwrap().source()
    else {
        panic!("expected scalar input signal")
    };
    *value
}

#[test]
fn remove_animation_cascades_parent_compositions_but_preserves_siblings() {
    let mut store = SemanticStore::new();
    let first = transform(&mut store, 1.0);
    let second = transform(&mut store, 2.0);
    let parent = store
        .insert_semantic_parallel_animation(&[first, second], AnimationOptions::new())
        .unwrap();
    let third = transform(&mut store, 3.0);
    let grandparent = store
        .insert_semantic_sequence_animation(&[parent, third], AnimationOptions::new())
        .unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_animation(first);
    let result = transaction.apply(&mut store).unwrap();

    assert!(store.node(first).is_none());
    assert!(store.node(parent).is_none());
    assert!(store.node(grandparent).is_none());
    assert!(store.semantic_animation_state(second).is_ok());
    assert!(store.semantic_animation_state(third).is_ok());
    assert_eq!(store.last_mutation_stats().slots_written, 3);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::NodeRemoved { node: first },
            SemanticMutationImpact::NodeRemoved { node: parent },
            SemanticMutationImpact::NodeRemoved { node: grandparent },
        ]
    );
}

#[test]
fn removing_composition_does_not_delete_referenced_children() {
    let mut store = SemanticStore::new();
    let first = transform(&mut store, 1.0);
    let second = transform(&mut store, 2.0);
    let composition = store
        .insert_semantic_parallel_animation(&[first, second], AnimationOptions::new())
        .unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_animation(composition);
    let result = transaction.apply(&mut store).unwrap();

    assert!(store.node(composition).is_none());
    assert!(store.semantic_animation_state(first).is_ok());
    assert!(store.semantic_animation_state(second).is_ok());
    assert_eq!(store.last_mutation_stats().slots_written, 1);
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::NodeRemoved { node: composition }]
    );
}

#[test]
fn remove_animation_rejects_non_animation_before_earlier_commit() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let not_animation = object(&mut store, 1.0);

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .remove_animation(not_animation);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::NotAnimation {
            index: 1,
            animation: not_animation,
        })
    );
    assert_eq!(scalar_input(&store, signal), 1.0);
    assert!(store.node(not_animation).is_some());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn stale_remove_animation_target_rolls_back_earlier_mutation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let stale = transform(&mut store, 1.0);
    store.remove_node(stale).unwrap();
    let replacement = object(&mut store, 9.0);
    assert_eq!(stale.slot(), replacement.slot());
    assert_ne!(stale.generation(), replacement.generation());

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .remove_animation(stale);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::UnknownAnimation {
            index: 1,
            animation: stale,
        })
    );
    assert_eq!(scalar_input(&store, signal), 1.0);
    assert!(store.node(replacement).is_some());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn remove_animation_is_terminal_with_other_structural_removals() {
    let mut store = SemanticStore::new();
    let animation = transform(&mut store, 1.0);
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();

    let mut invalid_order = SemanticMutationTransaction::new();
    invalid_order
        .remove_animation(animation)
        .set_signal(signal, 2.0_f64);
    assert_eq!(
        invalid_order.apply(&mut store),
        Err(SemanticMutationTransactionError::MutationAfterRemove { index: 1 })
    );
    assert!(store.semantic_animation_state(animation).is_ok());
    assert_eq!(scalar_input(&store, signal), 1.0);
    assert_eq!(store.last_mutation_stats().slots_written, 0);

    let unrelated = object(&mut store, 8.0);
    let mut valid_terminal = SemanticMutationTransaction::new();
    valid_terminal
        .remove_animation(animation)
        .remove_node(unrelated);
    let result = valid_terminal.apply(&mut store).unwrap();
    assert!(store.node(animation).is_none());
    assert!(store.node(unrelated).is_none());
    assert_eq!(result.impacts().len(), 2);
}

#[test]
fn duplicate_typed_and_generic_removal_of_one_animation_is_rejected() {
    let mut store = SemanticStore::new();
    let animation = transform(&mut store, 1.0);

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .remove_animation(animation)
        .remove_node(animation);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::DuplicateNodeRemoval {
            index: 1,
            node: animation,
        })
    );
    assert!(store.semantic_animation_state(animation).is_ok());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn later_explicit_parent_removal_is_satisfied_by_earlier_cascade() {
    let mut store = SemanticStore::new();
    let first = transform(&mut store, 1.0);
    let second = transform(&mut store, 2.0);
    let parent = store
        .insert_semantic_parallel_animation(&[first, second], AnimationOptions::new())
        .unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_animation(first).remove_animation(parent);
    let result = transaction.apply(&mut store).unwrap();

    assert!(store.node(first).is_none());
    assert!(store.node(parent).is_none());
    assert!(store.semantic_animation_state(second).is_ok());
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(
        result.impacts(),
        &[
            SemanticMutationImpact::NodeRemoved { node: first },
            SemanticMutationImpact::NodeRemoved { node: parent },
        ]
    );
}

#[test]
fn remove_animation_cleanup_is_local_with_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let first = transform(&mut store, 0.25);
    let second = transform(&mut store, 0.5);
    let parent = store
        .insert_semantic_sequence_animation(&[first, second], AnimationOptions::new())
        .unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_animation(first);
    let result = transaction.apply(&mut store).unwrap();

    assert!(store.node(first).is_none());
    assert!(store.node(parent).is_none());
    assert!(store.semantic_animation_state(second).is_ok());
    assert_eq!(store.last_mutation_stats().slots_written, 2);
    assert_eq!(result.impacts().len(), 2);
}
