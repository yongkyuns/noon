use noon_core::{
    resolve_animation_options, resolve_composition_schedule, AnimationDefaults, AnimationOptions,
    AnimationOptionsError, CompositionError, CompositionInterval, CompositionTimeMap,
    CompositionTimeMapStep, ObjectId, PreparedSemanticMutationTransaction, RateFunction,
    ResolvedAnimationOptions, SemanticAnimationCompositionKind, SemanticAnimationError,
    SemanticAnimationIntent, SemanticFadeDirection, SemanticNodeId, SemanticStore,
    SemanticTransactionAnimationIntent, SemanticTransactionNodeRef, SemanticTransactionReadError,
    TrackTiming,
};

use super::SemanticExecutionIndex;

/// Compiler-owned scheduling result for one explicitly activated semantic animation root.
///
/// This is not a runtime animation graph. It resolves authored composition/default timing
/// into the same root timing and `CompositionTimeMap` representation already consumed by
/// compiled tracks, while preserving semantic target-state identity for the later payload
/// lowering step.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticAnimationScheduleProjection {
    root: SemanticNodeId,
    start_time: f64,
    run_time: f64,
    leaves: Vec<SemanticScheduledAnimationLeaf>,
}

impl SemanticAnimationScheduleProjection {
    pub const fn root(&self) -> SemanticNodeId {
        self.root
    }

    pub const fn start_time(&self) -> f64 {
        self.start_time
    }

    pub const fn run_time(&self) -> f64 {
        self.run_time
    }

    pub fn leaves(&self) -> &[SemanticScheduledAnimationLeaf] {
        &self.leaves
    }

    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }
}

/// Payload kind retained by one scheduled published animation leaf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SemanticScheduledAnimationPayload {
    TransformTo { target_state: SemanticNodeId },
    Rotate { angle: f64 },
    Fade { direction: SemanticFadeDirection },
    Create,
}

/// One scheduled semantic animation leaf ready for payload-specific track lowering.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticScheduledAnimationLeaf {
    pub animation: SemanticNodeId,
    pub target: SemanticNodeId,
    pub execution_object_id: ObjectId,
    pub payload: SemanticScheduledAnimationPayload,
    /// Every leaf uses the activated root interval. Nested child intervals are carried
    /// by `time_map`, matching the existing compiled-track evaluation contract.
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
    /// Resolved leaf-local options. Lifecycle flags remain explicit instead of being
    /// silently discarded before the payload/lifecycle lowering slice consumes them.
    pub options: ResolvedAnimationOptions,
}

/// Compiler scheduling result for an animation graph held by one prepared semantic transaction.
///
/// References remain transaction-local until the caller commits the prepared transaction. This
/// projection allocates no semantic identity and carries the same root timing and time-map values
/// as the published-store lowering path.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSemanticAnimationScheduleProjection {
    root: SemanticTransactionNodeRef,
    start_time: f64,
    run_time: f64,
    leaves: Vec<PreparedSemanticScheduledAnimationLeaf>,
}

impl PreparedSemanticAnimationScheduleProjection {
    pub const fn root(&self) -> SemanticTransactionNodeRef {
        self.root
    }

    pub const fn start_time(&self) -> f64 {
        self.start_time
    }

    pub const fn run_time(&self) -> f64 {
        self.run_time
    }

    pub fn leaves(&self) -> &[PreparedSemanticScheduledAnimationLeaf] {
        &self.leaves
    }

    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }
}

/// Payload kind retained by one scheduled transaction-local animation leaf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PreparedSemanticScheduledAnimationPayload {
    TransformTo {
        target_state: SemanticTransactionNodeRef,
    },
    Rotate {
        angle: f64,
    },
    Fade {
        direction: SemanticFadeDirection,
    },
    Create,
}

/// One scheduled leaf whose authored identities are still transaction-local.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSemanticScheduledAnimationLeaf {
    pub animation: SemanticTransactionNodeRef,
    pub target: SemanticTransactionNodeRef,
    pub execution_object_id: ObjectId,
    pub payload: PreparedSemanticScheduledAnimationPayload,
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
    pub options: ResolvedAnimationOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedSemanticAnimationLookupError {
    Transaction(SemanticTransactionReadError),
    Existing(SemanticAnimationError),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedSemanticAnimationScheduleError {
    InvalidStartTime(f64),
    Lookup {
        animation: SemanticTransactionNodeRef,
        error: PreparedSemanticAnimationLookupError,
    },
    Options {
        animation: SemanticTransactionNodeRef,
        error: AnimationOptionsError,
    },
    Composition {
        animation: SemanticTransactionNodeRef,
        error: CompositionError,
    },
    MissingExecutionTarget {
        animation: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
    },
    UnsupportedCompositionLifecycle {
        animation: SemanticTransactionNodeRef,
        remover: bool,
        introducer: bool,
    },
    InvalidResolvedInterval {
        animation: SemanticTransactionNodeRef,
        child_index: usize,
    },
}

impl std::fmt::Display for PreparedSemanticAnimationScheduleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "prepared semantic animation scheduling failed: {self:?}"
        )
    }
}

