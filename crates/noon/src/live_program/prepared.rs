//! Cooperative headless preparation over the existing Rust continuation owner.
//! One call resumes at most one source segment. Evaluation and completion still
//! use LiveProgram; the independent reader remains the only presentation owner.
use super::{LiveContinuation, LiveProgram, LiveProgramError, LiveProgramPhase, LiveProgramStatus};
use crate::{ExecutionSegment, PrefixBatch, PrefixError, PrefixPublisher, PrefixReader, Scene};
use noon_runtime::{ReplayError, ReplayLimits};

/// Producer lifecycle, not authored playback time or a rendering directive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedSourceStatus {
    Ready,
    ReaderReady,
    BatchReady,
    Finished,
    Failed,
    Cancelled,
}

#[derive(Debug)]
pub enum PreparedSourceError<E> {
    Lowering(noon_compile::SemanticExecutionLoweringError),
    Replay(ReplayError),
    Prefix(PrefixError),
    Program(LiveProgramError<E>),
    /// A headless prefix cannot admit externally prepared GPU resources.
    ResourcePreparationUnavailable,
    /// Invalid demand is rejected before invoking source code or changing state.
    InvalidPreparationTarget(f64),
}
impl<E: std::fmt::Display> std::fmt::Display for PreparedSourceError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lowering(e) => e.fmt(f),
            Self::Replay(e) => e.fmt(f),
            Self::Prefix(e) => e.fmt(f),
            Self::Program(e) => e.fmt(f),
            Self::InvalidPreparationTarget(time) => write!(f, "invalid preparation target {time}"),
            Self::ResourcePreparationUnavailable => {
                f.write_str("external shader preparation requires a rendering host")
            }
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for PreparedSourceError<E> {}

enum Pending {
    Reader(Box<PrefixReader>),
    Batch(Box<PrefixBatch>),
}

/// Consumes one scene and its ordinary LiveContinuation for finite preparation.
///
/// The producer retains at most one undelivered reader/batch. A pending output
/// applies backpressure: repeated preparation does not invoke source code again.
/// Deliver batches in order after borrowed admission; a rejected delivery can
/// keep the same batch here for retry. Cancellation/failure releases the source,
/// not any reader or batch that the caller has already taken.
///
/// This is cooperative, not preemptive. A single source call, completion, or
/// immutable-resource operation can be expensive. Hosts must yield between calls;
/// one segment per call is a work-count bound, not a wall-time guarantee.
pub struct PreparedSource<C> {
    program: Option<LiveProgram<C>>,
    publisher: Option<PrefixPublisher>,
    boundary: Option<ExecutionSegment>,
    published_until: f64,
    pending: Option<Pending>,
    phase: PreparedSourceStatus,
}

impl Scene {
    /// Opt in before source execution. No continuation runs in the constructor.
    pub fn into_prepared_source<C: LiveContinuation>(
        self,
        continuation: C,
        limits: ReplayLimits,
    ) -> Result<PreparedSource<C>, PreparedSourceError<C::Error>> {
        let mut program = self
            .into_live_program(continuation)
            .map_err(PreparedSourceError::Lowering)?;
        program
            .session()
            .validate_prepared_source_domain()
            .map_err(PreparedSourceError::Prefix)?;
        program
            .scene
            .owned_execution_mut()
            .begin_replay_retention(limits)
            .map_err(PreparedSourceError::Replay)?;
        let published_until = program.session().frame().time;
        Ok(PreparedSource {
            program: Some(program),
            publisher: None,
            boundary: None,
            published_until,
            pending: None,
            phase: PreparedSourceStatus::Ready,
        })
    }
}

impl<C: LiveContinuation> PreparedSource<C> {
    pub fn status(&self) -> PreparedSourceStatus {
        match self.pending {
            Some(Pending::Reader(_)) => PreparedSourceStatus::ReaderReady,
            Some(Pending::Batch(_)) => PreparedSourceStatus::BatchReady,
            None => self.phase,
        }
    }

    /// Transfer the initial independent reader exactly once. Future source work
    /// stays headless, and never needs to borrow this displayed runtime again.
    pub fn take_reader(&mut self) -> Option<PrefixReader> {
        if !matches!(self.pending, Some(Pending::Reader(_))) {
            return None;
        }
        match self.pending.take() {
            Some(Pending::Reader(reader)) => Some(*reader),
            _ => unreachable!(),
        }
    }

    pub fn pending_batch(&self) -> Option<&PrefixBatch> {
        match &self.pending {
            Some(Pending::Batch(batch)) => Some(batch),
            _ => None,
        }
    }

    /// After borrowed receiver admission, move the original batch for delivery.
    /// Dropping a taken batch loses its ordered suffix; the producer cannot replay
    /// arbitrary source code to recover it. An untaken batch remains retryable.
    pub fn take_batch(&mut self) -> Option<PrefixBatch> {
        if !matches!(self.pending, Some(Pending::Batch(_))) {
            return None;
        }
        match self.pending.take() {
            Some(Pending::Batch(batch)) => Some(*batch),
            _ => unreachable!(),
        }
    }

    /// Stop without pretending the previously delivered open range is finished.
    pub fn cancel(&mut self) {
        if self.program.is_some() || self.pending.is_some() {
            self.program = None;
            self.publisher = None;
            self.pending = None;
            self.phase = PreparedSourceStatus::Cancelled;
        }
    }

