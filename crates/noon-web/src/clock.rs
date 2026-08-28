#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClockError {
    InvalidLoopDuration(f64),
    InvalidSceneTime(f64),
    SceneTimeOutsideLoop { time: f64, duration: f64 },
    NonFiniteTimestamp(f64),
    TimestampWentBackwards { previous: f64, actual: f64 },
}

impl std::fmt::Display for ClockError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLoopDuration(duration) => {
                write!(formatter, "invalid playback loop duration {duration}")
            }
            Self::InvalidSceneTime(time) => write!(formatter, "invalid playback scene time {time}"),
            Self::SceneTimeOutsideLoop { time, duration } => write!(
                formatter,
                "playback scene time {time} exceeds loop duration {duration}"
            ),
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
/// only this logical clock: pausing freezes the current phase, resuming re-anchors it
/// to the last accepted browser timestamp, and seeking establishes an exact logical
/// time without replaying intermediate frames.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackClock {
    loop_duration: Option<f64>,
    anchor_ms: Option<f64>,
    anchor_scene_time: f64,
    previous_ms: Option<f64>,
    playing: bool,
}

impl PlaybackClock {
    pub const fn once() -> Self {
        Self {
            loop_duration: None,
            anchor_ms: None,
            anchor_scene_time: 0.0,
            previous_ms: None,
            playing: true,
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
        let anchor = *self.anchor_ms.get_or_insert(timestamp_ms);
        self.previous_ms = Some(timestamp_ms);
        if !self.playing {
            return Ok(self.anchor_scene_time);
        }
        Ok(self.running_time(anchor, timestamp_ms))
    }

    pub fn pause(&mut self) {
        if !self.playing {
            return;
        }
        self.reanchor_to_current_time();
        self.playing = false;
    }

    pub fn resume(&mut self) {
        if self.playing {
            return;
        }
        self.anchor_ms = self.previous_ms;
        self.playing = true;
    }

    pub fn seek(&mut self, scene_time: f64) -> Result<(), ClockError> {
        self.validate_scene_time(scene_time)?;
        self.anchor_scene_time = scene_time;
        self.anchor_ms = self.previous_ms;
        Ok(())
    }

    pub fn set_loop_duration(&mut self, duration: f64) -> Result<(), ClockError> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(ClockError::InvalidLoopDuration(duration));
        }

        self.reanchor_to_current_time();
        if self.anchor_scene_time > duration {
            self.anchor_scene_time = self.anchor_scene_time.rem_euclid(duration);
        }
        self.loop_duration = Some(duration);
        Ok(())
    }

    pub fn reset(&mut self) {
        self.anchor_ms = None;
        self.anchor_scene_time = 0.0;
        self.previous_ms = None;
        self.playing = true;
    }

    pub const fn loop_duration(&self) -> Option<f64> {
        self.loop_duration
    }

    pub const fn is_playing(&self) -> bool {
        self.playing
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

    fn validate_scene_time(&self, scene_time: f64) -> Result<(), ClockError> {
        if !scene_time.is_finite() || scene_time < 0.0 {
            return Err(ClockError::InvalidSceneTime(scene_time));
        }
        if let Some(duration) = self.loop_duration {
            if scene_time > duration {
                return Err(ClockError::SceneTimeOutsideLoop {
                    time: scene_time,
                    duration,
                });
            }
        }
        Ok(())
    }

    fn reanchor_to_current_time(&mut self) {
        let Some(previous) = self.previous_ms else {
            self.anchor_ms = None;
            return;
        };
        if self.playing {
            let anchor = self.anchor_ms.unwrap_or(previous);
            self.anchor_scene_time = self.running_time(anchor, previous);
        }
        self.anchor_ms = Some(previous);
    }

    fn running_time(&self, anchor_ms: f64, timestamp_ms: f64) -> f64 {
        let elapsed = (timestamp_ms - anchor_ms) / 1_000.0;
        let raw = self.anchor_scene_time + elapsed;
        match self.loop_duration {
            Some(duration) if elapsed == 0.0 && self.anchor_scene_time == duration => duration,
            Some(duration) => raw.rem_euclid(duration),
            None => raw,
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
    fn pause_freezes_and_resume_continues_without_a_jump() {
        let mut clock = PlaybackClock::looping(4.0).expect("valid loop");
        assert_eq!(clock.scene_time(100.0).unwrap(), 0.0);
        assert_eq!(clock.scene_time(1_600.0).unwrap(), 1.5);

        clock.pause();
        assert!(!clock.is_playing());
        assert_eq!(clock.scene_time(5_000.0).unwrap(), 1.5);
        assert_eq!(clock.scene_time(8_000.0).unwrap(), 1.5);

        clock.resume();
        assert!(clock.is_playing());
        assert_eq!(clock.scene_time(8_000.0).unwrap(), 1.5);
        assert_eq!(clock.scene_time(8_500.0).unwrap(), 2.0);
    }

    #[test]
    fn seek_is_exact_while_paused_and_reanchors_running_playback() {
        let mut clock = PlaybackClock::looping(4.0).expect("valid loop");
        clock.scene_time(100.0).unwrap();
        clock.scene_time(1_100.0).unwrap();
        clock.pause();

        clock.seek(3.25).unwrap();
        assert_eq!(clock.scene_time(2_000.0).unwrap(), 3.25);
        assert_eq!(clock.scene_time(7_000.0).unwrap(), 3.25);

        clock.resume();
        clock.seek(1.25).unwrap();
        assert_eq!(clock.scene_time(7_000.0).unwrap(), 1.25);
        assert_eq!(clock.scene_time(7_500.0).unwrap(), 1.75);
    }

    #[test]
    fn exact_loop_endpoint_survives_seek_until_playback_advances() {
        let mut clock = PlaybackClock::looping(4.0).expect("valid loop");
        clock.scene_time(1_000.0).unwrap();
        clock.seek(4.0).unwrap();

        assert_eq!(clock.scene_time(1_000.0).unwrap(), 4.0);
        assert_eq!(clock.scene_time(1_250.0).unwrap(), 0.25);
    }

    #[test]
    fn invalid_seek_does_not_mutate_the_clock() {
        let mut clock = PlaybackClock::looping(4.0).expect("valid loop");
        clock.scene_time(100.0).unwrap();
        clock.scene_time(1_100.0).unwrap();

        assert!(matches!(
            clock.seek(-1.0),
            Err(ClockError::InvalidSceneTime(-1.0))
        ));
        assert!(matches!(
            clock.seek(4.1),
            Err(ClockError::SceneTimeOutsideLoop { .. })
        ));
        assert_eq!(clock.scene_time(1_100.0).unwrap(), 1.0);
    }

    #[test]
    fn reset_establishes_a_new_time_origin_and_resumes_playback() {
        let mut clock = PlaybackClock::once();
        clock.scene_time(100.0).expect("valid frame");
        clock.scene_time(600.0).expect("valid frame");
        clock.pause();
        clock.reset();

        assert!(clock.is_playing());
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
