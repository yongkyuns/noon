//! Opt-in experimental prefix delivery over the existing finite replay history.
//! One execution bootstrap is followed by changed-payload batches, not snapshots.
//! The reader accepts future revisions without seeking to the producer frontier.
use super::{ReplayError, ReplayLimits, ReplayStats, Revision};
use crate::{
    EvaluationError, FrameState, PreparedTransientAnimation, RendererPublication, RuntimeIdentity,
    SceneInstance,
};
use noon_compile::{CompiledObject, CompiledResourceError, CompiledResources};
use noon_core::{GeometryRef, ObjectId, SemanticClickIndicate};
use std::sync::Arc;

mod storage;

#[derive(Debug, PartialEq)]
pub enum PrefixError {
    Replay(ReplayError),
    Evaluation(EvaluationError),
    SourceChanged,
    NotReady,
    UnexpectedBatch,
    OutsideRange {
        time: f64,
        start: f64,
        end_exclusive: f64,
    },
    Allocation,
    CapacityRequired {
        object_slots: usize,
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
    frontier: f64,
    object_slots: usize,
    resources: CompiledResources,
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
    revisions: Vec<Revision>,
    stats: PrefixBatchStats,
    from_object_slots: usize,
    new_objects: Vec<CompiledObject>,
    resources: CompiledResources,
}
impl PrefixBatch {
    /// Reserve explicitly before append when additional object storage is needed.
    pub fn required_object_capacity(&self) -> usize {
        self.from_object_slots + self.new_objects.len()
    }
    pub fn stats(&self) -> PrefixBatchStats {
        self.stats
    }
    pub fn frontier(&self) -> f64 {
        self.frontier
    }
}

/// A derived historical reader, with no mutable/source/runtime escape hatch.
/// Presentation-only interaction uses the existing runtime without editing history.
/// The interval is [start, frontier). Source can still mutate at the frontier's
/// exact timestamp. Terminal endpoint publication is a separate future contract.
#[derive(Debug)]
pub struct PrefixReader {
    runtime: SceneInstance,
    stream: Arc<()>,
    start: f64,
    frontier: f64,
    received_revisions: usize,
    limits: ReplayLimits,
    object_capacity: usize,
    initial_object_slots: usize,
}
impl PrefixReader {
    pub fn frontier(&self) -> f64 {
        self.frontier
    }
    pub fn frame(&self) -> &FrameState {
        self.runtime.frame()
    }
    pub fn replay_stats(&self) -> ReplayStats {
        self.runtime.replay_stats()
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, PrefixError> {
        self.validate_time(time)?;
        self.runtime.extend_prefix_frame_storage();
        self.runtime.seek(time)?;
        Ok(self.runtime.frame())
    }

    /// Advance a ready historical interval without treating every frame as a seek.
    /// Within one execution revision, disjoint session effects keep running;
    /// authored-driver conflicts and historical revision changes retain precedence.
    /// Use `seek` explicitly to rewind or cancel those effects.
    /// Invalid or unavailable times leave the displayed frame and effects intact.
    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, PrefixError> {
        self.validate_time(time)?;
        let current = self.runtime.frame().time;
        if time < current {
            return Err(EvaluationError::NonMonotonicPreparedAdvance {
                current,
                requested: time,
            }
            .into());
        }
        self.runtime.extend_prefix_frame_storage();
        self.runtime.advance_to(time)?;
        Ok(self.runtime.frame())
    }

    fn validate_time(&self, time: f64) -> Result<(), PrefixError> {
        if !time.is_finite() || time < self.start || time >= self.frontier {
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
        object: ObjectId,
        binding: SemanticClickIndicate,
    ) -> Result<Option<PreparedTransientAnimation>, PrefixError> {
        let row = self.displayed_interaction_target(object)?;
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
        self.runtime
            .prepare_click_indicate(object, binding)
            .map_err(PrefixError::Interaction)
    }

    /// Prepare the authored click action from the displayed historical revision.
    /// A visible target with no binding returns `None`. Future binding changes do
    /// not take effect until playback selects their revision. Pointer collection,
    /// picking, and trigger synthesis remain owned by the common input session.
    pub fn prepare_bound_click_indicate(
        &self,
        object: ObjectId,
    ) -> Result<Option<PreparedTransientAnimation>, PrefixError> {
        self.displayed_interaction_target(object)?;
        let Some(binding) = self.runtime.click_indicate_binding(object) else {
            return Ok(None);
        };
        self.prepare_click_indicate(object, binding)
    }

    fn displayed_interaction_target(
        &self,
        object: ObjectId,
    ) -> Result<&crate::FrameObjectState, PrefixError> {
        self.runtime
            .frame_index_for_object(object)
            .filter(|&index| {
                self.runtime.object_slot_is_live(index)
                    && self.runtime.frame().presences.get(index) == Some(&true)
            })
            .and_then(|index| self.runtime.frame().objects.get(index))
            .filter(|row| row.draw_enabled)
            .ok_or_else(|| {
                PrefixError::Interaction(
                    "interaction target is not live in the displayed prefix".into(),
                )
            })
    }

    /// Admit only a prepared effect belonging to this reader and its displayed
    /// historical state. This does not publish a source or replay revision.
    pub fn start_transient_animation(
        &mut self,
        prepared: PreparedTransientAnimation,
    ) -> Result<(), PrefixError> {
        self.runtime
            .start_transient_animation(prepared)
            .map_err(PrefixError::Interaction)
    }

    /// Advance session effects using platform monotonic seconds, including while
    /// authored playback is paused. No authored time or retained history changes.
    pub fn advance_interactions(&mut self, wall_time: f64) -> Result<(), PrefixError> {
        self.runtime
            .advance_interactions(wall_time)
            .map_err(PrefixError::Interaction)
    }

    pub fn interactions_active(&self) -> bool {
        self.runtime.interactions_active()
    }

    pub fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        self.runtime.take_renderer_publication()
    }

    /// Install only future history. The displayed frame, runtime identity,
    /// evaluation cursor and renderer dirty state are unchanged. All fallible
    /// validation precedes moving revisions into the already-reserved history.
    pub fn append(&mut self, batch: PrefixBatch) -> Result<PrefixBatchStats, PrefixError> {
        if !Arc::ptr_eq(&self.stream, &batch.stream)
            || batch.from_revision != self.received_revisions
            || batch.from_frontier != self.frontier
            || batch.frontier <= self.frontier
        {
            return Err(PrefixError::UnexpectedBatch);
        }
        if batch.from_object_slots != self.runtime.compiled.objects().len() {
            return Err(PrefixError::UnexpectedBatch);
        }
        let required = batch.required_object_capacity();
        if required > self.object_capacity {
            return Err(PrefixError::CapacityRequired {
                object_slots: required,
            });
        }
        self.runtime
            .compiled
            .resources()
            .preflight_merge(&batch.resources)?;
        let history = self
            .runtime
            .replay_history
            .as_mut()
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
        // Only this sealed reader owns its history; capacity was reserved once.
        debug_assert!(count <= history.revisions.capacity());
        self.runtime
            .compiled
            .merge_prepared_resources(batch.resources);
        self.runtime
            .compiled
            .install_prefix_tombstones(batch.new_objects);
        history.revisions.extend(batch.revisions);
        history.stats.revisions_retained = count;
        history.stats.payloads_retained = payloads;
        history.end = Some(batch.frontier);
        self.received_revisions = count;
        self.frontier = batch.frontier;
        Ok(batch.stats)
    }
}

impl SceneInstance {
    /// Low-level bootstrap. The owning ExecutionSession must additionally prove
    /// its source segment and callback completion BEFORE invoking this method.
    /// This experimental path excludes native-reactive bindings and external
    /// input. It copies the existing execution projection exactly once.
    #[doc(hidden)]
    pub fn create_prefix_reader(&self) -> Result<(PrefixPublisher, PrefixReader), PrefixError> {
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
        if self
            .reactive
            .as_ref()
            .is_some_and(|reactive| !reactive.is_empty())
        {
            return Err(ReplayError::UnsupportedDomain.into());
        }
        let stream = Arc::new(());
        let publisher = PrefixPublisher {
            source: self.runtime_identity(),
            scope: history.scope.clone(),
            stream: stream.clone(),
            next_revision: history.revisions.len(),
            frontier: self.frame.time,
            object_slots: self.compiled.objects().len(),
            resources: self.compiled.resources().clone(),
        };
        let mut runtime = self.clone();
        let copied = runtime
            .replay_history
            .as_mut()
            .expect("cloned finite history");
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
        runtime.seal_replay()?;
        runtime.seek(history.start)?;
        let reader = PrefixReader {
            runtime,
            stream,
            start: history.start,
            frontier: self.frame.time,
            received_revisions: history.revisions.len(),
            limits: history.limits,
            object_capacity: self.compiled.objects().len(),
            initial_object_slots: self.compiled.objects().len(),
        };
        Ok((publisher, reader))
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
        let history = self
            .replay_history
            .as_ref()
            .ok_or(ReplayError::NotRecording)?;
        if self.runtime_identity() != publisher.source
            || !Arc::ptr_eq(&history.scope, &publisher.scope)
        {
            return Err(PrefixError::SourceChanged);
        }
        if let Some(error) = history.failure {
            return Err(error.into());
        }
        if history.end.is_some()
            || history.applied != history.revisions.len()
            || self.frame.time <= publisher.frontier
        {
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
        if stats.revisions > history.limits.revisions || stats.payloads > history.limits.payloads {
            return Err(ReplayError::RetentionLimit.into());
        }
        let forward = self
            .compiled
            .forward_prefix_revisions(&inverse)
            .ok_or(ReplayError::UnsupportedDomain)?;
        let new_objects = self
            .compiled
            .objects()
            .get(publisher.object_slots..)
            .ok_or(PrefixError::SourceChanged)?
            .to_vec();
        let resources = self
            .compiled
            .prefix_resource_delta(&forward, &publisher.resources)?;
        stats.new_object_slots = new_objects.len();
        stats.resource_payloads = resources.prefix_payload_count();
        stats.payloads = stats
            .payloads
            .checked_add(stats.new_object_slots)
            .and_then(|cost| cost.checked_add(stats.resource_payloads))
            .ok_or(ReplayError::RetentionLimit)?;
        if stats.payloads > history.limits.payloads {
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
            revisions,
            stats,
            from_object_slots: publisher.object_slots,
            new_objects,
            resources,
        };
        publisher
            .resources
            .extend_prefix_knowledge(&batch.resources);
        publisher.object_slots = self.compiled.objects().len();
        publisher.next_revision = history.revisions.len();
        publisher.frontier = self.frame.time;
        Ok(batch)
    }
}
