//! Opt-in experimental prefix delivery over the existing finite replay history.
//! One execution bootstrap is followed by changed-payload batches, not snapshots.
//! The reader accepts future revisions without seeking to the producer frontier.
use super::{ReplayError, ReplayLimits, Revision};
use crate::{EvaluationError, PreparedTransientAnimation, RuntimeIdentity, SceneInstance};
use noon_compile::{CompiledObject, CompiledResourceError, CompiledResources};
use noon_core::{GeometryRef, ObjectId, SemanticClickIndicate};
use std::sync::Arc;

mod history_payload_budget;
mod object_budget;
mod resource_budget;
mod storage;
use history_payload_budget::HistoryPayloadBudget;
use resource_budget::ResourceBudget;

#[cfg(test)]
mod history_index_budget_tests;
#[cfg(test)]
mod history_payload_budget_tests;
#[cfg(test)]
mod inset_tests;
#[cfg(test)]
mod object_budget_tests;
#[cfg(test)]
mod payload_budget_tests;
#[cfg(test)]
mod resource_budget_tests;

#[derive(Debug, PartialEq)]
pub enum PrefixError {
    Replay(ReplayError),
    Evaluation(EvaluationError),
    SourceChanged,
    ReaderChanged,
    Slots(crate::ExecutionSlotError),
    NotReady,
    UnexpectedBatch,
    OutsideRange {
        time: f64,
        start: f64,
        end_exclusive: f64,
    },
    /// The source is complete; waiting for more preparation cannot admit this time.
    OutsideFinishedRange {
        time: f64,
        start: f64,
        end: f64,
    },
    Allocation,
    CapacityRequired {
        object_slots: usize,
    },
    /// Logical immutable-resource payloads exceed this stream's explicit cap.
    /// Existing history remains valid; this never evicts promised past resources.
    ResourceBudgetExceeded {
        retained_bytes: usize,
        additional_bytes: usize,
        limit_bytes: usize,
    },
    /// The preallocated revision index exceeds its byte ceiling. These fields
    /// remain representable even when capacity times entry size would overflow.
    HistoryIndexBudgetExceeded {
        revision_capacity: usize,
        bytes_per_revision: usize,
        limit_bytes: usize,
    },
    /// Saved row/channel/track vector capacity exceeds the stream ceiling.
    /// This excludes indirect payloads, the revision index and allocator peaks.
    HistoryPayloadBudgetExceeded {
        retained_bytes: usize,
        additional_bytes: usize,
        limit_bytes: usize,
    },
    /// Aggregate backing storage for compiled/runtime object vectors. `None`
    /// denotes checked arithmetic overflow, not an unknown/uncharged amount.
    ObjectStorageBudgetExceeded {
        object_slots: usize,
        required_bytes: Option<usize>,
        limit_bytes: usize,
    },
    Resources(CompiledResourceError),
    Interaction(String),
}
impl std::fmt::Display for PrefixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "prepared prefix unavailable: {self:?}")
    }
}
impl std::error::Error for PrefixError {}
impl From<ReplayError> for PrefixError {
    fn from(error: ReplayError) -> Self {
        Self::Replay(error)
    }
}
impl From<EvaluationError> for PrefixError {
    fn from(error: EvaluationError) -> Self {
        Self::Evaluation(error)
    }
}

