use serde::{Deserialize, Serialize};

use crate::{
    validate_track_timing, Property, ReactiveGraphDefinition, ReactiveValue, SemanticScene,
    SignalId, SignalSource, TimelineError, TrackTiming,
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
}

impl TimedSemanticScene {
    pub fn new(scene: SemanticScene) -> Self {
        Self {
            scene,
            signal_timeline: SignalTimelineDefinition::new(),
        }
    }

    pub fn from_parts(
        scene: SemanticScene,
        signal_timeline: SignalTimelineDefinition,
    ) -> Result<Self, SignalTimelineError> {
        let validated = SignalTimelineDefinition::from_parts(
            scene.reactive(),
            signal_timeline.tracks().to_vec(),
        )?;
        Ok(Self {
            scene,
            signal_timeline: validated,
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

    pub fn into_parts(self) -> (SemanticScene, SignalTimelineDefinition) {
        (self.scene, self.signal_timeline)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SignalTimelineError {
    Timeline(TimelineError),
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
                "signal {} is timeline-driven and cannot also be mutated as an external input",
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

#[cfg(test)]
mod tests {
    use crate::{GeometryRef, RateFunction, TrackTiming};

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
}