impl std::error::Error for PreparedSemanticAnimationScheduleError {}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticAnimationScheduleError {
    InvalidStartTime(f64),
    Animation(SemanticAnimationError),
    Options {
        animation: SemanticNodeId,
        error: AnimationOptionsError,
    },
    Composition {
        animation: SemanticNodeId,
        error: CompositionError,
    },
    MissingExecutionTarget {
        animation: SemanticNodeId,
        target: SemanticNodeId,
    },
    UnsupportedCompositionLifecycle {
        animation: SemanticNodeId,
        remover: bool,
        introducer: bool,
    },
    InvalidResolvedInterval {
        animation: SemanticNodeId,
        child_index: usize,
    },
}

impl std::fmt::Display for SemanticAnimationScheduleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStartTime(value) => {
                write!(formatter, "semantic animation start time must be finite, got {value}")
            }
            Self::Animation(error) => error.fmt(formatter),
            Self::Options { animation, error } => write!(
                formatter,
                "semantic animation {}:{} option resolution failed: {error}",
                animation.slot(),
                animation.generation()
            ),
            Self::Composition { animation, error } => write!(
                formatter,
                "semantic animation {}:{} composition scheduling failed: {error}",
                animation.slot(),
                animation.generation()
            ),
            Self::MissingExecutionTarget { animation, target } => write!(
                formatter,
                "semantic animation {}:{} targets object {}:{} outside the lowered execution membership",
                animation.slot(),
                animation.generation(),
                target.slot(),
                target.generation()
            ),
            Self::UnsupportedCompositionLifecycle {
                animation,
                remover,
                introducer,
            } => write!(
                formatter,
                "semantic animation composition {}:{} cannot yet lower remover={remover} introducer={introducer} lifecycle semantics",
                animation.slot(),
                animation.generation()
            ),
            Self::InvalidResolvedInterval {
                animation,
                child_index,
            } => write!(
                formatter,
                "semantic animation composition {}:{} produced an invalid interval for child {child_index}",
                animation.slot(),
                animation.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticAnimationScheduleError {}

/// Resolve one explicitly selected semantic animation declaration into deterministic
/// execution timing without creating another scheduler or evaluator.
///
/// The caller supplies the activation start and `Scene.play`-style root overrides.
/// Detached declarations are therefore never scheduled merely because they exist in
/// the semantic store. Target membership is read from the already-established
/// semantic-to-execution index; this function never allocates execution object identity.
pub fn lower_semantic_animation_schedule(
    store: &SemanticStore,
    index: &SemanticExecutionIndex,
    root: SemanticNodeId,
    start_time: f64,
    play_options: AnimationOptions,
) -> Result<SemanticAnimationScheduleProjection, SemanticAnimationScheduleError> {
    let lookup = PublishedAnimationLookup { store, index };
    let projection = lower_animation_schedule(&lookup, root, start_time, play_options)
        .map_err(published_schedule_error)?;

    Ok(SemanticAnimationScheduleProjection {
        root,
        start_time: projection.start_time,
        run_time: projection.run_time,
        leaves: projection
            .leaves
            .into_iter()
            .map(|leaf| SemanticScheduledAnimationLeaf {
                animation: leaf.animation,
                target: leaf.target,
                execution_object_id: leaf.execution_object_id,
                payload: match leaf.payload {
                    ScheduledAnimationPayload::TransformTo { target_state } => {
                        SemanticScheduledAnimationPayload::TransformTo { target_state }
                    }
                    ScheduledAnimationPayload::Rotate { angle } => {
                        SemanticScheduledAnimationPayload::Rotate { angle }
                    }
                    ScheduledAnimationPayload::Fade { direction } => {
                        SemanticScheduledAnimationPayload::Fade { direction }
                    }
                    ScheduledAnimationPayload::Create => SemanticScheduledAnimationPayload::Create,
                },
                timing: leaf.timing,
                time_map: leaf.time_map,
                options: leaf.options,
            })
            .collect(),
    })
}

/// Resolve one not-yet-committed animation graph through the same scheduler used for published
/// declarations. Object reads use the prepared transaction's final staged view. Existing members
/// use the semantic-to-execution index; one entering existing object uses the same deterministic
/// execution identity that membership lowering will publish after commit.
pub fn lower_prepared_semantic_animation_schedule(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    index: &SemanticExecutionIndex,
    root: impl Into<SemanticTransactionNodeRef>,
    start_time: f64,
    play_options: AnimationOptions,
) -> Result<PreparedSemanticAnimationScheduleProjection, PreparedSemanticAnimationScheduleError> {
    let root = root.into();
    let lookup = PreparedAnimationLookup { prepared, index };
    let projection = lower_animation_schedule(&lookup, root, start_time, play_options)
        .map_err(prepared_schedule_error)?;
    Ok(PreparedSemanticAnimationScheduleProjection {
        root,
        start_time: projection.start_time,
        run_time: projection.run_time,
        leaves: projection
            .leaves
            .into_iter()
            .map(|leaf| PreparedSemanticScheduledAnimationLeaf {
                animation: leaf.animation,
                target: leaf.target,
                execution_object_id: leaf.execution_object_id,
                payload: match leaf.payload {
                    ScheduledAnimationPayload::TransformTo { target_state } => {
                        PreparedSemanticScheduledAnimationPayload::TransformTo { target_state }
                    }
                    ScheduledAnimationPayload::Rotate { angle } => {
                        PreparedSemanticScheduledAnimationPayload::Rotate { angle }
                    }
                    ScheduledAnimationPayload::Fade { direction } => {
                        PreparedSemanticScheduledAnimationPayload::Fade { direction }
                    }
                    ScheduledAnimationPayload::Create => {
                        PreparedSemanticScheduledAnimationPayload::Create
                    }
                },
                timing: leaf.timing,
                time_map: leaf.time_map,
                options: leaf.options,
            })
            .collect(),
    })
}

#[derive(Clone, Debug)]
struct AnimationDeclaration<R> {
    intent: AnimationDeclarationIntent<R>,
    options: AnimationOptions,
}

#[derive(Clone, Debug)]
enum AnimationDeclarationIntent<R> {
    TransformTo {
        target: R,
        target_state: R,
    },
    Rotate {
        target: R,
        angle: f64,
    },
    Fade {
        target: R,
        direction: SemanticFadeDirection,
    },
    Create {
        target: R,
    },
    Composition {
        kind: SemanticAnimationCompositionKind,
        children: Vec<R>,
    },
}

trait AnimationScheduleLookup {
    type Reference: Copy;
    type Error;

    fn animation(
        &self,
        animation: Self::Reference,
    ) -> Result<AnimationDeclaration<Self::Reference>, Self::Error>;

    fn execution_object_id(&self, target: Self::Reference) -> Option<ObjectId>;

    fn entering_execution_object_id(&self, _target: Self::Reference) -> Option<ObjectId> {
        None
    }
}

struct PublishedAnimationLookup<'a> {
    store: &'a SemanticStore,
    index: &'a SemanticExecutionIndex,
}