impl From<CompiledResourceError> for PrefixError {
    fn from(error: CompiledResourceError) -> Self {
        Self::Resources(error)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrefixBatchStats {
    pub revisions: usize,
    pub payloads: usize,
    pub row_payloads: usize,
    pub new_object_slots: usize,
    /// Unique immutable resource entries delivered, not resource bytes.
    pub resource_payloads: usize,
}

/// Source-bound cursor only. Does not own a Semantic Scene or another evaluator.
/// Dropping a batch loses delivery; a new reader/bootstrap is then required.
/// Transport retry/acknowledgment policy is intentionally not implemented here.
#[derive(Debug)]
pub struct PrefixPublisher {
    source: RuntimeIdentity,
    scope: Arc<()>,
    stream: Arc<()>,
    next_revision: usize,
    // Reader-side charge includes bootstrap and all extracted batches, even
    // pending ones. Source inverse-history cost omits new slots/resources.
    extracted_payloads: usize,
    frontier: f64,
    finished: bool,
    object_slots: usize,
    resources: CompiledResources,
    resource_budget: ResourceBudget,
    history_payload_budget: HistoryPayloadBudget,
    object_vector_budget_bytes: usize,
}

impl PrefixPublisher {
    /// Saved revision-vector bytes extracted, including undelivered batches.
    /// Not total history/heap memory; see the compiler-owned accounting contract.
    pub fn extracted_history_payload_bytes(&self) -> usize {
        self.history_payload_budget.bytes()
    }

    /// Resource payload bytes extracted for this stream, including pending output.
    /// This is not a delivery receipt or process/allocator/GPU memory measurement.
    pub fn extracted_resource_bytes(&self) -> usize {
        self.resource_budget.bytes()
    }
    pub fn resource_budget_bytes(&self) -> usize {
        self.resource_budget.limit()
    }

    /// The last extracted frontier, not a displayed time or proof of delivery.
    pub fn frontier(&self) -> f64 {
        self.frontier
    }
}

/// An owned, ordered extension of one read-only historical projection.
/// Fields are private: callers cannot mint revisions, edit identities, or combine
/// streams. Moving it across threads requires no serialization or shared writes.
#[derive(Debug)]
pub struct PrefixBatch {
    stream: Arc<()>,
    from_revision: usize,
    from_frontier: f64,
    frontier: f64,
    terminal: bool,
    revisions: Vec<Revision>,
    stats: PrefixBatchStats,
    from_object_slots: usize,
    new_objects: Vec<CompiledObject>,
    resources: CompiledResources,
    resource_bytes: usize,
    history_payload_bytes: usize,
}
impl PrefixBatch {
    /// Actual row/channel/track vector backing bytes moved into the reader.
    pub fn history_payload_bytes(&self) -> usize {
        self.history_payload_bytes
    }

    /// Newly introduced immutable payload bytes; known resource handles cost zero.
    pub fn resource_bytes(&self) -> usize {
        self.resource_bytes
    }

    /// Reserve explicitly before append when additional object storage is needed.
    pub fn required_object_capacity(&self) -> usize {
        self.from_object_slots + self.new_objects.len()
    }
    /// Inspect only the newly delivered physical rows, in storage order.
    /// These keys are not live targets or proof of stream membership. A session
    /// must validate the batch before staging identity slots and must still use
    /// the displayed revision for picking. No existing rows are scanned.
    pub fn new_object_ids(&self) -> impl ExactSizeIterator<Item = ObjectId> + '_ {
        self.new_objects.iter().map(|object| object.id)
    }

    pub fn stats(&self) -> PrefixBatchStats {
        self.stats
    }
    pub fn frontier(&self) -> f64 {
        self.frontier
    }

    /// Whether successful adoption closes this stream at its exact frontier.
    /// Inspection alone neither installs the suffix nor marks a reader finished.
    pub fn is_terminal(&self) -> bool {
        self.terminal
    }
}

/// Ordered receive cursor for one independent historical runtime incarnation.
///
/// The cursor owns no scene, evaluator, frame, or input policy. The enclosing
/// session owns the returned `SceneInstance` and borrows it for checked delivery
/// and readiness admission. It must not expose unguarded authored-time mutation
/// through its public reader facade. A foreign, cloned, or unsealed runtime is
/// rejected rather than silently rebinding this cursor.
#[derive(Debug)]
pub struct PrefixReceiver {
    runtime: RuntimeIdentity,
    scope: Arc<()>,
    stream: Arc<()>,
    start: f64,
    frontier: f64,
    finished: bool,
    received_revisions: usize,
    limits: ReplayLimits,
    object_capacity: usize,
    initial_object_slots: usize,
    resource_budget: ResourceBudget,
    history_payload_budget: HistoryPayloadBudget,
    object_vector_budget_bytes: usize,
}
impl PrefixReceiver {
    /// Saved revision-vector bytes held by this reader, including future history.
    /// Seeking or removing visible membership does not release this charge.
    pub fn retained_history_payload_bytes(&self) -> usize {
        self.history_payload_budget.bytes()
    }

