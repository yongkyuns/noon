use super::{
    AnimationOptions, AnimationOptionsError, SemanticNodeId, SemanticNodeKind,
    SemanticSceneOperationError, SemanticStore,
};

/// One authored animation operation before execution scheduling/lowering.
///
/// Targets and composition children are semantic identities. Execution tracks,
/// runtime slots, retained object IDs, and transport IDs are deliberately absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticAnimationIntent {
    /// Transform one semantic object toward the authored state of another semantic
    /// object. The target-state node is a semantic reference, not an execution
    /// snapshot; A1.6 lowering decides when/how to snapshot and interpolate it.
    TransformTo {
        target: SemanticNodeId,
        target_state: SemanticNodeId,
    },
    /// Ordered semantic animation composition.
    ///
    /// Child order and multiplicity are authored state. `AnimationOptions::lag_ratio`
    /// is the single timing-intent field: `0` is parallel, `1` is succession, and
    /// other non-negative values express lagged composition. Resolved intervals do
    /// not belong in the Semantic Scene.
    Composition { children: Vec<SemanticNodeId> },
}

impl SemanticAnimationIntent {
    pub const fn transform_endpoints(&self) -> Option<(SemanticNodeId, SemanticNodeId)> {
        match self {
            Self::TransformTo {
                target,
                target_state,
            } => Some((*target, *target_state)),
            Self::Composition { .. } => None,
        }
    }

    pub fn composition_children(&self) -> Option<&[SemanticNodeId]> {
        match self {
            Self::TransformTo { .. } => None,
            Self::Composition { children } => Some(children),
        }
    }
}

/// Authored animation declaration owned by the Semantic Scene.
///
/// Options intentionally remain unresolved so frontend-local defaults and
/// `Scene.play` overrides do not become a second animation authority. Lowering
/// resolves defaults and may reject execution capabilities it cannot yet express.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticAnimationState {
    intent: SemanticAnimationIntent,
    options: AnimationOptions,
}

impl SemanticAnimationState {
    pub const fn new(intent: SemanticAnimationIntent, options: AnimationOptions) -> Self {
        Self { intent, options }
    }

    pub fn intent(&self) -> &SemanticAnimationIntent {
        &self.intent
    }

