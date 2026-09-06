mod callback;
mod completion;
mod publication;
mod signal_timeline;
pub use callback::*;
pub use completion::*;
pub use publication::*;
pub use signal_timeline::SignalTimelineAppendError;

use callback::{CallbackPublicationReceipt, CallbackSchedule, PendingCallbackPhase};
use signal_timeline::SignalTimelineSchedule;

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::execution_segment::{
    ExecutionSegment, ExecutionSegmentError, ExecutionSegmentSequence, ExecutionSegmentToken,
    PendingSegmentCompletion, PendingSegmentCompletionKind, SegmentCompletionEntry,
};
use noon_compile::{
    lower_prepared_scalar_signal_timeline_entry, lower_prepared_semantic_animation_composition,
    lower_semantic_affine_animation_tracks, lower_semantic_animation_schedule,
    lower_semantic_execution, lower_semantic_execution_root, CompilePatchError,
    EffectiveAnimationProperties, ExecutionMutationTransaction, ExecutionPatch,
    PreparedScalarSignalTimelineError, PreparedSemanticAnimationLoweringError,
    SemanticAffineAnimationTrackError, SemanticAnimationScheduleError, SemanticExecutionIndex,
    SemanticExecutionLoweringError, SemanticExecutionLoweringOutput, SemanticExecutionReachability,
    SemanticReactiveProjection,
};
use noon_core::{
    AnimationOptions, Camera2DState, NativeEventOccurrence, NativeInputRuntimeError,
    NativeInputValue, NativeStateSource, NativeStateUpdate, ObjectId, RateFunction, ReactiveError,
    ReactiveValue, Rect, SemanticAffineLifecycleDirection, SemanticAffineLifecycleEndpoint,
    SemanticAnimationCompositionKind, SemanticFadeDirection, SemanticMutationTransaction,
    SemanticMutationTransactionResult, SemanticNodeCreation, SemanticNodeId,
    SemanticScalarSignalQueryError, SemanticSceneOperationError, SemanticStore,
    SemanticTransactionNodeRef, TimelineError, TrackId, TrackTiming,
};
use noon_runtime::{
    EvaluationError, ExecutionSpatialIndex, FrameChanges, FrameState, RendererPublication,
    RuntimeWakeState, SceneInstance, SpatialIndexUpdateStats, SpatialQueryStats,
};

const NATIVE_EVENT_SEQUENCE_WRAP: f32 = 1_000_000.0;

fn resolve_committed_node(
    node: SemanticTransactionNodeRef,
    result: &SemanticMutationTransactionResult,
) -> SemanticNodeId {
    match node {
        SemanticTransactionNodeRef::Existing(node) => node,
        SemanticTransactionNodeRef::Pending(token) => result
            .resolve(token)
            .expect("prepared animation reference must resolve through its semantic commit"),
    }
}

#[derive(Clone)]
enum PreparedAnimationLifecycle {
    Introduce(SemanticNodeId),
    FadeOut(SemanticNodeId),
    AffineRemove {
        root: SemanticNodeId,
        target: SemanticNodeId,
        admitted: bool,
    },
    Composition {
        root: SemanticNodeId,
        admits: bool,
        removals: Vec<(SemanticNodeId, SemanticNodeId)>,
    },
}

/// Inert recursive declaration input for one atomic live composition.
///
/// The tree borrows no execution state and owns only its child topology. All semantic
/// identity, schedule lowering, admission, and runtime publication remain transaction-local
/// until `declare_and_activate_composition` succeeds.
#[derive(Clone)]
pub(crate) enum SemanticCompositionRequest {
    TransformTo {
        source: SemanticNodeId,
        target_state: SemanticNodeId,
        interpolation: noon_core::SemanticTransformInterpolation,
        options: AnimationOptions,
    },
    Rotate {
        target: SemanticNodeId,
        angle: f64,
        options: AnimationOptions,
    },
    Wait {
        duration: f64,
    },
    Add {
        target: SemanticNodeId,
        options: AnimationOptions,
    },
    Fade {
        target: SemanticNodeId,
        direction: SemanticFadeDirection,
        options: AnimationOptions,
    },
    Create {
        target: SemanticNodeId,
        options: AnimationOptions,
    },
    AffineLifecycle {
        target: SemanticNodeId,
        direction: SemanticAffineLifecycleDirection,
        endpoint: SemanticAffineLifecycleEndpoint,
        options: AnimationOptions,
    },
    Composition {
        kind: SemanticAnimationCompositionKind,
        children: Vec<SemanticCompositionRequest>,
        options: AnimationOptions,
    },
}

impl PreparedAnimationLifecycle {
    const fn root(&self) -> SemanticNodeId {
        match self {
            Self::Introduce(root) | Self::FadeOut(root) | Self::AffineRemove { root, .. } => *root,
            Self::Composition { root, .. } => *root,
        }
    }

    const fn removal(&self) -> Option<(SemanticNodeId, SemanticNodeId)> {
        match self {
            Self::AffineRemove { root, target, .. } => Some((*root, *target)),
            Self::Introduce(_) | Self::FadeOut(_) | Self::Composition { .. } => None,
        }
    }

    const fn admits(&self) -> bool {
        match self {
            Self::Introduce(_) => true,
            Self::AffineRemove { admitted, .. } => *admitted,
            Self::FadeOut(_) => false,
            Self::Composition { admits, .. } => *admits,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExecutionEvaluationMode {
    Evaluate,
    Seek,
    Advance,
}

impl ExecutionEvaluationMode {
    const fn requires_seek(self, current: f64, requested: f64) -> bool {
        matches!(self, Self::Seek) || requested < current
    }
}

/// Candidate-sized viewport result in canonical frame painter order.
///
/// Execution slots remain the index identity; frame indices are a transient
/// projection for the renderer's currently published dense compatibility view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionViewportQuery {
    object_indices: Vec<usize>,
    spatial_stats: SpatialQueryStats,
}

impl ExecutionViewportQuery {
    pub fn object_indices(&self) -> &[usize] {
        &self.object_indices
    }

    pub const fn spatial_stats(&self) -> SpatialQueryStats {
        self.spatial_stats
    }
}

/// Error produced when semantic/native reactive input cannot be applied to this execution session.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionInputError {
    RequiredCallbackPending,
    RequiredCallbacksConfigured,
    UnknownSemanticSignal(SemanticNodeId),
    NativeInput(NativeInputRuntimeError),
    NativeEventOutOfOrder { previous: u64, next: u64 },
    Reactive(ReactiveError),
    Evaluation(EvaluationError),
    TimelineOwnedSignal { signal: SemanticNodeId },
    NativeOwnedSignal { signal: SemanticNodeId },
}

impl std::fmt::Display for ExecutionSessionInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequiredCallbackPending => {
                formatter.write_str("a required callback publication is pending")
            }
            Self::RequiredCallbacksConfigured => formatter.write_str(
                "direct native/reactive input is unsupported while required callbacks are configured",
            ),
            Self::UnknownSemanticSignal(signal) => write!(
                formatter,
                "semantic signal {}:{} is not present in this execution session",
                signal.slot(),
                signal.generation()
            ),
            Self::NativeInput(error) => error.fmt(formatter),
            Self::NativeEventOutOfOrder { previous, next } => write!(
                formatter,
                "native input event sequence must increase: previous {previous}, next {next}"
            ),
            Self::Reactive(error) => error.fmt(formatter),
            Self::Evaluation(error) => error.fmt(formatter),
            Self::TimelineOwnedSignal { signal } => write!(
                formatter,
                "semantic signal {}:{} is timeline-owned and cannot be set directly",
                signal.slot(),
                signal.generation()
            ),
            Self::NativeOwnedSignal { signal } => write!(
                formatter,
                "semantic signal {}:{} is native-owned and cannot be set directly",
                signal.slot(),
                signal.generation()
            ),
        }
    }
}

impl std::error::Error for ExecutionSessionInputError {}

impl From<NativeInputRuntimeError> for ExecutionSessionInputError {
    fn from(value: NativeInputRuntimeError) -> Self {
        Self::NativeInput(value)
    }
}

impl From<ReactiveError> for ExecutionSessionInputError {
    fn from(value: ReactiveError) -> Self {
        Self::Reactive(value)
    }
}

impl From<EvaluationError> for ExecutionSessionInputError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

/// Error produced when the canonical semantic camera cannot be derived from the
/// current effective runtime frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionSessionCameraError {
    object: ObjectId,
}

impl ExecutionSessionCameraError {
    pub const fn object(self) -> ObjectId {
        self.object
    }
}

impl std::fmt::Display for ExecutionSessionCameraError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "execution camera object {} is missing or not a valid 2D frame",
            self.object.get()
        )
    }
}

impl std::error::Error for ExecutionSessionCameraError {}

/// Unsupported lifecycle shape for the bounded canonical leaf-fade operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionSessionFadeError {
    RootIsNotInExecutionDomain,
    TargetIsNotDetached,
    TargetIsNotDirectRootMember,
    TargetIsAliased,
    ReactiveBindingsUnsupported,
    RequiredCallbacksUnsupported,
}

impl std::fmt::Display for ExecutionSessionFadeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootIsNotInExecutionDomain => {
                formatter.write_str("fade root does not belong to this execution session")
            }
            Self::TargetIsNotDetached => formatter.write_str("FadeIn target must be detached"),
            Self::TargetIsNotDirectRootMember => {
                formatter.write_str("FadeOut target must be a direct member of this scene root")
            }
            Self::TargetIsAliased => {
                formatter.write_str("single-leaf fade does not support aliased family membership")
            }
            Self::ReactiveBindingsUnsupported => formatter
                .write_str("single-leaf fade does not yet support reactive object bindings"),
            Self::RequiredCallbacksUnsupported => {
                formatter.write_str("single-leaf fade does not yet support required host callbacks")
            }
        }
    }
}

impl std::error::Error for ExecutionSessionFadeError {}

/// Unsupported lifecycle shape for the bounded canonical Create operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionSessionCreateError {
    RootIsNotInExecutionDomain,
    EmptyParallel,
    DuplicateTarget,
    TargetIsNotDetached,
    ReactiveBindingsUnsupported,
    RequiredCallbacksUnsupported,
    UnsupportedUncreateRateFunction(RateFunction),
}

impl std::fmt::Display for ExecutionSessionCreateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootIsNotInExecutionDomain => {
                formatter.write_str("Create root does not belong to this execution session")
            }
            Self::EmptyParallel => {
                formatter.write_str("parallel Create requires at least one target")
            }
            Self::DuplicateTarget => {
                formatter.write_str("parallel Create targets must be distinct")
            }
            Self::TargetIsNotDetached => formatter.write_str("Create target must be detached"),
            Self::ReactiveBindingsUnsupported => {
                formatter.write_str("Create does not yet support reactive object bindings")
            }
            Self::RequiredCallbacksUnsupported => {
                formatter.write_str("Create does not yet support required host callbacks")
            }
            Self::UnsupportedUncreateRateFunction(rate) => write!(
                formatter,
                "Uncreate currently supports only linear and smooth rate functions, got {}",
                rate.semantic_id()
            ),
        }
    }
}

impl std::error::Error for ExecutionSessionCreateError {}

/// Error produced while activating one authoritative semantic animation declaration.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionAnimationError {
    RequiredCallbackPending,
    SegmentCompletionPending,
    ForeignSemanticStore,
    StaleSceneRevision {
        expected: noon_core::SceneRevision,
        actual: noon_core::SceneRevision,
    },
    Schedule(SemanticAnimationScheduleError),
    Segment(ExecutionSegmentError),
    Payload(SemanticAffineAnimationTrackError),
    PreparedAnimation(PreparedSemanticAnimationLoweringError),
    PreparedScalarTimeline(PreparedScalarSignalTimelineError),
    ScalarTimeline(SignalTimelineAppendError),
    ScalarQuery(SemanticScalarSignalQueryError),
    ReactiveEnrollment(ReactiveError),
    ScalarEffectiveValue {
        signal: SemanticNodeId,
        authored: f32,
        effective: Option<ReactiveValue>,
    },
    TimelineOwnedSignal {
        signal: SemanticNodeId,
    },
    TargetState {
        target: SemanticNodeId,
        error: SemanticSceneOperationError,
    },
    FadeTarget {
        target: SemanticNodeId,
        error: ExecutionSessionFadeError,
    },
    CreateTarget {
        target: SemanticNodeId,
        error: ExecutionSessionCreateError,
    },
    InvalidComposition(String),
    PreparedTrack(TimelineError),
    Publication(CompilePatchError),
    AuthoredPublication(ExecutionSessionPublicationError),
    TrackIdExhausted,
    SegmentSequenceExhausted,
}

