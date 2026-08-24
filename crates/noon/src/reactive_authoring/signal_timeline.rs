use std::ops::{Deref, DerefMut};

use noon_core::{
    RateFunction, ReactiveValue, SignalSource, SignalTimelineDefinition, SignalTimelineError,
    TimedSemanticScene, TrackTiming,
};

use crate::{AuthoringError, ReactiveScene, ValueTracker};

/// Reactive authoring scene with deterministic timeline-driven tracker inputs.
///
/// Ordinary object animation continues to use the existing `Scene` API through
/// `Deref`. Tracker animation uses the same scene cursor and `RateFunction`
/// vocabulary, but writes native signal tracks rather than object tracks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReactiveTimelineScene {
    scene: ReactiveScene,
    signal_timeline: SignalTimelineDefinition,
}

impl ReactiveTimelineScene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn play_value(&mut self, tracker: ValueTracker, to: f32) -> ValuePlay<'_> {
        ValuePlay {
            scene: self,
            tracker,
            to,
            rate_func: RateFunction::Smooth,
        }
    }

    pub fn signal_timeline(&self) -> &SignalTimelineDefinition {
        &self.signal_timeline
    }

    pub fn timed_semantic_scene(&self) -> Result<TimedSemanticScene, SignalTimelineError> {
        TimedSemanticScene::from_parts(self.scene.semantic_scene(), self.signal_timeline.clone())
    }

    pub fn into_timed_semantic_scene(self) -> Result<TimedSemanticScene, SignalTimelineError> {
        TimedSemanticScene::from_parts(self.scene.into_semantic_scene(), self.signal_timeline)
    }

    fn schedule_value(
        &mut self,
        tracker: ValueTracker,
        to: f32,
        duration: f64,
        rate_func: RateFunction,
    ) -> Result<(), ReactiveTimelineAuthoringError> {
        if !to.is_finite() {
            return Err(ReactiveTimelineAuthoringError::InvalidValue(to));
        }
        let from = self.current_tracker_value(tracker)?;
        let start = self.scene.time();
        self.signal_timeline.add_scalar_track(
            self.scene.reactive_graph(),
            tracker.signal_id(),
            from,
            to,
            TrackTiming::new(start, duration, rate_func),
        )?;
        self.scene.wait(duration)?;
        Ok(())
    }

    fn current_tracker_value(
        &self,
        tracker: ValueTracker,
    ) -> Result<f32, ReactiveTimelineAuthoringError> {
        if let Some(track) = self
            .signal_timeline
            .tracks()
            .iter()
            .rev()
            .find(|track| track.signal == tracker.signal_id())
        {
            return Ok(track.to);
        }
        let signal = self
            .scene
            .reactive_graph()
            .signals()
            .iter()
            .find(|signal| signal.id == tracker.signal_id())
            .ok_or(SignalTimelineError::UnknownSignal(tracker.signal_id()))?;
        match &signal.source {
            SignalSource::Input(ReactiveValue::Scalar(value)) => Ok(*value),
            SignalSource::Input(_) => {
                Err(SignalTimelineError::NonScalarSignal(tracker.signal_id()).into())
            }
            SignalSource::Derived(_) => {
                Err(SignalTimelineError::NotInputSignal(tracker.signal_id()).into())
            }
        }
    }
}

impl Deref for ReactiveTimelineScene {
    type Target = ReactiveScene;

    fn deref(&self) -> &Self::Target {
        &self.scene
    }
}

impl DerefMut for ReactiveTimelineScene {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.scene
    }
}

pub struct ValuePlay<'a> {
    scene: &'a mut ReactiveTimelineScene,
    tracker: ValueTracker,
    to: f32,
    rate_func: RateFunction,
}

impl ValuePlay<'_> {
    pub fn rate_func(mut self, rate_func: RateFunction) -> Self {
        self.rate_func = rate_func;
        self
    }

    pub fn run_time(self, duration: f64) -> Result<(), ReactiveTimelineAuthoringError> {
        self.scene
            .schedule_value(self.tracker, self.to, duration, self.rate_func)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReactiveTimelineAuthoringError {
    Authoring(AuthoringError),
    SignalTimeline(SignalTimelineError),
    InvalidValue(f32),
}

impl std::fmt::Display for ReactiveTimelineAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(error) => error.fmt(formatter),
            Self::SignalTimeline(error) => error.fmt(formatter),
            Self::InvalidValue(value) => write!(formatter, "invalid tracker target value {value}"),
        }
    }
}

impl std::error::Error for ReactiveTimelineAuthoringError {}

impl From<AuthoringError> for ReactiveTimelineAuthoringError {
    fn from(value: AuthoringError) -> Self {
        Self::Authoring(value)
    }
}

impl From<SignalTimelineError> for ReactiveTimelineAuthoringError {
    fn from(value: SignalTimelineError) -> Self {
        Self::SignalTimeline(value)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{Property, ReactiveValue, RIGHT};

    use super::*;
    use crate::Circle;

    #[test]
    fn tracker_play_uses_shared_cursor_rate_function_and_native_signal_track() {
        let mut scene = ReactiveTimelineScene::new();
        let circle = scene.add(Circle::new(0.5));
        let tracker = scene.value_tracker(0.0);
        let position = scene.position_from_tracker(tracker, RIGHT, noon_core::Vec2::ZERO);
        scene.bind_position(circle, position);

        scene
            .play_value(tracker, 2.0)
            .rate_func(RateFunction::Linear)
            .run_time(2.0)
            .unwrap();

        assert_eq!(scene.time(), 2.0);
        let track = &scene.signal_timeline().tracks()[0];
        assert_eq!(track.signal, tracker.signal_id());
        assert_eq!(track.from, 0.0);
        assert_eq!(track.to, 2.0);
        assert_eq!(track.timing.easing, RateFunction::Linear);
        let timed = scene.timed_semantic_scene().unwrap();
        assert_eq!(
            timed.semantic().reactive().bindings()[0].property,
            Property::Position
        );
    }

    #[test]
    fn consecutive_tracker_plays_chain_from_previous_target() {
        let mut scene = ReactiveTimelineScene::new();
        let tracker = scene.value_tracker(1.0);
        scene.play_value(tracker, 3.0).run_time(1.0).unwrap();
        scene.wait(0.5).unwrap();
        scene.play_value(tracker, 5.0).run_time(1.0).unwrap();

        let tracks = scene.signal_timeline().tracks();
        assert_eq!(tracks[0].from, 1.0);
        assert_eq!(tracks[0].to, 3.0);
        assert_eq!(tracks[1].from, 3.0);
        assert_eq!(tracks[1].to, 5.0);
        assert_eq!(tracks[1].timing.start_time, 1.5);
        assert_eq!(
            scene
                .timed_semantic_scene()
                .unwrap()
                .semantic()
                .compile_reactive()
                .unwrap()
                .instantiate()
                .value(tracker.signal_id()),
            Some(&ReactiveValue::Scalar(1.0))
        );
    }
}