    pub const fn options(&self) -> AnimationOptions {
        self.options
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticAnimationError {
    UnknownAnimation(SemanticNodeId),
    NotAnimation(SemanticNodeId),
    Target(SemanticSceneOperationError),
    Options(AnimationOptionsError),
    SameTargetAndTargetState(SemanticNodeId),
    EmptyComposition,
    UnknownCompositionChild {
        index: usize,
        id: SemanticNodeId,
    },
    NotAnimationChild {
        index: usize,
        id: SemanticNodeId,
    },
    CompositionLagRatioConflict {
        expected: f64,
        actual: f64,
    },
}

impl std::fmt::Display for SemanticAnimationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownAnimation(id) => write!(
                formatter,
                "unknown semantic animation {}:{}",
                id.slot(),
                id.generation()
            ),
            Self::NotAnimation(id) => write!(
                formatter,
                "semantic node {}:{} is not an animation",
                id.slot(),
                id.generation()
            ),
            Self::Target(error) => error.fmt(formatter),
            Self::Options(error) => error.fmt(formatter),
            Self::SameTargetAndTargetState(id) => write!(
                formatter,
                "semantic animation target {}:{} must use a distinct target-state node",
                id.slot(),
                id.generation()
            ),
            Self::EmptyComposition => {
                formatter.write_str("semantic animation composition requires at least one child")
            }
            Self::UnknownCompositionChild { index, id } => write!(
                formatter,
                "semantic animation composition child {index} references unknown node {}:{}",
                id.slot(),
                id.generation()
            ),
            Self::NotAnimationChild { index, id } => write!(
                formatter,
                "semantic animation composition child {index} node {}:{} is not an animation",
                id.slot(),
                id.generation()
            ),
            Self::CompositionLagRatioConflict { expected, actual } => write!(
                formatter,
                "semantic animation composition requires lag_ratio={expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for SemanticAnimationError {}

impl From<SemanticSceneOperationError> for SemanticAnimationError {
    fn from(value: SemanticSceneOperationError) -> Self {
        Self::Target(value)
    }
}

impl From<AnimationOptionsError> for SemanticAnimationError {
    fn from(value: AnimationOptionsError) -> Self {
        Self::Options(value)
    }
}

fn validate_authored_animation_options(
    options: AnimationOptions,
) -> Result<(), AnimationOptionsError> {
    if let Some(run_time) = options.run_time {
        if !run_time.is_finite() || run_time <= 0.0 {
            return Err(AnimationOptionsError::InvalidRunTime(run_time));
        }
    }
    if let Some(lag_ratio) = options.lag_ratio {
        if !lag_ratio.is_finite() || lag_ratio < 0.0 {
            return Err(AnimationOptionsError::InvalidLagRatio(lag_ratio));
        }
    }
    if let Some(path_arc) = options.path_arc {
        if !path_arc.is_finite() {
            return Err(AnimationOptionsError::InvalidPathArc(path_arc));
        }
    }
    Ok(())
}

fn require_composition_lag_ratio(
    mut options: AnimationOptions,
    expected: f64,
) -> Result<AnimationOptions, SemanticAnimationError> {
    validate_authored_animation_options(options)?;
    if let Some(actual) = options.lag_ratio {
        if actual != expected {
            return Err(SemanticAnimationError::CompositionLagRatioConflict {
                expected,
                actual,
            });
        }
    }
    options.lag_ratio = Some(expected);
    Ok(options)
}

impl SemanticStore {
    /// Insert one target-state transform declaration into the scene-global semantic
    /// identity arena.
    ///
    /// This does not schedule or lower anything. Both endpoints must be target
    /// semantic objects, validation completes before insertion, and successful
    /// insertion writes exactly one new semantic slot.
    pub fn insert_semantic_transform_animation(
        &mut self,
        target: SemanticNodeId,
        target_state: SemanticNodeId,
        options: AnimationOptions,
    ) -> Result<SemanticNodeId, SemanticAnimationError> {
        self.set_last_mutation_writes(0);
        self.semantic_object_state_checked(target)?;
        self.semantic_object_state_checked(target_state)?;
        if target == target_state {
            return Err(SemanticAnimationError::SameTargetAndTargetState(target));
        }
        validate_authored_animation_options(options)?;

        Ok(
            self.insert_semantic_animation_state(SemanticAnimationState::new(
                SemanticAnimationIntent::TransformTo {
                    target,
                    target_state,
                },
                options,
            )),
        )
    }

    /// Insert one ordered semantic animation composition.
    ///
    /// Every child must already be a live semantic animation. Because composition
    /// declarations are immutable after insertion in A1.4 and may only reference
    /// pre-existing animation nodes, cycles are impossible by construction. Child
    /// order and repeated aliases are preserved exactly. Scheduling is deferred to
    /// A1.6 lowering.
    pub fn insert_semantic_animation_composition(
        &mut self,
        children: &[SemanticNodeId],
        options: AnimationOptions,
    ) -> Result<SemanticNodeId, SemanticAnimationError> {
        self.set_last_mutation_writes(0);
        if children.is_empty() {
            return Err(SemanticAnimationError::EmptyComposition);
        }
        validate_authored_animation_options(options)?;
        for (index, id) in children.iter().copied().enumerate() {
            let Some(node) = self.node(id) else {
                return Err(SemanticAnimationError::UnknownCompositionChild { index, id });
            };
            if !matches!(node.kind(), SemanticNodeKind::Animation(_)) {
                return Err(SemanticAnimationError::NotAnimationChild { index, id });
            }
        }

        Ok(
            self.insert_semantic_animation_state(SemanticAnimationState::new(
                SemanticAnimationIntent::Composition {
                    children: children.to_vec(),
                },
                options,
            )),
        )
    }

    /// Insert an explicitly parallel semantic composition.
    ///
    /// `lag_ratio` has one semantic owner. A conflicting caller value is rejected;
    /// otherwise this constructor canonicalizes the authored value to `0`.
    pub fn insert_semantic_parallel_animation(
        &mut self,
        children: &[SemanticNodeId],
        options: AnimationOptions,
    ) -> Result<SemanticNodeId, SemanticAnimationError> {
        self.set_last_mutation_writes(0);
        let options = require_composition_lag_ratio(options, 0.0)?;
        self.insert_semantic_animation_composition(children, options)
    }

    /// Insert an explicitly sequential semantic composition.
    ///
    /// `lag_ratio` has one semantic owner. A conflicting caller value is rejected;
    /// otherwise this constructor canonicalizes the authored value to `1`.
    pub fn insert_semantic_sequence_animation(
        &mut self,
        children: &[SemanticNodeId],
        options: AnimationOptions,
    ) -> Result<SemanticNodeId, SemanticAnimationError> {
        self.set_last_mutation_writes(0);
        let options = require_composition_lag_ratio(options, 1.0)?;
        self.insert_semantic_animation_composition(children, options)
    }

    pub fn semantic_animation_state(
        &self,
        id: SemanticNodeId,
    ) -> Result<&SemanticAnimationState, SemanticAnimationError> {
        let node = self
            .node(id)
            .ok_or(SemanticAnimationError::UnknownAnimation(id))?;
        match node.kind() {
            SemanticNodeKind::Animation(state) => Ok(state),
            _ => Err(SemanticAnimationError::NotAnimation(id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RateFunction, SemanticObjectState, StoredGeometry};

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    fn transform(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        let target = object(store, radius);
        let target_state = object(store, radius + 0.25);
        store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap()
    }

    #[test]
    fn transform_intent_uses_scene_global_semantic_identity() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let target_state = object(&mut store, 2.0);
        let options = AnimationOptions::new()
            .run_time(1.5)
            .rate_func(RateFunction::Linear)
            .lag_ratio(0.25);

        let animation = store
            .insert_semantic_transform_animation(target, target_state, options)
            .unwrap();
        let state = store.semantic_animation_state(animation).unwrap();

        assert_eq!(
            state.intent(),
            &SemanticAnimationIntent::TransformTo {
                target,
                target_state,
            }
        );
        assert_eq!(state.intent().transform_endpoints(), Some((target, target_state)));
        assert_eq!(state.options(), options);
        assert!(!store.node(animation).unwrap().is_scene_owned());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
    }

    #[test]
    fn semantic_layer_preserves_valid_unresolved_options_lowering_may_not_support_yet() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let target_state = object(&mut store, 2.0);
        let options = AnimationOptions::new()
            .path_arc(0.75)
            .reverse_rate_function(true);

        let animation = store
            .insert_semantic_transform_animation(target, target_state, options)
            .unwrap();
        assert_eq!(
            store.semantic_animation_state(animation).unwrap().options(),
            options
        );
    }

    #[test]
    fn malformed_authored_options_fail_before_allocation() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let target_state = object(&mut store, 2.0);
        let before_len = store.len();

        assert_eq!(
            store.insert_semantic_transform_animation(
                target,
                target_state,
                AnimationOptions::new().run_time(0.0),
            ),
            Err(SemanticAnimationError::Options(
                AnimationOptionsError::InvalidRunTime(0.0)
            ))
        );
        assert_eq!(store.len(), before_len);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.insert_semantic_transform_animation(
                target,
                target_state,
                AnimationOptions::new().lag_ratio(-0.1),
            ),
            Err(SemanticAnimationError::Options(
                AnimationOptionsError::InvalidLagRatio(-0.1)
            ))
        );
        assert_eq!(store.len(), before_len);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn targets_must_be_distinct_live_semantic_objects() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let family = store.insert_family();

        assert_eq!(
            store.insert_semantic_transform_animation(target, target, AnimationOptions::new()),
            Err(SemanticAnimationError::SameTargetAndTargetState(target))
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert!(matches!(
            store.insert_semantic_transform_animation(
                target,
                family,
                AnimationOptions::new(),
            ),
            Err(SemanticAnimationError::Target(
                SemanticSceneOperationError::NotSemanticObject(id)
            )) if id == family
        ));
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn deleted_target_state_never_retargets_after_slot_reuse() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let target_state = object(&mut store, 2.0);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();

        store.remove_node(target_state).unwrap();
        let replacement = object(&mut store, 3.0);
        assert_eq!(target_state.slot(), replacement.slot());
        assert_ne!(target_state.generation(), replacement.generation());

        let intent = store.semantic_animation_state(animation).unwrap().intent();
        assert_eq!(intent.transform_endpoints(), Some((target, target_state)));
        assert!(matches!(
            store.semantic_object_state_checked(target_state),
            Err(SemanticSceneOperationError::UnknownNode(id)) if id == target_state
        ));
    }

    #[test]
    fn animation_identity_uses_the_same_generation_safe_arena() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let target_state = object(&mut store, 2.0);
        let first = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        store.remove_node(first).unwrap();
        let replacement = object(&mut store, 4.0);

        assert_eq!(first.slot(), replacement.slot());
        assert_ne!(first.generation(), replacement.generation());
        assert_eq!(
            store.semantic_animation_state(first),
            Err(SemanticAnimationError::UnknownAnimation(first))
        );
    }

    #[test]
    fn animation_insertion_cost_is_independent_of_unrelated_scene_size() {
        let mut store = SemanticStore::new();
        for index in 0..10_000 {
            object(&mut store, index as f32 + 1.0);
        }
        let target = object(&mut store, 0.5);
        let target_state = object(&mut store, 0.75);

        store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        assert_eq!(store.last_mutation_stats().slots_written, 1);
    }

    #[test]
    fn composition_preserves_order_aliases_and_nested_semantic_identity() {
        let mut store = SemanticStore::new();
        let first = transform(&mut store, 1.0);
        let second = transform(&mut store, 2.0);
        let inner = store
            .insert_semantic_parallel_animation(
                &[first, second, first],
                AnimationOptions::new().run_time(2.0),
            )
            .unwrap();
        let outer = store
            .insert_semantic_sequence_animation(&[inner, second], AnimationOptions::new())
            .unwrap();

        assert_eq!(
            store
                .semantic_animation_state(inner)
                .unwrap()
                .intent()
                .composition_children(),
            Some(&[first, second, first][..])
        );
        assert_eq!(
            store
                .semantic_animation_state(outer)
                .unwrap()
                .intent()
                .composition_children(),
            Some(&[inner, second][..])
        );
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert!(!store.node(inner).unwrap().is_scene_owned());
        assert!(!store.node(outer).unwrap().is_scene_owned());
    }

    #[test]
    fn parallel_and_sequence_normalize_the_single_lag_ratio_authority() {
        let mut store = SemanticStore::new();
        let first = transform(&mut store, 1.0);
        let second = transform(&mut store, 2.0);
        let parallel_options = AnimationOptions::new()
            .run_time(1.5)
            .rate_func(RateFunction::Linear);
        let parallel = store
            .insert_semantic_parallel_animation(&[first, second], parallel_options)
            .unwrap();
        let sequence = store
            .insert_semantic_sequence_animation(&[first, second], AnimationOptions::new())
            .unwrap();

        let authored_parallel = store.semantic_animation_state(parallel).unwrap().options();
        assert_eq!(authored_parallel.run_time, Some(1.5));
        assert_eq!(authored_parallel.rate_func, Some(RateFunction::Linear));
        assert_eq!(authored_parallel.lag_ratio, Some(0.0));
        assert_eq!(
            store.semantic_animation_state(sequence).unwrap().options().lag_ratio,
            Some(1.0)
        );

        let before_len = store.len();
        assert_eq!(
            store.insert_semantic_parallel_animation(
                &[first, second],
                AnimationOptions::new().lag_ratio(0.5),
            ),
            Err(SemanticAnimationError::CompositionLagRatioConflict {
                expected: 0.0,
                actual: 0.5,
            })
        );
        assert_eq!(store.len(), before_len);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn generic_composition_preserves_custom_unresolved_lagged_options() {
        let mut store = SemanticStore::new();
        let first = transform(&mut store, 1.0);
        let second = transform(&mut store, 2.0);
        let options = AnimationOptions::new()
            .lag_ratio(0.35)
            .run_time(4.0)
            .reverse_rate_function(true);

        let composition = store
            .insert_semantic_animation_composition(&[first, second], options)
            .unwrap();
        assert_eq!(
            store.semantic_animation_state(composition).unwrap().options(),
            options
        );
    }

    #[test]
    fn composition_validation_is_atomic_and_fail_closed() {
        let mut store = SemanticStore::new();
        let animation = transform(&mut store, 1.0);
        let object = object(&mut store, 3.0);
        let before_len = store.len();

        assert_eq!(
            store.insert_semantic_animation_composition(&[], AnimationOptions::new()),
            Err(SemanticAnimationError::EmptyComposition)
        );
        assert_eq!(store.len(), before_len);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.insert_semantic_animation_composition(
                &[animation, object],
                AnimationOptions::new(),
            ),
            Err(SemanticAnimationError::NotAnimationChild {
                index: 1,
                id: object,
            })
        );
        assert_eq!(store.len(), before_len);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.insert_semantic_animation_composition(
                &[animation],
                AnimationOptions::new().lag_ratio(-0.1),
            ),
            Err(SemanticAnimationError::Options(
                AnimationOptionsError::InvalidLagRatio(-0.1)
            ))
        );
        assert_eq!(store.len(), before_len);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn stale_composition_child_never_retargets_after_slot_reuse() {
        let mut store = SemanticStore::new();
        let first = transform(&mut store, 1.0);
        let stale = transform(&mut store, 2.0);
        let composition = store
            .insert_semantic_animation_composition(&[first, stale], AnimationOptions::new())
            .unwrap();

        store.remove_node(stale).unwrap();
        let replacement = transform(&mut store, 3.0);
        assert_eq!(stale.slot(), replacement.slot());
        assert_ne!(stale.generation(), replacement.generation());
        assert_eq!(
            store
                .semantic_animation_state(composition)
                .unwrap()
                .intent()
                .composition_children(),
            Some(&[first, stale][..])
        );
        assert_eq!(
            store.semantic_animation_state(stale),
            Err(SemanticAnimationError::UnknownAnimation(stale))
        );

        let before_len = store.len();
        assert_eq!(
            store.insert_semantic_animation_composition(&[first, stale], AnimationOptions::new()),
            Err(SemanticAnimationError::UnknownCompositionChild {
                index: 1,
                id: stale,
            })
        );
        assert_eq!(store.len(), before_len);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn composition_insertion_is_local_to_its_child_degree() {
        let mut store = SemanticStore::new();
        for index in 0..10_000 {
            object(&mut store, index as f32 + 1.0);
        }
        let first = transform(&mut store, 0.5);
        let second = transform(&mut store, 0.75);

        let composition = store
            .insert_semantic_animation_composition(
                &[first, second, first],
                AnimationOptions::new().lag_ratio(0.2),
            )
            .unwrap();
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(
            store
                .semantic_animation_state(composition)
                .unwrap()
                .intent()
                .composition_children(),
            Some(&[first, second, first][..])
        );
    }
}
