use std::ops::{Deref, DerefMut};

use noon_core::{
    NativeEventSource, NativeInputDefinition, NativeStateSource, RateFunction, ReactiveValue,
    SignalSource, SignalTimelineDefinition, SignalTimelineError, TimedSemanticScene, TrackTiming,
    Vec2,
};

use crate::{AuthoringError, BoolSignal, ReactiveScene, ValueTracker, VectorSignal};

/// Reactive authoring scene with deterministic timeline-driven tracker inputs and
/// declarative native browser/runtime inputs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReactiveTimelineScene {
    scene: ReactiveScene,
    signal_timeline: SignalTimelineDefinition,
    native_inputs: NativeInputDefinition,
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

    pub fn native_inputs(&self) -> &NativeInputDefinition {
        &self.native_inputs
    }

    pub fn pointer_position_signal(&mut self) -> VectorSignal {
        let signal = self.scene.vector_signal(Vec2::ZERO);
        self.native_inputs
            .bind_state(NativeStateSource::PointerPosition, signal.signal_id());
        signal
    }

    pub fn pointer_button_signal(&mut self, button: u8, initial: bool) -> BoolSignal {
        let signal = self.scene.bool_signal(initial);
        self.native_inputs.bind_state(
            NativeStateSource::PointerButton { button },
            signal.signal_id(),
        );
        signal
    }

    pub fn key_state_signal(&mut self, code: impl Into<String>, initial: bool) -> BoolSignal {
        let signal = self.scene.bool_signal(initial);
        self.native_inputs.bind_state(
            NativeStateSource::Key { code: code.into() },
            signal.signal_id(),
        );
        signal
    }

    pub fn viewport_size_signal(&mut self) -> VectorSignal {
        let signal = self.scene.vector_signal(Vec2::ZERO);
        self.native_inputs
            .bind_state(NativeStateSource::ViewportSize, signal.signal_id());
        signal
    }

    pub fn wheel_delta_signal(&mut self) -> VectorSignal {
        let signal = self.scene.vector_signal(Vec2::ZERO);
        self.native_inputs
            .bind_state(NativeStateSource::WheelDelta, signal.signal_id());
        signal
    }

    pub fn gesture_delta_signal(&mut self, name: impl Into<String>) -> VectorSignal {
        let signal = self.scene.vector_signal(Vec2::ZERO);
        self.native_inputs.bind_state(
            NativeStateSource::GestureDelta { name: name.into() },
            signal.signal_id(),
        );
        signal
    }

    pub fn control_signal(&mut self, name: impl Into<String>, initial: f32) -> ValueTracker {
        let signal = self.scene.value_tracker(initial);
        self.native_inputs.bind_state(
            NativeStateSource::Control { name: name.into() },
            signal.signal_id(),
        );
        signal
    }

    pub fn pointer_down_events(&mut self, button: u8) -> ValueTracker {
        self.event_tracker(NativeEventSource::PointerDown { button })
    }

    pub fn pointer_up_events(&mut self, button: u8) -> ValueTracker {
        self.event_tracker(NativeEventSource::PointerUp { button })
    }

    pub fn key_press_events(&mut self, code: impl Into<String>) -> ValueTracker {
        self.event_tracker(NativeEventSource::KeyPress { code: code.into() })
    }

    pub fn key_release_events(&mut self, code: impl Into<String>) -> ValueTracker {
        self.event_tracker(NativeEventSource::KeyRelease { code: code.into() })
    }

    pub fn wheel_events(&mut self) -> ValueTracker {
        self.event_tracker(NativeEventSource::Wheel)
    }

    pub fn gesture_events(&mut self, name: impl Into<String>) -> ValueTracker {
        self.event_tracker(NativeEventSource::Gesture { name: name.into() })
    }

    pub fn control_commit_events(&mut self, name: impl Into<String>) -> ValueTracker {
        self.event_tracker(NativeEventSource::ControlCommit { name: name.into() })
    }

    pub fn timed_semantic_scene(&self) -> Result<TimedSemanticScene, SignalTimelineError> {
        TimedSemanticScene::from_parts_with_native_inputs(
            self.scene.semantic_scene(),
            self.signal_timeline.clone(),
            self.native_inputs.clone(),
        )
    }

    pub fn into_timed_semantic_scene(self) -> Result<TimedSemanticScene, SignalTimelineError> {
        TimedSemanticScene::from_parts_with_native_inputs(
            self.scene.into_semantic_scene(),
            self.signal_timeline,
            self.native_inputs,
        )
    }

    fn event_tracker(&mut self, source: NativeEventSource) -> ValueTracker {
        let signal = self.scene.value_tracker(0.0);
        self.native_inputs.bind_event(source, signal.signal_id());
        signal
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
        if self.native_inputs.drives(tracker.signal_id()) {
            return Err(SignalTimelineError::ExternallyDrivenSignal(tracker.signal_id()).into());
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
    use noon_core::{NativeInputBinding, Property, ReactiveValue, RIGHT};

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

    #[test]
    fn native_input_handles_lower_to_language_neutral_bindings() {
        let mut scene = ReactiveTimelineScene::new();
        let circle = scene.add(Circle::new(0.5));
        let pointer = scene.pointer_position_signal();
        scene.bind_position(circle, pointer);
        let visible = scene.key_state_signal("Space", false);
        scene.bind_presence(circle, visible);
        let control = scene.control_signal("opacity", 1.0);
        scene.bind_opacity(circle, control);
        let clicks = scene.pointer_down_events(0);
        scene.bind_rotation(circle, clicks);

        let timed = scene.timed_semantic_scene().unwrap();
        assert_eq!(timed.native_inputs().bindings().len(), 4);
        assert!(matches!(
            &timed.native_inputs().bindings()[0],
            NativeInputBinding::State {
                source: NativeStateSource::PointerPosition,
                signal,
            } if *signal == pointer.signal_id()
        ));
    }

    #[test]
    fn timeline_cannot_drive_a_native_control_signal() {
        let mut scene = ReactiveTimelineScene::new();
        let control = scene.control_signal("opacity", 1.0);
        assert!(matches!(
            scene.play_value(control, 0.5).run_time(1.0),
            Err(ReactiveTimelineAuthoringError::SignalTimeline(
                SignalTimelineError::ExternallyDrivenSignal(signal)
            )) if signal == control.signal_id()
        ));
    }
}
