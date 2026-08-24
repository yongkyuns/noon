use noon_core::{
    ObjectDefinition, PatchError, ReactiveBinding, ReactiveError, ReactiveGraphDefinition,
    SemanticScene, SignalDefinition, TrackDefinition,
};
use serde::{Deserialize, Serialize};

use crate::{IrError, FORMAT_VERSION};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReactiveGraphDocument {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<SignalDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<ReactiveBinding>,
}

impl ReactiveGraphDocument {
    pub fn from_graph(graph: &ReactiveGraphDefinition) -> Self {
        Self {
            signals: graph.signals().to_vec(),
            bindings: graph.bindings().to_vec(),
        }
    }

    pub fn into_graph(self) -> Result<ReactiveGraphDefinition, ReactiveError> {
        ReactiveGraphDefinition::from_parts(self.signals, self.bindings)
    }

    pub fn is_empty(&self) -> bool {
        self.signals.is_empty() && self.bindings.is_empty()
    }
}

/// Versioned semantic scene transport.
///
/// The deterministic scene fields deliberately retain the existing `SceneDocument`
/// shape. Native reactive declarations are an optional additive field so existing
/// scene JSON remains valid and stable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticSceneDocument {
    pub version: u32,
    pub objects: Vec<ObjectDefinition>,
    pub tracks: Vec<TrackDefinition>,
    #[serde(default, skip_serializing_if = "ReactiveGraphDocument::is_empty")]
    pub reactive: ReactiveGraphDocument,
}

impl SemanticSceneDocument {
    pub fn from_semantic(scene: &SemanticScene) -> Self {
        Self {
            version: FORMAT_VERSION,
            objects: scene.definition().objects().to_vec(),
            tracks: scene.definition().tracks().to_vec(),
            reactive: ReactiveGraphDocument::from_graph(scene.reactive()),
        }
    }

    pub fn into_semantic(self) -> Result<SemanticScene, SemanticIrError> {
        if self.version != FORMAT_VERSION {
            return Err(SemanticIrError::Scene(IrError::UnsupportedVersion(
                self.version,
            )));
        }
        let definition = noon_core::SceneDefinition::from_parts(self.objects, self.tracks)
            .map_err(IrError::Patch)
            .map_err(SemanticIrError::Scene)?;
        let reactive = self
            .reactive
            .into_graph()
            .map_err(SemanticIrError::Reactive)?;
        let mut scene = SemanticScene::from_definition(definition);
        *scene.reactive_mut() = reactive;
        scene
            .compile_reactive()
            .map_err(SemanticIrError::Reactive)?;
        Ok(scene)
    }
}

#[derive(Debug)]
pub enum SemanticIrError {
    Scene(IrError),
    Reactive(ReactiveError),
}

impl std::fmt::Display for SemanticIrError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scene(error) => error.fmt(formatter),
            Self::Reactive(error) => write!(formatter, "invalid Noon reactive graph: {error}"),
        }
    }
}

impl std::error::Error for SemanticIrError {}

impl From<IrError> for SemanticIrError {
    fn from(value: IrError) -> Self {
        Self::Scene(value)
    }
}

impl From<ReactiveError> for SemanticIrError {
    fn from(value: ReactiveError) -> Self {
        Self::Reactive(value)
    }
}

impl From<serde_json::Error> for SemanticIrError {
    fn from(value: serde_json::Error) -> Self {
        Self::Scene(IrError::Json(value))
    }
}

impl From<PatchError> for SemanticIrError {
    fn from(value: PatchError) -> Self {
        Self::Scene(IrError::Patch(value))
    }
}

pub fn encode_semantic_scene(scene: &SemanticScene) -> Result<String, SemanticIrError> {
    scene.compile_reactive()?;
    Ok(serde_json::to_string(&SemanticSceneDocument::from_semantic(
        scene,
    ))?)
}

pub fn decode_semantic_scene(json: &str) -> Result<SemanticScene, SemanticIrError> {
    let document: SemanticSceneDocument = serde_json::from_str(json)?;
    document.into_semantic()
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, Property, ReactiveExpr, ReactiveValue, Vec2};

    use super::*;

    #[test]
    fn ordinary_scene_document_is_valid_semantic_scene_with_empty_graph() {
        let mut definition = noon_core::SceneDefinition::new();
        definition.add(GeometryRef::circle(1.0));
        let json = crate::encode_scene(&definition).expect("plain scene must serialize");

        let semantic = decode_semantic_scene(&json).expect("plain scene must decode semantically");
        assert_eq!(semantic.definition(), &definition);
        assert!(semantic.reactive().signals().is_empty());
        assert!(semantic.reactive().bindings().is_empty());
    }

    #[test]
    fn reactive_scene_round_trip_preserves_signal_graph_without_private_counters() {
        let mut scene = SemanticScene::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let tracker = scene.add_input(0.25_f32);
        let position = scene.add_derived(ReactiveExpr::Add(
            Box::new(ReactiveExpr::vec2(Vec2::new(0.0, 1.0))),
            Box::new(ReactiveExpr::Mul(
                Box::new(ReactiveExpr::signal(tracker)),
                Box::new(ReactiveExpr::vec2(Vec2::new(1.0, 0.0))),
            )),
        ));
        scene.bind(position, object, Property::Position);

        let json = encode_semantic_scene(&scene).expect("semantic scene must serialize");
        assert!(json.contains("\"reactive\""));
        assert!(json.contains("\"signals\""));
        assert!(json.contains("\"bindings\""));
        assert!(!json.contains("next_signal_id"));

        let decoded = decode_semantic_scene(&json).expect("semantic scene must deserialize");
        assert_eq!(decoded.definition(), scene.definition());
        assert_eq!(decoded.reactive().signals(), scene.reactive().signals());
        assert_eq!(decoded.reactive().bindings(), scene.reactive().bindings());
        let state = decoded
            .compile_reactive()
            .expect("decoded graph must compile")
            .instantiate();
        assert_eq!(
            state.value(position),
            Some(&ReactiveValue::Vec2(Vec2::new(0.25, 1.0)))
        );
    }

    #[test]
    fn python_value_tracker_wire_shape_decodes_into_core_graph() {
        let json = r#"{
            "version":1,
            "objects":[{
                "id":0,
                "geometry":{"circle":{"radius":1.0}},
                "transform":{"translation":{"x":0.0,"y":0.0},"rotation":0.0,"scale":{"x":1.0,"y":1.0}},
                "style":{"fill":null,"stroke":null,"stroke_width":1.0,"stroke_join":"round","stroke_cap":"round","opacity":1.0}
            }],
            "tracks":[],
            "reactive":{
                "signals":[{"id":0,"source":{"input":{"scalar":1.5}}}],
                "bindings":[{"signal":0,"object":0,"property":"rotation"}]
            }
        }"#;

        let scene = decode_semantic_scene(json).expect("Python wire shape must decode");
        let program = scene.compile_reactive().expect("graph must compile");
        let state = program.instantiate();
        assert_eq!(state.value(noon_core::SignalId::new(0)), Some(&ReactiveValue::Scalar(1.5)));
        assert_eq!(scene.reactive().bindings()[0].property, Property::Rotation);
    }
}