    /// Logical immutable-resource bytes pinned by the adopted history, not heap/VRAM.
    /// Future resources remain charged even while an older frame is displayed.
    pub fn retained_resource_bytes(&self) -> usize {
        self.resource_budget.bytes()
    }
    pub fn resource_budget_bytes(&self) -> usize {
        self.resource_budget.limit()
    }

    fn validate_runtime(&self, runtime: &SceneInstance) -> Result<(), PrefixError> {
        let valid = runtime.runtime_identity() == self.runtime
            && runtime.replay_history.as_ref().is_some_and(|history| {
                Arc::ptr_eq(&history.scope, &self.scope)
                    && history.failure.is_none()
                    && history.end == Some(self.frontier)
                    && history.revisions.len() == self.received_revisions
            });
        if valid {
            Ok(())
        } else {
            Err(PrefixError::ReaderChanged)
        }
    }

    pub fn frontier(&self) -> f64 {
        self.frontier
    }
    /// Exact, inclusive endpoint only after this reader adopts the terminal batch.
    /// Producer completion or destruction alone does not change this observation.
    pub fn end_time(&self) -> Option<f64> {
        self.finished.then_some(self.frontier)
    }

    /// Admit an explicit seek before the session evaluates it through its ordinary
    /// seek path. All rejection precedes storage growth or interaction cancellation.
    pub fn prepare_seek(&self, runtime: &mut SceneInstance, time: f64) -> Result<(), PrefixError> {
        self.validate_runtime(runtime)?;
        self.validate_time(time)?;
        runtime.extend_prefix_frame_storage();
        Ok(())
    }

    /// Admit monotonic forward playback. The session still owns evaluation and
    /// gesture/selection reconciliation; this cursor owns readiness, not a clock.
    pub fn prepare_advance(
        &self,
        runtime: &mut SceneInstance,
        time: f64,
    ) -> Result<(), PrefixError> {
        self.validate_runtime(runtime)?;
        self.validate_time(time)?;
        let current = runtime.frame().time;
        if time < current {
            return Err(EvaluationError::NonMonotonicPreparedAdvance {
                current,
                requested: time,
            }
            .into());
        }
        runtime.extend_prefix_frame_storage();
        Ok(())
    }

    fn validate_time(&self, time: f64) -> Result<(), PrefixError> {
        if !time.is_finite()
            || time < self.start
            || time > self.frontier
            || (time == self.frontier && !self.finished)
        {
            if self.finished {
                return Err(PrefixError::OutsideFinishedRange {
                    time,
                    start: self.start,
                    end: self.frontier,
                });
            }
            return Err(PrefixError::OutsideRange {
                time,
                start: self.start,
                end_exclusive: self.frontier,
            });
        }
        Ok(())
    }

    /// Prepare the existing bounded Indicate effect against the displayed past,
    /// independently of the producer's current membership or authored bindings.
    /// Only filled analytic circles and rectangles are supported; the prefix
    /// itself already excludes reactive drivers. Unsupported targets are rejected
    /// before preparing or changing any effect.
    /// Newly appended future slots are not interaction targets until displayed.
    pub fn prepare_click_indicate(
        &self,
        runtime: &SceneInstance,
        object: ObjectId,
        binding: SemanticClickIndicate,
    ) -> Result<Option<PreparedTransientAnimation>, PrefixError> {
        let row = self.displayed_interaction_target(runtime, object)?;
        // SceneInstance preparation normally follows semantic binding admission.
        // This direct reader API must preserve that bounded target domain itself.
        if row.style.fill.is_none()
            || !matches!(
                row.content.geometry(),
                Some(GeometryRef::Circle { .. } | GeometryRef::Rectangle { .. })
            )
        {
            return Err(PrefixError::Interaction(
                "prefix Indicate requires a filled analytic circle or rectangle".into(),
            ));
        }
        runtime
            .prepare_click_indicate(object, binding)
            .map_err(PrefixError::Interaction)
    }

