use std::collections::{BTreeMap, BTreeSet, HashSet};

use noon_compile::{
    CompilePatchError, SemanticHostCallbackEventKind, SemanticHostCallbackPlan,
    SemanticOrderedUpdater,
};
use noon_core::{
    HostCallbackId, ObjectContentRef, PreparedSemanticMutationTransaction, Property,
    PublicationContext, ReactiveValue, Rect, SemanticMutation, SemanticNodeId,
    SemanticObjectProperty, SemanticObjectRole, SemanticOrientation, SemanticSignalValue,
    SemanticVec3, Style, Transform2D,
};
use noon_runtime::{
    EffectiveContentError, EffectiveContentLease, EffectiveObjectProperties,
    EffectivePropertyWrite as RuntimeEffectivePropertyWrite, EvaluationError, ExecutionSlotId,
    FrameState, PreparedFrameCommitError, PreparedFrameContentCommitError, PreparedFrameEvaluation,
    RuntimeIdentity,
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
pub(super) const CALLBACK_PRESENCE: u8 = 128;

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
    // State carried forward after all boundaries at `time` are applied.
    active_occurrences: BTreeSet<(usize, usize)>,
    // Registrations called at this frame, including one-shot inclusive endpoints.
    invocation_occurrences: BTreeSet<(usize, usize)>,
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
        store: &noon_core::SemanticStore,
        time: f64,
    ) {
        let targets = revision.targets().collect::<Vec<_>>();
        for &target in &targets {
            for index in self.plan.target_occurrences(target) {
                self.active_occurrences
                    .remove(&(self.plan.occurrence(index).order(), index));
            }
        }
        self.plan.apply_revision(revision, store);
        self.plan
            .refresh_native_bindings(store, targets.iter().copied());
        for target in targets {
            for index in self.plan.target_occurrences(target) {
                let occurrence = self.plan.occurrence(index);
                let activation = occurrence.activation();
                if !self.detached_targets.contains(&target) && activation.is_active_at(time) {
                    self.active_occurrences.insert((occurrence.order(), index));
                }
            }
        }
        self.completed_time = None;
        self.completed_publication = None;
    }

    pub(super) fn refresh_native_bindings(
        &mut self,
        store: &noon_core::SemanticStore,
        targets: impl IntoIterator<Item = SemanticNodeId>,
    ) {
        self.plan.refresh_native_bindings(store, targets);
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
            if live && activation.is_active_at(time) {
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
        let mut endpoint_occurrences = BTreeSet::new();
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
                    let occurrence = self.plan.occurrence(index);
                    if occurrence.activation().endpoint_policy()
                        == noon_core::SemanticUpdaterEndpointPolicy::InvokeAtEnd
                        && !self.detached_targets.contains(&occurrence.target())
                    {
                        endpoint_occurrences.insert(key);
                    }
                    active_occurrences.remove(&key);
                }
            }
        }
        let mut invocation_occurrences = active_occurrences.clone();
        invocation_occurrences.extend(endpoint_occurrences);
        CallbackSchedulePreview {
            time,
            active_occurrences,
            invocation_occurrences,
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
        if !preview.invocation_occurrences.is_empty() && self.completed_time != Some(current) {
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

    fn ordered_updates(
        &self,
        active: &BTreeSet<(usize, usize)>,
        relevant_native: &HashSet<(SemanticNodeId, SemanticObjectProperty)>,
    ) -> Vec<SemanticOrderedUpdater> {
        self.plan
            .ordered_relevant_updates(active, relevant_native)
            .into_iter()
            .filter(|update| match update {
                SemanticOrderedUpdater::Native(native) => {
                    !self.detached_targets.contains(&native.target())
                }
                SemanticOrderedUpdater::Host(_) => true,
            })
            .collect()
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

/// Progress after admitting one exact host region within an unpublished frame.
#[derive(Debug)]
pub enum CallbackRegionAdvance {
    HostRequired {
        invocations: Vec<RequiredCallbackInvocation>,
        overlay: CallbackPhaseOverlay,
    },
    Complete(EffectivePropertyBatch),
}

fn semantic_native_property(property: Property) -> Option<SemanticObjectProperty> {
    match property {
        Property::Presence => Some(SemanticObjectProperty::Presence),
        Property::Position => Some(SemanticObjectProperty::Translation),
        Property::Scale => Some(SemanticObjectProperty::Scale),
        Property::Rotation => Some(SemanticObjectProperty::RotationZ),
        Property::Opacity => Some(SemanticObjectProperty::ObjectOpacity),
        Property::StrokeWidth => Some(SemanticObjectProperty::StrokeWidth),
        _ => None,
    }
}

fn native_effective_write(
    object: SemanticNodeId,
    property: SemanticObjectProperty,
    value: ReactiveValue,
) -> EffectiveSemanticPropertyWrite {
    match (property, value) {
        (SemanticObjectProperty::Presence, ReactiveValue::Bool(presence)) => {
            EffectiveSemanticPropertyWrite::Presence { object, presence }
        }
        (SemanticObjectProperty::Translation, ReactiveValue::Vec2(translation)) => {
            EffectiveSemanticPropertyWrite::Translation {
                object,
                translation,
            }
        }
        (SemanticObjectProperty::Scale, ReactiveValue::Vec2(scale)) => {
            EffectiveSemanticPropertyWrite::Scale { object, scale }
        }
        (SemanticObjectProperty::RotationZ, ReactiveValue::Scalar(rotation)) => {
            EffectiveSemanticPropertyWrite::Rotation { object, rotation }
        }
        (SemanticObjectProperty::ObjectOpacity, ReactiveValue::Scalar(opacity)) => {
            EffectiveSemanticPropertyWrite::Opacity { object, opacity }
        }
        (SemanticObjectProperty::StrokeWidth, ReactiveValue::Scalar(stroke_width)) => {
            EffectiveSemanticPropertyWrite::StrokeWidth {
                object,
                stroke_width,
            }
        }
        _ => unreachable!("lowered native binding retains its validated value kind"),
    }
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
        EffectiveSemanticPropertyWrite::Presence { .. } => CALLBACK_PRESENCE,
        EffectiveSemanticPropertyWrite::Transform { .. } => {
            CALLBACK_TRANSLATION | CALLBACK_ROTATION | CALLBACK_SCALE
        }
        EffectiveSemanticPropertyWrite::WorldTransform { .. } => {
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

fn native_write_domain(property: SemanticObjectProperty) -> u8 {
    match property {
        SemanticObjectProperty::Presence => CALLBACK_PRESENCE,
        SemanticObjectProperty::Translation => CALLBACK_TRANSLATION,
        SemanticObjectProperty::RotationZ => CALLBACK_ROTATION,
        SemanticObjectProperty::Scale => CALLBACK_SCALE,
        SemanticObjectProperty::StrokeWidth => CALLBACK_STROKE_WIDTH,
        SemanticObjectProperty::ObjectOpacity => CALLBACK_OPACITY,
        SemanticObjectProperty::FillOpacity | SemanticObjectProperty::StrokeOpacity => 0,
    }
}

/// Final ordered effective writes for one exact required callback phase.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectivePropertyBatch {
    token: CallbackPhaseToken,
    region: u32,
    writes: Vec<EffectiveSemanticPropertyWrite>,
}

impl EffectivePropertyBatch {
    pub fn new(
        token: CallbackPhaseToken,
        writes: impl IntoIterator<Item = EffectiveSemanticPropertyWrite>,
    ) -> Self {
        Self {
            token,
            region: 0,
            writes: writes.into_iter().collect(),
        }
    }

    pub fn with_region(mut self, region: u32) -> Self {
        self.region = region;
        self
    }

    pub const fn region(&self) -> u32 {
        self.region
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
    region: u32,
    region_start: usize,
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

    pub const fn region(&self) -> u32 {
        self.region
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

    /// Stage a Rust-prepared layout transform with the exact bounds observed by
    /// following reads in this callback invocation.
    pub fn set_transform_and_bounds(
        &mut self,
        object: SemanticNodeId,
        transform: Transform2D,
        bounds: Option<noon_core::Rect>,
    ) -> Result<(), ExecutionSessionCallbackError> {
        self.set_transform(object, transform)?;
        self.objects
            .get_mut(&object)
            .expect("set_transform validated the callback row")
            .set_transform_and_bounds(transform, bounds);
        Ok(())
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
            EffectiveSemanticPropertyWrite::WorldTransform { .. } => {
                return Err(ExecutionSessionCallbackError::UnsupportedWorldTransform(
                    object,
                ));
            }
            EffectiveSemanticPropertyWrite::Presence { presence, .. } => {
                current.presence = presence
            }
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
        EffectivePropertyBatch::new(self.token, self.writes[self.region_start..].iter().copied())
            .with_region(self.region)
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
    StaleRegion {
        expected: u32,
        actual: u32,
    },
    IncompleteRegion,
    ContentRequiresFinalRegion,
    SequenceExhausted,
    NonMonotonicAdvance {
        current: f64,
        requested: f64,
    },
    UnsupportedCallbackTarget(SemanticNodeId),
    UnsupportedWorldTransform(SemanticNodeId),
    UnknownObject(SemanticNodeId),
    Read(ExecutionSessionCallbackReadError),
    Evaluation(EvaluationError),
    InvalidEffectiveWrite(CompilePatchError),
    Content(EffectiveContentError),
    ContentCommit(PreparedFrameContentCommitError),
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
            Self::StaleRegion { expected, actual } => write!(
                formatter,
                "callback region {actual} does not match pending region {expected}"
            ),
            Self::IncompleteRegion => {
                formatter.write_str("ordered callback regions must finish before publication")
            }
            Self::ContentRequiresFinalRegion => formatter
                .write_str("effective content replacement requires the final host callback region"),
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
            Self::UnsupportedWorldTransform(target) => write!(
                formatter,
                "world-transform writes are not supported by the planar callback wire protocol for {}:{}",
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
            Self::Content(error) => error.fmt(formatter),
            Self::ContentCommit(error) => error.fmt(formatter),
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
    ordered_updates: Vec<SemanticOrderedUpdater>,
    next_update: usize,
    region: u32,
    region_last_host_key: (usize, u64),
    overlay: CallbackPhaseOverlay,
    completed: bool,
    last_region_batch: Option<EffectivePropertyBatch>,
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
    /// This first content producer is terminal within the ordered callback
    /// sequence. Earlier regions need a content-aware read overlay before they
    /// can expose new geometry to later host callbacks.
    pub fn require_terminal_callback_content_region(
        &self,
        token: CallbackPhaseToken,
        region: u32,
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
        if region != pending.region {
            return Err(ExecutionSessionCallbackError::StaleRegion {
                expected: pending.region,
                actual: region,
            });
        }
        if pending.ordered_updates[pending.next_update..]
            .iter()
            .any(|update| matches!(update, SemanticOrderedUpdater::Host(_)))
        {
            return Err(ExecutionSessionCallbackError::ContentRequiresFinalRegion);
        }
        Ok(())
    }

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
        self.validate_callback_store(store, token)?;
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

    /// Validate that a callback-owned semantic read uses the execution
    /// session's store, phase token, and pinned revision.
    pub(crate) fn validate_callback_store(
        &self,
        store: &noon_core::SemanticStore,
        token: CallbackPhaseToken,
    ) -> Result<(), crate::FamilyCallbackPaintError> {
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
        Ok(())
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
                if let Some(object) = pending.overlay.object(semantic) {
                    return Ok(CallbackReadValue::Object(*object));
                }
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

    /// Prepare callback-owned affine values released by updater edits at the
    /// frame that owns the callback receipt. The prepared semantic transaction
    /// remains the authority for final updater membership and close semantics.
    pub(super) fn released_callback_affine_mutations(
        &self,
        prepared: &PreparedSemanticMutationTransaction<'_>,
    ) -> Result<
        Vec<(SemanticNodeId, SemanticObjectProperty, SemanticSignalValue)>,
        super::ExecutionSessionPublicationError,
    > {
        if self.last_callback_receipt.is_none() {
            return Ok(Vec::new());
        }
        let now = self.frame().time;
        let store = prepared.store();
        let targets = prepared
            .candidate_mutations()
            .filter_map(|mutation| match mutation {
                SemanticMutation::RemoveUpdater {
                    target,
                    inactive_from,
                    ..
                }
                | SemanticMutation::ClearUpdaters {
                    target,
                    inactive_from,
                } if *inactive_from <= now => target.existing(),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let mut staged = Vec::new();
        for target in targets {
            if prepared.node_is_removed(target) {
                continue;
            }
            let Some(registrations) = prepared.proposed_updater_registrations(target) else {
                continue;
            };
            if registrations
                .iter()
                .any(|updater| updater.is_active_at(now))
            {
                continue;
            }
            let Some(domains) = self
                .last_callback_receipt
                .as_ref()
                .and_then(|receipt| receipt.domains_at(target, now, self.publication_context()))
            else {
                continue;
            };
            let state = store
                .semantic_object_state_checked(target)
                .map_err(|_| super::ExecutionSessionPublicationError::UnknownObject(target))?;
            if !matches!(
                state.role(),
                SemanticObjectRole::Ordinary | SemanticObjectRole::Camera2D
            ) || !matches!(state.transform.orientation, SemanticOrientation::Planar(_))
            {
                continue;
            }
            let authored = state.transform;
            let effective = self
                .effective_semantic_object(store, target)?
                .object
                .transform;
            let explicitly_authored =
                prepared
                    .mutations()
                    .iter()
                    .fold(0, |domains, mutation| match mutation {
                        SemanticMutation::SetObjectTransform { object, .. }
                            if object.existing() == Some(target) =>
                        {
                            domains | CALLBACK_TRANSLATION | CALLBACK_ROTATION | CALLBACK_SCALE
                        }
                        SemanticMutation::SetProperty {
                            object, property, ..
                        } if object.existing() == Some(target) => {
                            domains
                                | match property {
                                    SemanticObjectProperty::Translation => CALLBACK_TRANSLATION,
                                    SemanticObjectProperty::RotationZ => CALLBACK_ROTATION,
                                    SemanticObjectProperty::Scale => CALLBACK_SCALE,
                                    _ => 0,
                                }
                        }
                        _ => domains,
                    });
            let domains = domains
                & (CALLBACK_TRANSLATION | CALLBACK_ROTATION | CALLBACK_SCALE)
                & !explicitly_authored;
            if domains == 0 {
                continue;
            }
            if domains & CALLBACK_TRANSLATION != 0 {
                staged.push((
                    target,
                    SemanticObjectProperty::Translation,
                    SemanticSignalValue::Vec3(SemanticVec3::new(
                        f64::from(effective.translation.x),
                        f64::from(effective.translation.y),
                        authored.translation.z,
                    )),
                ));
            }
            if domains & CALLBACK_SCALE != 0 {
                staged.push((
                    target,
                    SemanticObjectProperty::Scale,
                    SemanticSignalValue::Vec3(SemanticVec3::new(
                        f64::from(effective.scale.x),
                        f64::from(effective.scale.y),
                        authored.scale.z,
                    )),
                ));
            }
            if domains & CALLBACK_ROTATION != 0 {
                staged.push((
                    target,
                    SemanticObjectProperty::RotationZ,
                    SemanticSignalValue::Scalar(f64::from(effective.rotation)),
                ));
            }
        }

        Ok(staged)
    }

    pub(super) fn carry_callback_ownership_through_completion(
        &mut self,
        time: f64,
        previous_publication: PublicationContext,
    ) {
        let publication = self.publication_context();
        if let Some(receipt) = self
            .last_callback_receipt
            .as_mut()
            .filter(|receipt| receipt.time == time && receipt.publication == previous_publication)
        {
            receipt.publication = publication;
            receipt
                .domains
                .retain(|target, _| self.callback_schedule.continues_for_target(*target));
        } else {
            self.last_callback_receipt = None;
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
            .invocation_occurrences
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
                .execution_object_id(invocation.target())
                .is_none()
            {
                return Err(ExecutionSessionCallbackError::UnsupportedCallbackTarget(
                    invocation.target(),
                ));
            }
            if seen_read_objects.insert(invocation.target()) {
                read_objects.push(invocation.target());
            }
        }
        self.begin_required_callback_phase_with_schedule(
            preview.time,
            read_objects,
            Some(preview),
            true,
        )?;
        let pending = self.pending_callback.as_ref().expect("new callback phase");
        let relevant_native = pending
            .prepared
            .deferred_reactive_bindings()
            .iter()
            .filter_map(|&(object, property)| {
                Some((
                    self.execution_index.semantic_object_id(object)?,
                    semantic_native_property(property)?,
                ))
            })
            .collect::<HashSet<_>>();
        let ordered_updates = self.callback_schedule.ordered_updates(
            &pending
                .schedule
                .as_ref()
                .expect("scheduled callback phase")
                .invocation_occurrences,
            &relevant_native,
        );
        self.pending_callback
            .as_mut()
            .expect("new callback phase")
            .ordered_updates = ordered_updates;
        match self.advance_ordered_callback_region()? {
            CallbackRegionAdvance::HostRequired {
                invocations,
                overlay,
            } => Ok(CallbackAdvance::HostRequired {
                invocations,
                overlay,
            }),
            CallbackRegionAdvance::Complete(_) => {
                unreachable!("an active host occurrence requires a host region")
            }
        }
    }

    fn advance_ordered_callback_region(
        &mut self,
    ) -> Result<CallbackRegionAdvance, ExecutionSessionCallbackError> {
        let pending = self
            .pending_callback
            .as_ref()
            .expect("ordered phase pending");
        let (cursor, last_host_key, overlay, advance) = self.stage_next_callback_region(
            pending,
            &pending.ordered_updates,
            pending.next_update,
            pending.overlay.clone(),
        )?;
        let pending = self
            .pending_callback
            .as_mut()
            .expect("ordered phase pending");
        pending.next_update = cursor;
        pending.region_last_host_key = last_host_key;
        pending.overlay = overlay;
        Ok(advance)
    }

    fn ordered_update_key(&self, update: SemanticOrderedUpdater) -> (usize, u64) {
        match update {
            SemanticOrderedUpdater::Native(native) => native.sort_key(),
            SemanticOrderedUpdater::Host(index) => {
                let host = self.callback_schedule.plan.occurrence(index);
                (host.order(), host.activation().authored_order())
            }
        }
    }

    fn stage_next_callback_region(
        &self,
        pending: &PendingCallbackPhase,
        updates: &[SemanticOrderedUpdater],
        mut cursor: usize,
        mut overlay: CallbackPhaseOverlay,
    ) -> Result<
        (
            usize,
            (usize, u64),
            CallbackPhaseOverlay,
            CallbackRegionAdvance,
        ),
        ExecutionSessionCallbackError,
    > {
        let mut invocations = Vec::new();
        let mut last_host_key = pending.region_last_host_key;
        loop {
            let next = updates.get(cursor).copied();
            match next {
                Some(SemanticOrderedUpdater::Native(native)) if invocations.is_empty() => {
                    let signal = self
                        .reactive_projection
                        .execution_signal_id(native.signal())
                        .expect("lowered native updater references a projected signal");
                    let value = self
                        .runtime
                        .prepared_reactive_value(&pending.prepared, signal)
                        .expect("lowered native updater has a prepared signal value");
                    let write = native_effective_write(native.target(), native.property(), value);
                    if overlay.object(native.target()).is_none() {
                        let CallbackReadValue::Object(object) = self.required_callback_read(
                            pending.token,
                            CallbackReadRequest::Object(native.target()),
                        )?
                        else {
                            unreachable!("object request returns object properties")
                        };
                        overlay.cache_read_object(native.target(), object);
                    }
                    overlay.write(write)?;
                    cursor += 1;
                }
                Some(SemanticOrderedUpdater::Native(_)) => break,
                Some(SemanticOrderedUpdater::Host(index)) => {
                    // Only fuse hosts when the compiler index proves that no
                    // unchanged native declaration lies between them. A host
                    // write can make that declaration observable even if it
                    // was not part of the initial dirty binding set.
                    let key = self.ordered_update_key(SemanticOrderedUpdater::Host(index));
                    if !invocations.is_empty()
                        && self
                            .callback_schedule
                            .plan
                            .has_native_binding_between(last_host_key, key)
                    {
                        break;
                    }
                    let occurrence = self.callback_schedule.plan.occurrence(index);
                    invocations.push(RequiredCallbackInvocation {
                        occurrence_index: index,
                        callback_id: occurrence.callback_id(),
                        target: occurrence.target(),
                    });
                    last_host_key = key;
                    cursor += 1;
                }
                None => break,
            }
        }
        // Validate the complete ordered prefix while it is still local. A bad
        // native value or stale row cannot advance the pending cursor.
        self.prepare_callback_writes(
            EffectivePropertyBatch::new(pending.token, overlay.writes.iter().copied())
                .with_region(pending.region),
        )?;
        let advance = if invocations.is_empty() {
            CallbackRegionAdvance::Complete(
                EffectivePropertyBatch::new(pending.token, overlay.writes.iter().copied())
                    .with_region(overlay.region),
            )
        } else {
            overlay.region_start = overlay.writes.len();
            CallbackRegionAdvance::HostRequired {
                invocations,
                overlay: overlay.clone(),
            }
        };
        Ok((cursor, last_host_key, overlay, advance))
    }

    /// Admit one host region without publishing. The response contains only
    /// writes made in that region and carries its exact region index.
    pub fn submit_required_callback_region(
        &mut self,
        batch: EffectivePropertyBatch,
    ) -> Result<CallbackRegionAdvance, ExecutionSessionCallbackError> {
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
        if pending.completed && pending.last_region_batch.as_ref() == Some(&batch) {
            return Ok(CallbackRegionAdvance::Complete(
                EffectivePropertyBatch::new(pending.token, pending.overlay.writes.iter().copied())
                    .with_region(pending.region),
            ));
        }
        if pending.completed {
            return Err(ExecutionSessionCallbackError::IncompleteRegion);
        }
        if batch.region != pending.region {
            return Err(ExecutionSessionCallbackError::StaleRegion {
                expected: pending.region,
                actual: batch.region,
            });
        }
        let mut overlay = pending.overlay.clone();
        for &write in &batch.writes {
            if overlay.object(write.object()).is_none() {
                let CallbackReadValue::Object(object) = self
                    .required_callback_read(
                        pending.token,
                        CallbackReadRequest::Object(write.object()),
                    )
                    .map_err(|error| match error {
                        ExecutionSessionCallbackReadError::UnknownObject(object) => {
                            ExecutionSessionCallbackError::UnknownObject(object)
                        }
                        other => ExecutionSessionCallbackError::Read(other),
                    })?
                else {
                    unreachable!("object request returns object properties")
                };
                overlay.cache_read_object(write.object(), object);
            }
            overlay.write(write)?;
        }
        let next_region = pending
            .region
            .checked_add(1)
            .ok_or(ExecutionSessionCallbackError::SequenceExhausted)?;
        overlay.region = next_region;
        let mut updates = pending.ordered_updates.clone();
        let mut added = BTreeMap::new();
        for &write in &batch.writes {
            for native in self
                .callback_schedule
                .plan
                .native_updaters_for_target(write.object())
            {
                if native.sort_key() > pending.region_last_host_key
                    && callback_write_domains(write) & native_write_domain(native.property()) != 0
                {
                    added.insert(native.sort_key(), native);
                }
            }
        }
        for (key, native) in added {
            let suffix = &updates[pending.next_update..];
            match suffix.binary_search_by_key(&key, |&update| self.ordered_update_key(update)) {
                Ok(_) => {}
                Err(position) => updates.insert(
                    pending.next_update + position,
                    SemanticOrderedUpdater::Native(native),
                ),
            }
        }
        let (cursor, last_host_key, overlay, advance) =
            self.stage_next_callback_region(pending, &updates, pending.next_update, overlay)?;
        let pending = self
            .pending_callback
            .as_mut()
            .expect("validated region retains pending phase");
        pending.region = next_region;
        pending.ordered_updates = updates;
        pending.next_update = cursor;
        pending.region_last_host_key = last_host_key;
        pending.overlay = overlay;
        if matches!(advance, CallbackRegionAdvance::Complete(_)) {
            pending.completed = true;
            pending.last_region_batch = Some(batch);
        }
        Ok(advance)
    }

    /// Stage one forward timeline/native phase and return an owned sparse callback
    /// read/write overlay. The public frame and renderer publication remain pinned
    /// until the matching effective batch commits.
    pub fn begin_required_callback_phase(
        &mut self,
        time: f64,
        read_objects: impl IntoIterator<Item = SemanticNodeId>,
    ) -> Result<CallbackPhaseOverlay, ExecutionSessionCallbackError> {
        self.begin_required_callback_phase_with_schedule(time, read_objects, None, false)
    }

    fn begin_required_callback_phase_with_schedule(
        &mut self,
        time: f64,
        read_objects: impl IntoIterator<Item = SemanticNodeId>,
        schedule: Option<CallbackSchedulePreview>,
        defer_reactive_bindings: bool,
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
        let prepared = self
            .runtime
            .prepare_advance_to_with_reactive_inputs_deferred(
                time,
                signal_timeline
                    .as_ref()
                    .map_or(&[], SignalTimelinePreview::inputs),
                defer_reactive_bindings,
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
        let overlay = CallbackPhaseOverlay {
            token,
            region: 0,
            region_start: 0,
            time,
            delta_time,
            objects,
            writes: Vec::new(),
            staged_rows,
            prior_driver_rows,
        };
        self.pending_callback = Some(PendingCallbackPhase {
            token,
            prepared,
            schedule,
            signal_timeline,
            ordered_updates: Vec::new(),
            next_update: 0,
            region: 0,
            region_last_host_key: (0, 0),
            overlay: overlay.clone(),
            completed: false,
            last_region_batch: None,
        });
        Ok(overlay)
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
        let batch = self.complete_callback_batch_for_commit(batch)?;
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

    /// Commit one callback-produced content version with the native/property
    /// result at the same publication boundary. A failed content preparation
    /// leaves the exact pending phase retryable.
    pub fn commit_required_callback_phase_with_content(
        &mut self,
        batch: EffectivePropertyBatch,
        target: SemanticNodeId,
        content: ObjectContentRef,
        text_bounds: Option<Rect>,
        lease: Option<EffectiveContentLease>,
    ) -> Result<EffectiveContentLease, ExecutionSessionCallbackError> {
        let batch = self.complete_callback_batch_for_commit(batch)?;
        let (effective, receipt_domains) = self.prepare_callback_writes(batch)?;
        let object = self
            .execution_index
            .execution_object_id(target)
            .ok_or(ExecutionSessionCallbackError::UnknownObject(target))?;
        let replacement = self
            .runtime
            .prepare_effective_content_replacement(object, content, text_bounds, lease)
            .map_err(ExecutionSessionCallbackError::Content)?;
        self.commit_prepared_callback_content(effective, receipt_domains, replacement)
    }

    fn commit_prepared_callback_content(
        &mut self,
        effective: noon_runtime::PreparedEffectivePropertyBatch,
        receipt_domains: BTreeMap<SemanticNodeId, u8>,
        replacement: noon_runtime::PreparedEffectiveContentReplacement,
    ) -> Result<EffectiveContentLease, ExecutionSessionCallbackError> {
        self.runtime
            .preflight_prepared_frame_with_content(
                &self
                    .pending_callback
                    .as_ref()
                    .expect("phase remains pending")
                    .prepared,
                &effective,
                &replacement,
            )
            .map_err(ExecutionSessionCallbackError::ContentCommit)?;
        let pending = self
            .pending_callback
            .take()
            .expect("pending phase remained live throughout content preflight");
        let (frame, completion) = pending.into_parts();
        let lease = self
            .runtime
            .commit_prepared_frame_with_content(frame, effective, replacement)
            .expect("preflighted callback remains valid during synchronous commit");
        self.finish_callback_publication(completion, receipt_domains);
        Ok(lease)
    }

    fn callback_content_lease(
        &self,
        target: SemanticNodeId,
        object: noon_core::ObjectId,
    ) -> Result<Option<EffectiveContentLease>, ExecutionSessionCallbackError> {
        let known = self.callback_content_leases.get(&target).copied();
        let current = self.runtime.effective_content_lease(object);
        match (known, current) {
            (None, None) | (Some(_), None) => Ok(None),
            (Some(known), Some(current)) if known == current => Ok(Some(current)),
            _ => Err(ExecutionSessionCallbackError::Content(
                EffectiveContentError::DriverConflict(object),
            )),
        }
    }

    /// The callback session owns this producer lease across frames. A foreign
    /// effective-content owner remains a conflict rather than being adopted.
    pub fn commit_required_callback_phase_with_owned_content(
        &mut self,
        batch: EffectivePropertyBatch,
        target: SemanticNodeId,
        content: ObjectContentRef,
    ) -> Result<EffectiveContentLease, ExecutionSessionCallbackError> {
        let object = self
            .execution_index
            .execution_object_id(target)
            .ok_or(ExecutionSessionCallbackError::UnknownObject(target))?;
        let lease = self.callback_content_lease(target, object)?;
        let lease =
            self.commit_required_callback_phase_with_content(batch, target, content, None, lease)?;
        self.callback_content_leases.insert(target, lease);
        Ok(lease)
    }

    /// Commit a small set of producer-owned inline content rows with one
    /// callback property batch and one publication. Every target is prepared
    /// and lease-checked before the pending phase is consumed.
    pub fn commit_required_callback_phase_with_owned_contents(
        &mut self,
        batch: EffectivePropertyBatch,
        replacements: Vec<(SemanticNodeId, ObjectContentRef)>,
    ) -> Result<Vec<EffectiveContentLease>, ExecutionSessionCallbackError> {
        let batch = self.complete_callback_batch_for_commit(batch)?;
        let (effective, receipt_domains) = self.prepare_callback_writes(batch)?;
        let mut seen = BTreeSet::new();
        let mut prepared = Vec::with_capacity(replacements.len());
        let mut targets = Vec::with_capacity(replacements.len());
        for (target, content) in replacements {
            if !seen.insert(target) {
                return Err(ExecutionSessionCallbackError::Content(
                    EffectiveContentError::DuplicateTarget(
                        self.execution_index
                            .execution_object_id(target)
                            .ok_or(ExecutionSessionCallbackError::UnknownObject(target))?,
                    ),
                ));
            }
            let object = self
                .execution_index
                .execution_object_id(target)
                .ok_or(ExecutionSessionCallbackError::UnknownObject(target))?;
            let lease = self.callback_content_lease(target, object)?;
            prepared.push(
                self.runtime
                    .prepare_effective_content_replacement(object, content, None, lease)
                    .map_err(ExecutionSessionCallbackError::Content)?,
            );
            targets.push(target);
        }
        let pending = self
            .pending_callback
            .as_ref()
            .expect("completed phase remains pending");
        self.runtime
            .preflight_prepared_frame_with_contents(&pending.prepared, &effective, &prepared)
            .map_err(ExecutionSessionCallbackError::ContentCommit)?;
        let pending = self
            .pending_callback
            .take()
            .expect("phase remained pending through preflight");
        let (frame, completion) = pending.into_parts();
        let leases = self
            .runtime
            .commit_prepared_frame_with_contents(frame, effective, prepared)
            .expect("preflighted callback batch remains valid during synchronous commit");
        for (target, lease) in targets.into_iter().zip(leases.iter().copied()) {
            self.callback_content_leases.insert(target, lease);
        }
        self.finish_callback_publication(completion, receipt_domains);
        Ok(leases)
    }

    /// Commit callback-produced external geometry through the same session-owned
    /// effective-content lease used by analytic and text content results.
    pub fn commit_required_callback_phase_with_owned_geometry(
        &mut self,
        batch: EffectivePropertyBatch,
        target: SemanticNodeId,
        handle: noon_core::GeometryResourceHandle,
        source: &noon_core::GeometryResourceArena,
    ) -> Result<EffectiveContentLease, ExecutionSessionCallbackError> {
        let object = self
            .execution_index
            .execution_object_id(target)
            .ok_or(ExecutionSessionCallbackError::UnknownObject(target))?;
        let lease = self.callback_content_lease(target, object)?;
        let batch = self.complete_callback_batch_for_commit(batch)?;
        let (effective, receipt_domains) = self.prepare_callback_writes(batch)?;
        let replacement = self
            .runtime
            .prepare_effective_geometry_replacement(object, handle, source, lease)
            .map_err(ExecutionSessionCallbackError::Content)?;
        let lease =
            self.commit_prepared_callback_content(effective, receipt_domains, replacement)?;
        self.callback_content_leases.insert(target, lease);
        Ok(lease)
    }

    /// Publish prebuilt text through the same callback-owned content lease.
    pub fn commit_required_callback_phase_with_owned_text(
        &mut self,
        batch: EffectivePropertyBatch,
        target: SemanticNodeId,
        handle: noon_core::TextResourceHandle,
        texts: &noon_core::TextResourceArena,
        fonts: &noon_core::FontResourceArena,
        geometries: &noon_core::GeometryResourceArena,
    ) -> Result<EffectiveContentLease, ExecutionSessionCallbackError> {
        let object = self
            .execution_index
            .execution_object_id(target)
            .ok_or(ExecutionSessionCallbackError::UnknownObject(target))?;
        let lease = self.callback_content_lease(target, object)?;
        let batch = self.complete_callback_batch_for_commit(batch)?;
        let (effective, receipt_domains) = self.prepare_callback_writes(batch)?;
        let replacement = self
            .runtime
            .prepare_effective_text_replacement(object, handle, texts, fonts, geometries, lease)
            .map_err(ExecutionSessionCallbackError::Content)?;
        let lease =
            self.commit_prepared_callback_content(effective, receipt_domains, replacement)?;
        self.callback_content_leases.insert(target, lease);
        Ok(lease)
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
        if batch.region != pending.region {
            return Err(ExecutionSessionCallbackError::StaleRegion {
                expected: pending.region,
                actual: batch.region,
            });
        }
        let mut receipt_domains = BTreeMap::new();
        let mut writes = Vec::with_capacity(batch.writes.len());
        for write in batch.writes {
            let semantic = write.object();
            if matches!(write, EffectiveSemanticPropertyWrite::WorldTransform { .. }) {
                return Err(ExecutionSessionCallbackError::UnsupportedWorldTransform(
                    semantic,
                ));
            }
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

    fn complete_callback_batch_for_commit(
        &mut self,
        batch: EffectivePropertyBatch,
    ) -> Result<EffectivePropertyBatch, ExecutionSessionCallbackError> {
        let pending = self
            .pending_callback
            .as_ref()
            .ok_or(ExecutionSessionCallbackError::NoPendingPhase)?;
        if pending.ordered_updates.is_empty() || pending.completed {
            return Ok(batch);
        }
        if batch.token != pending.token {
            return Err(ExecutionSessionCallbackError::StaleToken {
                expected: pending.token,
                actual: batch.token,
            });
        }
        if batch.region != pending.region {
            return Err(ExecutionSessionCallbackError::StaleRegion {
                expected: pending.region,
                actual: batch.region,
            });
        }
        if pending.ordered_updates[pending.next_update..]
            .iter()
            .any(|update| matches!(update, SemanticOrderedUpdater::Host(_)))
        {
            return Err(ExecutionSessionCallbackError::IncompleteRegion);
        }
        match self.submit_required_callback_region(batch)? {
            CallbackRegionAdvance::Complete(batch) => Ok(batch),
            CallbackRegionAdvance::HostRequired { .. } => {
                unreachable!("no later host update was present")
            }
        }
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
                    self.callback_content_leases.remove(&target);
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
        let batch = self.complete_callback_batch_for_commit(batch)?;
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
mod tests;
