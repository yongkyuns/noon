use crate::{
    CallbackAdvance, EvaluationError, ExecutionSession, ExecutionSessionCallbackError, FrameState,
    RuntimeIdentity, TimelineWakeState,
};
use noon_compile::SemanticAnimationCompletion;
use noon_core::{ObjectId, Property, SemanticNodeId, TrackId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExecutionSegmentSequence(u64);

impl ExecutionSegmentSequence {
    pub(crate) const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExecutionSegmentToken {
    runtime: RuntimeIdentity,
    sequence: ExecutionSegmentSequence,
}

impl ExecutionSegmentToken {
    pub(crate) const fn new(runtime: RuntimeIdentity, sequence: ExecutionSegmentSequence) -> Self {
        Self { runtime, sequence }
    }

    pub const fn runtime(self) -> RuntimeIdentity {
        self.runtime
    }

    pub const fn sequence(self) -> ExecutionSegmentSequence {
        self.sequence
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SegmentCompletionEntry {
    pub semantic_object: SemanticNodeId,
    pub completion: SemanticAnimationCompletion,
    pub execution_object: ObjectId,
    pub property: Property,
    pub track: TrackId,
    pub end_time: f64,
}

#[derive(Clone, Debug)]
pub(crate) enum PendingSegmentCompletionKind {
    ObjectTracks {
        lifecycle_root: Option<SemanticNodeId>,
        lifecycle_removal: Option<(SemanticNodeId, SemanticNodeId)>,
        entries: Vec<SegmentCompletionEntry>,
    },
    ScalarTrack {
        signal: SemanticNodeId,
        authored_endpoint: f64,
        runtime_endpoint: f32,
        end_time: f64,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct PendingSegmentCompletion {
    pub token: ExecutionSegmentToken,
    pub activation_scene_revision: noon_core::SceneRevision,
    pub kind: PendingSegmentCompletionKind,
}

/// One authored-time continuation boundary owned by the execution session layer.
///
/// A segment is not a timeline track or scheduler entry. It records only the
/// deterministic authored interval that a `play()`/`wait()` continuation must not
/// cross before resuming. Runtime timeline cadence remains owned by the existing
/// scheduler and is observed through [`ExecutionSession::segment_state`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExecutionSegment {
    start_time: f64,
    end_time: f64,
    token: Option<ExecutionSegmentToken>,
}

impl ExecutionSegment {
    pub(crate) fn from_duration(
        start_time: f64,
        duration: f64,
    ) -> Result<Self, ExecutionSegmentError> {
        if !duration.is_finite() || duration < 0.0 {
            return Err(ExecutionSegmentError::InvalidDuration(duration));
        }
        let end_time = start_time + duration;
        if !end_time.is_finite() || (duration > 0.0 && end_time <= start_time) {
            return Err(ExecutionSegmentError::EndTimeOverflow {
                start_time,
                duration,
            });
        }
        Ok(Self {
            start_time,
            end_time,
            token: None,
        })
    }

    pub(crate) fn with_completion_token(mut self, token: ExecutionSegmentToken) -> Self {
        self.token = Some(token);
        self
    }

    pub(crate) const fn token(self) -> Option<ExecutionSegmentToken> {
        self.token
    }

    pub const fn start_time(self) -> f64 {
        self.start_time
    }

    pub const fn end_time(self) -> f64 {
        self.end_time
    }

    pub fn duration(self) -> f64 {
        self.end_time - self.start_time
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExecutionSegmentError {
    InvalidDuration(f64),
    EndTimeOverflow { start_time: f64, duration: f64 },
}

impl std::fmt::Display for ExecutionSegmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDuration(duration) => write!(
                formatter,
                "execution segment duration must be finite and non-negative, got {duration}"
            ),
            Self::EndTimeOverflow {
                start_time,
                duration,
            } => write!(
                formatter,
                "execution segment end time is not representable after start {start_time} with duration {duration}"
            ),
        }
    }
}

impl std::error::Error for ExecutionSegmentError {}

/// Failure to drive one logical segment through its owning execution session.
///
/// Animated segments carry the existing runtime-derived completion token, so a
/// foreign or superseded animated segment is rejected before it can advance the
/// frame. A pure wait deliberately has no token: it has no driver-release work
/// and is only a value boundary in authored time, so this operation does not
/// manufacture a second identity space merely to tag it.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSegmentAdvanceError {
    ForeignSegment {
        expected: RuntimeIdentity,
        actual: RuntimeIdentity,
    },
    NoPendingCompletion {
        actual: ExecutionSegmentToken,
    },
    StaleSegment {
        expected: ExecutionSegmentToken,
        actual: ExecutionSegmentToken,
    },
    Evaluation(EvaluationError),
    Callback(ExecutionSessionCallbackError),
}

impl std::fmt::Display for ExecutionSegmentAdvanceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForeignSegment { expected, actual } => write!(
                formatter,
                "execution segment belongs to runtime {actual:?}, expected {expected:?}"
            ),
            Self::NoPendingCompletion { actual } => write!(
                formatter,
                "execution segment token {actual:?} has no pending completion"
            ),
            Self::StaleSegment { expected, actual } => write!(
                formatter,
                "execution segment token {actual:?} is stale; expected {expected:?}"
            ),
            Self::Evaluation(error) => error.fmt(formatter),
            Self::Callback(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecutionSegmentAdvanceError {}

impl From<EvaluationError> for ExecutionSegmentAdvanceError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

impl From<ExecutionSessionCallbackError> for ExecutionSegmentAdvanceError {
    fn from(value: ExecutionSessionCallbackError) -> Self {
        Self::Callback(value)
    }
}

/// Target-neutral observation used while driving one logical authored segment.
///
/// `timeline` is the existing runtime cadence clipped to the segment boundary. A
/// pure wait therefore yields `Deadline(end_time)` without manufacturing a timeline
/// event, while an active animation remains `Continuous` until the same boundary.
/// Completion is independent of renderer presentation: once the authored boundary
/// has been synchronously evaluated, the continuation may observe the coherent
/// effective frame even if that frame still needs to be presented.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExecutionSegmentState {
    complete: bool,
    timeline: TimelineWakeState,
}

impl ExecutionSegmentState {
    pub const fn is_complete(self) -> bool {
        self.complete
    }

    pub const fn timeline(self) -> TimelineWakeState {
        self.timeline
    }
}

impl ExecutionSession {
    /// Start a logical wait segment at the current authored scene time.
    ///
    /// This does not add a no-op track or mutate the runtime scheduler. The returned
    /// segment supplies the continuation deadline while ordinary runtime wake state
    /// continues to represent only real authored timeline work.
    pub fn wait_segment(&self, duration: f64) -> Result<ExecutionSegment, ExecutionSegmentError> {
        ExecutionSegment::from_duration(self.frame().time, duration)
    }

    /// Observe completion and the next cadence needed to drive one segment.
    ///
    /// Runtime deadlines after this segment are intentionally clipped away: they
    /// belong to later authored work and must not delay the current continuation.
    pub fn segment_state(&self, segment: ExecutionSegment) -> ExecutionSegmentState {
        let now = self.frame().time;
        if segment
            .token()
            .is_some_and(|token| token.runtime() != self.runtime_identity())
        {
            return ExecutionSegmentState {
                complete: false,
                timeline: TimelineWakeState::Quiescent,
            };
        }
        if segment
            .token()
            .is_some_and(|token| self.segment_was_completed(token))
        {
            return ExecutionSegmentState {
                complete: true,
                timeline: TimelineWakeState::Quiescent,
            };
        }
        if now >= segment.end_time {
            let completion_pending = self.segment_completion_is_pending(segment.token());
            let complete = !completion_pending && self.callback_progression_is_coherent_at(now);
            return ExecutionSegmentState {
                complete,
                timeline: if complete || self.callback_progression_is_terminal() {
                    TimelineWakeState::Quiescent
                } else {
                    TimelineWakeState::Continuous
                },
            };
        }

        let timeline = match self.wake_state().timeline() {
            TimelineWakeState::Continuous => TimelineWakeState::Continuous,
            TimelineWakeState::Deadline(runtime_deadline) => {
                TimelineWakeState::Deadline(runtime_deadline.min(segment.end_time))
            }
            TimelineWakeState::Quiescent => TimelineWakeState::Deadline(segment.end_time),
        };
        ExecutionSegmentState {
            complete: false,
            timeline,
        }
    }

    /// Validate an animated segment's existing runtime-derived token before a
    /// drive operation can change the frame. A previously completed receipt is
    /// intentionally an idempotent no-op. Tokenless waits carry no driver
    /// completion record and remain ordinary authored-time boundaries.
    pub(crate) fn validate_segment_for_advance(
        &self,
        segment: ExecutionSegment,
    ) -> Result<bool, ExecutionSegmentAdvanceError> {
        let Some(token) = segment.token() else {
            return Ok(false);
        };
        let runtime = self.runtime_identity();
        if token.runtime() != runtime {
            return Err(ExecutionSegmentAdvanceError::ForeignSegment {
                expected: runtime,
                actual: token.runtime(),
            });
        }
        if self.segment_was_completed(token) {
            return Ok(true);
        }
        let pending = self
            .pending_segment_token()
            .ok_or(ExecutionSegmentAdvanceError::NoPendingCompletion { actual: token })?;
        if pending != token {
            return Err(ExecutionSegmentAdvanceError::StaleSegment {
                expected: pending,
                actual: token,
            });
        }
        Ok(false)
    }

    /// Return the one forward, endpoint-clamped target shared by every segment
    /// drive. `None` means an animated segment was already reconciled, or a
    /// tokenless wait has already been passed, so callers must not evaluate or
    /// invoke callbacks again. A wait exactly at its endpoint still returns that
    /// endpoint so required endpoint callbacks are serviced.
    fn segment_drive_target(
        &self,
        segment: ExecutionSegment,
        requested_time: f64,
    ) -> Result<Option<f64>, ExecutionSegmentAdvanceError> {
        if !requested_time.is_finite() {
            return Err(EvaluationError::InvalidTime(requested_time).into());
        }
        if self.validate_segment_for_advance(segment)? {
            return Ok(None);
        }
        let current = self.frame().time;
        if segment.token().is_none() && current > segment.end_time {
            return Ok(None);
        }
        Ok(Some(if current >= segment.end_time {
            current
        } else {
            requested_time.max(current).min(segment.end_time)
        }))
    }

    /// Advance monotonically toward one logical segment boundary without overshoot.
    ///
    /// Wall/presentation clocks may request a time after the authored boundary. This
    /// operation clamps that request to the exact segment endpoint so resumed
    /// authoring observes the coherent endpoint rather than a later scene time. An
    /// already-complete segment is a no-op and never rewinds a session that has since
    /// advanced farther.
    pub fn advance_segment_to(
        &mut self,
        segment: ExecutionSegment,
        requested_time: f64,
    ) -> Result<&FrameState, ExecutionSegmentAdvanceError> {
        let Some(target) = self.segment_drive_target(segment, requested_time)? else {
            return Ok(self.frame());
        };
        let current = self.frame().time;
        if target == current {
            return Ok(self.frame());
        }
        self.advance_to(target).map_err(Into::into)
    }

    /// Advance one logical segment through the existing callback barrier.
    ///
    /// This composes the normal segment clamp with
    /// [`ExecutionSession::advance_to_callback_barrier`]. It owns no cursor or
    /// scheduler: hosts retain only the returned segment and query
    /// [`Self::segment_state`] for cadence. Unlike the frame-only drive, a
    /// same-time request intentionally still enters the barrier so time-zero
    /// and endpoint callbacks cannot be skipped.
    pub fn advance_segment_to_callback_barrier(
        &mut self,
        segment: ExecutionSegment,
        requested_time: f64,
    ) -> Result<CallbackAdvance<'_>, ExecutionSegmentAdvanceError> {
        let Some(target) = self.segment_drive_target(segment, requested_time)? else {
            return Ok(CallbackAdvance::Ready(self.frame()));
        };
        self.advance_to_callback_barrier(target).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, HostCallbackId, RateFunction, SemanticMutationTransaction,
        SemanticObjectState, SemanticStore, SemanticVec3, StoredGeometry,
    };

    use super::*;

    fn static_session() -> ExecutionSession {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        ExecutionSession::from_semantic_store(&store).unwrap()
    }

    #[test]
    fn pure_wait_exposes_deadline_without_creating_timeline_work() {
        let mut session = static_session();
        session.take_frame_changes();
        assert_eq!(
            session.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );

        let segment = session.wait_segment(2.0).unwrap();
        assert_eq!(segment.start_time(), 0.0);
        assert_eq!(segment.end_time(), 2.0);
        assert_eq!(segment.duration(), 2.0);
        assert_eq!(
            session.segment_state(segment).timeline(),
            TimelineWakeState::Deadline(2.0)
        );
        assert_eq!(
            session.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );

        session.advance_segment_to(segment, 1.0).unwrap();
        assert_eq!(session.frame().time, 1.0);
        assert!(!session.segment_state(segment).is_complete());

        session.advance_segment_to(segment, 9.0).unwrap();
        assert_eq!(session.frame().time, 2.0);
        assert!(session.segment_state(segment).is_complete());
        assert_eq!(
            session.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );
    }

    #[test]
    fn segment_drive_clamps_to_endpoint_and_never_rewinds_after_completion() {
        let mut session = static_session();
        session.seek(5.0).unwrap();
        let segment = session.wait_segment(2.0).unwrap();

        session.advance_segment_to(segment, 50.0).unwrap();
        assert_eq!(session.frame().time, 7.0);
        assert!(session.segment_state(segment).is_complete());

        session.advance_to(9.0).unwrap();
        session.advance_segment_to(segment, 8.0).unwrap();
        assert_eq!(session.frame().time, 9.0);
    }

    #[test]
    fn completed_wait_drive_does_not_reenter_callback_barrier_after_its_endpoint() {
        let mut session = static_session();
        let wait = session.wait_segment(2.0).unwrap();

        session.advance_to(wait.end_time()).unwrap();
        assert_eq!(
            session.segment_drive_target(wait, wait.end_time()).unwrap(),
            Some(wait.end_time()),
            "an active wait endpoint must remain eligible for required callbacks"
        );

        session.advance_to(3.0).unwrap();
        assert_eq!(
            session.segment_drive_target(wait, 9.0).unwrap(),
            None,
            "a completed wait must not create a later callback barrier"
        );
        assert!(matches!(
            session.advance_segment_to_callback_barrier(wait, 9.0).unwrap(),
            CallbackAdvance::Ready(frame) if frame.time == 3.0
        ));
    }

    #[test]
    fn active_runtime_cadence_is_reused_until_segment_boundary() {
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
        session
            .activate_animation(
                &store,
                animation,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let segment = session.wait_segment(1.0).unwrap();

        assert_eq!(
            session.segment_state(segment).timeline(),
            TimelineWakeState::Continuous
        );
        session.advance_segment_to(segment, 1.0).unwrap();
        assert!(session.segment_state(segment).is_complete());
        assert_eq!(session.frame().objects[0].transform.translation.x, 4.0);
    }

    #[test]
    fn callback_aware_segment_drive_clamps_overshoot_to_the_wait_endpoint() {
        let mut session = static_session();
        let wait = session.wait_segment(2.0).unwrap();

        assert!(matches!(
            session.advance_segment_to_callback_barrier(wait, 9.0).unwrap(),
            CallbackAdvance::Ready(frame) if frame.time == 2.0
        ));
        assert!(session.segment_state(wait).is_complete());
    }

    #[test]
    fn callback_aware_segment_drive_services_time_zero_callbacks_without_advancing_time() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut registration = SemanticMutationTransaction::new();
        registration.add_updater(object, HostCallbackId::new(7), 0.0, None);
        registration.apply(&mut store).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let wait = session.wait_segment(1.0).unwrap();
        let before = session.frame().clone();
        let publication = session.publication_context();

        let overlay = match session
            .advance_segment_to_callback_barrier(wait, 0.0)
            .unwrap()
        {
            CallbackAdvance::HostRequired {
                invocations,
                overlay,
            } => {
                assert_eq!(invocations.len(), 1);
                assert_eq!(overlay.time(), 0.0);
                overlay
            }
            CallbackAdvance::Ready(_) => panic!("time-zero callback phase was skipped"),
        };
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), publication);
        session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();
        assert!(matches!(
            session.advance_segment_to_callback_barrier(wait, 0.0).unwrap(),
            CallbackAdvance::Ready(frame) if frame.time == 0.0
        ));
    }

    #[test]
    fn callback_aware_segment_drive_requires_endpoint_callback_before_ready() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut registration = SemanticMutationTransaction::new();
        registration.add_updater(object, HostCallbackId::new(8), 1.0, None);
        registration.apply(&mut store).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let wait = session.wait_segment(1.0).unwrap();

        let overlay = match session
            .advance_segment_to_callback_barrier(wait, wait.end_time())
            .unwrap()
        {
            CallbackAdvance::HostRequired { overlay, .. } => overlay,
            CallbackAdvance::Ready(_) => panic!("endpoint callback phase was skipped"),
        };
        assert_eq!(overlay.time(), wait.end_time());
        assert_eq!(session.frame().time, 0.0);
        session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();
        assert!(matches!(
            session
                .advance_segment_to_callback_barrier(wait, wait.end_time())
                .unwrap(),
            CallbackAdvance::Ready(frame) if frame.time == wait.end_time()
        ));
    }

    #[test]
    fn animated_segment_drive_rejects_foreign_and_stale_tokens_without_frame_changes() {
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
        let segment = session
            .activate_animation_segment(
                &store,
                animation,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let before = session.frame().clone();
        let publication = session.publication_context();

        let mut clone = session.clone();
        let clone_before = clone.frame().clone();
        let clone_publication = clone.publication_context();
        assert!(matches!(
            clone.advance_segment_to(segment, 1.0),
            Err(ExecutionSegmentAdvanceError::ForeignSegment { .. })
        ));
        assert_eq!(clone.frame(), &clone_before);
        assert_eq!(clone.publication_context(), clone_publication);

        let stale = ExecutionSegment::from_duration(0.0, 1.0)
            .unwrap()
            .with_completion_token(ExecutionSegmentToken::new(
                session.runtime_identity(),
                ExecutionSegmentSequence::new(99),
            ));
        assert!(matches!(
            session.advance_segment_to_callback_barrier(stale, 1.0),
            Err(ExecutionSegmentAdvanceError::StaleSegment { .. })
        ));
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), publication);
    }

    #[test]
    fn completed_animated_segment_drive_is_an_idempotent_receipt() {
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
        let segment = session
            .activate_animation_segment(
                &store,
                animation,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        session
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        session.complete_segment(&mut store, segment).unwrap();
        let before = session.frame().clone();
        let publication = session.publication_context();

        assert!(matches!(
            session.advance_segment_to_callback_barrier(segment, 0.0).unwrap(),
            CallbackAdvance::Ready(frame) if frame == &before
        ));
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), publication);
    }

    #[test]
    fn wait_segment_rejects_invalid_or_overflowing_duration() {
        let mut session = static_session();
        assert_eq!(
            session.wait_segment(-1.0),
            Err(ExecutionSegmentError::InvalidDuration(-1.0))
        );
        assert!(matches!(
            session.wait_segment(f64::NAN),
            Err(ExecutionSegmentError::InvalidDuration(value)) if value.is_nan()
        ));

        session.seek(f64::MAX).unwrap();
        assert_eq!(
            session.wait_segment(1.0),
            Err(ExecutionSegmentError::EndTimeOverflow {
                start_time: f64::MAX,
                duration: 1.0,
            })
        );
        assert_eq!(
            session.wait_segment(f64::MAX),
            Err(ExecutionSegmentError::EndTimeOverflow {
                start_time: f64::MAX,
                duration: f64::MAX,
            })
        );
    }
}
