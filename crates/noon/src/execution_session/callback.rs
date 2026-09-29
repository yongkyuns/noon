use std::collections::{BTreeMap, BTreeSet};

use noon_compile::{CompilePatchError, SemanticHostCallbackEventKind, SemanticHostCallbackPlan};
use noon_core::{
    HostCallbackId, PublicationContext, ReactiveValue, SemanticNodeId, Style, Transform2D,
};
use noon_runtime::{
    EffectiveObjectProperties, EffectivePropertyWrite as RuntimeEffectivePropertyWrite,
    EvaluationError, ExecutionSlotId, FrameState, PreparedFrameCommitError,
    PreparedFrameEvaluation, RuntimeIdentity,
};

use super::{ExecutionEvaluationMode, ExecutionSession};
use noon_runtime::SignalTimelinePreview;

pub(super) const CALLBACK_TRANSLATION: u8 = 1;
pub(super) const CALLBACK_ROTATION: u8 = 2;
pub(super) const CALLBACK_SCALE: u8 = 4;
pub(super) const CALLBACK_FILL: u8 = 8;
pub(super) const CALLBACK_STROKE: u8 = 16;
pub(super) const CALLBACK_STROKE_WIDTH: u8 = 32;
pub(super) const CALLBACK_OPACITY: u8 = 64;

#[derive(Clone, Debug)]
pub(super) struct CallbackPublicationReceipt {
    token: CallbackPhaseToken,
    time: f64,
    publication: PublicationContext,
    domains: BTreeMap<SemanticNodeId, u8>,
}

/// Renderer-facing dirty state for one callback-published runtime row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallbackRendererDirtyClassification {
    All,
    Added,
    Updated,
    Removed,
    Unchanged,
}

/// One small committed runtime observation pinned to an exact callback phase.
///
/// The execution slot is derived from the canonical session's durable slot table.
/// This value carries no mutable runtime authority and is intended only to be paired
/// with renderer preparation/upload/presentation evidence at a real host boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CommittedCallbackRendererObservation {
    token: CallbackPhaseToken,
    publication: PublicationContext,
    target: SemanticNodeId,
    execution_object: noon_core::ObjectId,
    execution_slot: ExecutionSlotId,
    frame_index: usize,
    time: f64,
    transform: Transform2D,
    style: Style,
    presence: bool,
    dirty: CallbackRendererDirtyClassification,
}

impl CommittedCallbackRendererObservation {
    pub const fn token(self) -> CallbackPhaseToken {
        self.token
    }

    pub const fn publication(self) -> PublicationContext {
        self.publication
    }

    pub const fn target(self) -> SemanticNodeId {
        self.target
    }

    pub const fn execution_object(self) -> noon_core::ObjectId {
        self.execution_object
    }

    pub const fn execution_slot(self) -> ExecutionSlotId {
        self.execution_slot
    }

    pub const fn frame_index(self) -> usize {
        self.frame_index
    }

    pub const fn time(self) -> f64 {
        self.time
    }

    pub const fn transform(self) -> Transform2D {
        self.transform
    }

    pub const fn style(self) -> Style {
        self.style
    }

    pub const fn presence(self) -> bool {
        self.presence
    }

    pub const fn dirty(self) -> CallbackRendererDirtyClassification {
        self.dirty
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CallbackRendererObservationOutcome {
    Committed(CommittedCallbackRendererObservation),
    StaleCallback {
        requested: CallbackPhaseToken,
        committed: Option<CallbackPhaseToken>,
    },
    StalePublication {
        requested: PublicationContext,
        applied: PublicationContext,
    },
    Absent {
        token: CallbackPhaseToken,
        target: SemanticNodeId,
    },
}

impl CallbackPublicationReceipt {
    pub(super) fn domains_at(
        &self,
        object: SemanticNodeId,
        time: f64,
        publication: PublicationContext,
    ) -> Option<u8> {
        (self.time == time && self.publication == publication)
            .then(|| self.domains.get(&object).copied())
            .flatten()
    }
}

#[derive(Clone, Debug)]
pub(super) struct CallbackSchedule {
    plan: SemanticHostCallbackPlan,
    processed_through: Option<f64>,
    // Semantic target preorder, then registration order. Derived occurrence IDs
    // remain stable for unrelated targets when a local target is revised.
    active_occurrences: BTreeSet<(usize, usize)>,
    detached_targets: BTreeSet<SemanticNodeId>,
    completed_time: Option<f64>,
    completed_publication: Option<PublicationContext>,
}

#[derive(Clone, Debug)]
struct CallbackSchedulePreview {
    time: f64,
    active_occurrences: BTreeSet<(usize, usize)>,
}

impl CallbackSchedule {
    pub(super) fn new(plan: SemanticHostCallbackPlan) -> Self {
        Self {
            plan,
            processed_through: None,
            active_occurrences: BTreeSet::new(),
            detached_targets: BTreeSet::new(),
            completed_time: None,
            completed_publication: None,
        }
    }

    pub(super) fn plan(&self) -> &SemanticHostCallbackPlan {
        &self.plan
    }

    /// Commit a target-local compiler delta after semantic/runtime preflight.
    /// Only changed targets are reconciled at the current time; no history replay.
    pub(super) fn apply_revision(
        &mut self,
        revision: noon_compile::SemanticHostCallbackRevision,
        time: f64,
    ) {
        let targets = revision.targets().collect::<Vec<_>>();
        for &target in &targets {
            for index in self.plan.target_occurrences(target) {
                self.active_occurrences
                    .remove(&(self.plan.occurrence(index).order(), index));
            }
        }
        self.plan.apply_revision(revision);
        for target in targets {
            for index in self.plan.target_occurrences(target) {
                let occurrence = self.plan.occurrence(index);
                let activation = occurrence.activation();
                if !self.detached_targets.contains(&target)
                    && activation.active_from() <= time
                    && activation.inactive_from().is_none_or(|end| time < end)
                {
                    self.active_occurrences.insert((occurrence.order(), index));
                }
            }
        }
        self.completed_time = None;
        self.completed_publication = None;
    }

    pub(super) fn is_empty(&self) -> bool {
        self.plan.is_empty()
    }

    fn set_target_live(&mut self, target: SemanticNodeId, live: bool, time: f64) {
        if self.plan.target_occurrences(target).next().is_none() {
            return;
        }
        if live {
            self.detached_targets.remove(&target);
        } else {
            self.detached_targets.insert(target);
        }
        for index in self.plan.target_occurrences(target) {
            let occurrence = self.plan.occurrence(index);
            let activation = occurrence.activation();
            let key = (occurrence.order(), index);
            if live
                && activation.active_from() <= time
                && activation.inactive_from().is_none_or(|end| time < end)
            {
                self.active_occurrences.insert(key);
            } else {
                self.active_occurrences.remove(&key);
            }
        }
        self.completed_time = None;
        self.completed_publication = None;
    }

    fn preview(&self, requested: f64, current: f64) -> CallbackSchedulePreview {
        let barrier = self
            .plan
            .next_activation_after(self.processed_through)
            .filter(|&time| time >= current && time <= requested);
        let time = barrier.unwrap_or(requested);
        let mut active_occurrences = self.active_occurrences.clone();
        for event in self
            .plan
            .events_after(self.processed_through)
            .take_while(|event| event.time() <= time)
        {
            let index = event.occurrence_index();
            let key = (self.plan.occurrence(index).order(), index);
            match event.kind() {
                SemanticHostCallbackEventKind::Activate => {
                    if !self
                        .detached_targets
                        .contains(&self.plan.occurrence(index).target())
                    {
                        active_occurrences.insert(key);
                    }
                }
                SemanticHostCallbackEventKind::Deactivate => {
                    active_occurrences.remove(&key);
                }
            }
        }
        CallbackSchedulePreview {
            time,
            active_occurrences,
        }
    }

    fn commit(&mut self, preview: CallbackSchedulePreview, publication: PublicationContext) {
        self.processed_through = Some(preview.time);
        self.active_occurrences = preview.active_occurrences;
        self.completed_time = Some(preview.time);
        self.completed_publication = Some(publication);
    }

    pub(super) fn wake_timeline(&self, current: f64) -> noon_runtime::TimelineWakeState {
        let preview = self.preview(current, current);
        if !preview.active_occurrences.is_empty() && self.completed_time != Some(current) {
            return noon_runtime::TimelineWakeState::Continuous;
        }
        if !self.active_occurrences.is_empty() {
            return noon_runtime::TimelineWakeState::Continuous;
        }
        self.plan
            .next_activation_after(self.processed_through)
            .map_or(
                noon_runtime::TimelineWakeState::Quiescent,
                noon_runtime::TimelineWakeState::Deadline,
            )
    }

    pub(super) fn continues_for_target(&self, target: SemanticNodeId) -> bool {
        self.active_occurrences
            .iter()
            .any(|&(_, index)| self.plan.occurrence(index).target() == target)
    }
    pub(super) fn carry_completed_publication(
        &mut self,
        time: f64,
        publication: PublicationContext,
    ) {
        if self.completed_time == Some(time) {
            self.completed_publication = Some(publication);
        }
    }
}

/// One compiler-selected semantic callback occurrence in authoring order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequiredCallbackInvocation {
    occurrence_index: usize,
    callback_id: HostCallbackId,
    target: SemanticNodeId,
}

impl RequiredCallbackInvocation {
    pub const fn occurrence_index(self) -> usize {
        self.occurrence_index
    }

