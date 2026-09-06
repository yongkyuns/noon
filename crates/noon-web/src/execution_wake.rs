use noon::{ExecutionSegment, ExecutionSession};
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

    /// Derive browser wake mechanics while driving one logical authored segment.
    ///
    /// Renderer dirtiness still comes from the ordinary runtime wake observation,
    /// while authored-time cadence is clipped/supplemented by the session segment
    /// boundary. A pure `wait()` can therefore request a timer without manufacturing
    /// a no-op runtime track or giving the browser its own timeline model.
    pub fn from_segment(session: &ExecutionSession, segment: ExecutionSegment) -> Self {
        let wake = session.wake_state();
        Self::from_parts(
            wake.frame_pending(),
            session.segment_state(segment).timeline(),
        )
    }

    /// Derive wake mechanics while a host still owes completion for its current segment.
    ///
    /// A zero-length wait is already at its endpoint, so its ordinary timeline state is
    /// quiescent even though the continuation must drive once to reconcile completion.
    /// Keep [`Self::from_segment`] stable for completed-handle observations and add the
    /// immediate deadline only at the live host's explicit pending-segment boundary.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn from_pending_segment(
        session: &ExecutionSession,
        segment: ExecutionSegment,
    ) -> Self {
        let state = session.segment_state(segment);
        let timeline = if matches!(state.timeline(), TimelineWakeState::Quiescent) {
            TimelineWakeState::Deadline(session.frame().time)
        } else {
            state.timeline()
        };
        Self::from_parts(session.wake_state().frame_pending(), timeline)
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

    /// Browser cadence corresponding exactly to the runtime/session wake state.
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

/// Concrete browser callback primitive after mapping authored scene time onto the
/// browser's monotonic high-resolution wall clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BrowserHostWake {
    AnimationFrame,
    TimerAfterMilliseconds(f64),
    Idle,
}

/// One browser-host scheduling observation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrowserExecutionWakeDirective {
    present_now: bool,
    wake: BrowserHostWake,
}

impl BrowserExecutionWakeDirective {
    pub const fn present_now(self) -> bool {
        self.present_now
    }

    pub const fn wake(self) -> BrowserHostWake {
        self.wake
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BrowserRealtimeAnchor {
    wall_origin_ms: f64,
    scene_origin: f64,
}

/// Stateful wall↔authored-time mapping for the direct browser execution host.
///
/// The mapping deliberately disappears while the runtime is idle. When timed work
/// starts again, the next host observation anchors the current authored time to the
/// current browser monotonic timestamp, so elapsed wall time while idle is never
/// charged to newly activated authored work. This is only a clock conversion layer;
/// cadence and deadlines remain owned by [`BrowserExecutionWakePlan`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BrowserExecutionWakeClock {
    anchor: Option<BrowserRealtimeAnchor>,
}

impl BrowserExecutionWakeClock {
    /// Start the next wall-time interval at an already-published authored time.
    ///
    /// Required host callback execution may take arbitrary wall time while the
    /// runtime is pinned at its barrier. Reanchoring after that phase commits
    /// prevents host latency from advancing authored time. Both values use the
    /// same units as directive: milliseconds and seconds respectively.
    pub fn reanchor(&mut self, wall_time_ms: f64, scene_time: f64) -> Option<()> {
        if !wall_time_ms.is_finite() || !scene_time.is_finite() {
            return None;
        }
        self.anchor = Some(BrowserRealtimeAnchor {
            wall_origin_ms: wall_time_ms,
            scene_origin: scene_time,
        });
        Some(())
    }

    /// Realize one target-neutral wake plan against a browser monotonic timestamp.
    ///
    /// `wall_time_ms` is expected to share the `performance.now()` / RAF timestamp
    /// time origin. Invalid or unrepresentable values fail closed with `None`.
    pub fn directive(
        &mut self,
        plan: BrowserExecutionWakePlan,
        wall_time_ms: f64,
        current_scene_time: f64,
    ) -> Option<BrowserExecutionWakeDirective> {
        if !wall_time_ms.is_finite() || !current_scene_time.is_finite() {
            return None;
        }

        let wake = match plan.cadence() {
            BrowserExecutionCadence::Idle => {
                self.anchor = None;
                BrowserHostWake::Idle
            }
            BrowserExecutionCadence::AnimationFrame => {
                self.ensure_anchor(wall_time_ms, current_scene_time);
                BrowserHostWake::AnimationFrame
            }
            BrowserExecutionCadence::TimerAtSceneTime(scene_deadline) => {
                if !scene_deadline.is_finite() {
                    return None;
                }
                let anchor = self.ensure_anchor(wall_time_ms, current_scene_time);
                let scene_offset_ms = (scene_deadline - anchor.scene_origin) * 1_000.0;
                if !scene_offset_ms.is_finite() {
                    return None;
                }
                let wall_deadline_ms = if scene_offset_ms <= 0.0 {
                    anchor.wall_origin_ms
                } else {
                    anchor.wall_origin_ms + scene_offset_ms
                };
                if !wall_deadline_ms.is_finite() {
                    return None;
                }
                BrowserHostWake::TimerAfterMilliseconds((wall_deadline_ms - wall_time_ms).max(0.0))
            }
        };

        Some(BrowserExecutionWakeDirective {
            present_now: plan.present_now(),
            wake,
        })
    }