    /// Prepare the authored click action from the displayed historical revision.
    /// A visible target with no binding returns `None`. Future binding changes do
    /// not take effect until playback selects their revision. Pointer collection,
    /// picking, and trigger synthesis remain owned by the common input session.
    pub fn prepare_bound_click_indicate(
        &self,
        runtime: &SceneInstance,
        object: ObjectId,
    ) -> Result<Option<PreparedTransientAnimation>, PrefixError> {
        self.displayed_interaction_target(runtime, object)?;
        let Some(binding) = runtime.click_indicate_binding(object) else {
            return Ok(None);
        };
        self.prepare_click_indicate(runtime, object, binding)
    }

    fn displayed_interaction_target<'a>(
        &self,
        runtime: &'a SceneInstance,
        object: ObjectId,
    ) -> Result<&'a crate::FrameObjectState, PrefixError> {
        self.validate_runtime(runtime)?;
        runtime
            .frame_index_for_object(object)
            .filter(|&index| {
                runtime.object_slot_is_live(index)
                    && runtime.frame().presences.get(index) == Some(&true)
            })
            .and_then(|index| runtime.frame().objects.get(index))
            .filter(|row| row.draw_enabled)
            .ok_or_else(|| {
                PrefixError::Interaction(
                    "interaction target is not live in the displayed prefix".into(),
                )
            })
    }

    /// Validate a borrowed batch without consuming it or changing the reader.
    /// Capacity can be reserved explicitly after `CapacityRequired`, then the
    /// same owned batch can be checked and delivered without another bootstrap.
    /// An out-of-order or foreign batch likewise remains with its caller.
    ///
    /// This is an observation, not a reservation or a reusable admission token.
    /// `append` always validates against the then-current stream and budgets.
    /// Reuses existing resource validation without scanning frames or history.
    pub fn preflight_append(
        &self,
        runtime: &SceneInstance,
        batch: &PrefixBatch,
    ) -> Result<PrefixBatchStats, PrefixError> {
        self.validate_append(runtime, batch).map(|_| batch.stats)
    }

    fn validate_append(
        &self,
        runtime: &SceneInstance,
        batch: &PrefixBatch,
    ) -> Result<(usize, usize, ResourceBudget, HistoryPayloadBudget), PrefixError> {
        self.validate_runtime(runtime)?;
        if self.finished
            || !Arc::ptr_eq(&self.stream, &batch.stream)
            || batch.from_revision != self.received_revisions
            || batch.from_frontier != self.frontier
            || batch.frontier < self.frontier
            || (batch.frontier == self.frontier && !batch.terminal)
        {
            return Err(PrefixError::UnexpectedBatch);
        }
        if batch.from_object_slots != runtime.compiled.objects().len() {
            return Err(PrefixError::UnexpectedBatch);
        }
        let resource_budget = self.resource_budget.with_additional(batch.resource_bytes)?;
        let history_payload_budget = self
            .history_payload_budget
            .with_additional(batch.history_payload_bytes)?;
        let history = runtime
            .replay_history
            .as_ref()
            .expect("private reader history");
        let count = history
            .revisions
            .len()
            .checked_add(batch.stats.revisions)
            .ok_or(ReplayError::RetentionLimit)?;
        let payloads = history
            .stats
            .payloads_retained
            .checked_add(batch.stats.payloads)
            .ok_or(ReplayError::RetentionLimit)?;
        if count > self.limits.revisions || payloads > self.limits.payloads {
            return Err(ReplayError::RetentionLimit.into());
        }
        runtime
            .compiled
            .resources()
            .preflight_merge(&batch.resources)?;
        // CapacityRequired is actionable only after permanent admission checks.
        // Never request storage growth for a batch the retained history cannot fit.
        let required = batch.required_object_capacity();
        object_budget::preflight_reserved(runtime, required, self.object_vector_budget_bytes)?;
        if required > self.object_capacity {
            return Err(PrefixError::CapacityRequired {
                object_slots: required,
            });
        }
        Ok((count, payloads, resource_budget, history_payload_budget))
    }

    /// Install only future history. The displayed frame, runtime identity,
    /// evaluation cursor and renderer dirty state are unchanged. All fallible
    /// validation precedes moving revisions into the already-reserved history.
    /// This consumes the batch even on failure; use `preflight_append` first
    /// when delivery must remain recoverable after capacity/order rejection.
    pub fn append(
        &mut self,
        runtime: &mut SceneInstance,
        batch: PrefixBatch,
    ) -> Result<PrefixBatchStats, PrefixError> {
        let (count, payloads, resource_budget, history_payload_budget) =
            self.validate_append(runtime, &batch)?;
        let history = runtime
            .replay_history
            .as_mut()
            .expect("private reader history");
        // Only this sealed reader owns its history; capacity was reserved once.
        debug_assert!(count <= history.revisions.capacity());
        runtime.compiled.merge_prepared_resources(batch.resources);
        runtime
            .compiled
            .install_prefix_tombstones(batch.new_objects);
        history.revisions.extend(batch.revisions);
        history.stats.revisions_retained = count;
        history.stats.payloads_retained = payloads;
        history.end = Some(batch.frontier);
        self.resource_budget = resource_budget;
        self.history_payload_budget = history_payload_budget;
        self.received_revisions = count;
        self.frontier = batch.frontier;
        self.finished = batch.terminal;
        Ok(batch.stats)
    }
}