impl std::fmt::Display for ExecutionSessionAnimationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequiredCallbackPending => {
                formatter.write_str("a required callback publication is pending")
            }
            Self::SegmentCompletionPending => formatter.write_str(
                "the previous animation segment still requires completion reconciliation",
            ),
            Self::ForeignSemanticStore => {
                formatter.write_str("semantic store does not own this execution session")
            }
            Self::StaleSceneRevision { expected, actual } => write!(
                formatter,
                "semantic scene revision {} does not match execution scene revision {}",
                actual.get(),
                expected.get()
            ),
            Self::Schedule(error) => {
                write!(formatter, "semantic animation scheduling failed: {error}")
            }
            Self::Segment(error) => write!(
                formatter,
                "semantic animation continuation segment failed: {error}"
            ),
            Self::Payload(error) => write!(
                formatter,
                "semantic animation payload lowering failed: {error}"
            ),
            Self::PreparedAnimation(error) => error.fmt(formatter),
            Self::PreparedScalarTimeline(error) => error.fmt(formatter),
            Self::ScalarTimeline(error) => error.fmt(formatter),
            Self::ScalarQuery(error) => error.fmt(formatter),
            Self::ReactiveEnrollment(error) => error.fmt(formatter),
            Self::ScalarEffectiveValue {
                signal,
                authored,
                effective,
            } => write!(
                formatter,
                "semantic scalar signal {}:{} authored value {authored} does not match current effective value {effective:?}",
                signal.slot(),
                signal.generation()
            ),
            Self::TimelineOwnedSignal { signal } => write!(
                formatter,
                "semantic scalar signal {}:{} still has an active timeline owner",
                signal.slot(),
                signal.generation()
            ),
            Self::TargetState { target, error } => write!(
                formatter,
                "animation target-state node {}:{} is invalid: {error}",
                target.slot(),
                target.generation()
            ),
            Self::FadeTarget { target, error } => write!(
                formatter,
                "fade target {}:{} is invalid: {error}",
                target.slot(),
                target.generation()
            ),
            Self::CreateTarget { target, error } => write!(
                formatter,
                "Create target {}:{} is invalid: {error}",
                target.slot(),
                target.generation()
            ),
            Self::InvalidComposition(error) => formatter.write_str(error),
            Self::PreparedTrack(error) => {
                write!(formatter, "prepared animation track failed: {error}")
            }
            Self::Publication(error) => {
                write!(formatter, "semantic animation publication failed: {error}")
            }
            Self::AuthoredPublication(error) => {
                write!(formatter, "semantic animation declaration failed: {error}")
            }
            Self::TrackIdExhausted => {
                formatter.write_str("execution animation track ID space exhausted")
            }
            Self::SegmentSequenceExhausted => {
                formatter.write_str("execution segment sequence space exhausted")
            }
        }
    }
}

impl std::error::Error for ExecutionSessionAnimationError {}

impl From<SemanticAnimationScheduleError> for ExecutionSessionAnimationError {
    fn from(value: SemanticAnimationScheduleError) -> Self {
        Self::Schedule(value)
    }
}

impl From<ExecutionSegmentError> for ExecutionSessionAnimationError {
    fn from(value: ExecutionSegmentError) -> Self {
        Self::Segment(value)
    }
}

impl From<SemanticAffineAnimationTrackError> for ExecutionSessionAnimationError {
    fn from(value: SemanticAffineAnimationTrackError) -> Self {
        Self::Payload(value)
    }
}

impl From<PreparedSemanticAnimationLoweringError> for ExecutionSessionAnimationError {
    fn from(value: PreparedSemanticAnimationLoweringError) -> Self {
        Self::PreparedAnimation(value)
    }
}

impl From<PreparedScalarSignalTimelineError> for ExecutionSessionAnimationError {
    fn from(value: PreparedScalarSignalTimelineError) -> Self {
        Self::PreparedScalarTimeline(value)
    }
}

impl From<SignalTimelineAppendError> for ExecutionSessionAnimationError {
    fn from(value: SignalTimelineAppendError) -> Self {
        Self::ScalarTimeline(value)
    }
}

impl From<SemanticScalarSignalQueryError> for ExecutionSessionAnimationError {
    fn from(value: SemanticScalarSignalQueryError) -> Self {
        Self::ScalarQuery(value)
    }
}

impl From<ReactiveError> for ExecutionSessionAnimationError {
    fn from(value: ReactiveError) -> Self {
        Self::ReactiveEnrollment(value)
    }
}

impl From<CompilePatchError> for ExecutionSessionAnimationError {
    fn from(value: CompilePatchError) -> Self {
        Self::Publication(value)
    }
}

/// Thin typed orchestration from authoritative semantic state into Noon's existing runtime.
///
/// The session does not own or mirror the [`SemanticStore`]. It retains only the
/// compiler-owned semantic/execution identity mappings needed by hosts plus the
/// existing [`SceneInstance`] runtime. Renderer and platform lifecycle remain outside
/// this type. The canonical camera is retained only as the execution identity of its
/// ordinary semantic frame object; its value is always derived from the effective
/// runtime frame.
///
/// Runtime mutation is intentionally not exposed as a mutable escape hatch here:
/// authored/live structural mutation remains owned by semantic transactions and
/// incremental lowering rather than the migration-era runtime patch surface.
#[derive(Debug)]
pub struct ExecutionSession {
    store_identity: noon_core::SemanticStoreIdentity,
    execution_index: SemanticExecutionIndex,
    reachability: SemanticExecutionReachability,
    painter_order: SemanticPainterOrderIndex,
    slots: noon_runtime::ExecutionSlotTable,
    spatial_index: ExecutionSpatialIndex,
    last_spatial_update: SpatialIndexUpdateStats,
    reactive_projection: SemanticReactiveProjection,
    signal_timeline: SignalTimelineSchedule,
    runtime: SceneInstance,
    camera_object: Option<ObjectId>,
    next_activation_track_id: Option<u64>,
    last_native_event_sequence: Option<u64>,
    last_structural_publication: StructuralPublicationStats,
    callback_schedule: CallbackSchedule,
    next_callback_sequence: Option<u64>,
    pending_callback: Option<PendingCallbackPhase>,
    callback_termination: Option<CallbackTermination>,
    next_segment_sequence: Option<u64>,
    pending_segment_completion: Option<PendingSegmentCompletion>,
    completed_segment_sequence: Option<ExecutionSegmentSequence>,
    last_callback_receipt: Option<CallbackPublicationReceipt>,
}

impl Clone for ExecutionSession {
    fn clone(&self) -> Self {
        let runtime = self.runtime.clone();
        let callback_termination = self.callback_termination.or_else(|| {
            self.pending_callback
                .as_ref()
                .map(|pending| pending.interrupted_clone(runtime.runtime_identity()))
        });
        Self {
            store_identity: self.store_identity.clone(),
            execution_index: self.execution_index.clone(),
            reachability: self.reachability.clone(),
            painter_order: self.painter_order.clone(),
            slots: self.slots.clone(),
            spatial_index: self.spatial_index.clone(),
            last_spatial_update: self.last_spatial_update,
            reactive_projection: self.reactive_projection.clone(),
            signal_timeline: self.signal_timeline.clone(),
            runtime,
            camera_object: self.camera_object,
            next_activation_track_id: self.next_activation_track_id,
            last_native_event_sequence: self.last_native_event_sequence,
            last_structural_publication: self.last_structural_publication,
            callback_schedule: self.callback_schedule.clone(),
            next_callback_sequence: Some(0),
            pending_callback: None,
            callback_termination,
            next_segment_sequence: self.next_segment_sequence,
            pending_segment_completion: None,
            completed_segment_sequence: self.completed_segment_sequence,
            last_callback_receipt: self.last_callback_receipt.clone(),
        }
    }
}

impl ExecutionSession {
    pub(crate) fn runtime_identity(&self) -> noon_runtime::RuntimeIdentity {
        self.runtime.runtime_identity()
    }

    pub(crate) fn segment_completion_is_pending(
        &self,
        token: Option<ExecutionSegmentToken>,
    ) -> bool {
        token.is_some() && self.pending_segment_token() == token
    }

    pub(crate) fn pending_segment_token(&self) -> Option<ExecutionSegmentToken> {
        self.pending_segment_completion
            .as_ref()
            .map(|pending| pending.token)
    }

    pub(crate) fn segment_was_completed(&self, token: ExecutionSegmentToken) -> bool {
        token.runtime() == self.runtime_identity()
            && self
                .completed_segment_sequence
                .is_some_and(|completed| token.sequence().get() <= completed.get())
    }

    fn ensure_direct_input_ingress_available(&self) -> Result<(), ExecutionSessionInputError> {
        if !self.callback_schedule.is_empty() {
            return Err(ExecutionSessionInputError::RequiredCallbacksConfigured);
        }
        if self.pending_callback.is_some() {
            return Err(ExecutionSessionInputError::RequiredCallbackPending);
        }
        Ok(())
    }

    /// Lower one authoritative semantic snapshot and instantiate the existing runtime.
    pub fn from_semantic_store(
        store: &SemanticStore,
    ) -> Result<Self, SemanticExecutionLoweringError> {
        let mut execution_index = SemanticExecutionIndex::new();
        let reachability = SemanticExecutionReachability::from_store(store)?;
        let painter_order = semantic_painter_order(store, &reachability);
        let lowered = lower_semantic_execution(store, &mut execution_index)?;
        Ok(Self::from_lowered(
            store.identity(),
            execution_index,
            reachability,
            painter_order,
            lowered,
        ))
    }

    /// Instantiate the existing runtime for one semantic scene family.
    ///
    /// Detached families are valid initial scene roots. Other families in the
    /// shared store do not enter this execution domain. Membership remains owned
    /// by the store; this constructor neither mutates it nor establishes a live
    /// structural mutation contract. `root` must originate from `store`.
    pub fn from_semantic_root(
        store: &SemanticStore,
        root: SemanticNodeId,
    ) -> Result<Self, SemanticExecutionLoweringError> {
        let mut execution_index = SemanticExecutionIndex::new();
        let reachability = SemanticExecutionReachability::from_root(store, root)?;
        let painter_order = semantic_painter_order(store, &reachability);
        let lowered = lower_semantic_execution_root(store, root, &mut execution_index)?;
        Ok(Self::from_lowered(
            store.identity(),
            execution_index,
            reachability,
            painter_order,
            lowered,
        ))
    }

    fn from_lowered(
        store_identity: noon_core::SemanticStoreIdentity,
        execution_index: SemanticExecutionIndex,
        reachability: SemanticExecutionReachability,
        painter_order: SemanticPainterOrderIndex,
        lowered: SemanticExecutionLoweringOutput,
    ) -> Self {
        let camera_object = lowered.camera_object();
        let next_activation_track_id = lowered
            .compiled()
            .tracks_iter()
            .map(|track| track.id.get())
            .max()
            .map_or(Some(0), |id| id.checked_add(1));
        let slots = noon_runtime::ExecutionSlotTable::from_compiled(lowered.compiled());
        let mut reactive_projection = lowered.reactive().clone();
        let signal_timeline =
            SignalTimelineSchedule::new(reactive_projection.take_scalar_timeline());
        let callback_schedule = CallbackSchedule::new(lowered.host_callbacks().clone());
        let mut runtime = SceneInstance::from_semantic_execution(lowered);
        let mut spatial_index = ExecutionSpatialIndex::default();
        let live_slots =
            runtime
                .frame()
                .objects
                .iter()
                .enumerate()
                .filter_map(|(index, object)| {
                    if !runtime.object_slot_is_live(index) {
                        return None;
                    }
                    slots.slot_for_object(object.id).map(|slot| (slot, index))
                });
        let last_spatial_update = spatial_index.rebuild(runtime.frame(), live_slots);
        let _ = runtime.take_spatial_changes();
        Self {
            store_identity,
            execution_index,
            reachability,
            painter_order,
            slots,
            spatial_index,
            last_spatial_update,
            reactive_projection,
            signal_timeline,
            runtime,
            camera_object,
            next_activation_track_id,
            last_native_event_sequence: None,
            last_structural_publication: StructuralPublicationStats::default(),
            callback_schedule,
            next_callback_sequence: Some(0),
            pending_callback: None,
            callback_termination: None,
            next_segment_sequence: Some(0),
            pending_segment_completion: None,
            completed_segment_sequence: None,
            last_callback_receipt: None,
        }
    }

    /// Current renderer-facing runtime frame.
    pub fn frame(&self) -> &FrameState {
        self.runtime.frame()
    }

    /// Work performed by the most recent incremental execution-plan patch.
    pub const fn last_patch_stats(&self) -> noon_runtime::RuntimePatchStats {
        self.runtime.last_patch_stats()
    }

    /// Read-only text resources projected with this execution session.
    pub fn text_resources(&self) -> &impl noon_core::TextResourceLookup {
        self.runtime.text_resources()
    }

    /// Read-only font resources projected with this execution session.
    pub fn font_resources(&self) -> &impl noon_core::FontResourceLookup {
        self.runtime.font_resources()
    }

    /// Read-only geometry resources projected with this execution session.
    pub fn geometry_resources(&self) -> &impl noon_core::GeometryResourceLookup {
        self.runtime.geometry_resources()
    }

    /// Stable runtime identity for a current frame row. Structural publication
    /// retires and allocates durable slots without renumbering unrelated identities.
    pub fn execution_slot_for_frame_index(
        &self,
        index: usize,
    ) -> Option<noon_runtime::ExecutionSlotId> {
        if !self.runtime.object_slot_is_live(index) {
            return None;
        }
        let object = self.frame().objects.get(index)?;
        self.slots.slot_for_object(object.id)
    }

    /// Query current visible rows through the session-owned execution-slot index.
    pub fn query_viewport(&mut self, bounds: Rect) -> ExecutionViewportQuery {
        self.sync_spatial_index();
        let query = self.spatial_index.query_rect(bounds);
        let object_indices: Vec<_> = query
            .slots()
            .iter()
            .filter_map(|&slot| {
                let object = self.slots.object_for_slot(slot)?;
                self.runtime.frame_index_for_object(object)
            })
            .collect();
        debug_assert_eq!(
            object_indices.len(),
            query.stats().results,
            "live spatial candidates must resolve through execution identity"
        );
        ExecutionViewportQuery {
            object_indices,
            spatial_stats: query.stats(),
        }
    }

    pub const fn last_spatial_update_stats(&self) -> SpatialIndexUpdateStats {
        self.last_spatial_update
    }

