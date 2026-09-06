use std::collections::{HashMap, HashSet};

use crate::{
    AnimationOptions, SemanticAnimationCompositionKind, SemanticAnimationIntent,
    SemanticAnimationState, SemanticFadeDirection, SemanticObjectState,
    SemanticTransformInterpolation,
};

use super::{
    SemanticLocalNodeToken, SemanticMutationTransactionError, SemanticNodeId, SemanticStore,
    SemanticTransactionNodeRef, TransactionNodeCatalog,
};

/// An authored animation intent whose references may name nodes staged by the
/// same semantic transaction.
///
/// This is transaction vocabulary, not a second authored animation model. It is
/// resolved to [`SemanticAnimationIntent`] only after complete transaction
/// preflight and semantic identity allocation.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticTransactionAnimationIntent {
    TransformTo {
        target: SemanticTransactionNodeRef,
        target_state: SemanticTransactionNodeRef,
        interpolation: SemanticTransformInterpolation,
    },
    Rotate {
        target: SemanticTransactionNodeRef,
        angle: f64,
    },
    Fade {
        target: SemanticTransactionNodeRef,
        direction: SemanticFadeDirection,
    },
    Create {
        target: SemanticTransactionNodeRef,
    },
    Composition {
        kind: SemanticAnimationCompositionKind,
        children: Vec<SemanticTransactionNodeRef>,
    },
}

impl SemanticTransactionAnimationIntent {
    pub fn node_references(&self) -> impl Iterator<Item = SemanticTransactionNodeRef> + '_ {
        let leaf = match self {
            Self::TransformTo {
                target,
                target_state,
                ..
            } => Some([Some(*target), Some(*target_state)]),
            Self::Rotate { target, .. } | Self::Fade { target, .. } | Self::Create { target } => {
                Some([Some(*target), None])
            }
            Self::Composition { .. } => None,
        };
        let children = match self {
            Self::Composition { children, .. } => children.as_slice(),
            Self::TransformTo { .. }
            | Self::Rotate { .. }
            | Self::Fade { .. }
            | Self::Create { .. } => &[],
        };
        leaf.into_iter()
            .flatten()
            .flatten()
            .chain(children.iter().copied())
    }
}

/// One uncommitted animation declaration in a semantic mutation transaction.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticTransactionAnimation {
    intent: SemanticTransactionAnimationIntent,
    options: AnimationOptions,
}

impl SemanticTransactionAnimation {
    pub const fn new(
        intent: SemanticTransactionAnimationIntent,
        options: AnimationOptions,
    ) -> Self {
        Self { intent, options }
    }

    pub const fn intent(&self) -> &SemanticTransactionAnimationIntent {
        &self.intent
    }

    pub const fn options(&self) -> AnimationOptions {
        self.options
    }

    pub(super) fn from_published(state: SemanticAnimationState) -> Self {
        let options = state.options();
        let intent = match state.intent() {
            SemanticAnimationIntent::TransformTo {
                target,
                target_state,
                interpolation,
            } => SemanticTransactionAnimationIntent::TransformTo {
                target: (*target).into(),
                target_state: (*target_state).into(),
                interpolation: *interpolation,
            },
            SemanticAnimationIntent::Rotate { target, angle } => {
                SemanticTransactionAnimationIntent::Rotate {
                    target: (*target).into(),
                    angle: *angle,
                }
            }
            SemanticAnimationIntent::Fade { target, direction } => {
                SemanticTransactionAnimationIntent::Fade {
                    target: (*target).into(),
                    direction: *direction,
                }
            }
            SemanticAnimationIntent::Create { target } => {
                SemanticTransactionAnimationIntent::Create {
                    target: (*target).into(),
                }
            }
            SemanticAnimationIntent::Composition { kind, children } => {
                SemanticTransactionAnimationIntent::Composition {
                    kind: *kind,
                    children: children.iter().copied().map(Into::into).collect(),
                }
            }
        };
        Self { intent, options }
    }

    pub(super) fn resolve(
        &self,
        committed: &std::collections::HashMap<SemanticLocalNodeToken, SemanticNodeId>,
    ) -> SemanticAnimationState {
        let intent = match &self.intent {
            SemanticTransactionAnimationIntent::TransformTo {
                target,
                target_state,
                interpolation,
            } => SemanticAnimationIntent::TransformTo {
                target: resolve_node_ref(*target, committed),
                target_state: resolve_node_ref(*target_state, committed),
                interpolation: *interpolation,
            },
            SemanticTransactionAnimationIntent::Rotate { target, angle } => {
                SemanticAnimationIntent::Rotate {
                    target: resolve_node_ref(*target, committed),
                    angle: *angle,
                }
            }
            SemanticTransactionAnimationIntent::Fade { target, direction } => {
                SemanticAnimationIntent::Fade {
                    target: resolve_node_ref(*target, committed),
                    direction: *direction,
                }
            }
            SemanticTransactionAnimationIntent::Create { target } => {
                SemanticAnimationIntent::Create {
                    target: resolve_node_ref(*target, committed),
                }
            }
            SemanticTransactionAnimationIntent::Composition { kind, children } => {
                SemanticAnimationIntent::Composition {
                    kind: *kind,
                    children: children
                        .iter()
                        .map(|child| resolve_node_ref(*child, committed))
                        .collect(),
                }
            }
        };
        SemanticAnimationState::new(intent, self.options)
    }
}

