use crate::{SceneInstance, TimelineEventScheduler};

/// Timeline cadence requested by the existing event-driven runtime scheduler.
///
/// This is a scheduling observation, not a second scheduler. Hosts decide how to
/// realize it with RAF, `WaitUntil`, platform timers, or another wake primitive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimelineWakeState {
    /// At least one authored timeline channel is active and can vary with scene time.
    Continuous,
    /// No channel is active; the next authored timeline boundary is at this scene time.
    Deadline(f64),
    /// No active channel or future authored timeline boundary remains.
    Quiescent,
}

/// Target-neutral host wake state for one runtime instance.
///
/// Presentation dirtiness is orthogonal to timeline cadence: a static scene can need
/// one presentation after input and then become quiescent again. Keeping both facts
/// prevents hosts from inventing their own dirty/deadline model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeWakeState {
    frame_pending: bool,
    timeline: TimelineWakeState,
}

impl RuntimeWakeState {
    pub const fn frame_pending(self) -> bool {
        self.frame_pending
    }

    pub const fn timeline(self) -> TimelineWakeState {
        self.timeline
    }

    pub const fn is_quiescent(self) -> bool {
        !self.frame_pending && matches!(self.timeline, TimelineWakeState::Quiescent)
    }

    /// Combine another execution-owned cadence observation without transferring
    /// scheduling authority to the platform host.
    pub fn with_additional_timeline(mut self, additional: TimelineWakeState) -> Self {
        self.timeline = match (self.timeline, additional) {
            (TimelineWakeState::Continuous, _) | (_, TimelineWakeState::Continuous) => {
                TimelineWakeState::Continuous
            }
            (TimelineWakeState::Deadline(left), TimelineWakeState::Deadline(right)) => {
                TimelineWakeState::Deadline(left.min(right))
            }
            (TimelineWakeState::Deadline(deadline), TimelineWakeState::Quiescent)
            | (TimelineWakeState::Quiescent, TimelineWakeState::Deadline(deadline)) => {
                TimelineWakeState::Deadline(deadline)
            }
            (TimelineWakeState::Quiescent, TimelineWakeState::Quiescent) => {
                TimelineWakeState::Quiescent
            }
        };
        self
    }

    pub fn without_timeline_wake(mut self) -> Self {
        self.timeline = TimelineWakeState::Quiescent;
        self
    }
}

impl TimelineEventScheduler {
    pub fn wake_state(&self) -> TimelineWakeState {
        if !self.active_groups().is_empty() {
            TimelineWakeState::Continuous
        } else if let Some(deadline) = self.next_event_time() {
            TimelineWakeState::Deadline(deadline)
        } else {
            TimelineWakeState::Quiescent
        }
    }
}

impl SceneInstance {
    /// Whether the current execution projection retains any authored timeline channel.
    ///
    /// This is an O(1) derived query over the scheduler's existing channel index. It is
    /// useful to looping hosts after the final channel has settled: a quiescent scheduler
    /// may still need one wake at the loop boundary so that deterministic history can be
    /// replayed. The runtime remains the owner of the channels and their event cursor.
    pub fn has_timeline_channels(&self) -> bool {
        self.timeline_scheduler.live_group_count() != 0
    }

    /// Current runtime-owned dirty/deadline/completion state for platform hosts.
    pub fn wake_state(&self) -> RuntimeWakeState {
        RuntimeWakeState {
            frame_pending: !self.changes.is_empty(),
            timeline: self.timeline_scheduler.wake_state(),
        }
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::CompiledTrack;
    use noon_core::{
        CompositionTimeMap, Property, RateFunction, TrackId, TrackTiming, TrackValues, Vec2,
    };

    use super::*;

    fn position_track(start: f64, duration: f64) -> CompiledTrack {
        CompiledTrack {
            id: TrackId::new(1),
            object_index: 0,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(1.0, 0.0),
            },
            timing: TrackTiming {
                start_time: start,
                duration,
                easing: RateFunction::Linear,
            },
            time_map: CompositionTimeMap::default(),
            transform_geometry_plan: None,
            reconciled: false,
        }
    }

    #[test]
    fn timeline_wake_state_reuses_active_set_and_next_event_index() {
        let mut scheduler = TimelineEventScheduler::new(&[position_track(5.0, 2.0)]);
        scheduler.seek(0.0);
        assert_eq!(scheduler.wake_state(), TimelineWakeState::Deadline(5.0));

        scheduler.advance(5.0);
        assert_eq!(scheduler.wake_state(), TimelineWakeState::Continuous);

        scheduler.advance(7.0);
        assert_eq!(scheduler.wake_state(), TimelineWakeState::Quiescent);
    }

    #[test]
    fn empty_timeline_is_quiescent_without_a_synthetic_deadline() {
        let mut scheduler = TimelineEventScheduler::new(&[]);
        scheduler.seek(0.0);
        assert_eq!(scheduler.wake_state(), TimelineWakeState::Quiescent);
    }

    #[test]
    fn settled_lifecycle_history_remains_indexed_for_loop_replay() {
        let mut lifecycle = position_track(1.0, 0.0);
        lifecycle.property = Property::Presence;
        lifecycle.values = TrackValues::Bool {
            from: true,
            to: false,
        };
        let mut scheduler = TimelineEventScheduler::new(&[lifecycle]);
        assert_eq!(scheduler.live_group_count(), 1);
        scheduler.seek(2.0);
        assert_eq!(scheduler.wake_state(), TimelineWakeState::Quiescent);
        assert_eq!(
            scheduler.live_group_count(),
            1,
            "settling a presence event must not erase deterministic loop history"
        );
    }
}
