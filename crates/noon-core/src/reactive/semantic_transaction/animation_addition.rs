use std::collections::{HashMap, HashSet};

use crate::{
    AnimationOptions, Color, SemanticAffineLifecycleDirection, SemanticAffineLifecycleEndpoint,
    SemanticAnimationCompositionKind, SemanticAnimationIntent, SemanticAnimationState,
    SemanticFadeDirection, SemanticFadeEndpoint, SemanticObjectState,
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
    Indicate {
        target: SemanticTransactionNodeRef,
        scale_factor: f64,
        color: Color,
        scale_center: crate::SemanticVec3,
    },
    DrawBorderThenFill {
        target: SemanticTransactionNodeRef,
        stroke_width: f64,
        stroke_color: Option<Color>,
        phase_rate_function: crate::RateFunction,
    },
    Rotate {
        target: SemanticTransactionNodeRef,
        angle: f64,
    },
    Fade {
        target: SemanticTransactionNodeRef,
        direction: SemanticFadeDirection,
        endpoint: SemanticFadeEndpoint,
    },
    AffineLifecycle {
        target: SemanticTransactionNodeRef,
        direction: SemanticAffineLifecycleDirection,
        endpoint: SemanticAffineLifecycleEndpoint,
    },
    Create {
        target: SemanticTransactionNodeRef,
    },
    Add {
        target: SemanticTransactionNodeRef,
    },
    SetScalar {
        signal: SemanticNodeId,
        target: f64,
    },
    Wait,
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
            Self::Rotate { target, .. }
            | Self::Indicate { target, .. }
            | Self::DrawBorderThenFill { target, .. }
            | Self::Fade { target, .. }
            | Self::AffineLifecycle { target, .. }
            | Self::Create { target }
            | Self::Add { target } => Some([Some(*target), None]),
            Self::SetScalar { signal, .. } => Some([Some((*signal).into()), None]),
            Self::Wait | Self::Composition { .. } => None,
        };
        let children = match self {
            Self::Composition { children, .. } => children.as_slice(),
            Self::TransformTo { .. }
            | Self::Indicate { .. }
            | Self::DrawBorderThenFill { .. }
            | Self::Rotate { .. }
            | Self::Fade { .. }
            | Self::AffineLifecycle { .. }
            | Self::Create { .. }
            | Self::Add { .. }
            | Self::SetScalar { .. }
            | Self::Wait => &[],
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
            SemanticAnimationIntent::Indicate {
                target,
                scale_factor,
                color,
                scale_center,
            } => SemanticTransactionAnimationIntent::Indicate {
                target: (*target).into(),
                scale_factor: *scale_factor,
                color: *color,
                scale_center: *scale_center,
            },
            SemanticAnimationIntent::DrawBorderThenFill {
                target,
                stroke_width,
                stroke_color,
                phase_rate_function,
            } => SemanticTransactionAnimationIntent::DrawBorderThenFill {
                target: (*target).into(),
                stroke_width: *stroke_width,
                stroke_color: *stroke_color,
                phase_rate_function: *phase_rate_function,
            },
            SemanticAnimationIntent::Fade {
                target,
                direction,
                endpoint,
            } => SemanticTransactionAnimationIntent::Fade {
                target: (*target).into(),
                direction: *direction,
                endpoint: *endpoint,
            },
            SemanticAnimationIntent::AffineLifecycle {
                target,
                direction,
                endpoint,
            } => SemanticTransactionAnimationIntent::AffineLifecycle {
                target: (*target).into(),
                direction: *direction,
                endpoint: *endpoint,
            },
            SemanticAnimationIntent::Create { target } => {
                SemanticTransactionAnimationIntent::Create {
                    target: (*target).into(),
                }
            }
            SemanticAnimationIntent::Add { target } => SemanticTransactionAnimationIntent::Add {
                target: (*target).into(),
            },
            SemanticAnimationIntent::SetScalar { signal, target } => {
                SemanticTransactionAnimationIntent::SetScalar {
                    signal: *signal,
                    target: *target,
                }
            }
            SemanticAnimationIntent::Wait => SemanticTransactionAnimationIntent::Wait,
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
            SemanticTransactionAnimationIntent::Indicate {
                target,
                scale_factor,
                color,
                scale_center,
            } => SemanticAnimationIntent::Indicate {
                target: resolve_node_ref(*target, committed),
                scale_factor: *scale_factor,
                color: *color,
                scale_center: *scale_center,
            },
            SemanticTransactionAnimationIntent::DrawBorderThenFill {
                target,
                stroke_width,
                stroke_color,
                phase_rate_function,
            } => SemanticAnimationIntent::DrawBorderThenFill {
                target: resolve_node_ref(*target, committed),
                stroke_width: *stroke_width,
                stroke_color: *stroke_color,
                phase_rate_function: *phase_rate_function,
            },
            SemanticTransactionAnimationIntent::Fade {
                target,
                direction,
                endpoint,
            } => SemanticAnimationIntent::Fade {
                target: resolve_node_ref(*target, committed),
                direction: *direction,
                endpoint: *endpoint,
            },
            SemanticTransactionAnimationIntent::AffineLifecycle {
                target,
                direction,
                endpoint,
            } => SemanticAnimationIntent::AffineLifecycle {
                target: resolve_node_ref(*target, committed),
                direction: *direction,
                endpoint: *endpoint,
            },
            SemanticTransactionAnimationIntent::Create { target } => {
                SemanticAnimationIntent::Create {
                    target: resolve_node_ref(*target, committed),
                }
            }
            SemanticTransactionAnimationIntent::Add { target } => SemanticAnimationIntent::Add {
                target: resolve_node_ref(*target, committed),
            },
            SemanticTransactionAnimationIntent::SetScalar { signal, target } => {
                SemanticAnimationIntent::SetScalar {
                    signal: *signal,
                    target: *target,
                }
            }
            SemanticTransactionAnimationIntent::Wait => SemanticAnimationIntent::Wait,
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
    if matches!(
        animation.intent(),
        SemanticTransactionAnimationIntent::Add { .. }
    ) {
        preflight_add_animation_options(animation.options(), index)?;
    } else {
        preflight_animation_options(animation.options(), index)?;
    }
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
        SemanticTransactionAnimationIntent::Indicate {
            target,
            scale_factor,
            color,
            scale_center,
        } => {
            catalog.ensure_animation_target(*target, index)?;
            catalog.staged_object_state(staged_objects, staged_object_order, *target, index)?;
            let color_is_finite = color.red.is_finite()
                && color.green.is_finite()
                && color.blue.is_finite()
                && color.alpha.is_finite();
            if !scale_factor.is_finite()
                || *scale_factor < 0.0
                || *scale_factor > f32::MAX as f64
                || scale_center.lower_xy_f32().is_err()
                || scale_center.z != 0.0
                || !color_is_finite
            {
                return Err(SemanticMutationTransactionError::InvalidIndicateEndpoint { index });
            }
        }
        SemanticTransactionAnimationIntent::DrawBorderThenFill {
            target,
            stroke_width,
            stroke_color,
            ..
        } => {
            catalog.ensure_animation_target(*target, index)?;
            catalog.staged_object_state(staged_objects, staged_object_order, *target, index)?;
            let color_is_finite = stroke_color.is_none_or(|color| {
                color.red.is_finite()
                    && color.green.is_finite()
                    && color.blue.is_finite()
                    && color.alpha.is_finite()
            });
            if !stroke_width.is_finite()
                || *stroke_width < 0.0
                || *stroke_width > f32::MAX as f64
                || !color_is_finite
            {
                return Err(
                    SemanticMutationTransactionError::InvalidDrawBorderThenFillOutline { index },
                );
            }
        }
        SemanticTransactionAnimationIntent::Fade {
            target, endpoint, ..
        } => {
            catalog.ensure_animation_target(*target, index)?;
            catalog.staged_object_state(staged_objects, staged_object_order, *target, index)?;
            if !endpoint.is_valid() {
                return Err(SemanticMutationTransactionError::InvalidFadeEndpoint { index });
            }
        }
        SemanticTransactionAnimationIntent::AffineLifecycle {
            target, endpoint, ..
        } => {
            catalog.ensure_animation_target(*target, index)?;
            catalog.staged_object_state(staged_objects, staged_object_order, *target, index)?;
            if !endpoint.is_valid() {
                return Err(
                    SemanticMutationTransactionError::InvalidAffineLifecycleEndpoint { index },
                );
            }
        }
        SemanticTransactionAnimationIntent::Create { target } => {
            catalog.ensure_animation_target(*target, index)?;
            catalog.staged_object_state(staged_objects, staged_object_order, *target, index)?;
        }
        SemanticTransactionAnimationIntent::Add { target } => {
            catalog.ensure_animation_target(*target, index)?;
            catalog.staged_object_state(staged_objects, staged_object_order, *target, index)?;
        }
        SemanticTransactionAnimationIntent::SetScalar { signal, target } => {
            catalog.ensure_scalar_animation_target(*signal, *target, index)?;
        }
        SemanticTransactionAnimationIntent::Wait => {}
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

fn preflight_add_animation_options(
    options: AnimationOptions,
    index: usize,
) -> Result<(), SemanticMutationTransactionError> {
    if options
        .run_time
        .is_some_and(|run_time| !run_time.is_finite() || run_time < 0.0)
    {
        return Err(SemanticMutationTransactionError::InvalidAnimationRunTime { index });
    }
    preflight_animation_options(
        AnimationOptions {
            run_time: None,
            ..options
        },
        index,
    )
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
        SemanticAnimationIntent::Indicate {
            target,
            scale_factor,
            color,
            scale_center,
        } => store
            .insert_semantic_indicate_animation(
                *target,
                *scale_factor,
                *color,
                *scale_center,
                options,
            )
            .expect("preflighted semantic Indicate insertion must remain valid while transaction owns the store"),
        SemanticAnimationIntent::DrawBorderThenFill {
            target,
            stroke_width,
            stroke_color,
            phase_rate_function,
        } => store
            .insert_semantic_draw_border_then_fill_animation(
                *target,
                *stroke_width,
                *stroke_color,
                *phase_rate_function,
                options,
            )
            .expect("preflighted DrawBorderThenFill insertion must remain valid while transaction owns the store"),
        SemanticAnimationIntent::Fade {
            target,
            direction,
            endpoint,
        } => store
            .insert_semantic_fade_animation_with_endpoint(*target, *direction, *endpoint, options)
            .expect("preflighted semantic fade insertion must remain valid while transaction owns the store"),
        SemanticAnimationIntent::AffineLifecycle {
            target,
            direction,
            endpoint,
        } => store
            .insert_semantic_affine_lifecycle_animation(*target, *direction, *endpoint, options)
            .expect("preflighted affine lifecycle insertion must remain valid while transaction owns the store"),
        SemanticAnimationIntent::Create { target } => store
            .insert_semantic_create_animation(*target, options)
            .expect("preflighted semantic Create insertion must remain valid while transaction owns the store"),
        SemanticAnimationIntent::Add { target } => store
            .insert_semantic_add_animation(*target, options)
            .expect("preflighted semantic Add insertion must remain valid while transaction owns the store"),
        SemanticAnimationIntent::SetScalar { signal, target } => store
            .insert_semantic_scalar_animation(*signal, *target, options)
            .expect("preflighted semantic scalar animation insertion must remain valid while transaction owns the store"),
        SemanticAnimationIntent::Wait => store
            .insert_semantic_wait_animation(options.run_time.expect("preflighted wait has duration"))
            .expect("preflighted semantic Wait insertion must remain valid while transaction owns the store"),
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
