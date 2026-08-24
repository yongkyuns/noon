use serde::{Deserialize, Serialize};

use crate::{
    validate_track_timing, NativeInputDefinition, NativeInputError, Property,
    ReactiveGraphDefinition, ReactiveValue, SemanticScene, SignalId, SignalSource, TimelineError,
    TrackTiming,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignalTrackDefinition {
    pub signal: SignalId,
    pub from: f32,
    pub to: f32,
    pub timing: TrackTiming,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SignalTimelineDefinition {
    tracks: Vec<SignalTrackDefinition>,
}

impl SignalTimelineDefinition {
    pub const fn new() -> Self {
        Self { tracks: Vec::new() }
    }

    pub fn from_parts(
        graph: &ReactiveGraphDefinition,
        tracks: Vec<SignalTrackDefinition>,
    ) -> Result<Self, SignalTimelineError> {
        let mut result = Self::new();
        for track in tracks {
            result.add_track(graph, track)?;
        }
        Ok(result)
    }

    pub fn add_scalar_track(
        &mut self,
        graph: &ReactiveGraphDefinition,
        signal: SignalId,
        from: f32,
        to: f32,
        timing: TrackTiming,
    ) -> Result<&mut Self, SignalTimelineError> {
        self.add_track(
            graph,
            SignalTrackDefinition {
                signal,
                from,
                to,
                timing,
            },
        )?;
        Ok(self)
    }

    pub fn add_track(
        &mut self,
        graph: &ReactiveGraphDefinition,
        track: SignalTrackDefinition,
    ) -> Result<(), SignalTimelineError> {
        validate_track_timing(Property::Rotation, track.timing)?;
        if !track.from.is_finite() {
            return Err(SignalTimelineError::NonFiniteValue {
                signal: track.signal,
                value: track.from,
            });
        }
        if !track.to.is_finite() {
            return Err(SignalTimelineError::NonFiniteValue {
                signal: track.signal,
                value: track.to,
            });
        }
        let signal = graph
            .signals()
            .iter()
            .find(|definition| definition.id == track.signal)
            .ok_or(SignalTimelineError::UnknownSignal(track.signal))?;
        match &signal.source {
            SignalSource::Input(ReactiveValue::Scalar(_)) => {}
            SignalSource::Input(_) => {
                return Err(SignalTimelineError::NonScalarSignal(track.signal));
            }
            SignalSource::Derived(_) => {
                return Err(SignalTimelineError::NotInputSignal(track.signal));
            }
        }

        if let Some(previous) = self
            .tracks
            .iter()
            .rev()
            .find(|existing| existing.signal == track.signal)
        {
            let previous_end = previous.timing.start_time + previous.timing.duration;
            if track.timing.start_time < previous_end {
                return Err(SignalTimelineError::OverlappingTracks {
                    signal: track.signal,
                    previous_end,
                    next_start: track.timing.start_time,
                });
            }
            if previous.to != track.from {
                return Err(SignalTimelineError::DiscontinuousTrack {
                    signal: track.signal,
                    expected: previous.to,
                    actual: track.from,
                });
            }
        } else if let SignalSource::Input(ReactiveValue::Scalar(initial)) = &signal.source {
            if *initial != track.from {
                return Err(SignalTimelineError::DiscontinuousTrack {
                    signal: track.signal,
                    expected: *initial,
                    actual: track.from,
                });
            }
        }

        self.tracks.push(track);
        Ok(())
    }

    pub fn tracks(&self) -> &[SignalTrackDefinition] {
        &self.tracks
    }

    pub fn drives(&self, signal: SignalId) -> bool {
        self.tracks.iter().any(|track| track.signal == signal)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimedSemanticScene {
    scene: SemanticScene,
    signal_timeline: SignalTimelineDefinition,
    native_inputs: NativeInputDefinition,
}

impl TimedSemanticScene {
    pub fn new(scene: SemanticScene) -> Self {
        Self {
            scene,
            signal_timeline: SignalTimelineDefinition::new(),
            native_inputs: NativeInputDefinition::new(),
        }
    }

    pub fn from_parts(
        scene: SemanticScene,
        signal_timeline: SignalTimelineDefinition,
    ) -> Result<Self, SignalTimelineError> {
        Self::from_parts_with_native_inputs(scene, signal_timeline, NativeInputDefinition::new())
    }

    pub fn from_parts_with_native_inputs(
        scene: SemanticScene,
        signal_timeline: SignalTimelineDefinition,
        native_inputs: NativeInputDefinition,
    ) -> Result<Self, SignalTimelineError> {
        let validated = SignalTimelineDefinition::from_parts(
            scene.reactive(),
            signal_timeline.tracks().to_vec(),
        )?;
        native_inputs.validate(scene.reactive())?;
        if let Some(track) = validated
            .tracks()
            .iter()
            .find(|track| native_inputs.drives(track.signal))
        {
            return Err(SignalTimelineError::ExternallyDrivenSignal(track.signal));
        }
        Ok(Self {
            scene,
            signal_timeline: validated,
            native_inputs,
        })
    }

    pub fn semantic(&self) -> &SemanticScene {
        &self.scene
    }

    pub fn semantic_mut(&mut self) -> &mut SemanticScene {
        &mut self.scene
    }

    pub fn signal_timeline(&self) -> &SignalTimelineDefinition {
        &self.signal_timeline
    }

    pub fn signal_timeline_mut(&mut self) -> &mut SignalTimelineDefinition {
        &mut self.signal_timeline
    }

    pub fn native_inputs(&self) -> &NativeInputDefinition {
        &self.native_inputs
    }

    pub fn native_inputs_mut(&mut self) -> &mut NativeInputDefinition {
        &mut self.native_inputs
    }

    pub fn into_parts(self) -> (SemanticScene, SignalTimelineDefinition) {
        (self.scene, self.signal_timeline)
    }

    pub fn into_all_parts(
        self,
    ) -> (
        SemanticScene,
        SignalTimelineDefinition,
        NativeInputDefinition,
    ) {
        (self.scene, self.signal_timeline, self.native_inputs)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SignalTimelineError {
    Timeline(TimelineError),
    NativeInput(NativeInputError),
    UnknownSignal(SignalId),
    NotInputSignal(SignalId),
    NonScalarSignal(SignalId),
    NonFiniteValue {
        signal: SignalId,
        value: f32,
    },
    OverlappingTracks {
        signal: SignalId,
        previous_end: f64,
        next_start: f64,
    },
    DiscontinuousTrack {
        signal: SignalId,
        expected: f32,
        actual: f32,
    },
    ExternallyDrivenSignal(SignalId),
}

impl std::fmt::Display for SignalTimelineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeline(error) => error.fmt(formatter),
            Self::NativeInput(error) => write!(formatter, "invalid native input binding: {error}"),
            Self::UnknownSignal(signal) => write!(formatter, "unknown signal id {}", signal.get()),
            Self::NotInputSignal(signal) => write!(
                formatter,
                "signal {} is derived and cannot be driven by a signal track",
                signal.get()
            ),
            Self::NonScalarSignal(signal) => write!(
                formatter,
                "signal {} is not scalar and cannot use a ValueTracker track",
                signal.get()
            ),
            Self::NonFiniteValue { signal, value } => write!(
                formatter,
                "signal {} track contains non-finite value {value}",
                signal.get()
            ),
            Self::OverlappingTracks {
                signal,
                previous_end,
                next_start,
            } => write!(
                formatter,
                "signal {} tracks overlap: previous ends at {previous_end}, next starts at {next_start}",
                signal.get()
            ),
            Self::DiscontinuousTrack {
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "signal {} track chain is discontinuous: expected {expected}, got {actual}",
                signal.get()
            ),
            Self::ExternallyDrivenSignal(signal) => write!(
                formatter,
                "signal {} cannot be driven by both a signal timeline and an external/native input",
                signal.get()
            ),
        }
    }
}

impl std::error::Error for SignalTimelineError {}

impl From<TimelineError> for SignalTimelineError {
    fn from(value: TimelineError) -> Self {
        Self::Timeline(value)
    }
}

impl From<NativeInputError> for SignalTimelineError {
    fn from(value: NativeInputError) -> Self {
        Self::NativeInput(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        GeometryRef, NativeEventSource, NativeStateSource, RateFunction, TrackTiming, Vec2,
    };

    use super::*;

    #[test]
    fn scalar_input_tracks_validate_chronology_and_continuity() {
        let mut scene = SemanticScene::new();
        scene.add(GeometryRef::circle(1.0));
        let signal = scene.add_input(0.0_f32);
        let mut timeline = SignalTimelineDefinition::new();
        timeline
            .add_scalar_track(
                scene.reactive(),
                signal,
                0.0,
                1.0,
                TrackTiming::new(0.0, 1.0, RateFunction::Smooth),
            )
            .unwrap()
            .add_scalar_track(
                scene.reactive(),
                signal,
                1.0,
                3.0,
                TrackTiming::new(1.5, 0.5, RateFunction::Linear),
            )
            .unwrap();
        assert_eq!(timeline.tracks().len(), 2);
    }

    #[test]
    fn signal_tracks_reject_overlap_and_discontinuity() {
        let mut scene = SemanticScene::new();
        let signal = scene.add_input(0.0_f32);
        let mut timeline = SignalTimelineDefinition::new();
        timeline
            .add_scalar_track(
                scene.reactive(),
                signal,
                0.0,
                1.0,
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .unwrap();
        assert!(matches!(
            timeline.add_scalar_track(
                scene.reactive(),
                signal,
                1.0,
                2.0,
                TrackTiming::new(0.5, 1.0, RateFunction::Linear),
            ),
            Err(SignalTimelineError::OverlappingTracks { .. })
        ));
        assert!(matches!(
            timeline.add_scalar_track(
                scene.reactive(),
                signal,
                2.0,
                3.0,
                TrackTiming::new(1.0, 1.0, RateFunction::Linear),
            ),
            Err(SignalTimelineError::DiscontinuousTrack { .. })
        ));
    }

    #[test]
    fn timed_scene_validates_native_input_types_and_timeline_ownership() {
        let mut scene = SemanticScene::new();
        let pointer = scene.add_input(Vec2::ZERO);
        let clicks = scene.add_input(0.0_f32);
        let mut inputs = NativeInputDefinition::new();
        inputs
            .bind_state(NativeStateSource::PointerPosition, pointer)
            .bind_event(NativeEventSource::PointerDown { button: 0 }, clicks);
        let timed = TimedSemanticScene::from_parts_with_native_inputs(
            scene.clone(),
            SignalTimelineDefinition::new(),
            inputs,
        )
        .unwrap();
        assert_eq!(timed.native_inputs().bindings().len(), 2);

        let mut conflicting_inputs = NativeInputDefinition::new();
        conflicting_inputs.bind_event(NativeEventSource::Wheel, clicks);
        let mut timeline = SignalTimelineDefinition::new();
        timeline
            .add_scalar_track(
                scene.reactive(),
                clicks,
                0.0,
                1.0,
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .unwrap();
        assert!(matches!(
            TimedSemanticScene::from_parts_with_native_inputs(
                scene,
                timeline,
                conflicting_inputs,
            ),
            Err(SignalTimelineError::ExternallyDrivenSignal(signal)) if signal == clicks
        ));
    }
}