    /// Exact authored/executable/effective publication context of this session.
    ///
    /// Hosts can pin coherent live reads and cross-context work to this typed context
    /// without receiving mutable runtime authority or substituting identity generations
    /// for revision clocks.
    pub const fn publication_context(&self) -> noon_core::PublicationContext {
        self.runtime.publication_context()
    }

    /// Current canonical 2D camera derived from the effective runtime frame object.
    ///
    /// Scenes without an authored camera use the shared Manim-compatible default.
    /// An authored camera never falls back silently: if its effective frame becomes
    /// invalid, the host receives an error just as the transport encoder does.
    pub fn camera(&self) -> Result<Camera2DState, ExecutionSessionCameraError> {
        let Some(camera_object) = self.camera_object else {
            return Ok(Camera2DState::default());
        };
        let object =
            self.runtime
                .effective_object(camera_object)
                .ok_or(ExecutionSessionCameraError {
                    object: camera_object,
                })?;
        object
            .geometry()
            .and_then(|geometry| Camera2DState::from_frame_object(geometry, object.transform))
            .ok_or(ExecutionSessionCameraError {
                object: camera_object,
            })
    }

    fn sync_spatial_index(&mut self) {
        let changes = self.runtime.take_spatial_changes();
        if changes.is_empty() {
            self.last_spatial_update = SpatialIndexUpdateStats::default();
            return;
        }
        if changes.is_all() {
            let live_slots =
                self.runtime
                    .frame()
                    .objects
                    .iter()
                    .enumerate()
                    .filter_map(|(index, object)| {
                        if !self.runtime.object_slot_is_live(index) {
                            return None;
                        }
                        self.slots
                            .slot_for_object(object.id)
                            .map(|slot| (slot, index))
                    });
            self.last_spatial_update = self.spatial_index.rebuild(self.runtime.frame(), live_slots);
            return;
        }
        let mut stats = SpatialIndexUpdateStats::default();
        for &index in changes.object_indices() {
            let Some(object) = self.runtime.frame().objects.get(index) else {
                continue;
            };
            if self.runtime.object_slot_is_live(index) {
                if let Some(slot) = self.slots.slot_for_object(object.id) {
                    stats.merge_from(self.spatial_index.upsert_frame_slot(
                        self.runtime.frame(),
                        slot,
                        index,
                        index as u64,
                    ));
                }
            } else {
                stats.merge_from(self.spatial_index.remove_object(object.id));
            }
        }
        self.last_spatial_update = stats;
    }

    /// Read runtime-owned presentation dirtiness and timeline cadence without
    /// exposing the runtime scheduler or introducing a host-side timing model.
    pub fn wake_state(&self) -> RuntimeWakeState {
        if self.callback_termination.is_some() {
            return self.runtime.wake_state().without_timeline_wake();
        }
        let callback_timeline = if self.pending_callback.is_some() {
            noon_runtime::TimelineWakeState::Continuous
        } else {
            self.callback_schedule
                .wake_timeline(self.runtime.frame().time)
        };
        self.runtime
            .wake_state()
            .with_additional_timeline(callback_timeline)
            .with_additional_timeline(self.signal_timeline.wake_state())
    }

    /// Whether looping playback must revisit authored timeline history after the
    /// current runtime wake state settles.
    ///
    /// This is an O(1) query over the runtime and scalar-signal timeline indices.
    /// It does not scan tracks or create a host-side schedule. Opaque host callback
    /// history is deliberately excluded: callback sessions use non-looping playback
    /// and reject attempts to enable looping rather than replaying external effects.
    pub fn has_replay_timeline_work(&self) -> bool {
        self.runtime.has_timeline_channels() || !self.signal_timeline.is_empty()
    }

    /// Consume renderer-facing invalidation state accumulated by the runtime.
    pub fn take_frame_changes(&mut self) -> FrameChanges {
        self.runtime.take_frame_changes()
    }

