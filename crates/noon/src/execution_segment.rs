use crate::{EvaluationError, ExecutionSession, FrameState, RuntimeIdentity, TimelineWakeState};
use noon_core::{
    ObjectId, Property, SemanticNodeId, SemanticObjectProperty, SemanticSignalValue, TrackId,
};

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
    pub semantic_property: SemanticObjectProperty,
    pub completion_value: SemanticSignalValue,
    pub execution_object: ObjectId,
    pub property: Property,
    pub track: TrackId,
    pub end_time: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingSegmentCompletion {
    pub token: ExecutionSegmentToken,
    pub activation_scene_revision: noon_core::SceneRevision,
    pub entries: Vec<SegmentCompletionEntry>,
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
    ) -> Result<&FrameState, EvaluationError> {
        if !requested_time.is_finite() {
            return Err(EvaluationError::InvalidTime(requested_time));
        }
        let current = self.frame().time;
        if current >= segment.end_time {
            return Ok(self.frame());
        }
        let target = requested_time.max(current).min(segment.end_time);
        if target == current {
            return Ok(self.frame());
        }
        self.advance_to(target)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, RateFunction, SemanticObjectState, SemanticStore, SemanticVec3,
        StoredGeometry,
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
