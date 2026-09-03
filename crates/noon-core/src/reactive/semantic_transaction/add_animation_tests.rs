use super::*;
use crate::{
    AnimationOptions, SemanticAnimationCompositionKind, SemanticAnimationIntent,
    SemanticAnimationState, SemanticObjectState, StoredGeometry,
};

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

fn transform_state(
    target: SemanticNodeId,
    target_state: SemanticNodeId,
    options: AnimationOptions,
) -> SemanticAnimationState {
    SemanticAnimationState::new(
        SemanticAnimationIntent::TransformTo {
            target,
            target_state,
        },
        options,
    )
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
fn add_animation_commits_one_authored_animation_and_reports_its_identity() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let state = transform_state(
        target,
        target_state,
        AnimationOptions::new().run_time(1.5).lag_ratio(0.25),
    );
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_animation(state.clone());
    let result = transaction.apply(&mut store).unwrap();

    let [SemanticMutationImpact::AnimationAdded { animation }] = result.impacts() else {
        panic!("expected one animation-added impact")
    };
    assert_eq!(store.semantic_animation_state(*animation).unwrap(), &state);
    assert_eq!(store.len(), before_len + 1);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn add_animation_preserves_composition_order_and_unresolved_options() {
    let mut store = SemanticStore::new();
    let first = transform(&mut store, 1.0);
    let second = transform(&mut store, 2.0);
    let options = AnimationOptions::new().run_time(3.0).path_arc(0.75);
    let state = SemanticAnimationState::new(
        SemanticAnimationIntent::Composition {
            kind: SemanticAnimationCompositionKind::Sequence,
            children: vec![second, first],
        },
        options,
    );

    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_animation(state.clone());
    let result = transaction.apply(&mut store).unwrap();

    let [SemanticMutationImpact::AnimationAdded { animation }] = result.impacts() else {
        panic!("expected one animation-added impact")
    };
    assert_eq!(store.semantic_animation_state(*animation).unwrap(), &state);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn invalid_animation_target_rolls_back_earlier_mutation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let target = object(&mut store, 1.0);
    let family = store.insert_family();
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .add_animation(transform_state(target, family, AnimationOptions::new()));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::AnimationTarget {
            index: 1,
            error: SemanticSceneOperationError::NotSemanticObject(family),
        })
    );
    assert_eq!(scalar_input(&store, signal), 1.0);
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn stale_animation_reference_rolls_back_before_allocation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let target = object(&mut store, 1.0);
    let stale = object(&mut store, 2.0);
    store.remove_node(stale).unwrap();
    let replacement = object(&mut store, 3.0);
    assert_eq!(stale.slot(), replacement.slot());
    assert_ne!(stale.generation(), replacement.generation());
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .add_animation(transform_state(target, stale, AnimationOptions::new()));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::AnimationTarget {
            index: 1,
            error: SemanticSceneOperationError::UnknownNode(stale),
        })
    );
    assert_eq!(scalar_input(&store, signal), 1.0);
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn malformed_animation_options_use_equality_safe_transaction_errors() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let before_len = store.len();

    let mut invalid_run_time = SemanticMutationTransaction::new();
    invalid_run_time.add_animation(transform_state(
        target,
        target_state,
        AnimationOptions::new().run_time(0.0),
    ));
    assert_eq!(
        invalid_run_time.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidAnimationRunTime { index: 0 })
    );

    let mut invalid_lag_ratio = SemanticMutationTransaction::new();
    invalid_lag_ratio.add_animation(transform_state(
        target,
        target_state,
        AnimationOptions::new().lag_ratio(-0.1),
    ));
    assert_eq!(
        invalid_lag_ratio.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidAnimationLagRatio { index: 0 })
    );

    let mut invalid_path_arc = SemanticMutationTransaction::new();
    invalid_path_arc.add_animation(transform_state(
        target,
        target_state,
        AnimationOptions::new().path_arc(f64::NAN),
    ));
    assert_eq!(
        invalid_path_arc.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidAnimationPathArc { index: 0 })
    );

    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn animation_cannot_reference_a_node_removed_by_the_same_transaction() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_animation(transform_state(
            target,
            target_state,
            AnimationOptions::new(),
        ))
        .remove_node(target_state);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::AnimationUsesRemovedNode {
            index: 0,
            node: target_state,
        })
    );
    assert_eq!(store.len(), before_len);
    assert!(store.node(target_state).is_some());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn structural_removal_then_add_animation_is_rejected_before_commit() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let unrelated = object(&mut store, 3.0);
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .remove_node(unrelated)
        .add_animation(transform_state(
            target,
            target_state,
            AnimationOptions::new(),
        ));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::MutationAfterRemove { index: 1 })
    );
    assert_eq!(store.len(), before_len);
    assert!(store.node(unrelated).is_some());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn repeated_identical_additions_create_distinct_semantic_identities() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let state = transform_state(target, target_state, AnimationOptions::new());

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_animation(state.clone())
        .add_animation(state);
    let result = transaction.apply(&mut store).unwrap();

    let [SemanticMutationImpact::AnimationAdded { animation: first }, SemanticMutationImpact::AnimationAdded { animation: second }] =
        result.impacts()
    else {
        panic!("expected two animation-added impacts")
    };
    assert_ne!(first, second);
    assert_eq!(store.last_mutation_stats().slots_written, 2);
}

#[test]
fn add_animation_can_precede_an_unrelated_terminal_removal() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let unrelated = object(&mut store, 3.0);

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_animation(transform_state(
            target,
            target_state,
            AnimationOptions::new(),
        ))
        .remove_node(unrelated);
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(result.impacts().len(), 2);
    assert!(matches!(
        result.impacts()[0],
        SemanticMutationImpact::AnimationAdded { .. }
    ));
    assert_eq!(
        result.impacts()[1],
        SemanticMutationImpact::NodeRemoved { node: unrelated }
    );
    assert_eq!(store.last_mutation_stats().slots_written, 2);
}

#[test]
fn add_animation_is_local_with_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let target = object(&mut store, 0.25);
    let target_state = object(&mut store, 0.5);

    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_animation(transform_state(
        target,
        target_state,
        AnimationOptions::new(),
    ));
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(result.impacts().len(), 1);
    assert!(matches!(
        result.impacts()[0],
        SemanticMutationImpact::AnimationAdded { .. }
    ));
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}