impl SceneInstance {
    /// Provisional experimental cap for logical immutable-resource payloads.
    /// Not a total memory limit: revisions, object/storage capacity, allocator
    /// overhead, source allocations and renderer residency require separate caps.
    pub const DEFAULT_PREFIX_RESOURCE_BUDGET_BYTES: usize = 64 * 1024 * 1024;

    /// Provisional ceiling for the reader's preallocated revision index,
    /// including unused Vec capacity. This excludes nested row/track payloads,
    /// source history, object storage, allocator overhead and GPU residency.
    pub const MAX_PREFIX_HISTORY_INDEX_BYTES: usize = 64 * 1024 * 1024;

    /// Provisional aggregate ceiling for saved row/channel/track Vec storage.
    /// Separate from the index cap; excludes pointed-to/shared payloads, source
    /// allocations, active object/track storage, allocator overhead and GPU state.
    pub const MAX_PREFIX_HISTORY_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

    /// Low-level bootstrap. The owning ExecutionSession must additionally prove
    /// its source segment and callback completion BEFORE invoking this method.
    /// This experimental path excludes native-reactive bindings and external
    /// input. It copies the existing execution projection exactly once.
    #[doc(hidden)]
    pub fn create_prefix_projection(
        &self,
    ) -> Result<(PrefixPublisher, SceneInstance, PrefixReceiver), PrefixError> {
        self.create_prefix_projection_with_resource_budget(
            Self::DEFAULT_PREFIX_RESOURCE_BUDGET_BYTES,
        )
    }

    /// Same bootstrap with an explicit logical immutable-resource byte cap.
    /// Admission happens before cloning the projection or reserving its history.
    /// Zero permits resource-free scenes. No history is evicted to meet the cap.
    #[doc(hidden)]
    pub fn create_prefix_projection_with_resource_budget(
        &self,
        resource_budget_bytes: usize,
    ) -> Result<(PrefixPublisher, SceneInstance, PrefixReceiver), PrefixError> {
        self.create_prefix_projection_with_budgets(
            resource_budget_bytes,
            Self::MAX_PREFIX_HISTORY_PAYLOAD_BYTES,
        )
    }

    // Internal limit parameter permits small deterministic admission tests. No
    // additional frontend configuration or public migration mode is introduced.
    fn create_prefix_projection_with_budgets(
        &self,
        resource_budget_bytes: usize,
        history_payload_budget_bytes: usize,
    ) -> Result<(PrefixPublisher, SceneInstance, PrefixReceiver), PrefixError> {
        self.create_prefix_projection_with_object_budget(
            resource_budget_bytes,
            history_payload_budget_bytes,
            Self::MAX_PREFIX_OBJECT_VECTOR_BYTES,
        )
    }