    /// Consume one coherent renderer publication from this typed session.
    pub fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        self.runtime.take_renderer_publication()
    }

    /// Evaluate deterministically at an absolute time.
    pub fn evaluate(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        if self.pending_callback.is_some() {
            return Err(EvaluationError::RequiredCallbackPending);
        }
        if !self.callback_schedule.is_empty() {
            return Err(EvaluationError::RequiredCallbackBarrier);
        }
        self.evaluate_signal_timeline(time, ExecutionEvaluationMode::Evaluate)
    }

    /// Seek deterministically to an absolute time.
    pub fn seek(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        if self.pending_callback.is_some() {
            return Err(EvaluationError::RequiredCallbackPending);
        }
        if !self.callback_schedule.is_empty() {
            return Err(EvaluationError::RequiredCallbackBarrier);
        }
        self.evaluate_signal_timeline(time, ExecutionEvaluationMode::Seek)
    }

    /// Advance to an absolute time, falling back to deterministic seek when time moves backward.
    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        if self.pending_callback.is_some() {
            return Err(EvaluationError::RequiredCallbackPending);
        }
        if !self.callback_schedule.is_empty() {
            return Err(EvaluationError::RequiredCallbackBarrier);
        }
        self.evaluate_signal_timeline(time, ExecutionEvaluationMode::Advance)
    }

    /// Activate one semantic animation root at the session's current deterministic time.
    ///
    /// This compatibility surface preserves the existing frame-returning API. Continuation
    /// consumers should use [`Self::activate_animation_segment`] so the logical segment comes
    /// from the same compiler-resolved scheduling/publication pass rather than re-resolving
    /// duration in an authoring facade.
    pub fn activate_animation(
        &mut self,
        store: &SemanticStore,
        root: SemanticNodeId,
        play_options: AnimationOptions,
    ) -> Result<&FrameState, ExecutionSessionAnimationError> {
        self.activate_animation_segment(store, root, play_options)?;
        Ok(self.runtime.frame())
    }

    /// Activate one semantic animation and return its canonical logical continuation segment.
    ///
    /// The authoritative semantic store remains caller-owned. Scheduling reads the current
    /// declaration once; the segment is constructed directly from that resolved projection's
    /// `start_time` / `run_time`; affine payload lowering captures each target's effective
    /// runtime transform at most once; and the session attaches execution-local track identity.
    /// All emitted tracks are preflighted and published as one execution transaction,
    /// so a failed activation cannot expose a partial timeline or continuation boundary.
    pub fn activate_animation_segment(
        &mut self,
        store: &SemanticStore,
        root: SemanticNodeId,
        play_options: AnimationOptions,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        if self.pending_callback.is_some() {
            return Err(ExecutionSessionAnimationError::RequiredCallbackPending);
        }
        if self.pending_segment_completion.is_some() {
            return Err(ExecutionSessionAnimationError::SegmentCompletionPending);
        }
        if store.identity() != self.store_identity {
            return Err(ExecutionSessionAnimationError::ForeignSemanticStore);
        }
        let expected = self.publication_context().scene_revision();
        let actual = store.scene_revision();
        if actual != expected {
            return Err(ExecutionSessionAnimationError::StaleSceneRevision { expected, actual });
        }
        let schedule = lower_semantic_animation_schedule(
            store,
            &self.execution_index,
            root,
            self.runtime.frame().time,
            play_options,
        )?;
        let mut segment =
            ExecutionSegment::from_duration(schedule.start_time(), schedule.run_time())?;
        let tracks = lower_semantic_affine_animation_tracks(store, &schedule, |object| {
            let index = self.runtime.frame_index_for_object(object)?;
            let row = self.runtime.frame().objects.get(index)?;
            Some(EffectiveAnimationProperties {
                transform: row.transform,
                style: row.style,
                appearance: row.appearance,
            })
        })?;
        if tracks.is_empty() {
            return Ok(segment);
        }

        let mut next_track_id = self.next_activation_track_id;
        let mut definitions = Vec::with_capacity(tracks.len());
        let mut completions = Vec::with_capacity(tracks.len());
        for track in tracks.tracks() {
            let raw_id = next_track_id.ok_or(ExecutionSessionAnimationError::TrackIdExhausted)?;
            let track_id = TrackId::new(raw_id);
            let definition = track.with_track_id(track_id)?;
            let end_time = track.timing.start_time + track.timing.duration;
            completions.push(SegmentCompletionEntry {
                semantic_object: track.target,
                completion: track.completion.clone(),
                execution_object: track.execution_object_id,
                property: track.property,
                track: track_id,
                end_time,
            });
            definitions.push(definition);
            next_track_id = raw_id.checked_add(1);
        }

        self.runtime
            .preflight_reconcilable_track_additions(&definitions)
            .map_err(ExecutionSessionAnimationError::Publication)?;
        let raw_sequence = self
            .next_segment_sequence
            .ok_or(ExecutionSessionAnimationError::SegmentSequenceExhausted)?;
        let next_segment_sequence = raw_sequence.checked_add(1);
        let transaction = ExecutionMutationTransaction::from_mutations(
            definitions.into_iter().map(ExecutionPatch::AddTrack),
        );
        self.runtime.apply_execution_transaction(&transaction)?;
        let token = ExecutionSegmentToken::new(
            self.runtime.runtime_identity(),
            ExecutionSegmentSequence::new(raw_sequence),
        );
        self.next_segment_sequence = next_segment_sequence;
        segment = segment.with_completion_token(token);
        self.pending_segment_completion = Some(PendingSegmentCompletion {
            token,
            activation_scene_revision: store.scene_revision(),
            kind: PendingSegmentCompletionKind::ObjectTracks {
                lifecycle_root: None,
                lifecycle_removals: Vec::new(),
                entries: completions,
            },
        });
        self.next_activation_track_id = next_track_id;
        Ok(segment)
    }

    /// Declare and activate one supported transform/style animation against this session.
    ///
    /// Compiler projection, execution track allocation, semantic transaction preparation, and
    /// runtime publication all complete before semantic identity is committed. The final
    /// semantic/runtime commit is one synchronous publication, so every returned error leaves
    /// the store revision, runtime frame, activation counters, and segment state unchanged.
    pub fn declare_and_activate_transform_to(
        &mut self,
        store: &mut SemanticStore,
        source: SemanticNodeId,
        target_state: SemanticNodeId,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        let mut declaration = SemanticMutationTransaction::new();
        let target_state =
            self.stage_animation_target_state(store, &mut declaration, target_state)?;
        let root = declaration.create_transform_animation(source, target_state, options);
        self.declare_and_activate_prepared_animation(
            store,
            declaration,
            root,
            AnimationOptions::new(),
            None,
        )
    }

    /// Atomically create and enroll one input-only scalar in this live root.
    pub fn create_scoped_value_tracker(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
        initial: f64,
    ) -> Result<SemanticNodeId, ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        if !self.reachability.is_execution_root(root) {
            return Err(ExecutionSessionAnimationError::AuthoredPublication(
                ExecutionSessionPublicationError::UnknownObject(root),
            ));
        }
        let runtime_value = lower_live_scalar_value(initial)?;
        let creation = SemanticNodeCreation::input_signal(initial).map_err(|error| {
            ExecutionSessionAnimationError::AuthoredPublication(
                ExecutionSessionPublicationError::Semantic(
                    noon_core::SemanticMutationTransactionError::Signal { index: 0, error },
                ),
            )
        })?;
        let mut transaction = SemanticMutationTransaction::new();
        let pending = transaction.create_node(creation);
        transaction.scope_signal(root, pending);
        let prepared = transaction.prepare(store).map_err(|error| {
            ExecutionSessionAnimationError::AuthoredPublication(
                ExecutionSessionPublicationError::Semantic(error),
            )
        })?;
        let runtime_enrollment = self
            .runtime
            .prepare_reactive_signal_enrollment(None, ReactiveValue::Scalar(runtime_value))?;
        let runtime_publication = self
            .runtime
            .prepare_authored_plan_change(
                self.publication_context(),
                prepared.proposed_scene_revision(),
            )
            .map_err(|error| {
                ExecutionSessionAnimationError::AuthoredPublication(
                    ExecutionSessionPublicationError::Runtime(error),
                )
            })?;
        let (result, store) = prepared.commit_with_store();
        let signal = result
            .resolve(pending)
            .expect("committed signal creation resolves its transaction-local token");
        let execution = self
            .reactive_projection
            .install_input_signal(signal, ReactiveValue::Scalar(runtime_value))
            .expect("fresh semantic signal maps to one fresh derived execution identity");
        self.runtime
            .commit_reactive_signal_enrollment(runtime_enrollment, execution);
        self.runtime
            .apply_prepared_authored_plan_change(runtime_publication)
            .expect("signal enrollment publication was preflighted under exclusive ownership");
        debug_assert_eq!(
            store.scene_revision(),
            self.publication_context().scene_revision()
        );
        Ok(signal)
    }

    /// Atomically associate and, when necessary, sparsely enroll one existing
    /// detached scalar signal in this execution root.
    pub fn associate_value_tracker(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
        signal: SemanticNodeId,
    ) -> Result<(), ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        if !self.reachability.is_execution_root(root) {
            return Err(ExecutionSessionAnimationError::AuthoredPublication(
                ExecutionSessionPublicationError::UnknownObject(root),
            ));
        }
        let initial = match store
            .semantic_signal_state(signal)
            .map_err(|error| {
                ExecutionSessionAnimationError::AuthoredPublication(
                    ExecutionSessionPublicationError::Semantic(
                        noon_core::SemanticMutationTransactionError::Signal { index: 0, error },
                    ),
                )
            })?
            .source()
        {
            noon_core::SemanticSignalSource::Input(noon_core::SemanticSignalValue::Scalar(
                value,
            )) => lower_live_scalar_value(*value)?,
            _ => {
                return Err(ReactiveError::NotInputSignal(
                    noon_compile::semantic_execution_signal_id(signal),
                )
                .into())
            }
        };
        let mut transaction = SemanticMutationTransaction::new();
        transaction.scope_signal(root, signal);
        let prepared = transaction.prepare(store).map_err(|error| {
            ExecutionSessionAnimationError::AuthoredPublication(
                ExecutionSessionPublicationError::Semantic(error),
            )
        })?;
        if prepared.candidate_mutations().next().is_none() {
            return Ok(());
        }
        let execution = noon_compile::semantic_execution_signal_id(signal);
        let needs_enrollment = self
            .reactive_projection
            .execution_signal_id(signal)
            .is_none();
        let runtime_enrollment = needs_enrollment
            .then(|| {
                self.runtime.prepare_reactive_signal_enrollment(
                    Some(execution),
                    ReactiveValue::Scalar(initial),
                )
            })
            .transpose()?;
        let runtime_publication = self
            .runtime
            .prepare_authored_plan_change(
                self.publication_context(),
                prepared.proposed_scene_revision(),
            )
            .map_err(|error| {
                ExecutionSessionAnimationError::AuthoredPublication(
                    ExecutionSessionPublicationError::Runtime(error),
                )
            })?;
        let (_result, store) = prepared.commit_with_store();
        if let Some(runtime_enrollment) = runtime_enrollment {
            self.reactive_projection
                .install_input_signal(signal, ReactiveValue::Scalar(initial))
                .expect("preflighted detached scalar enrollment remains valid");
            self.runtime
                .commit_reactive_signal_enrollment(runtime_enrollment, execution);
        }
        self.runtime
            .apply_prepared_authored_plan_change(runtime_publication)
            .expect("signal scope publication was preflighted under exclusive ownership");
        debug_assert_eq!(
            store.scene_revision(),
            self.publication_context().scene_revision()
        );
        Ok(())
    }

    /// Atomically append and activate one scalar ValueTracker timeline interval.
    ///
    /// The semantic track, derived schedule entry, revision publication, and
    /// completion token become visible together. The first live slice admits only
    /// entries beginning at the current frame and rejects event interleaving.
    pub fn declare_and_activate_value_tracker(
        &mut self,
        store: &mut SemanticStore,
        signal: SemanticNodeId,
        target: f64,
        duration: f64,
        rate_func: RateFunction,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        let start_time = self.runtime.frame().time;
        let from = store.semantic_input_scalar_value_at(signal, start_time)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_scalar_signal_track(
            signal,
            from,
            target,
            TrackTiming::new(start_time, duration, rate_func),
        );
        let prepared = transaction.prepare(store).map_err(|error| {
            ExecutionSessionAnimationError::AuthoredPublication(
                ExecutionSessionPublicationError::Semantic(error),
            )
        })?;
        let entry =
            lower_prepared_scalar_signal_timeline_entry(&prepared, &self.reactive_projection)?;
        let noon_compile::CompiledScalarSignalTimelineEntry::Track(track) = entry else {
            unreachable!("tracker activation prepared one scalar track")
        };
        let effective = self.effective_signal_value(signal).cloned();
        if effective != Some(ReactiveValue::Scalar(track.from())) {
            return Err(ExecutionSessionAnimationError::ScalarEffectiveValue {
                signal,
                authored: track.from(),
                effective,
            });
        }
        let schedule = self.signal_timeline.prepare_append(entry, start_time)?;
        let mut segment = ExecutionSegment::from_duration(start_time, duration)?;
        let runtime_publication = self
            .runtime
            .prepare_authored_plan_change(
                self.publication_context(),
                prepared.proposed_scene_revision(),
            )
            .map_err(|error| {
                ExecutionSessionAnimationError::AuthoredPublication(
                    ExecutionSessionPublicationError::Runtime(error),
                )
            })?;
        let raw_sequence = self
            .next_segment_sequence
            .ok_or(ExecutionSessionAnimationError::SegmentSequenceExhausted)?;
        let next_segment_sequence = raw_sequence.checked_add(1);
        let token = ExecutionSegmentToken::new(
            self.runtime.runtime_identity(),
            ExecutionSegmentSequence::new(raw_sequence),
        );

        let (_result, store) = prepared.commit_with_store();
        self.signal_timeline.commit_append(schedule);
        self.runtime
            .apply_prepared_authored_plan_change(runtime_publication)
            .expect("scalar authored plan publication was preflighted under exclusive ownership");
        self.next_segment_sequence = next_segment_sequence;
        segment = segment.with_completion_token(token);
        self.pending_segment_completion = Some(PendingSegmentCompletion {
            token,
            activation_scene_revision: store.scene_revision(),
            kind: PendingSegmentCompletionKind::ScalarTrack {
                signal,
                authored_endpoint: target,
                runtime_endpoint: track.to(),
                end_time: segment.end_time(),
            },
        });
        Ok(segment)
    }

    /// Persist one scalar input value at the current live authored time.
    ///
    /// This appends a semantic Hold, updates only the signal's compiled event
    /// group, evaluates its dirty reactive closure, and publishes all three
    /// layers under one revision transition. Active segment ownership and native
    /// source ownership are rejected during preflight.
    pub fn set_scalar_signal_value(
        &mut self,
        store: &mut SemanticStore,
        signal: SemanticNodeId,
        value: f64,
    ) -> Result<&FrameState, ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        if self.signal_timeline.owns(signal) {
            return Err(ExecutionSessionAnimationError::TimelineOwnedSignal { signal });
        }
        let time = self.runtime.frame().time;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_scalar_signal_at(signal, value, time);
        let prepared = transaction.prepare(store).map_err(|error| {
            ExecutionSessionAnimationError::AuthoredPublication(
                ExecutionSessionPublicationError::Semantic(error),
            )
        })?;
        let entry =
            lower_prepared_scalar_signal_timeline_entry(&prepared, &self.reactive_projection)?;
        let noon_compile::CompiledScalarSignalTimelineEntry::Hold(hold) = entry else {
            unreachable!("persistent scalar publication prepared one Hold entry")
        };
        let schedule = self.signal_timeline.prepare_append(entry, time)?;
        let runtime_publication = self
            .runtime
            .prepare_authored_reactive_plan_change(
                self.publication_context(),
                prepared.proposed_scene_revision(),
                &[(hold.execution_signal(), ReactiveValue::Scalar(hold.value()))],
            )
            .map_err(|error| {
                ExecutionSessionAnimationError::AuthoredPublication(
                    ExecutionSessionPublicationError::Runtime(error),
                )
            })?;

        let (_result, store) = prepared.commit_with_store();
        self.signal_timeline.commit_append(schedule);
        self.runtime
            .apply_prepared_authored_reactive_plan_change(runtime_publication)
            .expect("persistent scalar publication was preflighted under exclusive ownership");
        debug_assert_eq!(
            store.scene_revision(),
            self.publication_context().scene_revision()
        );
        Ok(self.runtime.frame())
    }

    /// Atomically declare and activate one recursive semantic composition.
    ///
    /// The declaration tree is inert input: every child declaration, detached admission,
    /// target snapshot, schedule projection, and runtime publication is prepared before its
    /// single semantic commit. Nested waits intentionally contribute no tracks; their authored
    /// interval remains the returned shared execution segment.
    pub(crate) fn declare_and_activate_composition(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
        request: &SemanticCompositionRequest,
        play_options: AnimationOptions,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        if !self.reachability.is_execution_root(root) {
            return Err(ExecutionSessionAnimationError::CreateTarget {
                target: root,
                error: ExecutionSessionCreateError::RootIsNotInExecutionDomain,
            });
        }
        let mut declaration = SemanticMutationTransaction::new();
        let mut admitted = HashSet::new();
        let mut removals = Vec::new();
        let animation = self.stage_composition_request(
            store,
            root,
            request,
            &mut declaration,
            &mut admitted,
            &mut removals,
        )?;
        self.declare_and_activate_prepared_animation(
            store,
            declaration,
            animation,
            play_options,
            Some(PreparedAnimationLifecycle::Composition {
                root,
                admits: !admitted.is_empty(),
                removals,
            }),
        )
    }

    /// Keep the direct transform convenience as a composition declaration; it owns no separate
    /// lowering or activation path.
    pub fn declare_and_activate_transform_composition(
        &mut self,
        store: &mut SemanticStore,
        kind: SemanticAnimationCompositionKind,
        children: &[(SemanticNodeId, SemanticNodeId, AnimationOptions)],
        composition_options: AnimationOptions,
        play_options: AnimationOptions,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        let children = children
            .iter()
            .map(
                |(source, target_state, options)| SemanticCompositionRequest::TransformTo {
                    source: *source,
                    target_state: *target_state,
                    interpolation: noon_core::SemanticTransformInterpolation::Affine,
                    options: *options,
                },
            )
            .collect();
        let request = SemanticCompositionRequest::Composition {
            kind,
            children,
            options: composition_options,
        };
        // Transform-only callers operate on currently mounted sources, so no admission root is
        // available through this low-level convenience. The public live facade uses the recursive
        // root-aware entrypoint below.
        self.declare_and_activate_composition_without_admission(store, &request, play_options)
    }

    fn declare_and_activate_composition_without_admission(
        &mut self,
        store: &mut SemanticStore,
        request: &SemanticCompositionRequest,
        play_options: AnimationOptions,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        let mut declaration = SemanticMutationTransaction::new();
        let animation =
            self.stage_composition_request_without_admission(store, request, &mut declaration)?;
        self.declare_and_activate_prepared_animation(
            store,
            declaration,
            animation,
            play_options,
            None,
        )
    }

    fn stage_composition_request(
        &self,
        store: &SemanticStore,
        root: SemanticNodeId,
        request: &SemanticCompositionRequest,
        declaration: &mut SemanticMutationTransaction,
        admitted: &mut HashSet<SemanticNodeId>,
        removals: &mut Vec<(SemanticNodeId, SemanticNodeId)>,
    ) -> Result<noon_core::SemanticLocalNodeToken, ExecutionSessionAnimationError> {
        let admit = |target: SemanticNodeId,
                     declaration: &mut SemanticMutationTransaction,
                     admitted: &mut HashSet<SemanticNodeId>|
         -> Result<(), ExecutionSessionAnimationError> {
            let state = store
                .semantic_object_state_checked(target)
                .map_err(|error| ExecutionSessionAnimationError::TargetState { target, error })?;
            if !state.signal_bindings().is_empty() {
                return Err(ExecutionSessionAnimationError::CreateTarget {
                    target,
                    error: ExecutionSessionCreateError::ReactiveBindingsUnsupported,
                });
            }
            if self.execution_index.execution_object_id(target).is_some() {
                return Ok(());
            }
            let node = store
                .node(target)
                .expect("validated semantic object has a live node");
            if node.is_scene_owned()
                || node
                    .parents()
                    .iter()
                    .any(|parent| self.reachability.is_reachable(*parent))
            {
                return Err(ExecutionSessionAnimationError::CreateTarget {
                    target,
                    error: ExecutionSessionCreateError::TargetIsNotDetached,
                });
            }
            if !admitted.insert(target) {
                return Err(ExecutionSessionAnimationError::CreateTarget {
                    target,
                    error: ExecutionSessionCreateError::DuplicateTarget,
                });
            }
            declaration.add_member(root, target);
            Ok(())
        };
        match request {
            SemanticCompositionRequest::TransformTo {
                source,
                target_state,
                interpolation,
                options,
            } => {
                admit(*source, declaration, admitted)?;
                let target_state =
                    self.stage_animation_target_state(store, declaration, *target_state)?;
                Ok(declaration.create_transform_animation_with_interpolation(
                    *source,
                    target_state,
                    *interpolation,
                    *options,
                ))
            }
            SemanticCompositionRequest::Rotate {
                target,
                angle,
                options,
            } => {
                admit(*target, declaration, admitted)?;
                Ok(declaration.create_rotate_animation(*target, *angle, *options))
            }
            SemanticCompositionRequest::Wait { duration } => {
                if !duration.is_finite() || *duration < 0.0 {
                    return Err(ExecutionSessionAnimationError::Segment(
                        ExecutionSegmentError::InvalidDuration(*duration),
                    ));
                }
                Ok(declaration.create_wait_animation(*duration))
            }
            SemanticCompositionRequest::Add { target, options } => {
                admit(*target, declaration, admitted)?;
                Ok(declaration.create_add_animation(*target, *options))
            }
            SemanticCompositionRequest::Fade {
                target,
                direction,
                options,
            } => {
                self.require_fade_target(store, root, *target, *direction)?;
                if *direction == SemanticFadeDirection::In {
                    admit(*target, declaration, admitted)?;
                }
                Ok(declaration.create_fade_animation(*target, *direction, *options))
            }
            SemanticCompositionRequest::Create { target, options } => {
                self.require_create_target(store, *target)?;
                admit(*target, declaration, admitted)?;
                Ok(declaration.create_create_animation(*target, *options))
            }
            SemanticCompositionRequest::AffineLifecycle {
                target,
                direction,
                endpoint,
                options,
            } => {
                let needs_admission =
                    self.require_affine_lifecycle_target(store, root, *target, *direction)?;
                if needs_admission {
                    admit(*target, declaration, admitted)?;
                }
                if *direction == SemanticAffineLifecycleDirection::RemoveTo {
                    removals.push((root, *target));
                }
                Ok(declaration
                    .create_affine_lifecycle_animation(*target, *direction, *endpoint, *options))
            }
            SemanticCompositionRequest::Composition {
                kind,
                children,
                options,
            } => {
                if children.is_empty() {
                    return Err(ExecutionSessionAnimationError::CreateTarget {
                        target: root,
                        error: ExecutionSessionCreateError::EmptyParallel,
                    });
                }
                let children = children
                    .iter()
                    .map(|child| {
                        self.stage_composition_request(
                            store,
                            root,
                            child,
                            declaration,
                            admitted,
                            removals,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(declaration.create_animation_composition(*kind, children, *options))
            }
        }
    }

    fn stage_composition_request_without_admission(
        &self,
        store: &SemanticStore,
        request: &SemanticCompositionRequest,
        declaration: &mut SemanticMutationTransaction,
    ) -> Result<noon_core::SemanticLocalNodeToken, ExecutionSessionAnimationError> {
        match request {
            SemanticCompositionRequest::TransformTo { source, target_state, interpolation, options } => {
                if self.execution_index.execution_object_id(*source).is_none() {
                    return Err(ExecutionSessionAnimationError::CreateTarget { target: *source, error: ExecutionSessionCreateError::TargetIsNotDetached });
                }
                let target_state = self.stage_animation_target_state(store, declaration, *target_state)?;
                Ok(declaration.create_transform_animation_with_interpolation(*source, target_state, *interpolation, *options))
            }
            SemanticCompositionRequest::Rotate { target, angle, options } => {
                if self.execution_index.execution_object_id(*target).is_none() {
                    return Err(ExecutionSessionAnimationError::CreateTarget { target: *target, error: ExecutionSessionCreateError::TargetIsNotDetached });
                }
                Ok(declaration.create_rotate_animation(*target, *angle, *options))
            }
            SemanticCompositionRequest::Wait { duration } => Ok(declaration.create_wait_animation(*duration)),
            SemanticCompositionRequest::Composition { kind, children, options } => {
                let children = children.iter().map(|child| self.stage_composition_request_without_admission(store, child, declaration)).collect::<Result<Vec<_>, _>>()?;
                Ok(declaration.create_animation_composition(*kind, children, *options))
            }
            _ => Err(ExecutionSessionAnimationError::InvalidComposition(
                "this direct convenience accepts only transform, rotate, wait, and nested composition leaves".into(),
            )),
        }
    }

    /// Atomically declare and activate one canonical single-leaf fade.
    ///
    /// FadeIn admits one detached existing object and its appearance-zero track in one
    /// publication. FadeOut keeps membership until exact segment completion removes it with the
    /// appearance driver. No lifecycle state is retained outside the semantic store/session.
    pub fn declare_and_activate_fade(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
        target: SemanticNodeId,
        direction: SemanticFadeDirection,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        self.require_fade_target(store, root, target, direction)?;
        let mut declaration = SemanticMutationTransaction::new();
        if direction == SemanticFadeDirection::In {
            declaration.add_member(root, target);
        }
        let animation = declaration.create_fade_animation(target, direction, options);
        self.declare_and_activate_prepared_animation(
            store,
            declaration,
            animation,
            AnimationOptions::new(),
            Some(match direction {
                SemanticFadeDirection::In => PreparedAnimationLifecycle::Introduce(root),
                SemanticFadeDirection::Out => PreparedAnimationLifecycle::FadeOut(root),
            }),
        )
    }

    /// Atomically activate one single-leaf affine appearance lifecycle.
    ///
    /// Introduction admits a detached object in the declaration transaction. Removal keeps the
    /// live object present through the endpoint and removes membership during segment completion.
    pub fn declare_and_activate_affine_lifecycle(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
        target: SemanticNodeId,
        direction: SemanticAffineLifecycleDirection,
        endpoint: SemanticAffineLifecycleEndpoint,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        let admitted = self.require_affine_lifecycle_target(store, root, target, direction)?;
        let mut declaration = SemanticMutationTransaction::new();
        if admitted {
            declaration.add_member(root, target);
        }
        let animation =
            declaration.create_affine_lifecycle_animation(target, direction, endpoint, options);
        self.declare_and_activate_prepared_animation(
            store,
            declaration,
            animation,
            AnimationOptions::new(),
            Some(match direction {
                SemanticAffineLifecycleDirection::IntroduceFrom => {
                    PreparedAnimationLifecycle::Introduce(root)
                }
                SemanticAffineLifecycleDirection::RemoveTo => {
                    PreparedAnimationLifecycle::AffineRemove {
                        root,
                        target,
                        admitted,
                    }
                }
            }),
        )
    }

    /// Atomically introduce one detached leaf and activate its geometry reveal.
    pub fn declare_and_activate_create(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
        target: SemanticNodeId,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        self.require_create_root(root, target)?;
        self.require_create_target(store, target)?;
        let mut declaration = SemanticMutationTransaction::new();
        declaration.add_member(root, target);
        let animation = declaration.create_create_animation(target, options);
        self.declare_and_activate_prepared_animation(
            store,
            declaration,
            animation,
            AnimationOptions::new(),
            Some(PreparedAnimationLifecycle::Introduce(root)),
        )
    }

    /// Atomically admit one detached leaf, reverse its reveal, and remove it at completion.
    pub fn declare_and_activate_uncreate(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
        target: SemanticNodeId,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        let rate = options.rate_func.unwrap_or(RateFunction::Smooth);
        if !matches!(rate, RateFunction::Linear | RateFunction::Smooth) {
            return Err(ExecutionSessionAnimationError::CreateTarget {
                target,
                error: ExecutionSessionCreateError::UnsupportedUncreateRateFunction(rate),
            });
        }
        self.require_create_root(root, target)?;
        self.require_create_target(store, target)?;
        let mut declaration = SemanticMutationTransaction::new();
        declaration.add_member(root, target);
        let animation = declaration
            .create_create_animation(target, options.remover(true).reverse_rate_function(true));
        self.declare_and_activate_prepared_animation(
            store,
            declaration,
            animation,
            AnimationOptions::new(),
            Some(PreparedAnimationLifecycle::Introduce(root)),
        )
    }

    /// Atomically introduce flat parallel detached leaves with one shared reveal segment.
    ///
    /// Membership admission, all leaf declarations, the Parallel root, reveal tracks, and
    /// runtime publication are prepared together. Every target remains detached when checked;
    /// no target or wrapper-facing identity is committed if any leaf is invalid.
    pub fn declare_and_activate_create_parallel(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
        children: &[(SemanticNodeId, AnimationOptions)],
        play_options: AnimationOptions,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        let error_target = children.first().map(|(target, _)| *target).unwrap_or(root);
        self.require_create_root(root, error_target)?;
        if children.is_empty() {
            return Err(ExecutionSessionAnimationError::CreateTarget {
                target: error_target,
                error: ExecutionSessionCreateError::EmptyParallel,
            });
        }
        let mut targets = HashSet::with_capacity(children.len());
        for (target, _) in children {
            if !targets.insert(*target) {
                return Err(ExecutionSessionAnimationError::CreateTarget {
                    target: *target,
                    error: ExecutionSessionCreateError::DuplicateTarget,
                });
            }
            self.require_create_target(store, *target)?;
        }

        let mut declaration = SemanticMutationTransaction::new();
        let leaves = children
            .iter()
            .map(|(target, options)| {
                declaration.add_member(root, *target);
                declaration.create_create_animation(*target, *options)
            })
            .collect::<Vec<_>>();
        let animation_root = declaration.create_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            leaves,
            AnimationOptions::new(),
        );
        self.declare_and_activate_prepared_animation(
            store,
            declaration,
            animation_root,
            play_options,
            Some(PreparedAnimationLifecycle::Introduce(root)),
        )
    }

    fn require_create_root(
        &self,
        root: SemanticNodeId,
        error_target: SemanticNodeId,
    ) -> Result<(), ExecutionSessionAnimationError> {
        if !self.reachability.is_execution_root(root) {
            return Err(ExecutionSessionAnimationError::CreateTarget {
                target: error_target,
                error: ExecutionSessionCreateError::RootIsNotInExecutionDomain,
            });
        }
        if !self.callback_schedule.is_empty() {
            return Err(ExecutionSessionAnimationError::CreateTarget {
                target: error_target,
                error: ExecutionSessionCreateError::RequiredCallbacksUnsupported,
            });
        }
        Ok(())
    }

    fn require_create_target(
        &self,
        store: &SemanticStore,
        target: SemanticNodeId,
    ) -> Result<(), ExecutionSessionAnimationError> {
        let state = store
            .semantic_object_state_checked(target)
            .map_err(|error| ExecutionSessionAnimationError::TargetState { target, error })?;
        if !state.signal_bindings().is_empty() {
            return Err(ExecutionSessionAnimationError::CreateTarget {
                target,
                error: ExecutionSessionCreateError::ReactiveBindingsUnsupported,
            });
        }
        let node = store
            .node(target)
            .expect("validated semantic object has a live node");
        if node.is_scene_owned()
            || !node.parents().is_empty()
            || self.execution_index.execution_object_id(target).is_some()
        {
            return Err(ExecutionSessionAnimationError::CreateTarget {
                target,
                error: ExecutionSessionCreateError::TargetIsNotDetached,
            });
        }
        Ok(())
    }

    fn require_fade_target(
        &self,
        store: &SemanticStore,
        root: SemanticNodeId,
        target: SemanticNodeId,
        direction: SemanticFadeDirection,
    ) -> Result<(), ExecutionSessionAnimationError> {
        self.require_lifecycle_target_context(store, root, target)?;
        let node = store
            .node(target)
            .expect("validated semantic object has a live node");
        match direction {
            SemanticFadeDirection::In => {
                if node.is_scene_owned()
                    || !node.parents().is_empty()
                    || self.execution_index.execution_object_id(target).is_some()
                {
                    return Err(ExecutionSessionAnimationError::FadeTarget {
                        target,
                        error: ExecutionSessionFadeError::TargetIsNotDetached,
                    });
                }
            }
            SemanticFadeDirection::Out => {
                if node.parents().len() > 1 {
                    return Err(ExecutionSessionAnimationError::FadeTarget {
                        target,
                        error: ExecutionSessionFadeError::TargetIsAliased,
                    });
                }
                if node.parents() != [root]
                    || self.execution_index.execution_object_id(target).is_none()
                {
                    return Err(ExecutionSessionAnimationError::FadeTarget {
                        target,
                        error: ExecutionSessionFadeError::TargetIsNotDirectRootMember,
                    });
                }
            }
        }
        Ok(())
    }

    fn require_lifecycle_target_context(
        &self,
        store: &SemanticStore,
        root: SemanticNodeId,
        target: SemanticNodeId,
    ) -> Result<(), ExecutionSessionAnimationError> {
        if !self.reachability.is_execution_root(root) {
            return Err(ExecutionSessionAnimationError::FadeTarget {
                target,
                error: ExecutionSessionFadeError::RootIsNotInExecutionDomain,
            });
        }
        if !self.callback_schedule.is_empty() {
            return Err(ExecutionSessionAnimationError::FadeTarget {
                target,
                error: ExecutionSessionFadeError::RequiredCallbacksUnsupported,
            });
        }
        let state = store
            .semantic_object_state_checked(target)
            .map_err(|error| ExecutionSessionAnimationError::TargetState { target, error })?;
        if !state.signal_bindings().is_empty() {
            return Err(ExecutionSessionAnimationError::FadeTarget {
                target,
                error: ExecutionSessionFadeError::ReactiveBindingsUnsupported,
            });
        }
        Ok(())
    }

    /// Return whether a lifecycle leaf needs a transaction-local root edge.
    /// An unmounted family's parent edge retains authoring identity but has no execution slot.
    fn require_affine_lifecycle_target(
        &self,
        store: &SemanticStore,
        root: SemanticNodeId,
        target: SemanticNodeId,
        direction: SemanticAffineLifecycleDirection,
    ) -> Result<bool, ExecutionSessionAnimationError> {
        self.require_lifecycle_target_context(store, root, target)?;
        let node = store
            .node(target)
            .expect("validated semantic object has a live node");
        let execution_object = self.execution_index.execution_object_id(target);
        if execution_object.is_none() && !node.is_scene_owned() {
            return Ok(true);
        }
        match direction {
            SemanticAffineLifecycleDirection::IntroduceFrom => {
                Err(ExecutionSessionAnimationError::FadeTarget {
                    target,
                    error: ExecutionSessionFadeError::TargetIsNotDetached,
                })
            }
            SemanticAffineLifecycleDirection::RemoveTo => {
                if node
                    .parents()
                    .iter()
                    .any(|parent| *parent != root && self.reachability.is_reachable(*parent))
                {
                    return Err(ExecutionSessionAnimationError::FadeTarget {
                        target,
                        error: ExecutionSessionFadeError::TargetIsAliased,
                    });
                }
                if node.parents().contains(&root) && execution_object.is_some() {
                    Ok(false)
                } else {
                    Err(ExecutionSessionAnimationError::FadeTarget {
                        target,
                        error: ExecutionSessionFadeError::TargetIsNotDirectRootMember,
                    })
                }
            }
        }
    }

    fn require_animation_declaration_context(
        &self,
        store: &SemanticStore,
    ) -> Result<(), ExecutionSessionAnimationError> {
        if self.pending_callback.is_some() {
            return Err(ExecutionSessionAnimationError::RequiredCallbackPending);
        }
        if self.pending_segment_completion.is_some() {
            return Err(ExecutionSessionAnimationError::SegmentCompletionPending);
        }
        if store.identity() != self.store_identity {
            return Err(ExecutionSessionAnimationError::ForeignSemanticStore);
        }
        let expected = self.publication_context().scene_revision();
        let actual = store.scene_revision();
        if actual != expected {
            return Err(ExecutionSessionAnimationError::StaleSceneRevision { expected, actual });
        }
        Ok(())
    }

    fn stage_animation_target_state(
        &self,
        store: &SemanticStore,
        declaration: &mut SemanticMutationTransaction,
        target: SemanticNodeId,
    ) -> Result<noon_core::SemanticLocalNodeToken, ExecutionSessionAnimationError> {
        let state = store
            .semantic_object_state_checked(target)
            .map_err(|error| ExecutionSessionAnimationError::TargetState { target, error })?
            .clone();
        Ok(declaration.create_node(SemanticNodeCreation::object(state)))
    }

    fn declare_and_activate_prepared_animation(
        &mut self,
        store: &mut SemanticStore,
        declaration: SemanticMutationTransaction,
        root: noon_core::SemanticLocalNodeToken,
        play_options: AnimationOptions,
        lifecycle: Option<PreparedAnimationLifecycle>,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        let prepared = declaration.prepare(store).map_err(|error| {
            ExecutionSessionAnimationError::AuthoredPublication(
                ExecutionSessionPublicationError::Semantic(error),
            )
        })?;
        let projection = lower_prepared_semantic_animation_composition(
            &prepared,
            &self.execution_index,
            root,
            self.runtime.frame().time,
            play_options,
            |object| {
                let index = self.runtime.frame_index_for_object(object)?;
                let row = self.runtime.frame().objects.get(index)?;
                Some(EffectiveAnimationProperties {
                    transform: row.transform,
                    style: row.style,
                    appearance: row.appearance,
                })
            },
        )?;
        let mut segment =
            ExecutionSegment::from_duration(projection.start_time(), projection.run_time())?;
        let mut next_track_id = self.next_activation_track_id;
        let mut definitions = Vec::with_capacity(projection.tracks().len());
        let mut completions = Vec::with_capacity(projection.tracks().len());
        for track in projection.tracks() {
            let raw_id = next_track_id.ok_or(ExecutionSessionAnimationError::TrackIdExhausted)?;
            let track_id = TrackId::new(raw_id);
            let definition = track
                .with_track_id(track_id)
                .map_err(ExecutionSessionAnimationError::PreparedTrack)?;
            completions.push((
                track.target,
                track.completion.clone(),
                track.execution_object_id,
                track.property,
                track_id,
                track.timing.start_time + track.timing.duration,
            ));
            definitions.push(definition);
            next_track_id = raw_id.checked_add(1);
        }

        if lifecycle
            .as_ref()
            .is_some_and(PreparedAnimationLifecycle::admits)
        {
            let existing_tracks = definitions
                .iter()
                .filter(|definition| self.runtime.contains_object(definition.object))
                .cloned()
                .collect::<Vec<_>>();
            self.runtime
                .preflight_reconcilable_track_additions(&existing_tracks)
                .map_err(ExecutionSessionAnimationError::Publication)?;
        } else {
            self.runtime
                .preflight_reconcilable_track_additions(&definitions)
                .map_err(ExecutionSessionAnimationError::Publication)?;
        }

        let (token, next_segment_sequence) = if definitions.is_empty() {
            (None, self.next_segment_sequence)
        } else {
            let raw_sequence = self
                .next_segment_sequence
                .ok_or(ExecutionSessionAnimationError::SegmentSequenceExhausted)?;
            let token = ExecutionSegmentToken::new(
                self.runtime.runtime_identity(),
                ExecutionSegmentSequence::new(raw_sequence),
            );
            (Some(token), raw_sequence.checked_add(1))
        };

        let execution_prefix = definitions
            .into_iter()
            .map(ExecutionPatch::AddTrack)
            .collect();
        let result = self
            .apply_prepared_semantic_transaction_with_execution(prepared, execution_prefix, None)
            .map_err(ExecutionSessionAnimationError::AuthoredPublication)?;
        debug_assert!(result.resolve(root).is_some());
        let activation_scene_revision = self.publication_context().scene_revision();

        self.next_activation_track_id = next_track_id;
        if let Some(token) = token {
            let entries = completions
                .into_iter()
                .map(
                    |(target, completion, execution_object, property, track, end_time)| {
                        SegmentCompletionEntry {
                            semantic_object: resolve_committed_node(target, &result),
                            completion,
                            execution_object,
                            property,
                            track,
                            end_time,
                        }
                    },
                )
                .collect();
            segment = segment.with_completion_token(token);
            self.next_segment_sequence = next_segment_sequence;
            self.pending_segment_completion = Some(PendingSegmentCompletion {
                token,
                activation_scene_revision,
                kind: PendingSegmentCompletionKind::ObjectTracks {
                    lifecycle_root: lifecycle.as_ref().map(|lifecycle| lifecycle.root()),
                    lifecycle_removals: match lifecycle.as_ref() {
                        Some(PreparedAnimationLifecycle::Composition { removals, .. }) => {
                            removals.clone()
                        }
                        _ => lifecycle
                            .as_ref()
                            .and_then(|lifecycle| lifecycle.removal())
                            .into_iter()
                            .collect(),
                    },
                    entries,
                },
            });
        }
        Ok(segment)
    }

    /// Apply one native-reactive input using authoritative semantic signal identity.
    ///
    /// The execution VM key remains an internal lowering detail. Current native
    /// reactive values use the compact execution domain (`bool`, `f32`, `Vec2`);
    /// semantic structural/property mutation remains outside this method.
    pub fn set_reactive_input(
        &mut self,
        signal: SemanticNodeId,
        value: impl Into<ReactiveValue>,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        self.ensure_direct_input_ingress_available()?;
        if self.signal_timeline.has_history(signal) {
            return Err(ExecutionSessionInputError::TimelineOwnedSignal { signal });
        }
        if self.reactive_projection.is_native_owned(signal) {
            return Err(ExecutionSessionInputError::NativeOwnedSignal { signal });
        }
        let execution_signal = self
            .reactive_projection
            .execution_signal_id(signal)
            .ok_or(ExecutionSessionInputError::UnknownSemanticSignal(signal))?;
        self.apply_reactive_input_batch(vec![(execution_signal, value.into())])
    }

    /// Read the current effective value for one canonical semantic signal.
    pub fn effective_signal_value(&self, signal: SemanticNodeId) -> Option<&ReactiveValue> {
        let execution = self.reactive_projection.execution_signal_id(signal)?;
        self.runtime.reactive_value(execution)
    }

    pub const fn last_reactive_stats(&self) -> noon_runtime::ReactiveRuntimeStats {
        self.runtime.last_reactive_stats()
    }

    fn evaluate_signal_timeline(
        &mut self,
        time: f64,
        mode: ExecutionEvaluationMode,
    ) -> Result<&FrameState, EvaluationError> {
        if self.signal_timeline.is_empty() {
            return match mode {
                ExecutionEvaluationMode::Evaluate => self.runtime.evaluate(time),
                ExecutionEvaluationMode::Seek => self.runtime.seek(time),
                ExecutionEvaluationMode::Advance => self.runtime.advance_to(time),
            };
        }
        let current = self.runtime.frame().time;
        let requires_seek = mode.requires_seek(current, time);
        if !requires_seek && self.signal_timeline.is_coherent_at(current, time) {
            return Ok(self.runtime.frame());
        }
        let preview = if requires_seek {
            self.signal_timeline.preview_seek(time)
        } else {
            self.signal_timeline.preview(current, time)
        };
        if requires_seek {
            self.runtime
                .seek_with_reactive_inputs(time, preview.inputs())?;
        } else {
            self.runtime
                .advance_to_with_reactive_inputs(time, preview.inputs())?;
        }
        self.signal_timeline.commit(preview);
        Ok(self.runtime.frame())
    }

    /// Deliver one normalized sampled native state source through signal-owned routes.
    ///
    /// Source/value validation is shared with browser/native input envelopes. The
    /// source is resolved only against routes emitted by semantic reactive lowering;
    /// platform hosts never receive or construct execution `SignalId`s. An unbound
    /// source is a valid no-op.
    pub fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        let update = NativeStateUpdate::new(source, value)?;
        let targets = self
            .reactive_projection
            .native_state_targets(&update.source)
            .to_vec();
        if targets.is_empty() {
            return Ok(self.runtime.frame());
        }
        self.ensure_direct_input_ingress_available()?;
        let value = reactive_value_from_native(update.value);
        self.apply_reactive_input_batch(
            targets
                .into_iter()
                .map(|signal| (signal, value.clone()))
                .collect(),
        )
    }

    /// Deliver one explicitly ordered discrete native event occurrence.
    ///
    /// Occurrences are never coalesced. The session rejects duplicate/out-of-order
    /// sequence numbers before changing any event signal, then advances each lowered
    /// scalar event counter using the existing native-event convention.
    pub fn emit_native_event(
        &mut self,
        occurrence: NativeEventOccurrence,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        if let Some(previous) = self.last_native_event_sequence {
            if occurrence.sequence <= previous {
                return Err(ExecutionSessionInputError::NativeEventOutOfOrder {
                    previous,
                    next: occurrence.sequence,
                });
            }
        }

        let targets = self
            .reactive_projection
            .native_event_targets(&occurrence.source)
            .to_vec();
        if targets.is_empty() {
            self.last_native_event_sequence = Some(occurrence.sequence);
            return Ok(self.runtime.frame());
        }
        self.ensure_direct_input_ingress_available()?;
        let next_values = targets
            .iter()
            .map(|signal| {
                let value = self
                    .runtime
                    .reactive_value(*signal)
                    .expect("lowered native event target must remain a live reactive signal");
                let ReactiveValue::Scalar(current) = value else {
                    unreachable!(
                        "semantic native event declaration validates a scalar input signal"
                    )
                };
                if *current >= NATIVE_EVENT_SEQUENCE_WRAP {
                    0.0
                } else {
                    *current + 1.0
                }
            })
            .collect::<Vec<_>>();

        self.apply_reactive_input_batch(
            targets
                .into_iter()
                .zip(next_values)
                .map(|(signal, next)| (signal, ReactiveValue::Scalar(next)))
                .collect(),
        )?;
        self.last_native_event_sequence = Some(occurrence.sequence);
        Ok(self.runtime.frame())
    }

    /// Resolve an authoritative semantic object identity to its current execution key.
    pub fn execution_object_id(&self, node: SemanticNodeId) -> Option<ObjectId> {
        self.execution_index.execution_object_id(node)
    }

    fn apply_reactive_input_batch(
        &mut self,
        inputs: Vec<(noon_core::SignalId, ReactiveValue)>,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        let current = self.runtime.frame().time;
        let signal_timeline = (!self.signal_timeline.is_empty()
            && !self.signal_timeline.is_coherent_at(current, current))
        .then(|| self.signal_timeline.preview(current, current));
        let mut combined = signal_timeline
            .as_ref()
            .map_or_else(Vec::new, |preview| preview.inputs().to_vec());
        combined.extend(inputs);
        self.runtime
            .advance_to_with_reactive_inputs(current, &combined)?;
        if let Some(preview) = signal_timeline {
            self.signal_timeline.commit(preview);
        }
        Ok(self.runtime.frame())
    }
}

