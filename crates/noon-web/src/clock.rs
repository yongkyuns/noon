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

/// Converts monotonic browser presentation timestamps into deterministic scene time.
///
/// Browser scheduling is never the semantic time owner. Pause freezes the current
/// logical phase, resume and running seek re-anchor on the next accepted frame so
/// control latency cannot create a catch-up jump, and seek establishes one exact
/// logical time without replaying intermediate frames. For looping clocks, the exact
/// endpoint remains a valid seek target until playback advances positively.
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
        self.previous_ms = Some(timestamp_ms);

        if !self.playing {
            return Ok(self.anchor_scene_time);
        }

        let anchor_ms = *self.anchor_ms.get_or_insert(timestamp_ms);
        let elapsed = (timestamp_ms - anchor_ms) / 1_000.0;
        Ok(self.running_time(elapsed))
    }

    /// Anchor browser wake conversion without advancing the published scene sample.
    ///
    /// The semantic endpoint and this clock share one `performance.now()` origin. A
    /// renderer-worker timestamp is only admission for a wake and must never become
    /// the authored clock. Keeping `previous_ms` unchanged here also preserves normal
    /// engine/render processing latency in the next actual scene-time sample.
    pub(crate) fn observe_wake_time(&mut self, timestamp_ms: f64) -> Result<(), ClockError> {
        self.validate_timestamp(timestamp_ms)?;
        if self.playing && self.anchor_ms.is_none() {
            self.anchor_ms = Some(timestamp_ms);
        }
        Ok(())
    }

    /// Convert one runtime-owned scene-time boundary to a relative browser timer.
    ///
    /// Timeline event selection remains in the runtime. This method only projects its
    /// selected deadline through the existing playback anchor. `current_scene_time`
    /// is the last coherently published runtime sample and disambiguates an explicit
    /// seek to the exact loop endpoint from the first sample of the next loop.
    pub(crate) fn timer_delay_milliseconds(
        &mut self,
        scene_deadline: f64,
        timestamp_ms: f64,
        current_scene_time: f64,
    ) -> Result<f64, ClockError> {
        self.validate_scene_time(scene_deadline)?;
        if !current_scene_time.is_finite() || current_scene_time < 0.0 {
            return Err(ClockError::InvalidSceneTime(current_scene_time));
        }
        self.observe_wake_time(timestamp_ms)?;
        let Some(anchor_ms) = self.anchor_ms else {
            return Ok(0.0);
        };
        let reference_ms = self
            .previous_ms
            .filter(|previous| *previous >= anchor_ms)
            .unwrap_or(anchor_ms);
        let reference_scene_time = self.anchor_scene_time + (reference_ms - anchor_ms) / 1_000.0;
        let absolute_deadline = match self.loop_duration {
            Some(duration) if current_scene_time == duration && scene_deadline == duration => {
                reference_scene_time
            }
            Some(duration) => {
                let cycle = (reference_scene_time / duration).floor();
                cycle * duration + scene_deadline
            }
            None => scene_deadline,
        };
        let wall_deadline_ms = anchor_ms + (absolute_deadline - self.anchor_scene_time) * 1_000.0;
        if !wall_deadline_ms.is_finite() {
            return Err(ClockError::NonFiniteTimestamp(wall_deadline_ms));
        }
        Ok((wall_deadline_ms - timestamp_ms).max(0.0))
    }

    pub fn pause(&mut self) {
        if !self.playing {
            return;
        }
        self.anchor_scene_time = self.current_time();
        self.anchor_ms = self.previous_ms;
        self.playing = false;
    }

    pub fn resume(&mut self) {
        if self.playing {
            return;
        }
        self.playing = true;
        self.anchor_ms = None;
    }

    pub fn seek(&mut self, scene_time: f64) -> Result<f64, ClockError> {
        self.validate_scene_time(scene_time)?;
        self.anchor_scene_time = scene_time;
        self.anchor_ms = if self.playing { None } else { self.previous_ms };
        Ok(scene_time)
    }

    pub fn set_loop_duration(&mut self, duration: f64) -> Result<(), ClockError> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(ClockError::InvalidLoopDuration(duration));
        }

        // `anchor_ms == None` while playing means initial start, resume, or seek is
        // still waiting for the next browser frame to establish a wall-clock anchor.
        // Retiming in that window must not charge control latency into scene time.
        let reanchor_on_next_frame = self.playing && self.anchor_ms.is_none();
        let current_time = self.current_time();
        self.loop_duration = Some(duration);
        self.anchor_scene_time = if current_time > duration {
            current_time.rem_euclid(duration)
        } else {
            current_time
        };
        self.anchor_ms = if reanchor_on_next_frame {
            None
        } else {
            self.previous_ms
        };
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

    pub fn current_time(&self) -> f64 {
        if !self.playing {
            return self.anchor_scene_time;
        }
        let elapsed = match (self.anchor_ms, self.previous_ms) {
            (Some(anchor), Some(previous)) => (previous - anchor) / 1_000.0,
            _ => 0.0,
        };
        self.running_time(elapsed)
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

    fn running_time(&self, elapsed: f64) -> f64 {
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
        assert_eq!(clock.scene_time(4_250.0).unwrap(), 0.0);
        assert_eq!(clock.scene_time(5_500.0).unwrap(), 1.25);
    }

    #[test]
    fn looping_clock_wraps_at_declared_duration() {
        let mut clock = PlaybackClock::looping(2.0).unwrap();
        assert_eq!(clock.scene_time(100.0).unwrap(), 0.0);
        assert_eq!(clock.scene_time(2_600.0).unwrap(), 0.5);
    }

    #[test]
    fn changing_loop_duration_preserves_the_current_phase() {
        let mut clock = PlaybackClock::looping(4.0).unwrap();
        clock.scene_time(100.0).unwrap();
        assert_eq!(clock.scene_time(1_600.0).unwrap(), 1.5);
        clock.set_loop_duration(3.0).unwrap();
        assert_eq!(clock.scene_time(1_600.0).unwrap(), 1.5);
        assert_eq!(clock.scene_time(3_100.0).unwrap(), 0.0);
    }

    #[test]
    fn shrinking_loop_duration_normalizes_only_when_required() {
        let mut clock = PlaybackClock::looping(5.0).unwrap();
        clock.scene_time(100.0).unwrap();
        assert_eq!(clock.scene_time(3_600.0).unwrap(), 3.5);
        clock.set_loop_duration(2.0).unwrap();
        assert_eq!(clock.scene_time(3_600.0).unwrap(), 1.5);
    }

    #[test]
    fn pause_freezes_while_browser_timestamps_continue() {
        let mut clock = PlaybackClock::looping(4.0).unwrap();
        clock.scene_time(100.0).unwrap();
        assert_eq!(clock.scene_time(600.0).unwrap(), 0.5);
        clock.pause();
        assert!(!clock.is_playing());
        assert_eq!(clock.scene_time(1_600.0).unwrap(), 0.5);
        assert_eq!(clock.scene_time(5_600.0).unwrap(), 0.5);
        assert_eq!(clock.current_time(), 0.5);
    }

    #[test]
    fn resume_reanchors_on_next_frame_without_catch_up() {
        let mut clock = PlaybackClock::looping(4.0).unwrap();
        clock.scene_time(100.0).unwrap();
        clock.scene_time(600.0).unwrap();
        clock.pause();
        clock.scene_time(5_600.0).unwrap();
        clock.resume();
        assert!(clock.is_playing());
        assert_eq!(clock.scene_time(7_000.0).unwrap(), 0.5);
        assert_eq!(clock.scene_time(7_250.0).unwrap(), 0.75);
    }

    #[test]
    fn wake_observation_uses_engine_origin_without_consuming_processing_latency() {
        let mut clock = PlaybackClock::looping(5.0).unwrap();
        clock.observe_wake_time(10_000.0).unwrap();
        assert_eq!(
            clock.timer_delay_milliseconds(1.0, 10_250.0, 0.0).unwrap(),
            750.0
        );
        assert_eq!(clock.scene_time(10_500.0).unwrap(), 0.5);
    }

    #[test]
    fn loop_deadline_stays_due_until_the_runtime_consumes_the_wrap() {
        let mut clock = PlaybackClock::looping(5.0).unwrap();
        clock.observe_wake_time(100.0).unwrap();
        assert_eq!(
            clock.timer_delay_milliseconds(5.0, 5_200.0, 0.0).unwrap(),
            0.0
        );
        assert!((clock.scene_time(5_200.0).unwrap() - 0.1).abs() <= 1.0e-12);
        assert_eq!(
            clock.timer_delay_milliseconds(5.0, 5_200.0, 0.1).unwrap(),
            4_900.0
        );
    }

    #[test]
    fn running_seek_reanchors_on_next_frame_at_exact_phase() {
        let mut clock = PlaybackClock::looping(4.0).unwrap();
        clock.scene_time(100.0).unwrap();
        clock.scene_time(1_100.0).unwrap();
        assert_eq!(clock.seek(1.25).unwrap(), 1.25);
        assert_eq!(clock.current_time(), 1.25);
        assert_eq!(clock.scene_time(2_000.0).unwrap(), 1.25);
        assert_eq!(clock.scene_time(2_500.0).unwrap(), 1.75);
    }

    #[test]
    fn seek_while_paused_stays_exact_until_resume() {
        let mut clock = PlaybackClock::looping(4.0).unwrap();
        clock.scene_time(100.0).unwrap();
        clock.scene_time(1_100.0).unwrap();
        clock.pause();
        assert_eq!(clock.seek(3.25).unwrap(), 3.25);
        assert_eq!(clock.scene_time(5_000.0).unwrap(), 3.25);
        clock.resume();
        assert_eq!(clock.scene_time(7_000.0).unwrap(), 3.25);
        assert_eq!(clock.scene_time(7_250.0).unwrap(), 3.5);
    }

    #[test]
    fn exact_loop_endpoint_survives_seek_until_playback_advances() {
        let mut clock = PlaybackClock::looping(4.0).unwrap();
        clock.scene_time(1_000.0).unwrap();
        assert_eq!(clock.seek(4.0).unwrap(), 4.0);
        assert_eq!(clock.scene_time(2_000.0).unwrap(), 4.0);
        assert_eq!(clock.scene_time(2_250.0).unwrap(), 0.25);
    }

    #[test]
    fn invalid_seek_does_not_mutate_the_clock() {
        let mut clock = PlaybackClock::looping(4.0).unwrap();
        clock.scene_time(100.0).unwrap();
        clock.scene_time(1_100.0).unwrap();
        let before = clock.clone();
        assert!(matches!(
            clock.seek(-1.0),
            Err(ClockError::InvalidSceneTime(-1.0))
        ));
        assert_eq!(clock, before);
        assert!(matches!(
            clock.seek(4.1),
            Err(ClockError::SceneTimeOutsideLoop { .. })
        ));
        assert_eq!(clock, before);
    }

    #[test]
    fn retiming_during_pending_resume_preserves_reanchor() {
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
    fn retiming_during_pending_seek_preserves_exact_next_frame() {
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
    fn retiming_preserves_exact_endpoint_when_new_duration_matches() {
        let mut clock = PlaybackClock::looping(5.0).unwrap();
        clock.scene_time(100.0).unwrap();
        clock.seek(3.0).unwrap();
        clock.set_loop_duration(3.0).unwrap();
        assert_eq!(clock.scene_time(4_000.0).unwrap(), 3.0);
        let advanced = clock.scene_time(4_100.0).unwrap();
        assert!((advanced - 0.1).abs() <= 1.0e-12);
    }

    #[test]
    fn reset_returns_to_running_zero_origin() {
        let mut clock = PlaybackClock::once();
        clock.scene_time(100.0).unwrap();
        clock.scene_time(600.0).unwrap();
        clock.pause();
        clock.seek(2.0).unwrap();
        clock.reset();
        assert!(clock.is_playing());
        assert_eq!(clock.current_time(), 0.0);
        assert_eq!(clock.scene_time(20.0).unwrap(), 0.0);
    }

    #[test]
    fn invalid_or_regressing_time_never_mutates_the_clock() {
        assert!(matches!(
            PlaybackClock::looping(0.0),
            Err(ClockError::InvalidLoopDuration(0.0))
        ));
        let mut clock = PlaybackClock::once();
        clock.scene_time(100.0).unwrap();
        let before = clock.clone();
        assert!(matches!(
            clock.scene_time(99.0),
            Err(ClockError::TimestampWentBackwards { .. })
        ));
        assert_eq!(clock, before);
        assert_eq!(clock.scene_time(200.0).unwrap(), 0.1);
    }
}