impl AnimationScheduleLookup for PublishedAnimationLookup<'_> {
    type Reference = SemanticNodeId;
    type Error = SemanticAnimationError;

    fn animation(
        &self,
        animation: Self::Reference,
    ) -> Result<AnimationDeclaration<Self::Reference>, Self::Error> {
        let state = self.store.semantic_animation_state(animation)?;
        let intent = match state.intent() {
            SemanticAnimationIntent::TransformTo {
                target,
                target_state,
            } => {
                self.store
                    .semantic_object_state_checked(*target)
                    .map_err(SemanticAnimationError::Target)?;
                self.store
                    .semantic_object_state_checked(*target_state)
                    .map_err(SemanticAnimationError::Target)?;
                AnimationDeclarationIntent::TransformTo {
                    target: *target,
                    target_state: *target_state,
                }
            }
            SemanticAnimationIntent::Rotate { target, angle } => {
                self.store
                    .semantic_object_state_checked(*target)
                    .map_err(SemanticAnimationError::Target)?;
                AnimationDeclarationIntent::Rotate {
                    target: *target,
                    angle: *angle,
                }
            }
            SemanticAnimationIntent::Fade { target, direction } => {
                self.store
                    .semantic_object_state_checked(*target)
                    .map_err(SemanticAnimationError::Target)?;
                AnimationDeclarationIntent::Fade {
                    target: *target,
                    direction: *direction,
                }
            }
            SemanticAnimationIntent::Create { target } => {
                self.store
                    .semantic_object_state_checked(*target)
                    .map_err(SemanticAnimationError::Target)?;
                AnimationDeclarationIntent::Create { target: *target }
            }
            SemanticAnimationIntent::Composition { kind, children } => {
                AnimationDeclarationIntent::Composition {
                    kind: *kind,
                    children: children.clone(),
                }
            }
        };
        Ok(AnimationDeclaration {
            intent,
            options: state.options(),
        })
    }

    fn execution_object_id(&self, target: Self::Reference) -> Option<ObjectId> {
        self.index.execution_object_id(target)
    }
}

struct PreparedAnimationLookup<'a, 'store> {
    prepared: &'a PreparedSemanticMutationTransaction<'store>,
    index: &'a SemanticExecutionIndex,
}

impl AnimationScheduleLookup for PreparedAnimationLookup<'_, '_> {
    type Reference = SemanticTransactionNodeRef;
    type Error = PreparedSemanticAnimationLookupError;

