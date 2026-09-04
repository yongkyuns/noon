use noon::ExecutionSession;
use noon_runtime::{RuntimeWakeState, TimelineWakeState};

/// Browser primitive requested by the target-neutral execution-session wake state.
///
/// This is only a host adaptation. Timeline ownership remains in the runtime; browser
/// code must not synthesize another event heap or cadence model from scene contents.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BrowserExecutionCadence {
    /// Active authored-time work can vary continuously and should be driven by the
    /// browser's next presentation-frame callback.
    AnimationFrame,
    /// No channel is active; wake when authored time reaches this deterministic
    /// scene-time boundary. Browser code may realize this with a timer.
    TimerAtSceneTime(f64),
    /// No authored-time wake remains.
    Idle,
}

/// Mechanical browser projection of [`ExecutionSession::wake_state`].
///
/// Presentation dirtiness stays orthogonal to authored-time cadence. A static scene
/// can request one immediate presentation and still be `Idle` afterward, while an
/// active scene can request an animation frame even when no renderer changes are
/// pending at the instant this plan is observed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrowserExecutionWakePlan {
    present_now: bool,
    cadence: BrowserExecutionCadence,
}

impl BrowserExecutionWakePlan {
    /// Derive browser wake mechanics from the shared session/runtime authority.
    pub fn from_session(session: &ExecutionSession) -> Self {
        Self::from_runtime(session.wake_state())
    }

    /// Project one target-neutral runtime wake observation into browser primitives.
    pub fn from_runtime(state: RuntimeWakeState) -> Self {
        Self::from_parts(state.frame_pending(), state.timeline())
    }

    const fn from_parts(frame_pending: bool, timeline: TimelineWakeState) -> Self {
        let cadence = match timeline {
            TimelineWakeState::Continuous => BrowserExecutionCadence::AnimationFrame,
            TimelineWakeState::Deadline(deadline) => {
                BrowserExecutionCadence::TimerAtSceneTime(deadline)
            }
            TimelineWakeState::Quiescent => BrowserExecutionCadence::Idle,
        };
        Self {
            present_now: frame_pending,
            cadence,
        }
    }

    /// Whether renderer-facing effective state is waiting for one presentation.
    pub const fn present_now(self) -> bool {
        self.present_now
    }

    /// Browser cadence corresponding exactly to the runtime timeline wake state.
    pub const fn cadence(self) -> BrowserExecutionCadence {
        self.cadence
    }

    /// Whether the browser should request its next presentation-frame callback.
    pub const fn needs_animation_frame(self) -> bool {
        matches!(self.cadence, BrowserExecutionCadence::AnimationFrame)
    }

    /// Relative authored-time delay for a deterministic timer wake.
    ///
    /// This deliberately does not read wall time or own a clock. A browser host that
    /// is driving authored time 1:1 from a realtime wall clock can convert this delay
    /// into its timer primitive. Invalid time is rejected rather than producing an
    /// unrepresentable or accidental busy-loop delay.
    pub fn timer_delay_seconds(self, current_scene_time: f64) -> Option<f64> {
        let BrowserExecutionCadence::TimerAtSceneTime(deadline) = self.cadence else {
            return None;
        };
        if !current_scene_time.is_finite() || !deadline.is_finite() {
            return None;
        }
        Some((deadline - current_scene_time).max(0.0))
    }

    /// True only after presentation is clean and no authored-time wake remains.
    pub const fn is_idle(self) -> bool {
        !self.present_now && matches!(self.cadence, BrowserExecutionCadence::Idle)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{SemanticObjectState, SemanticStore, StoredGeometry};

    use super::*;

    #[test]
    fn browser_projection_preserves_presentation_and_timeline_as_orthogonal_facts() {
        let present_static =
            BrowserExecutionWakePlan::from_parts(true, TimelineWakeState::Quiescent);
        assert!(present_static.present_now());
        assert_eq!(present_static.cadence(), BrowserExecutionCadence::Idle);
        assert!(!present_static.is_idle());

        let clean_active =
            BrowserExecutionWakePlan::from_parts(false, TimelineWakeState::Continuous);
        assert!(!clean_active.present_now());
        assert!(clean_active.needs_animation_frame());

        let clean_deadline =
            BrowserExecutionWakePlan::from_parts(false, TimelineWakeState::Deadline(4.5));
        assert_eq!(
            clean_deadline.cadence(),
            BrowserExecutionCadence::TimerAtSceneTime(4.5)
        );
    }

    #[test]
    fn browser_deadline_delay_is_authored_time_relative_and_fails_closed() {
        let plan = BrowserExecutionWakePlan::from_parts(false, TimelineWakeState::Deadline(5.0));
        assert_eq!(plan.timer_delay_seconds(3.5), Some(1.5));
        assert_eq!(plan.timer_delay_seconds(6.0), Some(0.0));
        assert_eq!(plan.timer_delay_seconds(f64::NAN), None);

        let invalid =
            BrowserExecutionWakePlan::from_parts(false, TimelineWakeState::Deadline(f64::INFINITY));
        assert_eq!(invalid.timer_delay_seconds(0.0), None);
    }

    #[test]
    fn direct_static_session_settles_after_its_initial_presentation() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let initial = BrowserExecutionWakePlan::from_session(&session);
        assert!(initial.present_now());
        assert_eq!(initial.cadence(), BrowserExecutionCadence::Idle);

        session.take_frame_changes();
        let settled = BrowserExecutionWakePlan::from_session(&session);
        assert!(settled.is_idle());
    }
}