fn lower_live_scalar_value(value: f64) -> Result<f32, ExecutionSessionAnimationError> {
    if !value.is_finite() || value.abs() > f32::MAX as f64 {
        return Err(ExecutionSessionAnimationError::ReactiveEnrollment(
            ReactiveError::NonFiniteValue(noon_core::SignalId::new(0)),
        ));
    }
    Ok(value as f32)
}

fn semantic_painter_order(
    store: &SemanticStore,
    reachability: &SemanticExecutionReachability,
) -> SemanticPainterOrderIndex {
    let mut index = SemanticPainterOrderIndex::default();
    for node in reachability.reachable_objects() {
        let state = store
            .semantic_object_state_checked(node)
            .expect("reachable semantic object was validated during initial lowering");
        index.insert(node, state.presentation().order_key());
    }
    index
}

#[derive(Clone, Debug, Default)]
struct SemanticPainterOrderIndex {
    ordered: BTreeMap<(i32, u64), SemanticNodeId>,
    keys: HashMap<SemanticNodeId, (i32, u64)>,
}

impl SemanticPainterOrderIndex {
    fn tail(&self) -> Option<(i32, u64)> {
        self.ordered.last_key_value().map(|(key, _)| *key)
    }

    fn insert(&mut self, node: SemanticNodeId, key: (i32, u64)) {
        let previous_key = self.keys.insert(node, key);
        let previous_node = self.ordered.insert(key, node);
        debug_assert!(previous_key.is_none());
        debug_assert!(previous_node.is_none());
    }