    fn animation(
        &self,
        animation: Self::Reference,
    ) -> Result<AnimationDeclaration<Self::Reference>, Self::Error> {
        if self.prepared.node_is_removed(animation) {
            return Err(PreparedSemanticAnimationLookupError::Transaction(
                match animation {
                    SemanticTransactionNodeRef::Existing(node) => {
                        SemanticTransactionReadError::RemovedExistingNode(node)
                    }
                    SemanticTransactionNodeRef::Pending(token) => {
                        SemanticTransactionReadError::RemovedPendingNode(token)
                    }
                },
            ));
        }
        let (intent, options) = match animation {
            SemanticTransactionNodeRef::Existing(node) => {
                let state = self
                    .prepared
                    .store()
                    .semantic_animation_state(node)
                    .map_err(PreparedSemanticAnimationLookupError::Existing)?;
                let intent = match state.intent() {
                    SemanticAnimationIntent::TransformTo {
                        target,
                        target_state,
                    } => AnimationDeclarationIntent::TransformTo {
                        target: (*target).into(),
                        target_state: (*target_state).into(),
                    },
                    SemanticAnimationIntent::Rotate { target, angle } => {
                        AnimationDeclarationIntent::Rotate {
                            target: (*target).into(),
                            angle: *angle,
                        }
                    }
                    SemanticAnimationIntent::Fade { target, direction } => {
                        AnimationDeclarationIntent::Fade {
                            target: (*target).into(),
                            direction: *direction,
                        }
                    }
                    SemanticAnimationIntent::Create { target } => {
                        AnimationDeclarationIntent::Create {
                            target: (*target).into(),
                        }
                    }
                    SemanticAnimationIntent::Composition { kind, children } => {
                        AnimationDeclarationIntent::Composition {
                            kind: *kind,
                            children: children.iter().copied().map(Into::into).collect(),
                        }
                    }
                };
                (intent, state.options())
            }
            SemanticTransactionNodeRef::Pending(token) => {
                let state = self
                    .prepared
                    .pending_animation(token)
                    .map_err(PreparedSemanticAnimationLookupError::Transaction)?;
                let intent = match state.intent() {
                    SemanticTransactionAnimationIntent::TransformTo {
                        target,
                        target_state,
                    } => AnimationDeclarationIntent::TransformTo {
                        target: *target,
                        target_state: *target_state,
                    },
                    SemanticTransactionAnimationIntent::Rotate { target, angle } => {
                        AnimationDeclarationIntent::Rotate {
                            target: *target,
                            angle: *angle,
                        }
                    }
                    SemanticTransactionAnimationIntent::Fade { target, direction } => {
                        AnimationDeclarationIntent::Fade {
                            target: *target,
                            direction: *direction,
                        }
                    }
                    SemanticTransactionAnimationIntent::Create { target } => {
                        AnimationDeclarationIntent::Create { target: *target }
                    }
                    SemanticTransactionAnimationIntent::Composition { kind, children } => {
                        AnimationDeclarationIntent::Composition {
                            kind: *kind,
                            children: children.clone(),
                        }
                    }
                };
                (intent, state.options())
            }
        };
        match &intent {
            AnimationDeclarationIntent::TransformTo {
                target,
                target_state,
            } => {
                self.prepared
                    .object_state(*target)
                    .map_err(PreparedSemanticAnimationLookupError::Transaction)?;
                self.prepared
                    .object_state(*target_state)
                    .map_err(PreparedSemanticAnimationLookupError::Transaction)?;
            }
            AnimationDeclarationIntent::Rotate { target, .. } => {
                self.prepared
                    .object_state(*target)
                    .map_err(PreparedSemanticAnimationLookupError::Transaction)?;
            }
            AnimationDeclarationIntent::Fade { target, .. } => {
                self.prepared
                    .object_state(*target)
                    .map_err(PreparedSemanticAnimationLookupError::Transaction)?;
            }
            AnimationDeclarationIntent::Create { target } => {
                self.prepared
                    .object_state(*target)
                    .map_err(PreparedSemanticAnimationLookupError::Transaction)?;
            }
            AnimationDeclarationIntent::Composition { .. } => {}
        }
        Ok(AnimationDeclaration { intent, options })
    }

    fn execution_object_id(&self, target: Self::Reference) -> Option<ObjectId> {
        target
            .existing()
            .and_then(|target| self.index.execution_object_id(target))
    }

    fn entering_execution_object_id(&self, target: Self::Reference) -> Option<ObjectId> {
        target.existing().map(super::semantic_execution_object_id)
    }
}

#[derive(Clone, Debug)]
struct AnimationScheduleProjection<R> {
    start_time: f64,
    run_time: f64,
    leaves: Vec<ScheduledAnimationLeaf<R>>,
}

#[derive(Clone, Debug)]
struct ScheduledAnimationLeaf<R> {
    animation: R,
    target: R,
    execution_object_id: ObjectId,
    payload: ScheduledAnimationPayload<R>,
    timing: TrackTiming,
    time_map: CompositionTimeMap,
    options: ResolvedAnimationOptions,
}

#[derive(Clone, Copy, Debug)]
enum ScheduledAnimationPayload<R> {
    TransformTo { target_state: R },
    Rotate { angle: f64 },
    Fade { direction: SemanticFadeDirection },
    Create,
}

#[derive(Clone, Debug)]
enum AnimationSchedulePlanError<R, E> {
    InvalidStartTime(f64),
    Lookup {
        animation: R,
        error: E,
    },
    Options {
        animation: R,
        error: AnimationOptionsError,
    },
    Composition {
        animation: R,
        error: CompositionError,
    },
    MissingExecutionTarget {
        animation: R,
        target: R,
    },
    UnsupportedCompositionLifecycle {
        animation: R,
        remover: bool,
        introducer: bool,
    },
    InvalidResolvedInterval {
        animation: R,
        child_index: usize,
    },
}

type SchedulePlanResult<L, T> = Result<
    T,
    AnimationSchedulePlanError<
        <L as AnimationScheduleLookup>::Reference,
        <L as AnimationScheduleLookup>::Error,
    >,