    fn create_prefix_projection_with_object_budget(
        &self,
        resource_budget_bytes: usize,
        history_payload_budget_bytes: usize,
        object_vector_budget_bytes: usize,
    ) -> Result<(PrefixPublisher, SceneInstance, PrefixReceiver), PrefixError> {
        let history = self
            .replay_history
            .as_ref()
            .ok_or(ReplayError::NotRecording)?;
        if let Some(error) = history.failure {
            return Err(error.into());
        }
        if history.end.is_some()
            || history.applied != history.revisions.len()
            || self.frame.time <= history.start
        {
            return Err(PrefixError::NotReady);
        }
        // Root audio can retain visual revisions, but a separate reader still
        // needs audio resource-budget and output/frontier qualification (#2358).
        // Keep this gate explicit now that replay admission no longer excludes
        // every immutable audio plan.
        if self.has_audio()
            || self
                .reactive
                .as_ref()
                .is_some_and(|reactive| !reactive.is_empty())
        {
            return Err(ReplayError::UnsupportedDomain.into());
        }
        // Initial cloning includes retained inverse revisions. Checking only
        // active relations would admit an inset added and removed before this
        // boundary, then silently omit that view when the reader rewinds.
        if self.compiled.inset_2d_views().next().is_some()
            || history.revisions.iter().any(|revision| {
                revision
                    .inverse
                    .as_ref()
                    .is_some_and(|inverse| inverse.changes_inset_2d_view())
            })
        {
            return Err(ReplayError::UnsupportedDomain.into());
        }
        // Charge reserved entries, not just current history length. Reject a
        // pathological configured maximum before cloning any reader state.
        storage::preflight_history_index_capacity(
            history.limits.revisions.max(history.revisions.len()),
        )?;
        object_budget::preflight_clone(
            self,
            self.compiled.objects().len(),
            object_vector_budget_bytes,
        )?;
        let resource_bytes = self
            .compiled
            .resources()
            .prefix_resource_bytes()
            .ok_or(ReplayError::RetentionLimit)?;
        let resource_budget =
            ResourceBudget::new(resource_budget_bytes).with_additional(resource_bytes)?;
        let history_payload_budget = HistoryPayloadBudget::new(history_payload_budget_bytes);
        // Check the requested nested-vector storage before cloning the runtime.
        // Source spare capacity is not copied, so this pass uses occupied lengths.
        history_payload_budget.with_additional(history_payload_budget::clone_bytes(
            history
                .revisions
                .iter()
                .filter_map(|revision| revision.inverse.as_ref()),
        )?)?;
        let stream = Arc::new(());
        let mut runtime = self.clone();
        // Reserve every historical row, including inactive order slots, once.
        // Reuse the same explicit maintenance boundary as later suffix growth.
        storage::reserve_object_vectors(
            &mut runtime,
            self.compiled.objects().len(),
            object_vector_budget_bytes,
        )?;
        let copied = runtime
            .replay_history
            .as_mut()
            .expect("cloned finite history");
        // Validate reported capacities too, before exposing the reader. This
        // cannot bound an allocator's transient over-allocation or clone peak.
        let history_payload_budget =
            history_payload_budget.with_additional(history_payload_budget::retained_bytes(
                copied
                    .revisions
                    .iter()
                    .filter_map(|revision| revision.inverse.as_ref()),
            )?)?;
        // A named, bounded bootstrap allocation, not an unpredictable O(history)
        // relocation on every append. Payload budgets retain existing semantics.
        let extra = history
            .limits
            .revisions
            .saturating_sub(copied.revisions.len());
        copied
            .revisions
            .try_reserve_exact(extra)
            .map_err(|_| PrefixError::Allocation)?;
        // Vec may report more capacity than requested. Never publish an index
        // over the ceiling; this is not a bound on transient allocator peaks.
        storage::preflight_history_index_capacity(copied.revisions.capacity())?;
        runtime.seal_replay()?;
        runtime.seek(history.start)?;
        object_budget::preflight_reserved(&runtime, 0, object_vector_budget_bytes)?;
        let publisher = PrefixPublisher {
            source: self.runtime_identity(),
            scope: history.scope.clone(),
            stream: stream.clone(),
            next_revision: history.revisions.len(),
            extracted_payloads: history.stats.payloads_retained,
            frontier: self.frame.time,
            finished: false,
            object_slots: self.compiled.objects().len(),
            resources: self.compiled.resources().clone(),
            resource_budget,
            history_payload_budget,
            object_vector_budget_bytes,
        };
        let receiver = PrefixReceiver {
            runtime: runtime.runtime_identity(),
            scope: history.scope.clone(),
            stream,
            start: history.start,
            frontier: self.frame.time,
            finished: false,
            received_revisions: history.revisions.len(),
            limits: history.limits,
            object_capacity: self.compiled.objects().len(),
            initial_object_slots: self.compiled.objects().len(),
            resource_budget,
            history_payload_budget,
            object_vector_budget_bytes,
        };
        Ok((publisher, runtime, receiver))
    }