    pub const fn callback_id(self) -> HostCallbackId {
        self.callback_id
    }

    pub const fn target(self) -> SemanticNodeId {
        self.target
    }
}

/// Result of advancing through the canonical callback publication barrier.
#[derive(Debug)]
pub enum CallbackAdvance<'a> {
    Ready(&'a FrameState),
    HostRequired {
        invocations: Vec<RequiredCallbackInvocation>,
        overlay: CallbackPhaseOverlay,
    },
}

/// One arbitrary semantic read requested by a callback already pinned to a phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallbackReadRequest {
    ScalarSignal(SemanticNodeId),
    Object(SemanticNodeId),
}

/// Owned result from the unpublished prepared callback evaluation.
#[derive(Clone, Debug, PartialEq)]
pub enum CallbackReadValue {
    Scalar(f32),
    Object(EffectiveObjectProperties),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionCallbackReadError {
    NoPendingPhase,
    StaleToken {
        expected: CallbackPhaseToken,
        actual: CallbackPhaseToken,
    },
    UnknownSignal(SemanticNodeId),
    NonScalarSignal(SemanticNodeId),
    UnknownObject(SemanticNodeId),
}

impl std::fmt::Display for ExecutionSessionCallbackReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPendingPhase => formatter.write_str("no required callback phase is pending"),
            Self::StaleToken { expected, actual } => write!(
                formatter,
                "callback read sequence {} does not match pending sequence {}",
                actual.sequence().get(),
                expected.sequence().get()
            ),
            Self::UnknownSignal(signal) => write!(
                formatter,
                "semantic signal {}:{} is not live in this callback phase",
                signal.slot(),
                signal.generation()
            ),
            Self::NonScalarSignal(signal) => write!(
                formatter,
                "semantic signal {}:{} is not scalar",
                signal.slot(),
                signal.generation()
            ),
            Self::UnknownObject(object) => write!(
                formatter,
                "semantic object {}:{} is not live in this callback phase",
                object.slot(),
                object.generation()
            ),
        }
    }
}

impl std::error::Error for ExecutionSessionCallbackReadError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallbackTerminationKind {
    Failed,
    Interrupted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallbackTermination {
    token: CallbackPhaseToken,
    kind: CallbackTerminationKind,
}

impl CallbackTermination {
    pub const fn token(self) -> CallbackPhaseToken {
        self.token
    }

    pub const fn kind(self) -> CallbackTerminationKind {
        self.kind
    }

    pub(super) const fn interrupted_clone(
        pending: CallbackPhaseToken,
        runtime: RuntimeIdentity,
    ) -> Self {
        Self {
            token: CallbackPhaseToken::new(runtime, pending.publication(), pending.sequence()),
            kind: CallbackTerminationKind::Interrupted,
        }
    }
}

/// Ordered callback request sequence. This clock is independent from authored,
/// execution, and frame revisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallbackSequence(u64);

impl CallbackSequence {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Exact runtime incarnation, coherent publication, and request sequence observed
/// by one callback phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallbackPhaseToken {
    runtime: RuntimeIdentity,
    publication: PublicationContext,
    sequence: CallbackSequence,
}

impl CallbackPhaseToken {
    pub const fn new(
        runtime: RuntimeIdentity,
        publication: PublicationContext,
        sequence: CallbackSequence,
    ) -> Self {
        Self {
            runtime,
            publication,
            sequence,
        }
    }

    pub const fn runtime(self) -> RuntimeIdentity {
        self.runtime
    }

    pub const fn publication(self) -> PublicationContext {
        self.publication
    }

    pub const fn sequence(self) -> CallbackSequence {
        self.sequence
    }
}

/// The runtime effective-write vocabulary addressed by semantic identity.
/// Identity resolves only when the pinned callback result is prepared.
pub type EffectiveSemanticPropertyWrite = RuntimeEffectivePropertyWrite<SemanticNodeId>;

fn callback_write_domains(write: EffectiveSemanticPropertyWrite) -> u8 {
    match write {
        EffectiveSemanticPropertyWrite::Transform { .. } => {
            CALLBACK_TRANSLATION | CALLBACK_ROTATION | CALLBACK_SCALE
        }
        EffectiveSemanticPropertyWrite::Style { .. } => {
            CALLBACK_FILL | CALLBACK_STROKE | CALLBACK_STROKE_WIDTH | CALLBACK_OPACITY
        }
        EffectiveSemanticPropertyWrite::Translation { .. } => CALLBACK_TRANSLATION,
        EffectiveSemanticPropertyWrite::Rotation { .. } => CALLBACK_ROTATION,
        EffectiveSemanticPropertyWrite::Scale { .. } => CALLBACK_SCALE,
        EffectiveSemanticPropertyWrite::Fill { .. } => CALLBACK_FILL,
        EffectiveSemanticPropertyWrite::Stroke { .. } => CALLBACK_STROKE,
        EffectiveSemanticPropertyWrite::StrokeWidth { .. } => CALLBACK_STROKE_WIDTH,
        EffectiveSemanticPropertyWrite::Opacity { .. } => CALLBACK_OPACITY,
    }
}

/// Final ordered effective writes for one exact required callback phase.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectivePropertyBatch {
    token: CallbackPhaseToken,
    writes: Vec<EffectiveSemanticPropertyWrite>,
}

impl EffectivePropertyBatch {
    pub fn new(
        token: CallbackPhaseToken,
        writes: impl IntoIterator<Item = EffectiveSemanticPropertyWrite>,
    ) -> Self {
        Self {
            token,
            writes: writes.into_iter().collect(),
        }
    }

    pub const fn token(&self) -> CallbackPhaseToken {
        self.token
    }

    pub fn writes(&self) -> &[EffectiveSemanticPropertyWrite] {
        &self.writes
    }
}

/// Owned sparse callback read view and ordered write overlay.
///
/// Reads see the last write performed by an earlier callback in this phase. The
/// base snapshot already includes the unpublished timeline/native evaluation.
#[derive(Clone, Debug)]
pub struct CallbackPhaseOverlay {
    token: CallbackPhaseToken,
    time: f64,
    delta_time: f64,
    objects: BTreeMap<SemanticNodeId, EffectiveObjectProperties>,
    writes: Vec<EffectiveSemanticPropertyWrite>,
    staged_rows: usize,
    prior_driver_rows: usize,
}

impl CallbackPhaseOverlay {
    pub const fn token(&self) -> CallbackPhaseToken {
        self.token
    }

    pub const fn time(&self) -> f64 {
        self.time
    }

    pub const fn delta_time(&self) -> f64 {
        self.delta_time
    }

    pub fn object(&self, object: SemanticNodeId) -> Option<&EffectiveObjectProperties> {
        self.objects.get(&object)
    }