>;

fn lower_animation_schedule<L>(
    lookup: &L,
    root: L::Reference,
    start_time: f64,
    play_options: AnimationOptions,
) -> SchedulePlanResult<L, AnimationScheduleProjection<L::Reference>>
where
    L: AnimationScheduleLookup,
{
    if !start_time.is_finite() {
        return Err(AnimationSchedulePlanError::InvalidStartTime(start_time));
    }
    let plan = plan_animation(lookup, root, play_options)?;
    let mut leaves = Vec::new();
    collect_leaves(
        &plan,
        start_time,
        plan.run_time,
        &mut Vec::new(),
        &mut leaves,
    );
    Ok(AnimationScheduleProjection {
        start_time,
        run_time: plan.run_time,
        leaves,
    })
}

#[derive(Clone, Debug)]
struct PlannedAnimation<R> {
    animation: R,
    run_time: f64,
    kind: PlannedAnimationKind<R>,
}

#[derive(Clone, Debug)]
enum PlannedAnimationKind<R> {
    Leaf {
        target: R,
        execution_object_id: ObjectId,
        payload: ScheduledAnimationPayload<R>,
        options: ResolvedAnimationOptions,
    },
    Composition {
        rate_func: RateFunction,
        children: Vec<PlannedCompositionChild<R>>,
    },
}

#[derive(Clone, Debug)]
struct PlannedCompositionChild<R> {
    interval: CompositionInterval,
    animation: PlannedAnimation<R>,
}