    /// Prepare an extension after the source owner validates another completed
    /// boundary. Reads only the NEW suffix, preserving every intermediate write.
    /// Unsupported domains fail without consuming the publisher cursor, changing
    /// the source, or damaging any already-delivered prefix.
    #[doc(hidden)]
    pub fn prepare_prefix_update(
        &self,
        publisher: &mut PrefixPublisher,
    ) -> Result<PrefixBatch, PrefixError> {
        self.prepare_prefix_batch(publisher, false)
    }

    /// Publish the final suffix from a sealed source at its actual endpoint.
    /// The owning session must still validate its source completion boundary.
    /// Equal-frontier publication is allowed here: it carries any last writes
    /// at that timestamp, or an empty terminal marker when no writes remain.
    /// Failure does not consume the cursor. Success closes this publisher.
    #[doc(hidden)]
    pub fn prepare_prefix_finish(
        &self,
        publisher: &mut PrefixPublisher,
    ) -> Result<PrefixBatch, PrefixError> {
        self.prepare_prefix_batch(publisher, true)
    }

    /// Validate source provenance before an interpreter advances or seals it.
    /// This read-only check deliberately permits an unfinished source segment.
    #[doc(hidden)]
    pub fn validate_prefix_publisher(
        &self,
        publisher: &PrefixPublisher,
    ) -> Result<(), PrefixError> {
        let history = self
            .replay_history
            .as_ref()
            .ok_or(ReplayError::NotRecording)?;
        if self.runtime_identity() != publisher.source
            || !Arc::ptr_eq(&history.scope, &publisher.scope)
        {
            return Err(PrefixError::SourceChanged);
        }
        Ok(())
    }