    pub fn objects(
        &self,
    ) -> impl Iterator<Item = (SemanticNodeId, &EffectiveObjectProperties)> + '_ {
        self.objects.iter().map(|(&object, state)| (object, state))
    }

    pub(crate) fn cache_read_object(
        &mut self,
        object: SemanticNodeId,
        value: EffectiveObjectProperties,
    ) {
        self.objects.entry(object).or_insert(value);
    }

    pub const fn staged_row_count(&self) -> usize {
        self.staged_rows
    }

    pub const fn prior_driver_row_count(&self) -> usize {
        self.prior_driver_rows
    }

    pub fn set_transform(
        &mut self,
        object: SemanticNodeId,
        transform: Transform2D,
    ) -> Result<(), ExecutionSessionCallbackError> {
        self.write(EffectiveSemanticPropertyWrite::Transform { object, transform })
    }

    pub fn set_style(
        &mut self,
        object: SemanticNodeId,
        style: Style,
    ) -> Result<(), ExecutionSessionCallbackError> {
        self.write(EffectiveSemanticPropertyWrite::Style { object, style })
    }

    /// Stage one scoped effective write. Later reads in this invocation observe
    /// its value, while commit validates the complete ordered batch atomically.
    pub fn write(
        &mut self,
        write: EffectiveSemanticPropertyWrite,
    ) -> Result<(), ExecutionSessionCallbackError> {
        let object = write.object();
        let current = self
            .objects
            .get_mut(&object)
            .ok_or(ExecutionSessionCallbackError::UnknownObject(object))?;
        let mut transform = current.transform;
        let mut style = current.style;
        match write {
            EffectiveSemanticPropertyWrite::Transform {
                transform: value, ..
            } => transform = value,
            EffectiveSemanticPropertyWrite::Style { style: value, .. } => style = value,
            EffectiveSemanticPropertyWrite::Translation { translation, .. } => {
                transform.translation = translation
            }
            EffectiveSemanticPropertyWrite::Rotation { rotation, .. } => {
                transform.rotation = rotation
            }
            EffectiveSemanticPropertyWrite::Scale { scale, .. } => transform.scale = scale,
            EffectiveSemanticPropertyWrite::Fill { fill, .. } => style.fill = fill,
            EffectiveSemanticPropertyWrite::Stroke { stroke, .. } => style.stroke = stroke,
            EffectiveSemanticPropertyWrite::StrokeWidth { stroke_width, .. } => {
                style.stroke_width = stroke_width
            }
            EffectiveSemanticPropertyWrite::Opacity { opacity, .. } => style.opacity = opacity,
        }
        current.set_transform(transform);
        current.set_style(style);
        self.writes.push(write);
        Ok(())
    }

    pub fn finish(self) -> EffectivePropertyBatch {
        EffectivePropertyBatch::new(self.token, self.writes)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionCallbackError {
    Pending(CallbackPhaseToken),
    NoPendingPhase,
    Terminated(CallbackTermination),
    StaleToken {
        expected: CallbackPhaseToken,
        actual: CallbackPhaseToken,
    },
    SequenceExhausted,
    NonMonotonicAdvance {
        current: f64,
        requested: f64,
    },
    UnsupportedCallbackTarget(SemanticNodeId),
    UnknownObject(SemanticNodeId),
    Read(ExecutionSessionCallbackReadError),
    Evaluation(EvaluationError),
    InvalidEffectiveWrite(CompilePatchError),
    Commit(PreparedFrameCommitError),
    Publication(super::ExecutionSessionPublicationError),
}

impl std::fmt::Display for ExecutionSessionCallbackError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending(token) => write!(
                formatter,
                "required callback sequence {} is still pending",
                token.sequence().get()
            ),
            Self::NoPendingPhase => formatter.write_str("no required callback phase is pending"),
            Self::Terminated(termination) => write!(
                formatter,
                "callback progression terminated as {:?} at sequence {}",
                termination.kind(),
                termination.token().sequence().get()
            ),
            Self::StaleToken { expected, actual } => write!(
                formatter,
                "callback result sequence {} does not match pending sequence {}",
                actual.sequence().get(),
                expected.sequence().get()
            ),
            Self::SequenceExhausted => formatter.write_str("callback sequence space exhausted"),
            Self::NonMonotonicAdvance { current, requested } => write!(
                formatter,
                "callback-aware advance cannot move backward from {current} to {requested}"
            ),
            Self::UnsupportedCallbackTarget(target) => write!(
                formatter,
                "callback target {}:{} is not an execution object",
                target.slot(),
                target.generation()
            ),
            Self::UnknownObject(object) => write!(
                formatter,
                "semantic object {}:{} is not live in this callback phase",
                object.slot(),
                object.generation()
            ),
            Self::Read(error) => error.fmt(formatter),
            Self::Evaluation(error) => error.fmt(formatter),
            Self::InvalidEffectiveWrite(error) => error.fmt(formatter),
            Self::Commit(error) => error.fmt(formatter),
            Self::Publication(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecutionSessionCallbackError {}

impl From<EvaluationError> for ExecutionSessionCallbackError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

impl From<CompilePatchError> for ExecutionSessionCallbackError {
    fn from(value: CompilePatchError) -> Self {
        Self::InvalidEffectiveWrite(value)
    }
}

impl From<PreparedFrameCommitError> for ExecutionSessionCallbackError {
    fn from(value: PreparedFrameCommitError) -> Self {
        Self::Commit(value)
    }
}

impl From<ExecutionSessionCallbackReadError> for ExecutionSessionCallbackError {
    fn from(value: ExecutionSessionCallbackReadError) -> Self {
        Self::Read(value)
    }
}

#[derive(Clone, Debug)]
pub(super) struct PendingCallbackPhase {
    token: CallbackPhaseToken,
    pub(super) prepared: PreparedFrameEvaluation,
    schedule: Option<CallbackSchedulePreview>,
    signal_timeline: Option<SignalTimelinePreview>,
}

pub(super) struct CallbackCompletion {
    token: CallbackPhaseToken,
    time: f64,
    schedule: Option<CallbackSchedulePreview>,
    signal_timeline: Option<SignalTimelinePreview>,
}

impl PendingCallbackPhase {
    pub(super) fn into_parts(self) -> (PreparedFrameEvaluation, CallbackCompletion) {
        let completion = CallbackCompletion {
            token: self.token,
            time: self.prepared.time(),
            schedule: self.schedule,
            signal_timeline: self.signal_timeline,
        };
        (self.prepared, completion)
    }

    pub(super) fn interrupted_clone(&self, runtime: RuntimeIdentity) -> CallbackTermination {
        CallbackTermination::interrupted_clone(self.token, runtime)
    }
}

impl ExecutionSession {
    /// Read through the exact unpublished evaluation pinned by `token`.
    /// This does not advance, commit, or mutate callback/runtime state.
    /// Read unique family leaves through the existing pinned object read path.
    pub fn required_callback_family_read(
        &self,
        store: &noon_core::SemanticStore,
        token: CallbackPhaseToken,
        family: SemanticNodeId,
    ) -> Result<Vec<(SemanticNodeId, EffectiveObjectProperties)>, crate::FamilyCallbackPaintError>
    {
        use crate::FamilyCallbackPaintError as Error;
        let pending = self.pending_callback.as_ref().ok_or(Error::Callback(
            ExecutionSessionCallbackError::NoPendingPhase,
        ))?;
        if pending.token != token {
            return Err(Error::Callback(ExecutionSessionCallbackError::StaleToken {
                expected: pending.token,
                actual: token,
            }));
        }
        if store.identity() != self.store_identity {
            return Err(Error::Authoring(crate::AuthoringError::ForeignStore));
        }
        if store.scene_revision() != token.publication().scene_revision() {
            return Err(Error::StaleRevision {
                expected: token.publication().scene_revision(),
                actual: store.scene_revision(),
            });
        }
        store
            .semantic_family_checked(family)
            .map_err(|e| Error::Authoring(e.into()))?;
        store
            .ordered_leaf_nodes(family)
            .map_err(Error::Store)?
            .into_iter()
            .map(|node| {
                match self
                    .required_callback_read(token, CallbackReadRequest::Object(node))
                    .map_err(|e| Error::Callback(e.into()))?
                {
                    CallbackReadValue::Object(properties) => Ok((node, properties)),
                    CallbackReadValue::Scalar(_) => unreachable!("object request returns object"),
                }
            })
            .collect()
    }

    pub fn required_callback_read(
        &self,
        token: CallbackPhaseToken,
        request: CallbackReadRequest,
    ) -> Result<CallbackReadValue, ExecutionSessionCallbackReadError> {
        let pending = self
            .pending_callback
            .as_ref()
            .ok_or(ExecutionSessionCallbackReadError::NoPendingPhase)?;
        if pending.token != token {
            return Err(ExecutionSessionCallbackReadError::StaleToken {
                expected: pending.token,
                actual: token,
            });
        }
        match request {
            CallbackReadRequest::ScalarSignal(semantic) => {
                let execution = self
                    .reactive_projection
                    .execution_signal_id(semantic)
                    .ok_or(ExecutionSessionCallbackReadError::UnknownSignal(semantic))?;
                match self
                    .runtime
                    .prepared_reactive_value(&pending.prepared, execution)
                {
                    Some(ReactiveValue::Scalar(value)) => Ok(CallbackReadValue::Scalar(value)),
                    Some(_) => Err(ExecutionSessionCallbackReadError::NonScalarSignal(semantic)),
                    None => Err(ExecutionSessionCallbackReadError::UnknownSignal(semantic)),
                }
            }
            CallbackReadRequest::Object(semantic) => {
                let execution = self
                    .execution_index
                    .execution_object_id(semantic)
                    .ok_or(ExecutionSessionCallbackReadError::UnknownObject(semantic))?;
                let object_index = self
                    .runtime
                    .frame_index_for_object(execution)
                    .filter(|index| self.runtime.object_slot_is_live(*index))
                    .ok_or(ExecutionSessionCallbackReadError::UnknownObject(semantic))?;
                let slot = self
                    .slots
                    .slot_for_object(execution)
                    .ok_or(ExecutionSessionCallbackReadError::UnknownObject(semantic))?;
                self.runtime
                    .prepared_properties_at(
                        &pending.prepared,
                        object_index,
                        self.spatial_index.bounds_for_slot(slot),
                    )
                    .map(CallbackReadValue::Object)
                    .ok_or(ExecutionSessionCallbackReadError::UnknownObject(semantic))
            }
        }
    }

    pub(crate) fn callback_progression_is_coherent_at(&self, time: f64) -> bool {
        self.callback_termination.is_none()
            && self.pending_callback.is_none()
            && (self.callback_schedule.is_empty()
                || (self.callback_schedule.completed_time == Some(time)
                    && self.callback_schedule.completed_publication
                        == Some(self.publication_context())))
            && (self.signal_timeline.is_empty()
                || self
                    .signal_timeline
                    .is_coherent_at(self.runtime.frame().time, time))
    }

    pub(crate) fn callback_progression_is_terminal(&self) -> bool {
        self.callback_termination.is_some()
    }

    pub fn has_required_callbacks(&self) -> bool {
        !self.callback_schedule.is_empty()
    }

    /// Resolve one callback-published target through the session's indexed
    /// semantic/execution/slot mappings.
    ///
    /// The exact phase token and resulting publication must still be current.
    /// This method borrows only one lightweight runtime row; it neither consumes
    /// renderer changes nor copies object content.
    pub fn committed_callback_renderer_observation(
        &self,
        token: CallbackPhaseToken,
        target: SemanticNodeId,
    ) -> CallbackRendererObservationOutcome {
        let Some(receipt) = self.last_callback_receipt.as_ref() else {
            return CallbackRendererObservationOutcome::StaleCallback {
                requested: token,
                committed: None,
            };
        };
        if receipt.token != token || token.runtime() != self.runtime.runtime_identity() {
            return CallbackRendererObservationOutcome::StaleCallback {
                requested: token,
                committed: Some(receipt.token),
            };
        }
        let publication = self.publication_context();
        if receipt.publication != publication {
            return CallbackRendererObservationOutcome::StalePublication {
                requested: receipt.publication,
                applied: publication,
            };
        }
        let Some(execution_object) = self.execution_index.execution_object_id(target) else {
            return CallbackRendererObservationOutcome::Absent { token, target };
        };
        let Some(frame_index) = self
            .runtime
            .frame_index_for_object(execution_object)
            .filter(|index| self.runtime.object_slot_is_live(*index))
        else {
            return CallbackRendererObservationOutcome::Absent { token, target };
        };
        let Some(execution_slot) = self.slots.slot_for_object(execution_object) else {
            return CallbackRendererObservationOutcome::Absent { token, target };
        };
        let frame = self.runtime.frame();
        let Some(object) = frame.objects.get(frame_index) else {
            return CallbackRendererObservationOutcome::Absent { token, target };
        };
        let changes = self.runtime.frame_changes();
        let dirty = if changes.is_all() {
            CallbackRendererDirtyClassification::All
        } else if changes
            .removed_indices()
            .binary_search(&frame_index)
            .is_ok()
            || !frame.is_present(frame_index)
        {
            CallbackRendererDirtyClassification::Removed
        } else if changes.added_indices().binary_search(&frame_index).is_ok() {
            CallbackRendererDirtyClassification::Added
        } else if changes.object_indices().binary_search(&frame_index).is_ok() {
            CallbackRendererDirtyClassification::Updated
        } else {
            CallbackRendererDirtyClassification::Unchanged
        };
        CallbackRendererObservationOutcome::Committed(CommittedCallbackRendererObservation {
            token,
            publication,
            target,
            execution_object,
            execution_slot,
            frame_index,
            time: frame.time,
            transform: object.transform,
            style: object.style,
            presence: frame.is_present(frame_index),
            dirty,
        })
    }

    /// Advance through the compiler-owned callback schedule. A large time jump
    /// stops at the first newly active occurrence boundary so a bounded updater
    /// interval cannot be skipped by host tick coalescing.
    pub fn advance_to_callback_barrier(
        &mut self,
        time: f64,
    ) -> Result<CallbackAdvance<'_>, ExecutionSessionCallbackError> {
        if let Some(pending) = &self.pending_callback {
            return Err(ExecutionSessionCallbackError::Pending(pending.token));
        }
        if let Some(termination) = self.callback_termination {
            return Err(ExecutionSessionCallbackError::Terminated(termination));
        }
        if !time.is_finite() {
            return Err(EvaluationError::InvalidTime(time).into());
        }
        if self.callback_schedule.is_empty() {
            self.evaluate_signal_timeline(time, ExecutionEvaluationMode::Advance)?;
            return Ok(CallbackAdvance::Ready(self.runtime.frame()));
        }
        let current = self.frame().time;
        if time < current {
            return Err(ExecutionSessionCallbackError::NonMonotonicAdvance {
                current,
                requested: time,
            });
        }
        if self.callback_schedule.completed_time == Some(time)
            && self.callback_schedule.completed_publication == Some(self.publication_context())
            && (self.signal_timeline.is_empty()
                || self
                    .signal_timeline
                    .is_coherent_at(self.runtime.frame().time, time))
        {
            return Ok(CallbackAdvance::Ready(self.runtime.frame()));
        }

        let preview = self.callback_schedule.preview(time, current);
        let invocations = preview
            .active_occurrences
            .iter()
            .map(|&(_, occurrence_index)| {
                let occurrence = self.callback_schedule.plan.occurrence(occurrence_index);
                RequiredCallbackInvocation {
                    occurrence_index,
                    callback_id: occurrence.callback_id(),
                    target: occurrence.target(),
                }
            })
            .collect::<Vec<_>>();
        if invocations.is_empty() {
            self.evaluate_signal_timeline(preview.time, ExecutionEvaluationMode::Advance)?;
            self.callback_schedule
                .commit(preview, self.runtime.publication_context());
            return Ok(CallbackAdvance::Ready(self.runtime.frame()));
        }

        let mut read_objects = Vec::with_capacity(invocations.len());
        let mut seen_read_objects = BTreeSet::new();
        for invocation in &invocations {
            if self
                .execution_index
                .execution_object_id(invocation.target)
                .is_none()
            {
                return Err(ExecutionSessionCallbackError::UnsupportedCallbackTarget(
                    invocation.target,
                ));
            }
            if seen_read_objects.insert(invocation.target) {
                read_objects.push(invocation.target);
            }
        }
        let overlay = self.begin_required_callback_phase_with_schedule(
            preview.time,
            read_objects,
            Some(preview),
        )?;
        Ok(CallbackAdvance::HostRequired {
            invocations,
            overlay,
        })
    }

    /// Stage one forward timeline/native phase and return an owned sparse callback
    /// read/write overlay. The public frame and renderer publication remain pinned
    /// until the matching effective batch commits.
    pub fn begin_required_callback_phase(
        &mut self,
        time: f64,
        read_objects: impl IntoIterator<Item = SemanticNodeId>,
    ) -> Result<CallbackPhaseOverlay, ExecutionSessionCallbackError> {
        self.begin_required_callback_phase_with_schedule(time, read_objects, None)
    }

    fn begin_required_callback_phase_with_schedule(
        &mut self,
        time: f64,
        read_objects: impl IntoIterator<Item = SemanticNodeId>,
        schedule: Option<CallbackSchedulePreview>,
    ) -> Result<CallbackPhaseOverlay, ExecutionSessionCallbackError> {
        if self.runtime.replay_is_sealed() {
            return Err(CompilePatchError::ReplaySealed.into());
        }
        if let Some(pending) = &self.pending_callback {
            return Err(ExecutionSessionCallbackError::Pending(pending.token));
        }
        if let Some(termination) = self.callback_termination {
            return Err(ExecutionSessionCallbackError::Terminated(termination));
        }
        self.sync_spatial_index();
        let sequence = self
            .next_callback_sequence
            .ok_or(ExecutionSessionCallbackError::SequenceExhausted)?;
        let signal_timeline = (!self.signal_timeline.is_empty()
            && !self
                .signal_timeline
                .is_coherent_at(self.runtime.frame().time, time))
        .then(|| {
            self.signal_timeline
                .preview(self.runtime.frame().time, time)
        });
        let prepared = self.runtime.prepare_advance_to_with_reactive_inputs(
            time,
            signal_timeline
                .as_ref()
                .map_or(&[], SignalTimelinePreview::inputs),
        )?;
        let mut objects = BTreeMap::new();
        for semantic in read_objects {
            let execution = self
                .execution_index
                .execution_object_id(semantic)
                .ok_or(ExecutionSessionCallbackError::UnknownObject(semantic))?;
            let object_index = self
                .runtime
                .frame_index_for_object(execution)
                .filter(|index| self.runtime.object_slot_is_live(*index))
                .ok_or(ExecutionSessionCallbackError::UnknownObject(semantic))?;
            let slot = self
                .slots
                .slot_for_object(execution)
                .expect("live execution object must retain its execution slot");
            let cached_bounds = self.spatial_index.bounds_for_slot(slot);
            let object = self
                .runtime
                .prepared_properties_at(&prepared, object_index, cached_bounds)
                .expect("live execution object must expose effective properties");
            objects.insert(semantic, object);
        }

        let token = CallbackPhaseToken::new(
            self.runtime.runtime_identity(),
            self.publication_context(),
            CallbackSequence::new(sequence),
        );
        let staged_rows = prepared.staged_row_count();
        let prior_driver_rows = prepared.prior_driver_rows();
        let delta_time = time - self.frame().time;
        self.next_callback_sequence = sequence.checked_add(1);
        self.runtime.invalidate_replay_domain();
        self.pending_callback = Some(PendingCallbackPhase {
            token,
            prepared,
            schedule,
            signal_timeline,
        });
        Ok(CallbackPhaseOverlay {
            token,
            time,
            delta_time,
            objects,
            writes: Vec::new(),
            staged_rows,
            prior_driver_rows,
        })
    }

    pub fn pending_callback_token(&self) -> Option<CallbackPhaseToken> {
        self.pending_callback.as_ref().map(|pending| pending.token)
    }

    /// Validate and atomically publish the exact pending phase. Invalid or stale
    /// results leave the phase pending and the coherent runtime unchanged.
    pub fn commit_required_callback_phase(
        &mut self,
        batch: EffectivePropertyBatch,
    ) -> Result<&FrameState, ExecutionSessionCallbackError> {
        let (effective, receipt_domains) = self.prepare_callback_writes(batch)?;
        let pending = self
            .pending_callback
            .take()
            .expect("pending phase remained live throughout preflight");
        let (frame, completion) = pending.into_parts();
        self.runtime
            .commit_prepared_frame(frame, effective)
            .expect("preflighted callback phase cannot stale before synchronous commit");
        self.finish_callback_publication(completion, receipt_domains);
        Ok(self.runtime.frame())
    }

    fn prepare_callback_writes(
        &self,
        batch: EffectivePropertyBatch,
    ) -> Result<
        (
            noon_runtime::PreparedEffectivePropertyBatch,
            BTreeMap<SemanticNodeId, u8>,
        ),
        ExecutionSessionCallbackError,
    > {
        let pending = self
            .pending_callback
            .as_ref()
            .ok_or(ExecutionSessionCallbackError::NoPendingPhase)?;
        if batch.token != pending.token {
            return Err(ExecutionSessionCallbackError::StaleToken {
                expected: pending.token,
                actual: batch.token,
            });
        }
        let mut receipt_domains = BTreeMap::new();
        let mut writes = Vec::with_capacity(batch.writes.len());
        for write in batch.writes {
            let semantic = write.object();
            let object = self
                .execution_index
                .execution_object_id(semantic)
                .ok_or(ExecutionSessionCallbackError::UnknownObject(semantic))?;
            *receipt_domains.entry(semantic).or_insert(0) |= callback_write_domains(write);
            let runtime_write = write.map_object(|_| object);
            writes.push(runtime_write);
        }
        let effective = self.runtime.prepare_effective_property_batch(&writes)?;
        self.runtime
            .preflight_prepared_frame_commit(&pending.prepared, &effective)?;

        Ok((effective, receipt_domains))
    }

    pub(super) fn reconcile_callback_membership(
        &mut self,
        objects: &[noon_core::ObjectId],
        live: bool,
    ) {
        for &object in objects {
            if let Some(target) = self.execution_index.semantic_object_id(object) {
                self.callback_schedule
                    .set_target_live(target, live, self.frame().time);
                if !live {
                    if let Some(receipt) = self.last_callback_receipt.as_mut() {
                        receipt.domains.remove(&target);
                    }
                }
            }
        }
    }

    pub(super) fn commit_callback_progress(&mut self, completion: &mut CallbackCompletion) {
        if let Some(signal_timeline) = completion.signal_timeline.take() {
            self.signal_timeline.commit(signal_timeline);
        }
        if let Some(schedule) = completion.schedule.take() {
            self.callback_schedule
                .commit(schedule, self.runtime.publication_context());
        }
    }

    fn finish_callback_publication(
        &mut self,
        mut completion: CallbackCompletion,
        mut receipt_domains: BTreeMap<SemanticNodeId, u8>,
    ) {
        self.commit_callback_progress(&mut completion);
        if !self.callback_schedule.is_empty() {
            if let Some(previous) = self.last_callback_receipt.as_ref() {
                for (&object, &domains) in &previous.domains {
                    if self.callback_schedule.continues_for_target(object) {
                        *receipt_domains.entry(object).or_insert(0) |= domains;
                    }
                }
            }
        }
        receipt_domains
            .retain(|object, _| self.execution_index.execution_object_id(*object).is_some());
        self.last_callback_receipt = Some(CallbackPublicationReceipt {
            token: completion.token,
            time: completion.time,
            publication: self.runtime.publication_context(),
            domains: receipt_domains,
        });
    }

    /// Publish a required host phase and a shared semantic transaction atomically.
    /// Prepared transaction-local handles/read views use the ordinary semantic
    /// allocator. Failed semantic/lowering preflight leaves this phase retryable.
    /// Active animation segment completion retains its existing publication gate.
    pub fn commit_required_callback_transaction(
        &mut self,
        store: &mut noon_core::SemanticStore,
        batch: EffectivePropertyBatch,
        transaction: noon_core::SemanticMutationTransaction,
    ) -> Result<noon_core::SemanticMutationTransactionResult, ExecutionSessionCallbackError> {
        let prepared = transaction.prepare(store).map_err(|error| {
            ExecutionSessionCallbackError::Publication(
                super::ExecutionSessionPublicationError::Semantic(error),
            )
        })?;
        self.commit_prepared_required_callback_transaction(batch, prepared, None)
    }

    /// Commit after inspecting provisional objects through the existing semantic
    /// transaction read view. No provisional node becomes globally visible until
    /// the callback token, lowering and runtime publication have all validated.
    pub fn commit_prepared_required_callback_transaction(
        &mut self,
        batch: EffectivePropertyBatch,
        prepared: noon_core::PreparedSemanticMutationTransaction<'_>,
        order_root: Option<SemanticNodeId>,
    ) -> Result<noon_core::SemanticMutationTransactionResult, ExecutionSessionCallbackError> {
        let token = batch.token;
        let (effective, domains) = self.prepare_callback_writes(batch)?;
        let (result, completion) = self
            .apply_prepared_semantic_transaction_with_execution_contract(
                prepared,
                Vec::new(),
                Some(effective).into(),
                super::publication::SemanticPublicationPurpose::Callback(token),
                None,
                order_root,
            )
            .map_err(ExecutionSessionCallbackError::Publication)?;
        self.finish_callback_publication(
            completion.expect("callback publication consumed the exact pending phase"),
            domains,
        );
        Ok(result)
    }

    /// Discard one exact pending phase without changing coherent runtime state.
    pub fn fail_required_callback_phase(
        &mut self,
        token: CallbackPhaseToken,
    ) -> Result<(), ExecutionSessionCallbackError> {
        self.terminate_required_callback_phase(token, CallbackTerminationKind::Failed)
    }

    pub fn interrupt_required_callback_phase(
        &mut self,
        token: CallbackPhaseToken,
    ) -> Result<(), ExecutionSessionCallbackError> {
        self.terminate_required_callback_phase(token, CallbackTerminationKind::Interrupted)
    }

    fn terminate_required_callback_phase(
        &mut self,
        token: CallbackPhaseToken,
        kind: CallbackTerminationKind,
    ) -> Result<(), ExecutionSessionCallbackError> {
        let pending = self
            .pending_callback
            .as_ref()
            .ok_or(ExecutionSessionCallbackError::NoPendingPhase)?;
        if token != pending.token {
            return Err(ExecutionSessionCallbackError::StaleToken {
                expected: pending.token,
                actual: token,
            });
        }
        self.pending_callback = None;
        self.callback_termination = Some(CallbackTermination { token, kind });
        Ok(())
    }

    pub const fn callback_termination(&self) -> Option<CallbackTermination> {
        self.callback_termination
    }
}

#[cfg(test)]
mod tests {
    use crate::{ExecutionSessionInputError, Scene};
    use noon_core::{
        HostCallbackId, NativeEventOccurrence, NativeEventSource, NativeInputValue,
        NativeStateSource, SemanticMutationTransaction, SemanticObjectProperty,
        SemanticObjectState, SemanticStore, StoredGeometry, Vec2,
    };
    use noon_runtime::TimelineWakeState;

    use super::*;

    #[test]
    fn token_pinned_callback_reads_prepared_scalar_and_arbitrary_object_without_commit() {
        let mut scene = Scene::new();
        let active = scene.circle(0.5).unwrap();
        let anchor = scene.circle(0.25).unwrap();
        scene.add(&active).unwrap();
        scene.add(&anchor).unwrap();
        let tracker = scene.value_tracker(0.0).unwrap();
        scene
            .play_value(&tracker, 2.0)
            .rate_func(noon_core::RateFunction::Linear)
            .run_time(1.0)
            .unwrap();
        let mut hold = SemanticMutationTransaction::new();
        hold.set_scalar_signal_at(tracker.node_id(), 3.0, 1.0);
        hold.apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let mut callbacks = SemanticMutationTransaction::new();
        callbacks.add_updater(active.node_id(), HostCallbackId::new(7), 0.0, None);
        callbacks
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let mut session = scene.execution_session().unwrap();

        let CallbackAdvance::HostRequired { overlay, .. } =
            session.advance_to_callback_barrier(0.0).unwrap()
        else {
            panic!("callback must run at authored time zero")
        };
        let token = overlay.token();
        assert_eq!(
            session
                .required_callback_read(token, CallbackReadRequest::ScalarSignal(tracker.node_id()))
                .unwrap(),
            CallbackReadValue::Scalar(0.0)
        );
        assert!(matches!(
            session
                .required_callback_read(token, CallbackReadRequest::Object(anchor.node_id()))
                .unwrap(),
            CallbackReadValue::Object(_)
        ));
        session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();

        let CallbackAdvance::HostRequired { overlay, .. } =
            session.advance_to_callback_barrier(0.5).unwrap()
        else {
            panic!("active callback must run at the requested midpoint")
        };
        assert_eq!(
            session
                .required_callback_read(
                    overlay.token(),
                    CallbackReadRequest::ScalarSignal(tracker.node_id()),
                )
                .unwrap(),
            CallbackReadValue::Scalar(1.0)
        );
        let stale = CallbackPhaseToken::new(
            overlay.token().runtime(),
            overlay.token().publication(),
            CallbackSequence::new(999),
        );
        assert!(matches!(
            session.required_callback_read(stale, CallbackReadRequest::Object(anchor.node_id())),
            Err(ExecutionSessionCallbackReadError::StaleToken { .. })
        ));
        session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();
        let CallbackAdvance::HostRequired { overlay, .. } =
            session.advance_to_callback_barrier(1.25).unwrap()
        else {
            panic!("active callback must observe the post-track Hold during wait")
        };
        assert_eq!(
            session
                .required_callback_read(
                    overlay.token(),
                    CallbackReadRequest::ScalarSignal(tracker.node_id()),
                )
                .unwrap(),
            CallbackReadValue::Scalar(3.0)
        );
        session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();
        assert!(matches!(
            session.required_callback_read(token, CallbackReadRequest::Object(anchor.node_id())),
            Err(ExecutionSessionCallbackReadError::NoPendingPhase)
        ));
    }

    #[test]
    fn callback_entrypoint_preserves_deterministic_seek_without_callbacks() {
        let store = SemanticStore::new();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.advance_to(2.25).unwrap();
        assert!(matches!(
            session.advance_to_callback_barrier(0.0).unwrap(),
            CallbackAdvance::Ready(frame) if frame.time == 0.0
        ));
    }

    #[test]
    fn structural_callback_commits_provisional_object_and_effective_frame_once() {
        use noon_core::{SemanticNodeCreation, SemanticVec3};
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.add_member(root, object).unwrap();
        store.attach_to_scene(root).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let mut overlay = session
            .begin_required_callback_phase(0.5, [object])
            .unwrap();
        overlay
            .set_transform(
                object,
                Transform2D {
                    translation: Vec2::new(3.0, 0.0),
                    ..Transform2D::IDENTITY
                },
            )
            .unwrap();
        let batch = overlay.finish();
        let mut tx = SemanticMutationTransaction::new();
        let provisional = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
            StoredGeometry::Circle { radius: 0.25 },
        )));
        tx.set_property(
            provisional,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(2.0, 4.0, 0.0),
        );
        tx.add_member(root, provisional);
        let prepared = tx.prepare(&mut store).unwrap();
        assert_eq!(
            prepared
                .object_state(provisional)
                .unwrap()
                .transform
                .translation
                .x,
            2.0
        );
        let planned = prepared.planned_node_id(provisional).unwrap();
        let result = session
            .commit_prepared_required_callback_transaction(batch, prepared, None)
            .unwrap();
        assert_eq!(result.resolve(provisional), Some(planned));
        assert_eq!(session.pending_callback_token(), None);
        assert_eq!(session.frame().time, 0.5);
        assert_eq!(
            session
                .effective_semantic_object(&store, object)
                .unwrap()
                .object
                .transform
                .translation
                .x,
            3.0
        );
        assert_eq!(
            store
                .semantic_object_state_checked(object)
                .unwrap()
                .transform
                .translation
                .x,
            0.0,
            "effective driver writes do not become authored values"
        );
        assert_eq!(
            session
                .effective_semantic_object(&store, planned)
                .unwrap()
                .object
                .transform
                .translation,
            Vec2::new(2.0, 4.0)
        );
        assert_eq!(
            session.publication_context().frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );
        assert_eq!(
            session.publication_context().scene_revision(),
            before.scene_revision().checked_next().unwrap()
        );
        assert_eq!(
            session.last_structural_publication_stats().entered_objects,
            1
        );
    }

    #[test]
    fn structural_callback_can_detach_its_own_target_without_leaking_driver_or_wake() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.add_member(root, object).unwrap();
        store.attach_to_scene(root).unwrap();
        let mut register = SemanticMutationTransaction::new();
        register.add_updater(object, HostCallbackId::new(1), 0.0, None);
        register.apply(&mut store).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let CallbackAdvance::HostRequired { mut overlay, .. } =
            session.advance_to_callback_barrier(0.0).unwrap()
        else {
            panic!("required updater");
        };
        overlay
            .set_transform(
                object,
                Transform2D {
                    translation: Vec2::new(3.0, 0.0),
                    ..Transform2D::IDENTITY
                },
            )
            .unwrap();
        let mut tx = SemanticMutationTransaction::new();
        tx.remove_member(root, object);
        session
            .commit_required_callback_transaction(&mut store, overlay.finish(), tx)
            .unwrap();
        assert!(session.effective_semantic_object(&store, object).is_err());
        assert_eq!(
            session.callback_schedule.wake_timeline(0.0),
            TimelineWakeState::Quiescent
        );
        assert!(matches!(
            session.advance_to_callback_barrier(0.5).unwrap(),
            CallbackAdvance::Ready(_)
        ));
        let mut tx = SemanticMutationTransaction::new();
        tx.add_member(root, object);
        session.apply_semantic_transaction(&mut store, tx).unwrap();
        assert!(matches!(
            session.advance_to_callback_barrier(0.5).unwrap(),
            CallbackAdvance::HostRequired { .. }
        ));
    }

    #[test]
    fn callback_reentry_restores_target_after_committing_the_prior_schedule_preview() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let objects = [0, 1].map(|_| {
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }))
        });
        for object in objects {
            store.add_member(root, object).unwrap();
        }
        store.attach_to_scene(root).unwrap();
        let mut register = SemanticMutationTransaction::new();
        for (index, object) in objects.into_iter().enumerate() {
            register.add_updater(object, HostCallbackId::new(index as u64), 0.0, None);
        }
        register.apply(&mut store).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let CallbackAdvance::HostRequired { overlay, .. } =
            session.advance_to_callback_barrier(0.0).unwrap()
        else {
            panic!("callbacks");
        };
        let mut detach = SemanticMutationTransaction::new();
        detach.remove_member(root, objects[1]);
        session
            .commit_required_callback_transaction(&mut store, overlay.finish(), detach)
            .unwrap();
        let CallbackAdvance::HostRequired {
            overlay,
            invocations,
        } = session.advance_to_callback_barrier(0.5).unwrap()
        else {
            panic!("remaining callback");
        };
        assert_eq!(invocations.len(), 1);
        let mut reentry = SemanticMutationTransaction::new();
        reentry.add_member(root, objects[1]);
        session
            .commit_required_callback_transaction(&mut store, overlay.finish(), reentry)
            .unwrap();
        let CallbackAdvance::HostRequired { invocations, .. } =
            session.advance_to_callback_barrier(0.6).unwrap()
        else {
            panic!("restored callbacks");
        };
        assert_eq!(
            invocations
                .iter()
                .map(|invocation| invocation.target())
                .collect::<Vec<_>>(),
            objects
        );
    }

    #[test]
    fn failed_structural_callback_keeps_same_token_frame_and_semantic_allocator_retryable() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let frame = session.frame().clone();
        let overlay = session
            .begin_required_callback_phase(1.0, [object])
            .unwrap();
        let batch = overlay.finish();
        let token = batch.token;
        let mut invalid = SemanticMutationTransaction::new();
        invalid.set_property(object, SemanticObjectProperty::RotationZ, f64::MAX);
        assert!(matches!(
            session.commit_required_callback_transaction(&mut store, batch.clone(), invalid),
            Err(ExecutionSessionCallbackError::Publication(
                super::super::ExecutionSessionPublicationError::Lowering(_)
            ))
        ));
        assert_eq!(session.pending_callback_token(), Some(token));
        assert_eq!(session.frame(), &frame);
        assert_eq!(session.publication_context(), before);
        assert_eq!(store.scene_revision(), before.scene_revision());
        assert!(session.take_frame_changes().is_empty());
        let mut valid = SemanticMutationTransaction::new();
        valid.set_property(object, SemanticObjectProperty::RotationZ, 0.5);
        session
            .commit_required_callback_transaction(&mut store, batch.clone(), valid)
            .unwrap();
        assert_eq!(session.frame().time, 1.0);
        assert_eq!(
            session
                .effective_semantic_object(&store, object)
                .unwrap()
                .object
                .transform
                .rotation,
            0.5
        );
        assert!(matches!(
            session.commit_required_callback_transaction(
                &mut store,
                batch,
                SemanticMutationTransaction::new()
            ),
            Err(ExecutionSessionCallbackError::NoPendingPhase)
        ));
    }

    #[test]
    fn required_callback_phase_preserves_coherent_frame_and_orders_overlay_writes() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.take_frame_changes();
        let before = session.frame().clone();
        let publication = session.publication_context();

        let mut overlay = session
            .begin_required_callback_phase(1.0, [object])
            .unwrap();
        assert_eq!(overlay.token().publication(), publication);
        assert_eq!(overlay.delta_time(), 1.0);
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(
            session.advance_to(2.0),
            Err(EvaluationError::RequiredCallbackPending)
        );
        assert_eq!(
            session.set_reactive_input(object, 1.0_f32),
            Err(ExecutionSessionInputError::RequiredCallbackPending)
        );

        let first = Transform2D {
            translation: Vec2::new(2.0, 0.0),
            ..Transform2D::IDENTITY
        };
        let second = Transform2D {
            translation: Vec2::new(3.0, 0.0),
            ..Transform2D::IDENTITY
        };
        overlay.set_transform(object, first).unwrap();
        assert_eq!(overlay.object(object).unwrap().transform, first);
        assert_eq!(
            overlay.object(object).unwrap().bounds.unwrap().center(),
            Vec2::new(2.0, 0.0)
        );
        overlay.set_transform(object, second).unwrap();
        assert_eq!(overlay.object(object).unwrap().transform, second);
        assert_eq!(
            overlay.object(object).unwrap().bounds.unwrap().center(),
            Vec2::new(3.0, 0.0)
        );

        let token = overlay.token();
        session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();
        assert_eq!(session.frame().time, 1.0);
        assert_eq!(session.frame().objects[0].transform, second);
        assert_eq!(session.pending_callback_token(), None);
        assert_eq!(
            session.publication_context().frame_epoch(),
            publication.frame_epoch().checked_next().unwrap()
        );
        let CallbackRendererObservationOutcome::Committed(observation) =
            session.committed_callback_renderer_observation(token, object)
        else {
            panic!("the exact committed callback target must remain observable");
        };
        assert_eq!(observation.token(), token);
        assert_eq!(observation.target(), object);
        assert_eq!(observation.execution_slot(), ExecutionSlotId::new(0, 0));
        assert_eq!(observation.frame_index(), 0);
        assert_eq!(observation.transform(), second);
        assert_eq!(
            observation.dirty(),
            CallbackRendererDirtyClassification::Updated
        );
        assert_eq!(session.take_frame_changes().object_indices(), &[0]);

        let mut next = session
            .begin_required_callback_phase(2.0, [object])
            .unwrap();
        assert_eq!(next.prior_driver_row_count(), 1);
        assert_eq!(next.staged_row_count(), 1);
        assert_eq!(
            next.object(object).unwrap().transform.translation,
            Vec2::new(3.0, 0.0)
        );
        let accumulated = Transform2D {
            translation: next.object(object).unwrap().transform.translation
                + Vec2::new(next.delta_time() as f32, 0.0),
            ..next.object(object).unwrap().transform
        };
        next.set_transform(object, accumulated).unwrap();
        assert_eq!(session.frame().objects[0].transform, second);
        session
            .commit_required_callback_phase(next.finish())
            .unwrap();
        assert_eq!(
            session.frame().objects[0].transform.translation,
            Vec2::new(4.0, 0.0)
        );
        assert!(matches!(
            session.committed_callback_renderer_observation(token, object),
            CallbackRendererObservationOutcome::StaleCallback { .. }
        ));
    }

    #[test]
    fn scoped_callback_writes_preserve_other_channels_and_whole_write_order() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let publication = session.publication_context();
        let mut overlay = session
            .begin_required_callback_phase(0.5, [object])
            .unwrap();
        let transform = Transform2D {
            translation: Vec2::new(1.0, 2.0),
            rotation: 0.75,
            scale: Vec2::new(2.0, 3.0),
        };
        overlay.set_transform(object, transform).unwrap();
        overlay
            .write(EffectiveSemanticPropertyWrite::Translation {
                object,
                translation: Vec2::new(4.0, 5.0),
            })
            .unwrap();
        overlay
            .write(EffectiveSemanticPropertyWrite::Opacity {
                object,
                opacity: 0.25,
            })
            .unwrap();
        let read = overlay.object(object).unwrap();
        assert_eq!(read.transform.rotation, transform.rotation);
        assert_eq!(read.transform.scale, transform.scale);
        assert_eq!(read.transform.translation, Vec2::new(4.0, 5.0));
        assert_eq!(read.style.opacity, 0.25);
        let expected = *read;
        session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();
        assert_eq!(session.frame().objects[0].transform, expected.transform);
        assert_eq!(session.frame().objects[0].style, expected.style);
        assert_eq!(
            session.publication_context().scene_revision(),
            publication.scene_revision()
        );
        assert_eq!(
            session.publication_context().execution_revision(),
            publication.execution_revision()
        );
        assert_eq!(
            session.publication_context().frame_epoch(),
            publication.frame_epoch().checked_next().unwrap()
        );
        let receipt = session.last_callback_receipt.as_ref().unwrap();
        assert_eq!(
            receipt.domains[&object],
            CALLBACK_TRANSLATION | CALLBACK_ROTATION | CALLBACK_SCALE | CALLBACK_OPACITY
        );
        let mut next = session
            .begin_required_callback_phase(1.0, [object])
            .unwrap();
        next.write(EffectiveSemanticPropertyWrite::Translation {
            object,
            translation: Vec2::new(9.0, 9.0),
        })
        .unwrap();
        next.set_transform(object, transform).unwrap();
        session
            .commit_required_callback_phase(next.finish())
            .unwrap();
        assert_eq!(session.frame().objects[0].transform, transform);
    }

    #[test]
    fn invalid_or_stale_callback_batch_is_atomic_and_leaves_barrier_retryable() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.take_frame_changes();
        let overlay = session
            .begin_required_callback_phase(1.0, [object])
            .unwrap();
        let token = overlay.token();
        let before = session.frame().clone();
        let publication = session.publication_context();

        let stale = CallbackPhaseToken::new(
            token.runtime(),
            token.publication(),
            CallbackSequence::new(token.sequence().get() + 1),
        );
        assert!(matches!(
            session.commit_required_callback_phase(EffectivePropertyBatch::new(stale, [])),
            Err(ExecutionSessionCallbackError::StaleToken { .. })
        ));

        let valid_transform = Transform2D {
            translation: Vec2::new(5.0, 0.0),
            ..Transform2D::IDENTITY
        };
        let invalid = EffectivePropertyBatch::new(
            token,
            [
                EffectiveSemanticPropertyWrite::Transform {
                    object,
                    transform: valid_transform,
                },
                EffectiveSemanticPropertyWrite::Style {
                    object,
                    style: Style {
                        opacity: f32::NAN,
                        ..Style::default()
                    },
                },
            ],
        );
        assert!(matches!(
            session.commit_required_callback_phase(invalid),
            Err(ExecutionSessionCallbackError::InvalidEffectiveWrite(_))
        ));
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.pending_callback_token(), Some(token));
        assert!(session.take_frame_changes().is_empty());

        let retry = EffectivePropertyBatch::new(
            token,
            [EffectiveSemanticPropertyWrite::Transform {
                object,
                transform: valid_transform,
            }],
        );
        session.commit_required_callback_phase(retry).unwrap();
        assert_eq!(session.frame().objects[0].transform, valid_transform);
    }

    #[test]
    fn failed_required_callback_discards_sparse_evaluation_without_advancing_time() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.take_frame_changes();
        let before = session.frame().clone();
        let publication = session.publication_context();
        let token = session
            .begin_required_callback_phase(4.0, [object])
            .unwrap()
            .token();

        session.fail_required_callback_phase(token).unwrap();
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), publication);
        assert!(session.take_frame_changes().is_empty());
        assert_eq!(session.pending_callback_token(), None);
        assert_eq!(
            session.callback_termination().unwrap().kind(),
            CallbackTerminationKind::Failed
        );
        assert!(matches!(
            session.advance_to_callback_barrier(4.0),
            Err(ExecutionSessionCallbackError::Terminated(_))
        ));
        assert_eq!(
            session.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );
    }

    #[test]
    fn equal_revision_sessions_reject_each_others_callback_batches() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut first = ExecutionSession::from_semantic_store(&store).unwrap();
        let mut second = ExecutionSession::from_semantic_store(&store).unwrap();

        let first_overlay = first.begin_required_callback_phase(1.0, [object]).unwrap();
        let second_overlay = second.begin_required_callback_phase(1.0, [object]).unwrap();
        assert_eq!(
            first_overlay.token().publication(),
            second_overlay.token().publication()
        );
        assert_eq!(
            first_overlay.token().sequence(),
            second_overlay.token().sequence()
        );
        assert_ne!(
            first_overlay.token().runtime(),
            second_overlay.token().runtime()
        );

        assert!(matches!(
            second.commit_required_callback_phase(first_overlay.finish()),
            Err(ExecutionSessionCallbackError::StaleToken { .. })
        ));
        assert_eq!(
            second.pending_callback_token(),
            Some(second_overlay.token())
        );
        assert_eq!(second.frame().time, 0.0);
    }

    #[test]
    fn cloning_a_pending_session_preserves_progress_as_an_interruption() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut original = ExecutionSession::from_semantic_store(&store).unwrap();
        let original_overlay = original
            .begin_required_callback_phase(1.0, [object])
            .unwrap();

        let mut cloned = original.clone();
        assert_eq!(cloned.pending_callback_token(), None);
        let termination = cloned.callback_termination().unwrap();
        assert_eq!(termination.kind(), CallbackTerminationKind::Interrupted);
        assert_ne!(
            original_overlay.token().runtime(),
            termination.token().runtime()
        );
        assert!(matches!(
            cloned.advance_to_callback_barrier(1.0),
            Err(ExecutionSessionCallbackError::Terminated(_))
        ));
        assert_eq!(
            original.pending_callback_token(),
            Some(original_overlay.token())
        );
        assert_eq!(original.frame().time, 0.0);
        assert_eq!(cloned.frame().time, 0.0);
    }

    #[test]
    fn callback_aware_advance_runs_time_zero_phase_once_in_compiler_order() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        let unrelated =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 2.0,
            }));
        store.attach_to_scene(object).unwrap();
        store.attach_to_scene(unrelated).unwrap();
        let input = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_signal(input, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();
        let bound_state_source = NativeStateSource::Control {
            name: "callback-input".to_owned(),
        };
        store
            .bind_semantic_native_state_input(input, bound_state_source.clone())
            .unwrap();
        let event_input = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_signal(event_input, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let bound_event_source = NativeEventSource::KeyPress {
            code: "Space".to_owned(),
        };
        store
            .bind_semantic_native_event_input(event_input, bound_event_source.clone())
            .unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(object, HostCallbackId::new(4), 0.0, None);
        transaction.add_updater(object, HostCallbackId::new(2), 0.0, None);
        transaction.apply(&mut store).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let zero_duration = session.wait_segment(0.0).unwrap();
        let coherent = session.frame().clone();

        assert_eq!(
            session.set_reactive_input(input, 1.0_f32),
            Err(ExecutionSessionInputError::RequiredCallbacksConfigured)
        );
        assert_eq!(
            session.set_native_state_input(bound_state_source, NativeInputValue::Scalar(1.0),),
            Err(ExecutionSessionInputError::RequiredCallbacksConfigured)
        );
        assert_eq!(
            session.emit_native_event(NativeEventOccurrence::new(0, bound_event_source,)),
            Err(ExecutionSessionInputError::RequiredCallbacksConfigured)
        );
        session
            .set_native_state_input(
                NativeStateSource::ViewportSize,
                NativeInputValue::Vec2(Vec2::new(10.0, 10.0)),
            )
            .unwrap();
        session
            .emit_native_event(NativeEventOccurrence::new(
                0,
                NativeEventSource::PointerDown { button: 0 },
            ))
            .unwrap();
        assert_eq!(session.frame(), &coherent);

        assert!(!session.segment_state(zero_duration).is_complete());
        assert_eq!(
            session.advance_to(0.0),
            Err(EvaluationError::RequiredCallbackBarrier)
        );
        assert_eq!(
            session.wake_state().timeline(),
            TimelineWakeState::Continuous
        );
        let (invocations, overlay) = match session.advance_to_callback_barrier(0.0).unwrap() {
            CallbackAdvance::HostRequired {
                invocations,
                overlay,
            } => (invocations, overlay),
            CallbackAdvance::Ready(_) => panic!("time-zero updater phase must be required"),
        };
        assert_eq!(
            invocations
                .iter()
                .map(|invocation| invocation.callback_id())
                .collect::<Vec<_>>(),
            vec![HostCallbackId::new(4), HostCallbackId::new(2)]
        );
        assert_eq!(overlay.objects().count(), 1);
        assert!(overlay.object(unrelated).is_none());
        session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();
        assert!(session.segment_state(zero_duration).is_complete());

        assert!(matches!(
            session.advance_to_callback_barrier(0.0).unwrap(),
            CallbackAdvance::Ready(frame) if frame.time == 0.0
        ));
    }

    #[test]
    fn large_advance_stops_at_bounded_callback_activation_before_crossing_it() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let callback = HostCallbackId::new(9);
        let mut add = SemanticMutationTransaction::new();
        add.add_updater(object, callback, 1.0, None);
        add.apply(&mut store).unwrap();
        let mut remove = SemanticMutationTransaction::new();
        remove.remove_updater(object, callback, 1.1);
        remove.apply(&mut store).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

        assert_eq!(
            session.wake_state().timeline(),
            TimelineWakeState::Deadline(1.0)
        );
        let overlay = match session.advance_to_callback_barrier(2.0).unwrap() {
            CallbackAdvance::HostRequired {
                invocations,
                overlay,
            } => {
                assert_eq!(overlay.time(), 1.0);
                assert_eq!(invocations.len(), 1);
                assert_eq!(invocations[0].callback_id(), callback);
                overlay
            }
            CallbackAdvance::Ready(_) => panic!("bounded updater interval was skipped"),
        };
        assert_eq!(session.frame().time, 0.0);
        session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();
        assert_eq!(session.frame().time, 1.0);

        assert!(matches!(
            session.advance_to_callback_barrier(2.0).unwrap(),
            CallbackAdvance::Ready(frame) if frame.time == 2.0
        ));
        assert_eq!(
            session.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );
    }
}