fn plan_animation<L>(
    lookup: &L,
    animation: L::Reference,
    play_options: AnimationOptions,
) -> SchedulePlanResult<L, PlannedAnimation<L::Reference>>
where
    L: AnimationScheduleLookup,
{
    let state = lookup
        .animation(animation)
        .map_err(|error| AnimationSchedulePlanError::Lookup { animation, error })?;

    match state.intent {
        AnimationDeclarationIntent::TransformTo {
            target,
            target_state,
        } => {
            let execution_object_id = lookup
                .execution_object_id(target)
                .ok_or(AnimationSchedulePlanError::MissingExecutionTarget { animation, target })?;
            let options =
                resolve_animation_options(AnimationDefaults::MANIM, state.options, play_options)
                    .map_err(|error| AnimationSchedulePlanError::Options { animation, error })?;

            Ok(PlannedAnimation {
                animation,
                run_time: options.run_time,
                kind: PlannedAnimationKind::Leaf {
                    target,
                    execution_object_id,
                    payload: ScheduledAnimationPayload::TransformTo { target_state },
                    options,
                },
            })
        }
        AnimationDeclarationIntent::Rotate { target, angle } => {
            let execution_object_id = lookup
                .execution_object_id(target)
                .or_else(|| lookup.entering_execution_object_id(target))
                .ok_or(AnimationSchedulePlanError::MissingExecutionTarget { animation, target })?;
            let options =
                resolve_animation_options(AnimationDefaults::MANIM, state.options, play_options)
                    .map_err(|error| AnimationSchedulePlanError::Options { animation, error })?;
            Ok(PlannedAnimation {
                animation,
                run_time: options.run_time,
                kind: PlannedAnimationKind::Leaf {
                    target,
                    execution_object_id,
                    payload: ScheduledAnimationPayload::Rotate { angle },
                    options,
                },
            })
        }
        AnimationDeclarationIntent::Fade { target, direction } => {
            let options = resolve_animation_options(
                AnimationDefaults {
                    introducer: matches!(direction, SemanticFadeDirection::In),
                    remover: matches!(direction, SemanticFadeDirection::Out),
                    ..AnimationDefaults::MANIM
                },
                state.options,
                play_options,
            )
            .map_err(|error| AnimationSchedulePlanError::Options { animation, error })?;
            let lifecycle_matches = match direction {
                SemanticFadeDirection::In => options.introducer && !options.remover,
                SemanticFadeDirection::Out => options.remover && !options.introducer,
            };
            if !lifecycle_matches {
                return Err(
                    AnimationSchedulePlanError::UnsupportedCompositionLifecycle {
                        animation,
                        remover: options.remover,
                        introducer: options.introducer,
                    },
                );
            }
            let execution_object_id = lookup
                .execution_object_id(target)
                .or_else(|| lookup.entering_execution_object_id(target))
                .ok_or(AnimationSchedulePlanError::MissingExecutionTarget { animation, target })?;
            Ok(PlannedAnimation {
                animation,
                run_time: options.run_time,
                kind: PlannedAnimationKind::Leaf {
                    target,
                    execution_object_id,
                    payload: ScheduledAnimationPayload::Fade { direction },
                    options,
                },
            })
        }
        AnimationDeclarationIntent::Create { target } => {
            // Reveal lowering owns reversal; the generic option validator deliberately
            // rejects it for animation kinds without a reversed realization.
            let reverse = play_options
                .reverse_rate_function
                .or(state.options.reverse_rate_function)
                .unwrap_or(false);
            let mut options = resolve_animation_options(
                AnimationDefaults {
                    introducer: true,
                    remover: false,
                    ..AnimationDefaults::MANIM
                },
                state.options,
                play_options.reverse_rate_function(false),
            )
            .map_err(|error| AnimationSchedulePlanError::Options { animation, error })?;
            options.reverse_rate_function = reverse;
            if !options.introducer {
                return Err(
                    AnimationSchedulePlanError::UnsupportedCompositionLifecycle {
                        animation,
                        remover: options.remover,
                        introducer: options.introducer,
                    },
                );
            }
            let execution_object_id = lookup
                .execution_object_id(target)
                .or_else(|| lookup.entering_execution_object_id(target))
                .ok_or(AnimationSchedulePlanError::MissingExecutionTarget { animation, target })?;
            Ok(PlannedAnimation {
                animation,
                run_time: options.run_time,
                kind: PlannedAnimationKind::Leaf {
                    target,
                    execution_object_id,
                    payload: ScheduledAnimationPayload::Create,
                    options,
                },
            })
        }
        AnimationDeclarationIntent::Composition { kind, children } => {
            let mut planned_children = Vec::with_capacity(children.len());
            for child in children {
                planned_children.push(plan_animation(lookup, child, AnimationOptions::new())?);
            }
            let child_run_times = planned_children
                .iter()
                .map(|child| child.run_time)
                .collect::<Vec<_>>();
            let default_lag_ratio = match kind {
                SemanticAnimationCompositionKind::Parallel => 0.0,
                SemanticAnimationCompositionKind::Sequence => 1.0,
            };
            let requested_lag_ratio = play_options
                .lag_ratio
                .or(state.options.lag_ratio)
                .unwrap_or(default_lag_ratio);
            let intrinsic =
                resolve_composition_schedule(&child_run_times, requested_lag_ratio, None).map_err(
                    |error| AnimationSchedulePlanError::Composition { animation, error },
                )?;
            let defaults = AnimationDefaults {
                run_time: intrinsic.run_time,
                rate_func: RateFunction::Linear,
                lag_ratio: default_lag_ratio,
                path_arc: 0.0,
                reverse_rate_function: false,
                remover: false,
                introducer: false,
            };
            let options = resolve_animation_options(defaults, state.options, play_options)
                .map_err(|error| AnimationSchedulePlanError::Options { animation, error })?;
            if options.remover || options.introducer {
                return Err(
                    AnimationSchedulePlanError::UnsupportedCompositionLifecycle {
                        animation,
                        remover: options.remover,
                        introducer: options.introducer,
                    },
                );
            }
            let schedule = resolve_composition_schedule(
                &child_run_times,
                options.lag_ratio,
                Some(options.run_time),
            )
            .map_err(|error| AnimationSchedulePlanError::Composition { animation, error })?;

            let children = planned_children
                .into_iter()
                .zip(schedule.intervals)
                .enumerate()
                .map(|(child_index, (animation_plan, interval))| {
                    if !interval.start_time.is_finite()
                        || !interval.duration.is_finite()
                        || interval.duration <= 0.0
                    {
                        return Err(AnimationSchedulePlanError::InvalidResolvedInterval {
                            animation,
                            child_index,
                        });
                    }
                    Ok(PlannedCompositionChild {
                        interval,
                        animation: animation_plan,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            Ok(PlannedAnimation {
                animation,
                run_time: schedule.run_time,
                kind: PlannedAnimationKind::Composition {
                    rate_func: options.rate_func,
                    children,
                },
            })
        }
    }
}

fn collect_leaves<R: Copy>(
    plan: &PlannedAnimation<R>,
    root_start_time: f64,
    root_run_time: f64,
    steps: &mut Vec<CompositionTimeMapStep>,
    leaves: &mut Vec<ScheduledAnimationLeaf<R>>,
) {
    match &plan.kind {
        PlannedAnimationKind::Leaf {
            target,
            execution_object_id,
            payload,
            options,
        } => leaves.push(ScheduledAnimationLeaf {
            animation: plan.animation,
            target: *target,
            execution_object_id: *execution_object_id,
            payload: *payload,
            timing: TrackTiming::new(root_start_time, root_run_time, options.rate_func),
            time_map: CompositionTimeMap::from_steps(steps.clone()),
            options: *options,
        }),
        PlannedAnimationKind::Composition {
            rate_func,
            children,
        } => {
            for child in children {
                steps.push(CompositionTimeMapStep::new(
                    child.interval.start_time / plan.run_time,
                    child.interval.duration / plan.run_time,
                    *rate_func,
                ));
                collect_leaves(
                    &child.animation,
                    root_start_time,
                    root_run_time,
                    steps,
                    leaves,
                );
                steps.pop();
            }
        }
    }
}

fn published_schedule_error(
    error: AnimationSchedulePlanError<SemanticNodeId, SemanticAnimationError>,
) -> SemanticAnimationScheduleError {
    match error {
        AnimationSchedulePlanError::InvalidStartTime(value) => {
            SemanticAnimationScheduleError::InvalidStartTime(value)
        }
        AnimationSchedulePlanError::Lookup { error, .. } => {
            SemanticAnimationScheduleError::Animation(error)
        }
        AnimationSchedulePlanError::Options { animation, error } => {
            SemanticAnimationScheduleError::Options { animation, error }
        }
        AnimationSchedulePlanError::Composition { animation, error } => {
            SemanticAnimationScheduleError::Composition { animation, error }
        }
        AnimationSchedulePlanError::MissingExecutionTarget { animation, target } => {
            SemanticAnimationScheduleError::MissingExecutionTarget { animation, target }
        }
        AnimationSchedulePlanError::UnsupportedCompositionLifecycle {
            animation,
            remover,
            introducer,
        } => SemanticAnimationScheduleError::UnsupportedCompositionLifecycle {
            animation,
            remover,
            introducer,
        },
        AnimationSchedulePlanError::InvalidResolvedInterval {
            animation,
            child_index,
        } => SemanticAnimationScheduleError::InvalidResolvedInterval {
            animation,
            child_index,
        },
    }
}

fn prepared_schedule_error(
    error: AnimationSchedulePlanError<
        SemanticTransactionNodeRef,
        PreparedSemanticAnimationLookupError,
    >,
) -> PreparedSemanticAnimationScheduleError {
    match error {
        AnimationSchedulePlanError::InvalidStartTime(value) => {
            PreparedSemanticAnimationScheduleError::InvalidStartTime(value)
        }
        AnimationSchedulePlanError::Lookup { animation, error } => {
            PreparedSemanticAnimationScheduleError::Lookup { animation, error }
        }
        AnimationSchedulePlanError::Options { animation, error } => {
            PreparedSemanticAnimationScheduleError::Options { animation, error }
        }
        AnimationSchedulePlanError::Composition { animation, error } => {
            PreparedSemanticAnimationScheduleError::Composition { animation, error }
        }
        AnimationSchedulePlanError::MissingExecutionTarget { animation, target } => {
            PreparedSemanticAnimationScheduleError::MissingExecutionTarget { animation, target }
        }
        AnimationSchedulePlanError::UnsupportedCompositionLifecycle {
            animation,
            remover,
            introducer,
        } => PreparedSemanticAnimationScheduleError::UnsupportedCompositionLifecycle {
            animation,
            remover,
            introducer,
        },
        AnimationSchedulePlanError::InvalidResolvedInterval {
            animation,
            child_index,
        } => PreparedSemanticAnimationScheduleError::InvalidResolvedInterval {
            animation,
            child_index,
        },
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptionsError, RateFunction, SemanticObjectState, SemanticStore, StoredGeometry,
    };

    use super::*;

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    fn visible_target(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        let target = object(store, radius);
        store.attach_semantic_object(target).unwrap();
        target
    }

    fn prepare_index(store: &SemanticStore) -> SemanticExecutionIndex {
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(store).unwrap();
        index
    }

    #[test]
    fn leaf_activation_maps_live_target_and_preserves_detached_target_state() {
        let mut store = SemanticStore::new();
        let target = visible_target(&mut store, 1.0);
        let target_state = object(&mut store, 2.0);
        let animation = store
            .insert_semantic_transform_animation(
                target,
                target_state,
                AnimationOptions::new()
                    .run_time(2.5)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let index = prepare_index(&store);

        let schedule = lower_semantic_animation_schedule(
            &store,
            &index,
            animation,
            3.0,
            AnimationOptions::new(),
        )
        .unwrap();

        assert_eq!(schedule.root(), animation);
        assert_eq!(schedule.start_time(), 3.0);
        assert_eq!(schedule.run_time(), 2.5);
        assert_eq!(schedule.len(), 1);
        let leaf = &schedule.leaves()[0];
        assert_eq!(leaf.animation, animation);
        assert_eq!(leaf.target, target);
        assert_eq!(
            leaf.payload,
            SemanticScheduledAnimationPayload::TransformTo { target_state }
        );
        assert_eq!(
            leaf.execution_object_id,
            index.execution_object_id(target).unwrap()
        );
        assert_eq!(
            leaf.timing,
            TrackTiming::new(3.0, 2.5, RateFunction::Linear)
        );
        assert!(leaf.time_map.is_identity());
        assert!(!store.node(target_state).unwrap().is_scene_owned());
    }

    #[test]
    fn play_options_override_only_the_activated_root_leaf() {
        let mut store = SemanticStore::new();
        let target = visible_target(&mut store, 1.0);
        let target_state = object(&mut store, 2.0);
        let animation = store
            .insert_semantic_transform_animation(
                target,
                target_state,
                AnimationOptions::new()
                    .run_time(4.0)
                    .rate_func(RateFunction::Smooth),
            )
            .unwrap();
        let index = prepare_index(&store);

        let schedule = lower_semantic_animation_schedule(
            &store,
            &index,
            animation,
            1.0,
            AnimationOptions::new()
                .run_time(0.5)
                .rate_func(RateFunction::ThereAndBack),
        )
        .unwrap();

        assert_eq!(schedule.run_time(), 0.5);
        assert_eq!(
            schedule.leaves()[0].timing,
            TrackTiming::new(1.0, 0.5, RateFunction::ThereAndBack)
        );
    }

    #[test]
    fn nested_sequence_and_parallel_lower_to_existing_root_to_leaf_time_map() {
        let mut store = SemanticStore::new();

        let first_target = visible_target(&mut store, 1.0);
        let first_state = object(&mut store, 1.5);
        let first = store
            .insert_semantic_transform_animation(
                first_target,
                first_state,
                AnimationOptions::new().run_time(1.0),
            )
            .unwrap();

        let second_target = visible_target(&mut store, 2.0);
        let second_state = object(&mut store, 2.5);
        let second = store
            .insert_semantic_transform_animation(
                second_target,
                second_state,
                AnimationOptions::new().run_time(2.0),
            )
            .unwrap();

        let parallel = store
            .insert_semantic_parallel_animation(
                &[first, second],
                AnimationOptions::new()
                    .run_time(5.0)
                    .rate_func(RateFunction::Smooth)
                    .lag_ratio(0.5),
            )
            .unwrap();

        let third_target = visible_target(&mut store, 3.0);
        let third_state = object(&mut store, 3.5);
        let third = store
            .insert_semantic_transform_animation(
                third_target,
                third_state,
                AnimationOptions::new().run_time(1.0),
            )
            .unwrap();

        let root = store
            .insert_semantic_sequence_animation(
                &[parallel, third],
                AnimationOptions::new().rate_func(RateFunction::Linear),
            )
            .unwrap();
        let index = prepare_index(&store);

        let schedule =
            lower_semantic_animation_schedule(&store, &index, root, 10.0, AnimationOptions::new())
                .unwrap();

        assert_eq!(schedule.run_time(), 6.0);
        assert_eq!(schedule.len(), 3);
        assert!(schedule
            .leaves()
            .iter()
            .all(|leaf| leaf.timing.start_time == 10.0 && leaf.timing.duration == 6.0));

        let first_steps = &schedule.leaves()[0].time_map.steps;
        assert_eq!(first_steps.len(), 2);
        assert_eq!(first_steps[0].start, 0.0);
        assert!((first_steps[0].duration - 5.0 / 6.0).abs() < 1e-12);
        assert_eq!(first_steps[0].rate_func, RateFunction::Linear);
        assert_eq!(first_steps[1].start, 0.0);
        assert!((first_steps[1].duration - 0.4).abs() < 1e-12);
        assert_eq!(first_steps[1].rate_func, RateFunction::Smooth);

        let second_steps = &schedule.leaves()[1].time_map.steps;
        assert_eq!(second_steps.len(), 2);
        assert!((second_steps[1].start - 0.2).abs() < 1e-12);
        assert!((second_steps[1].duration - 0.8).abs() < 1e-12);

        let third_steps = &schedule.leaves()[2].time_map.steps;
        assert_eq!(third_steps.len(), 1);
        assert!((third_steps[0].start - 5.0 / 6.0).abs() < 1e-12);
        assert!((third_steps[0].duration - 1.0 / 6.0).abs() < 1e-12);
    }

    #[test]
    fn detached_target_is_not_admitted_into_execution_by_animation_scheduling() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let target_state = object(&mut store, 2.0);
        let animation = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let index = prepare_index(&store);

        assert_eq!(
            lower_semantic_animation_schedule(
                &store,
                &index,
                animation,
                0.0,
                AnimationOptions::new(),
            ),
            Err(SemanticAnimationScheduleError::MissingExecutionTarget { animation, target })
        );
        assert!(index.execution_object_id(target).is_none());
    }

    #[test]
    fn unsupported_timing_capabilities_fail_closed_with_animation_identity() {
        let mut store = SemanticStore::new();
        let target = visible_target(&mut store, 1.0);
        let target_state = object(&mut store, 2.0);
        let animation = store
            .insert_semantic_transform_animation(
                target,
                target_state,
                AnimationOptions::new().path_arc(0.5),
            )
            .unwrap();
        let index = prepare_index(&store);

        assert_eq!(
            lower_semantic_animation_schedule(
                &store,
                &index,
                animation,
                0.0,
                AnimationOptions::new(),
            ),
            Err(SemanticAnimationScheduleError::Options {
                animation,
                error: AnimationOptionsError::UnsupportedPathArc(0.5),
            })
        );
    }

    #[test]
    fn composition_lifecycle_flags_are_not_silently_dropped() {
        let mut store = SemanticStore::new();
        let target = visible_target(&mut store, 1.0);
        let target_state = object(&mut store, 2.0);
        let leaf = store
            .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
            .unwrap();
        let root = store
            .insert_semantic_parallel_animation(&[leaf], AnimationOptions::new().remover(true))
            .unwrap();
        let index = prepare_index(&store);

        assert_eq!(
            lower_semantic_animation_schedule(&store, &index, root, 0.0, AnimationOptions::new(),),
            Err(
                SemanticAnimationScheduleError::UnsupportedCompositionLifecycle {
                    animation: root,
                    remover: true,
                    introducer: false,
                }
            )
        );
    }

    #[test]
    fn invalid_activation_time_fails_before_graph_planning() {
        let store = SemanticStore::new();
        let index = SemanticExecutionIndex::new();
        assert!(matches!(
            lower_semantic_animation_schedule(
                &store,
                &index,
                SemanticNodeId::new(99, 7),
                f64::NAN,
                AnimationOptions::new(),
            ),
            Err(SemanticAnimationScheduleError::InvalidStartTime(value)) if value.is_nan()
        ));
    }
}