fn resolve_node_ref(
    node: SemanticTransactionNodeRef,
    committed: &std::collections::HashMap<SemanticLocalNodeToken, SemanticNodeId>,
) -> SemanticNodeId {
    match node {
        SemanticTransactionNodeRef::Existing(node) => node,
        SemanticTransactionNodeRef::Pending(token) => committed[&token],
    }
}

pub(super) fn preflight_transaction_animation(
    catalog: &TransactionNodeCatalog<'_>,
    token: SemanticLocalNodeToken,
    animation: &SemanticTransactionAnimation,
    available_pending_animations: &mut HashSet<SemanticLocalNodeToken>,
    staged_objects: &mut HashMap<SemanticTransactionNodeRef, SemanticObjectState>,
    staged_object_order: &mut Vec<SemanticTransactionNodeRef>,
    index: usize,
) -> Result<(), SemanticMutationTransactionError> {
    preflight_animation_options(animation.options(), index)?;
    match animation.intent() {
        SemanticTransactionAnimationIntent::TransformTo {
            target,
            target_state,
            ..
        } => {
            catalog.ensure_animation_target(*target, index)?;
            catalog.ensure_animation_target(*target_state, index)?;
            catalog.staged_object_state(staged_objects, staged_object_order, *target, index)?;
            catalog.staged_object_state(
                staged_objects,
                staged_object_order,
                *target_state,
                index,
            )?;
            if target == target_state {
                return Err(match target {
                    SemanticTransactionNodeRef::Existing(node) => {
                        SemanticMutationTransactionError::SameAnimationTargetAndTargetState {
                            index,
                            node: *node,
                        }
                    }
                    SemanticTransactionNodeRef::Pending(_) => {
                        SemanticMutationTransactionError::SamePendingAnimationTargetAndTargetState {
                            index,
                            node: *target,
                        }
                    }
                });
            }
        }
        SemanticTransactionAnimationIntent::Rotate { target, angle } => {
            catalog.ensure_animation_target(*target, index)?;
            catalog.staged_object_state(staged_objects, staged_object_order, *target, index)?;
            if !angle.is_finite() {
                return Err(SemanticMutationTransactionError::InvalidAnimationAngle { index });
            }
        }
        SemanticTransactionAnimationIntent::Fade { target, .. } => {
            catalog.ensure_animation_target(*target, index)?;
            catalog.staged_object_state(staged_objects, staged_object_order, *target, index)?;
        }
        SemanticTransactionAnimationIntent::Create { target } => {
            catalog.ensure_animation_target(*target, index)?;
            catalog.staged_object_state(staged_objects, staged_object_order, *target, index)?;
        }
        SemanticTransactionAnimationIntent::Composition { children, .. } => {
            if children.is_empty() {
                return Err(SemanticMutationTransactionError::EmptyAnimationComposition { index });
            }
            for child in children {
                catalog.ensure_animation(*child, index)?;
                if let SemanticTransactionNodeRef::Pending(child) = child {
                    if !available_pending_animations.contains(child) {
                        return Err(
                            SemanticMutationTransactionError::PendingAnimationForwardReference {
                                index,
                                animation: *child,
                            },
                        );
                    }
                }
            }
        }
    }
    available_pending_animations.insert(token);
    Ok(())
}

pub(super) fn preflight_animation_options(
    options: AnimationOptions,
    index: usize,
) -> Result<(), SemanticMutationTransactionError> {
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
            interpolation,
        } => store
            .insert_semantic_transform_animation_with_interpolation(
                *target,
                *target_state,
                *interpolation,
                options,
            )
            .expect("preflighted semantic animation insertion must remain valid while transaction owns the store"),
        SemanticAnimationIntent::Rotate { target, angle } => store
            .insert_semantic_rotate_animation(*target, *angle, options)
            .expect("preflighted semantic Rotate insertion must remain valid while transaction owns the store"),
        SemanticAnimationIntent::Fade { target, direction } => store
            .insert_semantic_fade_animation(*target, *direction, options)
            .expect("preflighted semantic fade insertion must remain valid while transaction owns the store"),
        SemanticAnimationIntent::Create { target } => store
            .insert_semantic_create_animation(*target, options)
            .expect("preflighted semantic Create insertion must remain valid while transaction owns the store"),
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
