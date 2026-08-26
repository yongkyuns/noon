use noon_core::{
    NativeInputBinding, NativeInputDefinition, SignalTimelineDefinition, SignalTimelineError,
    TimedSemanticScene,
};
use serde::{Deserialize, Serialize};

use super::{ReactiveGraphDocument, SemanticIrError, SemanticSceneDocument};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimedSemanticSceneDocument {
    #[serde(flatten)]
    pub scene: SemanticSceneDocument,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signal_tracks: Vec<noon_core::SignalTrackDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_inputs: Vec<NativeInputBinding>,
}

impl TimedSemanticSceneDocument {
    pub fn from_timed(scene: &TimedSemanticScene) -> Self {
        Self {
            scene: SemanticSceneDocument::from_semantic(scene.semantic()),
            signal_tracks: scene.signal_timeline().tracks().to_vec(),
            native_inputs: scene.native_inputs().bindings().to_vec(),
        }
    }

    pub fn into_timed(self) -> Result<TimedSemanticScene, TimedSemanticIrError> {
        let semantic = self.scene.into_semantic()?;
        let timeline =
            SignalTimelineDefinition::from_parts(semantic.reactive(), self.signal_tracks)?;
        let native_inputs =
            NativeInputDefinition::from_parts(semantic.reactive(), self.native_inputs)
                .map_err(SignalTimelineError::from)?;
        Ok(TimedSemanticScene::from_parts_with_native_inputs(
            semantic,
            timeline,
            native_inputs,
        )?)
    }
}

#[derive(Debug)]
pub enum TimedSemanticIrError {
    Semantic(SemanticIrError),
    SignalTimeline(SignalTimelineError),
}

impl std::fmt::Display for TimedSemanticIrError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Semantic(error) => error.fmt(formatter),
            Self::SignalTimeline(error) => {
                write!(
                    formatter,
                    "invalid Noon signal timeline/input document: {error}"
                )
            }
        }
    }
}

impl std::error::Error for TimedSemanticIrError {}

impl From<SemanticIrError> for TimedSemanticIrError {
    fn from(value: SemanticIrError) -> Self {
        Self::Semantic(value)
    }
}

impl From<SignalTimelineError> for TimedSemanticIrError {
    fn from(value: SignalTimelineError) -> Self {
        Self::SignalTimeline(value)
    }
}

impl From<serde_json::Error> for TimedSemanticIrError {
    fn from(value: serde_json::Error) -> Self {
        Self::Semantic(SemanticIrError::from(value))
    }
}

pub fn encode_timed_semantic_scene(
    scene: &TimedSemanticScene,
) -> Result<String, TimedSemanticIrError> {
    scene
        .semantic()
        .compile_reactive()
        .map_err(SemanticIrError::Reactive)?;
    SignalTimelineDefinition::from_parts(
        scene.semantic().reactive(),
        scene.signal_timeline().tracks().to_vec(),
    )?;
    scene
        .native_inputs()
        .validate(scene.semantic().reactive())
        .map_err(SignalTimelineError::from)?;
    if let Some(track) = scene
        .signal_timeline()
        .tracks()
        .iter()
        .find(|track| scene.native_inputs().drives(track.signal))
    {
        return Err(SignalTimelineError::ExternallyDrivenSignal(track.signal).into());
    }
    Ok(serde_json::to_string(
        &TimedSemanticSceneDocument::from_timed(scene),
    )?)
}

pub fn decode_timed_semantic_scene(json: &str) -> Result<TimedSemanticScene, TimedSemanticIrError> {
    let document: TimedSemanticSceneDocument = serde_json::from_str(json)?;
    document.into_timed()
}

/// Convenience builder for frontends that already have the ordinary semantic document pieces.
pub fn timed_document_from_parts(
    version: u32,
    objects: Vec<noon_core::ObjectDefinition>,
    tracks: Vec<noon_core::TrackDefinition>,
    reactive: ReactiveGraphDocument,
    signal_tracks: Vec<noon_core::SignalTrackDefinition>,
) -> TimedSemanticSceneDocument {
    timed_document_from_parts_with_native_inputs(
        version,
        objects,
        tracks,
        reactive,
        signal_tracks,
        Vec::new(),
    )
}

pub fn timed_document_from_parts_with_native_inputs(
    version: u32,
    objects: Vec<noon_core::ObjectDefinition>,
    tracks: Vec<noon_core::TrackDefinition>,
    reactive: ReactiveGraphDocument,
    signal_tracks: Vec<noon_core::SignalTrackDefinition>,
    native_inputs: Vec<NativeInputBinding>,
) -> TimedSemanticSceneDocument {
    TimedSemanticSceneDocument {
        scene: SemanticSceneDocument {
            version,
            objects,
            tracks,
            camera_object: None,
            reactive,
        },
        signal_tracks,
        native_inputs,
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, NativeEventSource, NativeStateSource, Property, RateFunction, ReactiveExpr,
        SignalTimelineDefinition, TimedSemanticScene, TrackTiming, Vec2,
    };

    use super::*;

    #[test]
    fn timed_semantic_round_trip_preserves_signal_tracks() {
        let mut semantic = noon_core::SemanticScene::new();
        let object = semantic.add(GeometryRef::circle(0.5));
        let tracker = semantic.add_input(0.0_f32);
        let position = semantic.add_derived(ReactiveExpr::Mul(
            Box::new(ReactiveExpr::signal(tracker)),
            Box::new(ReactiveExpr::vec2(Vec2::new(2.0, 0.0))),
        ));
        semantic.bind(position, object, Property::Position);
        let mut signal_timeline = SignalTimelineDefinition::new();
        signal_timeline
            .add_scalar_track(
                semantic.reactive(),
                tracker,
                0.0,
                1.0,
                TrackTiming::new(0.25, 1.5, RateFunction::Smooth),
            )
            .unwrap();
        let scene = TimedSemanticScene::from_parts(semantic, signal_timeline).unwrap();

        let json = encode_timed_semantic_scene(&scene).unwrap();
        assert!(json.contains("\"signal_tracks\""));
        let decoded = decode_timed_semantic_scene(&json).unwrap();
        assert_eq!(decoded, scene);
    }

    #[test]
    fn timed_semantic_round_trip_preserves_native_input_bindings() {
        let mut semantic = noon_core::SemanticScene::new();
        let pointer = semantic.add_input(Vec2::ZERO);
        let clicks = semantic.add_input(0.0_f32);
        let mut inputs = NativeInputDefinition::new();
        inputs
            .bind_state(NativeStateSource::PointerPosition, pointer)
            .bind_event(NativeEventSource::PointerDown { button: 0 }, clicks);
        let scene = TimedSemanticScene::from_parts_with_native_inputs(
            semantic,
            SignalTimelineDefinition::new(),
            inputs,
        )
        .unwrap();

        let json = encode_timed_semantic_scene(&scene).unwrap();
        assert!(json.contains("\"native_inputs\""));
        assert!(json.contains("pointer_position"));
        assert!(json.contains("pointer_down"));
        assert_eq!(decode_timed_semantic_scene(&json).unwrap(), scene);
    }

    #[test]
    fn ordinary_semantic_json_decodes_as_timed_scene_without_signal_tracks_or_inputs() {
        let mut semantic = noon_core::SemanticScene::new();
        semantic.add(GeometryRef::circle(1.0));
        let json = super::super::encode_semantic_scene(&semantic).unwrap();
        let decoded = decode_timed_semantic_scene(&json).unwrap();
        assert!(decoded.signal_timeline().tracks().is_empty());
        assert!(decoded.native_inputs().is_empty());
    }
}
