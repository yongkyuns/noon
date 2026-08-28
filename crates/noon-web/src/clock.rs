#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClockError {
    InvalidLoopDuration(f64),
    InvalidSceneTime(f64),
    NonFiniteTimestamp(f64),
    TimestampWentBackwards { previous: f64, actual: f64 },
}

impl std::fmt::Display for ClockError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLoopDuration(duration) => {
                write!(formatter, "invalid playback loop duration {duration}")
            }
            Self::InvalidSceneTime(time) => {
                write!(formatter, "invalid playback scene time {time}")
            }
            Self::NonFiniteTimestamp(timestamp) => {
                write!(formatter, "non-finite animation timestamp {timestamp}")
            }
            Self::TimestampWentBackwards { previous, actual } => write!(
                formatter,
                "animation timestamp went backwards from {previous} ms to {actual} ms"
            ),
        }
    }
}

impl std::error::Error for ClockError {}

/// Converts monotonic `requestAnimationFrame` timestamps to deterministic scene time.
///
/// Browser scheduling remains outside the semantic runtime. Playback controls mutate
/// only the logical clock anchor: pause freezes scene time, resume establishes a fresh
/// timestamp anchor on the next frame so background/control latency cannot cause a
/// catch-up jump, and seek establishes a new exact logical phase without replaying
/// intermediate frames.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackClock {
    loop_duration: Option<f64>,
    anchor_ms: Option<f64>,
    anchor_scene_time: f64,
    previous_ms: Option<f64>,
    paused: bool,
}

impl PlaybackClock {
    pub const fn once() -> Self {
        Self {
            loop_duration: None,
            anchor_ms: None,
            anchor_scene_time: 0.0,
            previous_ms: None,
            paused: false,
        }
    }

    pub fn looping(duration: f64) -> Result<Self, ClockError> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(ClockError::InvalidLoopDuration(duration));
        }
        Ok(Self {
            loop_duration: Some(duration),
            ..Self::once()
        })
    }

    pub fn scene_time(&mut self, timestamp_ms: f64) -> Result<f64, ClockError> {
        self.validate_timestamp(timestamp_ms)?;
        self.previous_ms = Some(timestamp_ms);

        if self.paused {
            return Ok(self.normalize_scene_time(self.anchor_scene_time));
        }

        let anchor_ms = *self.anchor_ms.get_or_insert(timestamp_ms);
        let elapsed = (timestamp_ms - anchor_ms) / 1_000.0;
        Ok(self.normalize_scene_time(self.anchor_scene_time + elapsed))
    }

    /// Freeze logical scene time at the last accepted animation timestamp.
    ///
    /// Calling this before the first frame pauses at the current logical anchor (zero
    /// for a fresh clock). Repeated calls are idempotent.
    pub fn pause(&mut self) {
        if self.paused {
            return;
        }
        self.anchor_scene_time = self.current_time();
        self.anchor_ms = self.previous_ms;
        self.paused = true;
    }

    /// Resume playback without charging wall-clock time spent paused.
    ///
    /// The next accepted frame becomes a fresh timestamp anchor and therefore returns
    /// the exact paused scene time. Following frames advance normally from that point.
    pub fn resume(&mut self) {
        if !self.paused {
            return;
        }
        self.paused = false;
        self.anchor_ms = None;
    }

    /// Re-anchor playback at an absolute logical scene time.
    ///
    /// Looping clocks normalize the requested time into their loop phase. Non-looping
    /// clocks require a finite, non-negative time. The next frame returns the exact
    /// sought phase before subsequent timestamps advance it (unless paused).
    pub fn seek(&mut self, scene_time: f64) -> Result<f64, ClockError> {
        if !scene_time.is_finite() || scene_time < 0.0 {
            return Err(ClockError::InvalidSceneTime(scene_time));
        }
        let scene_time = self.normalize_scene_time(scene_time);
        self.anchor_scene_time = scene_time;
        self.anchor_ms = if self.paused { self.previous_ms } else { None };
        Ok(scene_time)
    }

    pub fn set_loop_duration(&mut self, duration: f64) -> Result<(), ClockError> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(ClockError::InvalidLoopDuration(duration));
        }

        // `anchor_ms == None` while running is meaningful: the next RAF is waiting to
        // establish a fresh wall-clock anchor after initial start, resume, or seek. A
        // retime in that window must preserve the pending re-anchor or it can charge
        // paused/control latency back into logical scene time.
        let reanchor_on_next_frame = !self.paused && self.anchor_ms.is_none();
        let current_time = self.current_time();
        self.loop_duration = Some(duration);
        self.anchor_scene_time = current_time.rem_euclid(duration);
        self.anchor_ms = if reanchor_on_next_frame {
            None
        } else {
            self.previous_ms
        };
        Ok(())
    }

    /// Reset to a running clock at scene time zero with no timestamp origin.
    pub fn reset(&mut self) {
        self.anchor_ms = None;
        self.anchor_scene_time = 0.0;
        self.previous_ms = None;
        self.paused = false;
    }

    pub const fn loop_duration(&self) -> Option<f64> {
        self.loop_duration
    }

    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    /// Return logical time at the latest accepted frame without mutating the clock.
    pub fn current_time(&self) -> f64 {
        if self.paused {
            return self.normalize_scene_time(self.anchor_scene_time);
        }
        let elapsed = match (self.anchor_ms, self.previous_ms) {
            (Some(anchor), Some(previous)) => (previous - anchor) / 1_000.0,
            _ => 0.0,
        };
        self.normalize_scene_time(self.anchor_scene_time + elapsed)
    }

    fn validate_timestamp(&self, timestamp_ms: f64) -> Result<(), ClockError> {
        if !timestamp_ms.is_finite() {
            return Err(ClockError::NonFiniteTimestamp(timestamp_ms));
        }
        if let Some(previous) = self.previous_ms {
            if timestamp_ms < previous {
                return Err(ClockError::TimestampWentBackwards {
                    previous,
                    actual: timestamp_ms,
                });
            }
        }
        Ok(())
    }

    fn normalize_scene_time(&self, scene_time: f64) -> f64 {
        match self.loop_duration {
            Some(duration) => scene_time.rem_euclid(duration),
            None => scene_time,
        }
    }
}