    /// Prepare only while the published frontier is below an absolute demand.
    ///
    /// `None` means demand is satisfied: the host should sleep until playback or
    /// explicit navigation changes demand. No continuation, retention operation,
    /// frame evaluation or allocation is performed in that case. Pending output
    /// and terminal lifecycle states are still returned for delivery/cleanup.
    /// Bootstrap always admits one step because it has no readable prefix yet.
    ///
    /// The next segment's duration is unknown until user code returns. It may
    /// overshoot the target by that one segment; this is neither a hard memory
    /// budget nor preemption. Retention limits still apply and rewind never
    /// discards history or reruns a completed source. A host blocked on an open
    /// frontier may explicitly request one `prepare_next` step to close it.
    pub fn prepare_until(
        &mut self,
        target: f64,
    ) -> Result<Option<PreparedSourceStatus>, PreparedSourceError<C::Error>> {
        if !target.is_finite() || target < 0.0 {
            return Err(PreparedSourceError::InvalidPreparationTarget(target));
        }
        if self.pending.is_none()
            && self.phase == PreparedSourceStatus::Ready
            && self.publisher.is_some()
            && self.published_until >= target
        {
            return Ok(None);
        }
        self.prepare_next().map(Some)
    }

    /// Invoke at most one continuation, complete its actual segment, and publish
    /// at most one suffix. Zero-duration waits yield without a busy inner loop.
    /// Pending output and terminal states are idempotent no-ops.
    ///
    /// An unwinding panic propagates unchanged, but leaves this owner failed and
    /// releases its source-owned state. Already-transferred history is unaffected.
    /// Abort/trap configurations require host-context teardown; this is not panic
    /// recovery on those targets or rollback of arbitrary external side effects.
    pub fn prepare_next(&mut self) -> Result<PreparedSourceStatus, PreparedSourceError<C::Error>> {
        if self.pending.is_some() || self.phase != PreparedSourceStatus::Ready {
            return Ok(self.status());
        }
        // Leave a failed, empty owner behind before invoking arbitrary source or
        // completion code. The one existing program/publisher move into this local
        // attempt; no scene, runtime or retained history is cloned. If unwinding
        // escapes, ordinary stack cleanup releases the attempt and its undelivered
        // output. A caller that catches the panic cannot resume partial source work
        // or take a terminal batch staged before a panicking continuation destructor.
        let mut attempt = Self {
            program: self.program.take(),
            publisher: self.publisher.take(),
            boundary: self.boundary,
            published_until: self.published_until,
            pending: None,
            phase: PreparedSourceStatus::Ready,
        };
        self.phase = PreparedSourceStatus::Failed;
        attempt.prepare_one()?;
        *self = attempt;
        Ok(self.status())
    }

    fn prepare_one(&mut self) -> Result<(), PreparedSourceError<C::Error>> {
        let program = self
            .program
            .as_mut()
            .expect("ready source owns its live program");
        program
            .session()
            .validate_prepared_source_domain()
            .map_err(PreparedSourceError::Prefix)?;
        let status = program.resume().map_err(PreparedSourceError::Program)?;
        program
            .session()
            .validate_prepared_source_domain()
            .map_err(PreparedSourceError::Prefix)?;
        if status == LiveProgramStatus::PreparingResources {
            return Err(PreparedSourceError::ResourcePreparationUnavailable);
        }
        if status == LiveProgramStatus::Finished {
            if self.publisher.is_none()
                && program.session().finite_timeline_end_time()
                    != Some(program.session().frame().time)
            {
                return Err(PreparedSourceError::Prefix(PrefixError::NotReady));
            }
            program
                .scene
                .owned_execution_mut()
                .seal_replay()
                .map_err(PreparedSourceError::Replay)?;
            self.pending = Some(match self.publisher.as_mut() {
                Some(publisher) => Pending::Batch(Box::new(
                    program
                        .session()
                        .finish_prepared_prefix(
                            self.boundary.expect("publisher has a completed boundary"),
                            publisher,
                        )
                        .map_err(PreparedSourceError::Prefix)?,
                )),
                None => Pending::Reader(Box::new(
                    program
                        .session()
                        .create_finished_prepared_reader()
                        .map_err(PreparedSourceError::Prefix)?,
                )),
            });
            self.phase = PreparedSourceStatus::Finished;
            self.program = None;
            self.publisher = None;
            return Ok(());
        }
        let LiveProgramPhase::Awaiting(segment) = program.phase else {
            unreachable!("successful resume yields one segment or finishes")
        };
        // Callback registrations were denied before driving; this empty ordinary
        // callback table preserves the same segment/completion checks as live use.
        program
            .drive_to(&mut crate::RustHostCallbackTable::new(), segment.end_time())
            .map_err(PreparedSourceError::Program)?;
        self.boundary = Some(segment);
        if segment.end_time() > self.published_until {
            self.pending = Some(match self.publisher.as_mut() {
                Some(publisher) => Pending::Batch(Box::new(
                    program
                        .session()
                        .publish_prepared_prefix(segment, publisher)
                        .map_err(PreparedSourceError::Prefix)?,
                )),
                None => {
                    let (publisher, reader) = program
                        .session()
                        .create_prepared_prefix(segment)
                        .map_err(PreparedSourceError::Prefix)?;
                    self.publisher = Some(publisher);
                    Pending::Reader(Box::new(reader))
                }
            });
            self.published_until = segment.end_time();
        }
        // Retention, not display, admits the completed source publication. This
        // drains producer renderer dirtiness only after its history is retained;
        // it never issues a displayed-frame receipt or acknowledges the reader.
        let awaiting_admission =
            matches!(program.status(), LiveProgramStatus::PublicationPending(_));
        let publication = program.take_renderer_publication().context();
        if awaiting_admission {
            program
                .admit_publication(publication)
                .map_err(PreparedSourceError::Program)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