    fn remove(&mut self, node: SemanticNodeId) {
        let key = self
            .keys
            .remove(&node)
            .expect("exited execution object has a painter-order entry");
        let removed = self.ordered.remove(&key);
        debug_assert_eq!(removed, Some(node));
    }
}

fn reactive_value_from_native(value: NativeInputValue) -> ReactiveValue {
    match value {
        NativeInputValue::Scalar(value) => ReactiveValue::Scalar(value),
        NativeInputValue::Bool(value) => ReactiveValue::Bool(value),
        NativeInputValue::Vec2(value) => ReactiveValue::Vec2(value),
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, NativeEventSource, NativeStateSource, RateFunction,
        SemanticMutationTransaction, SemanticObjectProperty, SemanticObjectRole,
        SemanticObjectState, SemanticSignalExpr, SemanticSignalValue, SemanticVec3, StoredGeometry,
        TrackTiming, Vec2,
    };
    use noon_runtime::TimelineWakeState;

    use super::*;

    fn linear_second() -> AnimationOptions {
        AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear)
    }

    fn camera_state(center: Vec2, height: f32) -> SemanticObjectState {
        let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(height * 16.0 / 9.0, height),
        });
        state.transform.translation = SemanticVec3::new(center.x as f64, center.y as f64, 0.0);
        state.set_role(SemanticObjectRole::Camera2D);
        state
    }

    #[test]
    fn semantic_family_sessions_isolate_camera_objects_and_reactive_state() {
        let mut store = SemanticStore::new();
        let left_root = store.insert_family();
        let right_root = store.insert_family();
        let left_camera = store.insert_semantic_object(camera_state(Vec2::new(-2.0, 0.0), 4.0));
        let right_camera = store.insert_semantic_object(camera_state(Vec2::new(3.0, 1.0), 6.0));
        let shared =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        store
            .bind_semantic_signal(signal, shared, SemanticObjectProperty::ObjectOpacity)
            .unwrap();
        for (root, camera) in [(left_root, left_camera), (right_root, right_camera)] {
            store.add_semantic_family_member(root, camera).unwrap();
            store.add_semantic_family_member(root, shared).unwrap();
        }
        let mut left = ExecutionSession::from_semantic_root(&store, left_root).unwrap();
        let right = ExecutionSession::from_semantic_root(&store, right_root).unwrap();
        assert_eq!(left.frame().objects.len(), 2);
        assert_eq!(right.frame().objects.len(), 2);
        assert_eq!(left.execution_object_id(right_camera), None);
        assert_eq!(right.execution_object_id(left_camera), None);
        assert_eq!(
            left.execution_object_id(shared),
            right.execution_object_id(shared)
        );
        assert_eq!(
            left.camera().unwrap(),
            Camera2DState {
                center: Vec2::new(-2.0, 0.0),
                height: 4.0
            }
        );
        assert_eq!(
            right.camera().unwrap(),
            Camera2DState {
                center: Vec2::new(3.0, 1.0),
                height: 6.0
            }
        );
        left.set_reactive_input(signal, 0.8_f32).unwrap();
        assert_eq!(left.frame().objects[1].style.opacity, 0.8);
        assert_eq!(right.frame().objects[1].style.opacity, 0.4);
        assert_eq!(store.scene_roots().count(), 0);
        // Existing all-roots consumers still see the store's actual attached roots.
        assert!(ExecutionSession::from_semantic_store(&store)
            .unwrap()
            .frame()
            .objects
            .is_empty());
    }

    #[test]
    fn semantic_store_runs_through_typed_session_into_renderer_facing_changes() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 2.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let execution_object = session.execution_object_id(object).unwrap();
        let initial_publication = session.publication_context();

        assert_eq!(session.frame().objects.len(), 1);
        assert_eq!(session.frame().objects[0].id, execution_object);
        assert_eq!(session.frame().objects[0].style.opacity, 0.4);
        assert_eq!(session.camera().unwrap(), Camera2DState::default());

        session.take_frame_changes();
        session.set_reactive_input(signal, 0.7_f32).unwrap();
        let reactive_publication = session.publication_context();

        assert_eq!(session.frame().objects[0].style.opacity, 0.7);
        assert_eq!(session.take_frame_changes().object_indices(), &[0]);
        assert_eq!(
            reactive_publication.scene_revision(),
            initial_publication.scene_revision()
        );
        assert_eq!(
            reactive_publication.execution_revision(),
            initial_publication.execution_revision()
        );
        assert_eq!(
            reactive_publication.frame_epoch(),
            initial_publication.frame_epoch().checked_next().unwrap()
        );

        session.set_reactive_input(signal, 0.7_f32).unwrap();
        assert_eq!(session.publication_context(), reactive_publication);

        session.seek(1.25).unwrap();
        let timeline_publication = session.publication_context();
        assert_eq!(session.frame().time, 1.25);
        assert_eq!(session.frame().objects[0].style.opacity, 0.7);
        assert_eq!(
            timeline_publication.scene_revision(),
            reactive_publication.scene_revision()
        );
        assert_eq!(
            timeline_publication.execution_revision(),
            reactive_publication.execution_revision()
        );
        assert_eq!(
            timeline_publication.frame_epoch(),
            reactive_publication.frame_epoch().checked_next().unwrap()
        );
    }

    #[test]
    fn viewport_query_maps_slots_to_painter_order_and_refits_local_changes() {
        let mut store = SemanticStore::new();
        let moving = store
            .insert_semantic_input_signal(SemanticVec3::new(0.0, 0.0, 0.0))
            .unwrap();
        let mut left_state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.5 });
        left_state.transform.translation = SemanticVec3::new(-10.0, 0.0, 0.0);
        let left = store.insert_semantic_object(left_state);
        let middle =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 0.5,
            }));
        let top = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 0.25,
        }));
        for object in [left, middle, top] {
            store.attach_to_scene(object).unwrap();
        }
        store
            .bind_semantic_signal(moving, middle, SemanticObjectProperty::Translation)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let viewport = Rect::new(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0));

        let initial = session.query_viewport(viewport);
        assert_eq!(initial.object_indices(), &[1, 2]);
        assert_eq!(initial.spatial_stats().results, 2);

        session
            .set_reactive_input(moving, Vec2::new(20.0, 0.0))
            .unwrap();
        let moved = session.query_viewport(viewport);
        assert_eq!(moved.object_indices(), &[2]);
        assert_eq!(session.last_spatial_update_stats().full_rebuilds, 0);
        assert_eq!(session.last_spatial_update_stats().leaves_upserted, 1);
    }

    #[test]
    fn wake_state_separates_one_pending_frame_from_static_completion() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

        let wake = session.wake_state();
        assert!(wake.frame_pending());
        assert_eq!(wake.timeline(), TimelineWakeState::Quiescent);
        assert!(!wake.is_quiescent());

        session.take_frame_changes();
        assert!(session.wake_state().is_quiescent());
    }

    #[test]
    fn activated_animation_requests_continuous_frames_until_endpoint() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut target_state = store.semantic_object_state_checked(object).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(4.0, 0.0, 0.0);
        let target = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(object, target, AnimationOptions::new())
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.take_frame_changes();

        let segment = session
            .activate_animation_segment(&store, animation, linear_second())
            .unwrap();
        assert_eq!(
            session.wake_state().timeline(),
            TimelineWakeState::Continuous
        );
        assert!(session.has_replay_timeline_work());

        session.seek(1.0).unwrap();
        assert_eq!(
            session.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );
        assert!(!session.segment_state(segment).is_complete());
        session.complete_segment(&mut store, segment).unwrap();
        assert!(session.segment_state(segment).is_complete());
    }

    #[test]
    fn animation_activation_returns_resolved_continuation_segment() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut target_state = store.semantic_object_state_checked(object).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(6.0, 0.0, 0.0);
        let target = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(
                object,
                target,
                AnimationOptions::new().run_time(4.0),
            )
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.seek(3.0).unwrap();
        session.take_frame_changes();

        let segment = session
            .activate_animation_segment(
                &store,
                animation,
                AnimationOptions::new()
                    .run_time(1.5)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        assert_eq!(segment.start_time(), 3.0);
        assert_eq!(segment.duration(), 1.5);
        assert_eq!(segment.end_time(), 4.5);
        assert_eq!(
            session.segment_state(segment).timeline(),
            TimelineWakeState::Continuous
        );

        session.advance_segment_to(segment, 100.0).unwrap();
        assert_eq!(session.frame().time, 4.5);
        assert!(!session.segment_state(segment).is_complete());
        session.complete_segment(&mut store, segment).unwrap();
        assert!(session.segment_state(segment).is_complete());
        assert_eq!(session.frame().objects[0].transform.translation.x, 6.0);
    }

    #[test]
    fn generic_activation_rejects_predeclared_lifecycle_without_publication() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let animation = store
            .insert_semantic_fade_animation(
                object,
                SemanticFadeDirection::Out,
                AnimationOptions::new(),
            )
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.take_frame_changes();
        let before = session.publication_context();

        assert_eq!(
            session.activate_animation_segment(&store, animation, linear_second()),
            Err(ExecutionSessionAnimationError::Payload(
                SemanticAffineAnimationTrackError::UnsupportedLifecycle {
                    animation,
                    remover: true,
                    introducer: false,
                }
            ))
        );
        assert_eq!(session.publication_context(), before);
        assert!(session.pending_segment_token().is_none());
        assert!(session.take_frame_changes().is_empty());
    }

    #[test]
    fn prepared_fade_rejects_a_root_outside_the_execution_domain() {
        let mut store = SemanticStore::new();
        let execution_root = store.insert_family();
        let other_root = store.insert_family();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store
            .add_semantic_family_member(execution_root, object)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_root(&store, execution_root).unwrap();
        session.take_frame_changes();
        let before = session.publication_context();

        assert_eq!(
            session.declare_and_activate_fade(
                &mut store,
                other_root,
                object,
                SemanticFadeDirection::Out,
                linear_second(),
            ),
            Err(ExecutionSessionAnimationError::FadeTarget {
                target: object,
                error: ExecutionSessionFadeError::RootIsNotInExecutionDomain,
            })
        );
        assert_eq!(session.publication_context(), before);
        assert!(session.pending_segment_token().is_none());
        assert!(session.take_frame_changes().is_empty());
        assert_eq!(store.node(object).unwrap().parents(), &[execution_root]);
    }

    #[test]
    fn animation_segment_overflow_fails_before_publication() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut target_state = store.semantic_object_state_checked(object).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(2.0, 0.0, 0.0);
        let target = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(object, target, AnimationOptions::new())
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.seek(f64::MAX).unwrap();
        session.take_frame_changes();
        let before = session.frame().clone();

        assert_eq!(
            session.activate_animation_segment(&store, animation, linear_second()),
            Err(ExecutionSessionAnimationError::Segment(
                ExecutionSegmentError::EndTimeOverflow {
                    start_time: f64::MAX,
                    duration: 1.0,
                }
            ))
        );
        assert_eq!(session.frame(), &before);
        assert!(session.wake_state().is_quiescent());
    }

    #[test]
    fn native_sampled_source_reaches_runtime_without_exposing_signal_identity() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.25_f64).unwrap();
        let source = NativeStateSource::Control {
            name: "opacity".to_owned(),
        };
        store
            .bind_semantic_native_state_input(signal, source.clone())
            .unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

        let before = session.frame().clone();
        let publication = session.publication_context();
        assert_eq!(
            session.set_reactive_input(signal, 0.9_f32),
            Err(ExecutionSessionInputError::NativeOwnedSignal { signal })
        );
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), publication);

        session.take_frame_changes();
        session
            .set_native_state_input(source, NativeInputValue::Scalar(0.8))
            .unwrap();
        assert_eq!(session.frame().objects[0].style.opacity, 0.8);
        assert_eq!(session.take_frame_changes().object_indices(), &[0]);
    }

    #[test]
    fn native_sampled_source_validates_value_before_runtime_mutation() {
        let mut store = SemanticStore::new();
        let signal = store
            .insert_semantic_input_signal(SemanticVec3::new(0.0, 0.0, 0.0))
            .unwrap();
        store
            .bind_semantic_native_state_input(signal, NativeStateSource::ViewportSize)
            .unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::Translation)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let before = session.frame().clone();

        assert!(matches!(
            session.set_native_state_input(
                NativeStateSource::ViewportSize,
                NativeInputValue::Scalar(1.0),
            ),
            Err(ExecutionSessionInputError::NativeInput(
                NativeInputRuntimeError::TypeMismatch { .. }
            ))
        ));
        assert_eq!(session.frame(), &before);
    }

    #[test]
    fn native_discrete_events_are_ordered_and_never_coalesced() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let source = NativeEventSource::KeyPress {
            code: "Space".to_owned(),
        };
        store
            .bind_semantic_native_event_input(signal, source.clone())
            .unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

        session
            .emit_native_event(NativeEventOccurrence::new(10, source.clone()))
            .unwrap();
        assert_eq!(session.frame().objects[0].transform.rotation, 1.0);
        session
            .emit_native_event(NativeEventOccurrence::new(11, source.clone()))
            .unwrap();
        assert_eq!(session.frame().objects[0].transform.rotation, 2.0);

        assert_eq!(
            session.emit_native_event(NativeEventOccurrence::new(11, source)),
            Err(ExecutionSessionInputError::NativeEventOutOfOrder {
                previous: 11,
                next: 11,
            })
        );
        assert_eq!(session.frame().objects[0].transform.rotation, 2.0);
    }

    #[test]
    fn native_discrete_event_counter_matches_existing_bounded_wrap_convention() {
        let mut store = SemanticStore::new();
        let signal = store
            .insert_semantic_input_signal(NATIVE_EVENT_SEQUENCE_WRAP)
            .unwrap();
        let source = NativeEventSource::PointerDown { button: 0 };
        store
            .bind_semantic_native_event_input(signal, source.clone())
            .unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

        session
            .emit_native_event(NativeEventOccurrence::new(0, source))
            .unwrap();
        assert_eq!(session.frame().objects[0].transform.rotation, 0.0);
    }

    #[test]
    fn canonical_camera_is_derived_from_effective_runtime_transform() {
        let mut store = SemanticStore::new();
        let camera = store.insert_semantic_object(camera_state(Vec2::new(1.0, 2.0), 8.0));
        store.attach_to_scene(camera).unwrap();

        let mut target_state = store.semantic_object_state_checked(camera).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(5.0, -2.0, 0.0);
        target_state.transform.scale = SemanticVec3::new(1.0, 0.5, 1.0);
        let target = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(camera, target, AnimationOptions::new())
            .unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        assert_eq!(
            session.camera().unwrap(),
            Camera2DState {
                center: Vec2::new(1.0, 2.0),
                height: 8.0,
            }
        );

        session
            .activate_animation(&store, animation, linear_second())
            .unwrap();
        session.seek(0.5).unwrap();
        assert_eq!(
            session.camera().unwrap(),
            Camera2DState {
                center: Vec2::new(3.0, 0.0),
                height: 6.0,
            }
        );
    }

    #[test]
    fn animation_activation_rejects_an_unpublished_semantic_revision() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let target =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        let animation = store
            .insert_semantic_transform_animation(object, target, AnimationOptions::new())
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.take_frame_changes();
        let context = session.publication_context();
        let frame = session.frame().clone();
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        transaction.set_property(object, SemanticObjectProperty::ObjectOpacity, 0.5_f64);
        transaction.apply(&mut store).unwrap();

        assert_eq!(
            session.activate_animation_segment(&store, animation, linear_second()),
            Err(ExecutionSessionAnimationError::StaleSceneRevision {
                expected: context.scene_revision(),
                actual: store.scene_revision(),
            })
        );
        assert_eq!(session.publication_context(), context);
        assert_eq!(session.frame(), &frame);
        assert!(session.take_frame_changes().is_empty());
    }

    #[test]
    fn execution_vm_signal_identity_does_not_escape_the_session_surface() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        assert_eq!(
            session.set_reactive_input(object, 1.0_f32),
            Err(ExecutionSessionInputError::UnknownSemanticSignal(object))
        );
    }

    #[test]
    fn canonical_scalar_track_drives_binding_before_publication_and_rejects_direct_input() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let tracker = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_signal(tracker, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_scalar_signal_track(
            tracker,
            0.0,
            4.0,
            TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        );
        transaction.apply(&mut store).unwrap();

        assert_eq!(
            store.semantic_input_scalar_value_at(tracker, 1.0).unwrap(),
            2.0
        );
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.evaluate(0.0).unwrap();
        assert_eq!(
            session.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(0.0))
        );
        session.advance_to(1.0).unwrap();
        assert_eq!(session.frame().objects[0].transform.rotation, 2.0);
        assert_eq!(
            session.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(2.0))
        );
        assert_eq!(session.last_reactive_stats().bindings_invalidated, 1);

        let context = session.publication_context();
        session.advance_to(1.0).unwrap();
        assert_eq!(session.publication_context(), context);
        assert_eq!(
            session.set_reactive_input(tracker, 9.0_f32),
            Err(ExecutionSessionInputError::TimelineOwnedSignal { signal: tracker })
        );
        assert_eq!(session.publication_context(), context);

        session.advance_to(2.0).unwrap();
        assert_eq!(session.frame().objects[0].transform.rotation, 4.0);
        session.seek(0.5).unwrap();
        assert_eq!(session.frame().objects[0].transform.rotation, 1.0);
    }

    #[test]
    fn scalar_track_active_at_zero_is_present_in_initial_coherent_frame() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let tracker = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_signal(tracker, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_scalar_signal_track(
            tracker,
            0.0,
            4.0,
            TrackTiming::new(-1.0, 2.0, RateFunction::Linear),
        );
        transaction.apply(&mut store).unwrap();

        let session = ExecutionSession::from_semantic_store(&store).unwrap();
        assert_eq!(session.frame().time, 0.0);
        assert_eq!(session.frame().objects[0].transform.rotation, 2.0);
        assert_eq!(
            session.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(2.0))
        );
        assert_eq!(
            session.wake_state().timeline(),
            TimelineWakeState::Continuous
        );
    }

    #[test]
    fn live_scalar_completion_releases_ownership_and_preserves_history_locally() {
        let mut store = SemanticStore::new();
        let driven =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        let unrelated =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 2.0,
            }));
        store.attach_to_scene(driven).unwrap();
        store.attach_to_scene(unrelated).unwrap();
        let tracker = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let position = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::Constant(SemanticSignalValue::Vec3(
                    SemanticVec3::new(-2.0, 0.0, 0.0),
                ))),
                Box::new(SemanticSignalExpr::Mul(
                    Box::new(SemanticSignalExpr::signal(tracker)),
                    Box::new(SemanticSignalExpr::Constant(SemanticSignalValue::Vec3(
                        SemanticVec3::new(1.0, 0.0, 0.0),
                    ))),
                )),
            ))
            .unwrap();
        store
            .bind_semantic_signal(position, driven, SemanticObjectProperty::Translation)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.take_frame_changes();

        let revision = store.scene_revision();
        let publication = session.publication_context();
        let segment = session
            .declare_and_activate_value_tracker(&mut store, tracker, 2.0, 2.0, RateFunction::Linear)
            .unwrap();
        assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
        assert_eq!(
            session.publication_context().scene_revision(),
            store.scene_revision()
        );
        assert_eq!(
            session.publication_context().execution_revision(),
            publication.execution_revision().checked_next().unwrap()
        );
        assert_eq!(session.signal_timeline.entry_count(), 1);
        assert_eq!(session.signal_timeline.event_count(), 2);
        assert_eq!(
            session.set_reactive_input(tracker, 9.0_f32),
            Err(ExecutionSessionInputError::TimelineOwnedSignal { signal: tracker })
        );

        session.advance_segment_to(segment, 1.0).unwrap();
        assert_eq!(
            session.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(1.0))
        );
        assert_eq!(
            session.frame().objects[0].transform.translation,
            Vec2::new(-1.0, 0.0)
        );
        session.seek(0.5).unwrap();
        assert_eq!(
            session.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(0.5))
        );
        session.advance_segment_to(segment, 2.0).unwrap();
        session.take_frame_changes();
        session.complete_segment(&mut store, segment).unwrap();
        assert_eq!(session.signal_timeline.entry_count(), 2);
        assert_eq!(session.signal_timeline.event_count(), 3);
        assert_eq!(store.semantic_input_scalar_value_at(tracker, 1.0), Ok(1.0));
        assert_eq!(store.semantic_input_scalar_value_at(tracker, 2.0), Ok(2.0));

        let unrelated_before = session.frame().objects[1].clone();
        session
            .set_scalar_signal_value(&mut store, tracker, 3.0)
            .unwrap();
        assert_eq!(
            session.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(3.0))
        );
        assert_eq!(
            session.frame().objects[0].transform.translation,
            Vec2::new(1.0, 0.0)
        );
        assert_eq!(session.frame().objects[1], unrelated_before);
        assert_eq!(session.take_frame_changes().object_indices(), &[0]);

        session.seek(1.0).unwrap();
        assert_eq!(
            session.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(1.0))
        );
        assert_eq!(
            session.set_reactive_input(tracker, 9.0_f32),
            Err(ExecutionSessionInputError::TimelineOwnedSignal { signal: tracker })
        );
        session.advance_to(2.5).unwrap();
        assert_eq!(
            session.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(3.0))
        );
        let second = session
            .declare_and_activate_value_tracker(&mut store, tracker, 5.0, 1.0, RateFunction::Linear)
            .unwrap();
        session.advance_segment_to(second, 3.5).unwrap();
        session.complete_segment(&mut store, second).unwrap();
        assert_eq!(store.semantic_input_scalar_value_at(tracker, 1.0), Ok(1.0));
        assert_eq!(store.semantic_input_scalar_value_at(tracker, 2.5), Ok(3.0));
        assert_eq!(store.semantic_input_scalar_value_at(tracker, 3.0), Ok(4.0));
        assert_eq!(store.semantic_input_scalar_value_at(tracker, 3.5), Ok(5.0));
    }

    #[test]
    fn live_scalar_lowering_failure_leaves_semantic_runtime_and_schedule_unchanged() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let tracker = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_signal(tracker, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let revision = store.scene_revision();
        let publication = session.publication_context();
        let frame = session.frame().clone();

        assert_eq!(
            session.declare_and_activate_value_tracker(
                &mut store,
                tracker,
                f64::MAX,
                1.0,
                RateFunction::Linear,
            ),
            Err(ExecutionSessionAnimationError::PreparedScalarTimeline(
                PreparedScalarSignalTimelineError::Lowering(
                    noon_compile::SemanticReactiveLoweringError::SignalValueOutOfRange {
                        signal: tracker,
                    },
                ),
            )),
        );
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame(), &frame);
        assert_eq!(session.signal_timeline.entry_count(), 0);
        assert_eq!(session.signal_timeline.event_count(), 0);
    }

    #[test]
    fn first_positive_time_hold_preserves_initial_history_and_same_time_track_order() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let tracker = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_signal(tracker, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.advance_to(1.0).unwrap();
        session
            .set_scalar_signal_value(&mut store, tracker, 3.0)
            .unwrap();
        assert_eq!(store.semantic_input_scalar_value_at(tracker, 0.0), Ok(0.0));
        assert_eq!(store.semantic_input_scalar_value_at(tracker, 1.0), Ok(3.0));

        session.seek(0.0).unwrap();
        assert_eq!(
            session.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(0.0))
        );
        session.advance_to(1.0).unwrap();
        assert_eq!(
            session.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(3.0))
        );

        let fresh = ExecutionSession::from_semantic_store(&store).unwrap();
        assert_eq!(fresh.frame().time, 0.0);
        assert_eq!(
            fresh.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(0.0))
        );

        let segment = session
            .declare_and_activate_value_tracker(&mut store, tracker, 5.0, 1.0, RateFunction::Linear)
            .unwrap();
        assert_eq!(store.semantic_input_scalar_value_at(tracker, 1.0), Ok(3.0));
        assert_eq!(
            session.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(3.0))
        );
        session.advance_segment_to(segment, 1.5).unwrap();
        assert_eq!(store.semantic_input_scalar_value_at(tracker, 1.5), Ok(4.0));
        assert_eq!(
            session.effective_signal_value(tracker),
            Some(&ReactiveValue::Scalar(4.0))
        );
    }

    #[test]
    fn one_native_occurrence_updates_shared_closure_atomically() {
        let mut store = SemanticStore::new();
        let source = NativeStateSource::Control {
            name: "shared".to_owned(),
        };
        let first = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let second = store.insert_semantic_input_signal(1.0_f64).unwrap();
        store
            .bind_semantic_native_state_input(first, source.clone())
            .unwrap();
        store
            .bind_semantic_native_state_input(second, source.clone())
            .unwrap();
        let sum = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(first)),
                Box::new(SemanticSignalExpr::signal(second)),
            ))
            .unwrap();
        let square = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Mul(
                Box::new(SemanticSignalExpr::signal(sum)),
                Box::new(SemanticSignalExpr::signal(sum)),
            ))
            .unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(square, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.take_frame_changes();
        let context = session.publication_context();

        session
            .set_native_state_input(source.clone(), NativeInputValue::Scalar(2.0))
            .unwrap();
        assert_eq!(session.frame().objects[0].transform.rotation, 16.0);
        assert_eq!(session.last_reactive_stats().derived_signals_evaluated, 2);
        assert_eq!(
            session.publication_context().frame_epoch(),
            context.frame_epoch().checked_next().unwrap()
        );

        let context = session.publication_context();
        let frame = session.frame().clone();
        assert!(matches!(
            session.set_native_state_input(source.clone(), NativeInputValue::Scalar(1.0e20)),
            Err(ExecutionSessionInputError::Evaluation(
                EvaluationError::Reactive(ReactiveError::NonFiniteValue(_))
            ))
        ));
        assert_eq!(session.publication_context(), context);
        assert_eq!(session.frame(), &frame);

        session
            .set_native_state_input(source, NativeInputValue::Scalar(3.0))
            .unwrap();
        assert_eq!(session.frame().objects[0].transform.rotation, 36.0);
    }

    #[test]
    fn chained_activation_starts_from_current_effective_affine_state() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();

        let mut first_state = store.semantic_object_state_checked(object).unwrap().clone();
        first_state.transform.translation = SemanticVec3::new(4.0, 0.0, 0.0);
        first_state.transform.rotation_z = 0.5;
        first_state.transform.scale = SemanticVec3::new(2.0, 2.0, 1.0);
        let first_state = store.insert_semantic_object(first_state);
        let first = store
            .insert_semantic_transform_animation(object, first_state, AnimationOptions::new())
            .unwrap();

        let mut second_state = store.semantic_object_state_checked(object).unwrap().clone();
        second_state.transform.translation = SemanticVec3::new(10.0, 0.0, 0.0);
        second_state.transform.rotation_z = 1.5;
        second_state.transform.scale = SemanticVec3::new(4.0, 1.0, 1.0);
        let second_state = store.insert_semantic_object(second_state);
        let second = store
            .insert_semantic_transform_animation(object, second_state, AnimationOptions::new())
            .unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let first_segment = session
            .activate_animation_segment(&store, first, linear_second())
            .unwrap();
        session.seek(1.0).unwrap();
        assert_eq!(
            session.frame().objects[0].transform,
            noon_core::Transform2D {
                translation: Vec2::new(4.0, 0.0),
                rotation: 0.5,
                scale: Vec2::new(2.0, 2.0),
            }
        );
        session.complete_segment(&mut store, first_segment).unwrap();

        session
            .activate_animation_segment(&store, second, linear_second())
            .unwrap();
        session.seek(1.5).unwrap();

        assert_eq!(
            session.frame().objects[0].transform,
            noon_core::Transform2D {
                translation: Vec2::new(7.0, 0.0),
                rotation: 1.0,
                scale: Vec2::new(3.0, 1.5),
            }
        );
    }
}