impl Default for PlaybackClock {
    fn default() -> Self {
        Self::once()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_uses_first_frame_as_deterministic_origin() {
        let mut clock = PlaybackClock::once();

        assert_eq!(clock.scene_time(4_250.0).expect("valid frame"), 0.0);
        assert_eq!(clock.scene_time(5_500.0).expect("valid frame"), 1.25);
    }

    #[test]
    fn looping_clock_wraps_at_declared_duration() {
        let mut clock = PlaybackClock::looping(2.0).expect("valid loop");

        assert_eq!(clock.scene_time(100.0).expect("valid frame"), 0.0);
        assert_eq!(clock.scene_time(2_600.0).expect("valid frame"), 0.5);
    }

    #[test]
    fn changing_loop_duration_preserves_the_current_phase() {
        let mut clock = PlaybackClock::looping(4.0).expect("valid loop");
        assert_eq!(clock.scene_time(100.0).expect("valid frame"), 0.0);
        assert_eq!(clock.scene_time(1_600.0).expect("valid frame"), 1.5);

        clock
            .set_loop_duration(3.0)
            .expect("updated duration must be valid");
        assert_eq!(clock.scene_time(1_600.0).expect("same frame"), 1.5);
        assert_eq!(clock.scene_time(3_100.0).expect("valid frame"), 0.0);
    }

    #[test]
    fn shrinking_loop_duration_normalizes_only_when_required() {
        let mut clock = PlaybackClock::looping(5.0).expect("valid loop");
        assert_eq!(clock.scene_time(100.0).expect("valid frame"), 0.0);
        assert_eq!(clock.scene_time(3_600.0).expect("valid frame"), 3.5);

        clock
            .set_loop_duration(2.0)
            .expect("updated duration must be valid");
        assert_eq!(clock.scene_time(3_600.0).expect("same frame"), 1.5);
    }

    #[test]
    fn pause_freezes_time_while_timestamps_continue() {
        let mut clock = PlaybackClock::looping(4.0).expect("valid loop");
        assert_eq!(clock.scene_time(100.0).unwrap(), 0.0);
        assert_eq!(clock.scene_time(600.0).unwrap(), 0.5);

        clock.pause();
        assert!(clock.is_paused());
        assert_eq!(clock.current_time(), 0.5);
        assert_eq!(clock.scene_time(1_600.0).unwrap(), 0.5);
        assert_eq!(clock.scene_time(4_600.0).unwrap(), 0.5);
        assert_eq!(clock.current_time(), 0.5);
    }

    #[test]
    fn resume_reanchors_on_next_frame_without_catch_up_jump() {
        let mut clock = PlaybackClock::looping(4.0).expect("valid loop");
        clock.scene_time(100.0).unwrap();
        assert_eq!(clock.scene_time(600.0).unwrap(), 0.5);
        clock.pause();
        assert_eq!(clock.scene_time(5_600.0).unwrap(), 0.5);

        clock.resume();
        assert!(!clock.is_paused());
        assert_eq!(clock.scene_time(6_000.0).unwrap(), 0.5);
        assert_eq!(clock.scene_time(6_250.0).unwrap(), 0.75);
    }

    #[test]
    fn seek_reanchors_running_clock_at_exact_requested_phase() {
        let mut clock = PlaybackClock::looping(4.0).expect("valid loop");
        clock.scene_time(100.0).unwrap();
        clock.scene_time(600.0).unwrap();

        assert_eq!(clock.seek(2.25).unwrap(), 2.25);
        assert_eq!(clock.current_time(), 2.25);
        assert_eq!(clock.scene_time(900.0).unwrap(), 2.25);
        assert_eq!(clock.scene_time(1_150.0).unwrap(), 2.5);
    }

    #[test]
    fn seek_while_paused_updates_frozen_phase_and_resume_starts_there() {
        let mut clock = PlaybackClock::looping(4.0).expect("valid loop");
        clock.scene_time(100.0).unwrap();
        clock.scene_time(600.0).unwrap();
        clock.pause();

        assert_eq!(clock.seek(3.25).unwrap(), 3.25);
        assert_eq!(clock.scene_time(2_000.0).unwrap(), 3.25);
        clock.resume();
        assert_eq!(clock.scene_time(2_500.0).unwrap(), 3.25);
        assert_eq!(clock.scene_time(2_750.0).unwrap(), 3.5);
    }

    #[test]
    fn looping_seek_normalizes_phase_and_once_seek_rejects_invalid_time() {
        let mut looping = PlaybackClock::looping(4.0).unwrap();
        assert_eq!(looping.seek(9.5).unwrap(), 1.5);

        let mut once = PlaybackClock::once();
        assert_eq!(once.seek(9.5).unwrap(), 9.5);
        assert!(matches!(
            once.seek(-0.1),
            Err(ClockError::InvalidSceneTime(time)) if time == -0.1
        ));
        assert!(matches!(
            once.seek(f64::NAN),
            Err(ClockError::InvalidSceneTime(time)) if time.is_nan()
        ));
    }

    #[test]
    fn retiming_paused_clock_preserves_frozen_phase() {
        let mut clock = PlaybackClock::looping(5.0).unwrap();
        clock.scene_time(100.0).unwrap();
        clock.scene_time(3_600.0).unwrap();
        clock.pause();

        clock.set_loop_duration(2.0).unwrap();
        assert_eq!(clock.current_time(), 1.5);
        assert_eq!(clock.scene_time(8_000.0).unwrap(), 1.5);
    }

    #[test]
    fn retiming_during_pending_resume_preserves_no_jump_reanchor() {
        let mut clock = PlaybackClock::looping(5.0).unwrap();
        clock.scene_time(100.0).unwrap();
        clock.scene_time(1_100.0).unwrap();
        clock.pause();
        clock.scene_time(6_100.0).unwrap();
        clock.resume();

        clock.set_loop_duration(3.0).unwrap();
        assert_eq!(clock.current_time(), 1.0);
        assert_eq!(clock.scene_time(7_000.0).unwrap(), 1.0);
        assert_eq!(clock.scene_time(7_250.0).unwrap(), 1.25);
    }

    #[test]
    fn retiming_during_pending_seek_preserves_exact_next_frame_phase() {
        let mut clock = PlaybackClock::looping(5.0).unwrap();
        clock.scene_time(100.0).unwrap();
        clock.scene_time(1_100.0).unwrap();
        clock.seek(2.5).unwrap();

        clock.set_loop_duration(4.0).unwrap();
        assert_eq!(clock.current_time(), 2.5);
        assert_eq!(clock.scene_time(4_000.0).unwrap(), 2.5);
        assert_eq!(clock.scene_time(4_500.0).unwrap(), 3.0);
    }

    #[test]
    fn reset_establishes_a_running_zero_time_origin() {
        let mut clock = PlaybackClock::once();
        clock.scene_time(100.0).expect("valid frame");
        clock.scene_time(600.0).expect("valid frame");
        clock.pause();
        clock.seek(2.0).unwrap();
        clock.reset();

        assert!(!clock.is_paused());
        assert_eq!(clock.current_time(), 0.0);
        assert_eq!(clock.scene_time(20.0).expect("valid frame"), 0.0);
    }

    #[test]
    fn invalid_or_regressing_time_never_mutates_the_clock() {
        assert!(matches!(
            PlaybackClock::looping(0.0),
            Err(ClockError::InvalidLoopDuration(0.0))
        ));

        let mut clock = PlaybackClock::once();
        clock.scene_time(100.0).expect("valid frame");
        assert!(matches!(
            clock.scene_time(99.0),
            Err(ClockError::TimestampWentBackwards { .. })
        ));
        assert_eq!(clock.scene_time(200.0).expect("valid frame"), 0.1);
    }
}
