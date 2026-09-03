use std::collections::HashSet;

use crate::{
    SemanticAnimationCompositionKind, SemanticAnimationError, SemanticAnimationIntent,
    SemanticAnimationState,
};

use super::{SemanticMutationTransactionError, SemanticNodeId, SemanticStore};

pub(super) fn preflight_add_animation(
    store: &SemanticStore,
    state: &SemanticAnimationState,
    removed_nodes: &HashSet<SemanticNodeId>,
    index: usize,
) -> Result<(), SemanticMutationTransactionError> {
    let options = state.options();
    if options
        .run_time
        .is_some_and(|run_time| !run_time.is_finite() || run_time <= 0.0)
    {
        return Err(SemanticMutationTransactionError::InvalidAnimationRunTime { index });
    }
    if options
        .lag_ratio
        .is_some_and(|lag_ratio| !lag_ratio.is_finite() || lag_ratio < 0.0)
    {
        return Err(SemanticMutationTransactionError::InvalidAnimationLagRatio { index });
    }
    if options
        .path_arc
        .is_some_and(|path_arc| !path_arc.is_finite())
    {
        return Err(SemanticMutationTransactionError::InvalidAnimationPathArc { index });
    }

    match state.intent() {
        SemanticAnimationIntent::TransformTo {
            target,
            target_state,
        } => {
            reject_removed_reference(removed_nodes, index, *target)?;
            reject_removed_reference(removed_nodes, index, *target_state)?;
            store
                .semantic_object_state_checked(*target)
                .map_err(|error| SemanticMutationTransactionError::AnimationTarget {
                    index,
                    error,
                })?;
            store
                .semantic_object_state_checked(*target_state)
                .map_err(|error| SemanticMutationTransactionError::AnimationTarget {
                    index,
                    error,
                })?;
            if target == target_state {
                return Err(
                    SemanticMutationTransactionError::SameAnimationTargetAndTargetState {
                        index,
                        node: *target,
                    },
                );
            }
        }
        SemanticAnimationIntent::Composition { children, .. } => {
            if children.is_empty() {
                return Err(SemanticMutationTransactionError::EmptyAnimationComposition { index });
            }
            for &child in children {
                reject_removed_reference(removed_nodes, index, child)?;
                store
                    .semantic_animation_state(child)
                    .map_err(|error| animation_lookup_error(index, error))?;
            }
        }
    }

    Ok(())
}

pub(super) fn commit_add_animation(
    store: &mut SemanticStore,
    state: &SemanticAnimationState,
) -> SemanticNodeId {
    let options = state.options();
    match state.intent() {
        SemanticAnimationIntent::TransformTo {
            target,
            target_state,
        } => store
            .insert_semantic_transform_animation(*target, *target_state, options)
            .expect("preflighted semantic animation insertion must remain valid while transaction owns the store"),
        SemanticAnimationIntent::Composition { kind, children } => match kind {
            SemanticAnimationCompositionKind::Parallel => store
                .insert_semantic_parallel_animation(children, options)
                .expect("preflighted semantic animation insertion must remain valid while transaction owns the store"),
            SemanticAnimationCompositionKind::Sequence => store
                .insert_semantic_sequence_animation(children, options)
                .expect("preflighted semantic animation insertion must remain valid while transaction owns the store"),
        },
    }
}

fn reject_removed_reference(
    removed_nodes: &HashSet<SemanticNodeId>,
    index: usize,
    node: SemanticNodeId,
) -> Result<(), SemanticMutationTransactionError> {
    if removed_nodes.contains(&node) {
        return Err(SemanticMutationTransactionError::AnimationUsesRemovedNode { index, node });
    }
    Ok(())
}

fn animation_lookup_error(
    index: usize,
    error: SemanticAnimationError,
) -> SemanticMutationTransactionError {
    match error {
        SemanticAnimationError::UnknownAnimation(animation) => {
            SemanticMutationTransactionError::UnknownAnimation { index, animation }
        }
        SemanticAnimationError::NotAnimation(animation) => {
            SemanticMutationTransactionError::NotAnimation { index, animation }
        }
        SemanticAnimationError::EmptyComposition
        | SemanticAnimationError::Target(_)
        | SemanticAnimationError::Options(_)
        | SemanticAnimationError::SameTargetAndTargetState(_) => {
            unreachable!("semantic_animation_state only validates animation identity")
        }
    }
}