    /// Map one browser monotonic timestamp into authored scene time while timed work
    /// is active. Timestamps before the current anchor saturate at the authored origin.
    pub fn scene_time_at(self, wall_time_ms: f64) -> Option<f64> {
        if !wall_time_ms.is_finite() {
            return None;
        }
        let anchor = self.anchor?;
        let elapsed_seconds = ((wall_time_ms - anchor.wall_origin_ms).max(0.0)) / 1_000.0;
        let scene_time = anchor.scene_origin + elapsed_seconds;
        scene_time.is_finite().then_some(scene_time)
    }

    fn ensure_anchor(
        &mut self,
        wall_time_ms: f64,
        current_scene_time: f64,
    ) -> BrowserRealtimeAnchor {
        *self.anchor.get_or_insert(BrowserRealtimeAnchor {
            wall_origin_ms: wall_time_ms,
            scene_origin: current_scene_time,
        })
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{SemanticObjectState, SemanticStore, StoredGeometry};

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
        let mut session = static_session();
        let initial = BrowserExecutionWakePlan::from_session(&session);
        assert!(initial.present_now());
        assert_eq!(initial.cadence(), BrowserExecutionCadence::Idle);

        session.take_frame_changes();
        let settled = BrowserExecutionWakePlan::from_session(&session);
        assert!(settled.is_idle());
    }

    #[test]
    fn pure_wait_uses_segment_deadline_while_raw_runtime_stays_idle() {
        let mut session = static_session();
        session.take_frame_changes();
        let segment = session.wait_segment(2.0).unwrap();

        assert!(BrowserExecutionWakePlan::from_session(&session).is_idle());
        let waiting = BrowserExecutionWakePlan::from_segment(&session, segment);
        assert_eq!(
            waiting.cadence(),
            BrowserExecutionCadence::TimerAtSceneTime(2.0)
        );
        assert_eq!(waiting.timer_delay_seconds(session.frame().time), Some(2.0));

        session.advance_segment_to(segment, 9.0).unwrap();
        assert!(BrowserExecutionWakePlan::from_segment(&session, segment).is_idle());
        assert_eq!(session.frame().time, 2.0);
    }

    #[test]
    fn pending_zero_wait_requests_one_immediate_host_drive() {
        let mut store = SemanticStore::new();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.take_frame_changes();
        let segment = session.wait_segment(0.0).unwrap();

        assert!(BrowserExecutionWakePlan::from_segment(&session, segment).is_idle());
        assert_eq!(
            BrowserExecutionWakePlan::from_pending_segment(&session, segment).cadence(),
            BrowserExecutionCadence::TimerAtSceneTime(0.0)
        );

        session.advance_segment_to(segment, 0.0).unwrap();
        session.complete_segment(&mut store, segment).unwrap();
        assert!(BrowserExecutionWakePlan::from_segment(&session, segment).is_idle());
    }

    #[test]
    fn browser_realtime_clock_does_not_charge_quiescent_wall_time_to_later_work() {
        let mut clock = BrowserExecutionWakeClock::default();
        let idle = BrowserExecutionWakePlan::from_parts(false, TimelineWakeState::Quiescent);
        assert_eq!(
            clock.directive(idle, 1_000.0, 0.0).unwrap().wake(),
            BrowserHostWake::Idle
        );
        assert_eq!(clock.scene_time_at(30_000.0), None);

        let active = BrowserExecutionWakePlan::from_parts(false, TimelineWakeState::Continuous);
        assert_eq!(
            clock.directive(active, 31_000.0, 0.0).unwrap().wake(),
            BrowserHostWake::AnimationFrame
        );
        assert_eq!(clock.scene_time_at(31_000.0), Some(0.0));
        assert_eq!(clock.scene_time_at(31_500.0), Some(0.5));

        clock.directive(idle, 32_000.0, 1.0).unwrap();
        assert_eq!(clock.scene_time_at(60_000.0), None);

        let deadline =
            BrowserExecutionWakePlan::from_parts(false, TimelineWakeState::Deadline(3.0));
        assert_eq!(
            clock.directive(deadline, 62_000.0, 1.0).unwrap().wake(),
            BrowserHostWake::TimerAfterMilliseconds(2_000.0)
        );
        assert_eq!(clock.scene_time_at(62_000.0), Some(1.0));
    }

    #[test]
    fn browser_realtime_clock_reanchors_after_opaque_host_work() {
        let mut clock = BrowserExecutionWakeClock::default();
        let active = BrowserExecutionWakePlan::from_parts(false, TimelineWakeState::Continuous);
        clock.directive(active, 1_000.0, 0.0).unwrap();
        assert_eq!(clock.scene_time_at(1_500.0), Some(0.5));

        clock.reanchor(9_000.0, 0.5).unwrap();
        assert_eq!(clock.scene_time_at(9_000.0), Some(0.5));
        assert_eq!(clock.scene_time_at(9_016.0), Some(0.516));
    }

    #[test]
    fn browser_realtime_clock_rejects_invalid_or_unrepresentable_wall_mapping() {
        let mut clock = BrowserExecutionWakeClock::default();
        let active = BrowserExecutionWakePlan::from_parts(false, TimelineWakeState::Continuous);
        assert_eq!(clock.directive(active, f64::NAN, 0.0), None);
        assert_eq!(clock.directive(active, 0.0, f64::INFINITY), None);

        let deadline =
            BrowserExecutionWakePlan::from_parts(false, TimelineWakeState::Deadline(f64::MAX));
        assert_eq!(clock.directive(deadline, 0.0, 0.0), None);
    }
}