    fn prepare_prefix_batch(
        &self,
        publisher: &mut PrefixPublisher,
        terminal: bool,
    ) -> Result<PrefixBatch, PrefixError> {
        let history = self
            .replay_history
            .as_ref()
            .ok_or(ReplayError::NotRecording)?;
        self.validate_prefix_publisher(publisher)?;
        if let Some(error) = history.failure {
            return Err(error.into());
        }
        let ready = if terminal {
            history.end == Some(self.frame.time) && self.frame.time >= publisher.frontier
        } else {
            history.end.is_none() && self.frame.time > publisher.frontier
        };
        if publisher.finished || history.applied != history.revisions.len() || !ready {
            return Err(PrefixError::NotReady);
        }
        if self
            .reactive
            .as_ref()
            .is_some_and(|reactive| !reactive.is_empty())
        {
            return Err(ReplayError::UnsupportedDomain.into());
        }
        let suffix = history
            .revisions
            .get(publisher.next_revision..)
            .ok_or(PrefixError::SourceChanged)?;
        let mut inverse = Vec::with_capacity(suffix.len());
        let mut stats = PrefixBatchStats {
            revisions: suffix.len(),
            ..Default::default()
        };
        for revision in suffix {
            if revision.time < publisher.frontier || revision.time > self.frame.time {
                return Err(PrefixError::SourceChanged);
            }
            let saved = revision
                .inverse
                .as_ref()
                .ok_or(ReplayError::UnsupportedDomain)?;
            stats.payloads = stats
                .payloads
                .checked_add(saved.retention_cost())
                .ok_or(ReplayError::RetentionLimit)?;
            stats.row_payloads += saved.object_indices().count();
            inverse.push(saved);
        }
        let new_objects = self
            .compiled
            .objects()
            .get(publisher.object_slots..)
            .ok_or(PrefixError::SourceChanged)?;
        stats.new_object_slots = new_objects.len();
        stats.payloads = stats
            .payloads
            .checked_add(stats.new_object_slots)
            .ok_or(ReplayError::RetentionLimit)?;
        let payloads = publisher
            .extracted_payloads
            .checked_add(stats.payloads)
            .ok_or(ReplayError::RetentionLimit)?;
        // Reject the known cumulative cost before forward conversion or cloning
        // new rows. Resource closure entries are added below, once per handle.
        if history.revisions.len() > history.limits.revisions || payloads > history.limits.payloads
        {
            return Err(ReplayError::RetentionLimit.into());
        }
        // Reject known object-vector growth before converting/cloning the suffix
        // or committing this publisher. Reader spare capacity is checked again
        // by receiver admission and the explicit reservation barrier.
        object_budget::preflight_clone(
            self,
            self.compiled.objects().len(),
            publisher.object_vector_budget_bytes,
        )?;
        // Forward conversion preserves vector lengths, not necessarily their
        // reported capacities. Admit its known cost before allocating that copy.
        publisher
            .history_payload_budget
            .with_additional(history_payload_budget::clone_bytes(
                inverse.iter().copied(),
            )?)?;
        let forward = self
            .compiled
            .forward_prefix_revisions(&inverse)
            .ok_or(ReplayError::UnsupportedDomain)?;
        let history_payload_bytes = history_payload_budget::retained_bytes(forward.iter())?;
        let history_payload_budget = publisher
            .history_payload_budget
            .with_additional(history_payload_bytes)?;
        let resources = self
            .compiled
            .prefix_resource_delta(&forward, &publisher.resources)?;
        // Measure only the new dependency closure, never the retained arena.
        // Reject before moving the cursor or retaining new publisher pins.
        let resource_bytes = resources
            .prefix_resource_bytes()
            .ok_or(ReplayError::RetentionLimit)?;
        let resource_budget = publisher.resource_budget.with_additional(resource_bytes)?;
        stats.resource_payloads = resources.prefix_payload_count();
        stats.payloads = stats
            .payloads
            .checked_add(stats.resource_payloads)
            .ok_or(ReplayError::RetentionLimit)?;
        let extracted_payloads = payloads
            .checked_add(stats.resource_payloads)
            .ok_or(ReplayError::RetentionLimit)?;
        if extracted_payloads > history.limits.payloads {
            return Err(ReplayError::RetentionLimit.into());
        }
        let revisions = suffix
            .iter()
            .zip(forward)
            .map(|(source, forward)| Revision {
                time: source.time,
                inverse: Some(forward),
            })
            .collect();
        let batch = PrefixBatch {
            stream: publisher.stream.clone(),
            from_revision: publisher.next_revision,
            from_frontier: publisher.frontier,
            frontier: self.frame.time,
            terminal,
            revisions,
            stats,
            from_object_slots: publisher.object_slots,
            new_objects: new_objects.to_vec(),
            resources,
            resource_bytes,
            history_payload_bytes,
        };
        publisher
            .resources
            .extend_prefix_knowledge(&batch.resources);
        publisher.resource_budget = resource_budget;
        publisher.history_payload_budget = history_payload_budget;
        publisher.extracted_payloads = extracted_payloads;
        publisher.object_slots = self.compiled.objects().len();
        publisher.next_revision = history.revisions.len();
        publisher.frontier = self.frame.time;
        publisher.finished = terminal;
        Ok(batch)
    }
}
