#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClockError {
    InvalidLoopDuration(f64),
    NonFiniteTimestamp(f64),
    TimestampWentBackwards { previous: f64, actual: f64 },
}

impl std::fmt::Display for ClockError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLoopDuration(duration) => {
                write!(formatter, "invalid playback loop duration {duration}")
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
/// Browser scheduling remains outside the semantic runtime: the same timestamp always
/// produces the same scene time, and resetting the clock establishes a new time origin.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackClock {
    loop_duration: Option<f64>,
    origin_ms: Option<f64>,
    previous_ms: Option<f64>,
}

impl PlaybackClock {
    pub const fn once() -> Self {
        Self {
            loop_duration: None,
            origin_ms: None,
            previous_ms: None,
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

        let origin = *self.origin_ms.get_or_insert(timestamp_ms);
        self.previous_ms = Some(timestamp_ms);
        let elapsed = (timestamp_ms - origin) / 1_000.0;
        Ok(match self.loop_duration {
            Some(duration) => elapsed.rem_euclid(duration),
            None => elapsed,
        })
    }

    pub fn set_loop_duration(&mut self, duration: f64) -> Result<(), ClockError> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(ClockError::InvalidLoopDuration(duration));
        }

        if let (Some(origin), Some(previous)) = (self.origin_ms, self.previous_ms) {
            let elapsed = (previous - origin) / 1_000.0;
            let current_time = match self.loop_duration {
                Some(previous_duration) => elapsed.rem_euclid(previous_duration),
                None => elapsed,
            };
            let phase = current_time.rem_euclid(duration);
            self.origin_ms = Some(previous - phase * 1_000.0);
        }
        self.loop_duration = Some(duration);
        Ok(())
    }

    pub fn reset(&mut self) {
        self.origin_ms = None;
        self.previous_ms = None;
    }

    pub const fn loop_duration(&self) -> Option<f64> {
        self.loop_duration
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
    fn reset_establishes_a_new_time_origin() {
        let mut clock = PlaybackClock::once();
        clock.scene_time(100.0).expect("valid frame");
        clock.scene_time(600.0).expect("valid frame");
        clock.reset();

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
