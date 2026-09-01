use noon::RetainedScene;
use noon_compile::RetainedCompiledScene;
use noon_core::{FamilyAnimationRequest, ObjectId, SceneDefinition, TrackDefinition};
use noon_ir::{SceneSpec, SceneSpecError};
use serde::Deserialize;

use crate::{
    retained_authoring_scene_spec, retained_scene_spec_runtime, MixedRetainedAuthoringError,
    RetainedAuthoringDocument, RetainedAuthoringSceneSpecError, RetainedTrackAuthoringSpec,
};

/// One protocol-v2 retained payload.
///
/// The established object document remains the compatibility source-level text schema;
/// animation tracks and semantic-family animations are additive wire fields. Inputs are
/// normalized into one canonical `SceneSpec` before any retained/runtime lowering.
#[derive(Debug, Deserialize)]
struct RetainedAuthoringWireDocument {
    #[serde(flatten)]
    document: RetainedAuthoringDocument,
    #[serde(default)]
    tracks: Vec<RetainedTrackAuthoringSpec>,
    #[serde(default)]
    family_animations: Vec<FamilyAnimationRequest>,
}

/// Normalize the current browser compatibility pair into one canonical mixed scene JSON.
///
/// This is the bounded producer/transport migration edge for #367. Python may still
/// assemble legacy geometry plus the retained text sidecar internally, but browser code
/// can immediately collapse that transitional shape into `SceneSpec` and carry only one
/// semantic scene document beyond the authoring worker.
pub fn canonical_retained_scene_spec_json(
    legacy_scene_json: &str,
    retained_document_json: &str,
) -> Result<String, MixedRetainedAuthoringError> {
    retained_authoring_scene_spec_from_json(legacy_scene_json, retained_document_json)?
        .to_json()
        .map_err(map_scene_spec_error)
}

fn retained_authoring_scene_spec_from_json(
    legacy_scene_json: &str,
    retained_document_json: &str,
) -> Result<SceneSpec, MixedRetainedAuthoringError> {
    let legacy = noon_ir::decode_scene(legacy_scene_json)?;
    let wire: RetainedAuthoringWireDocument = serde_json::from_str(retained_document_json)
        .map_err(|error| {
            MixedRetainedAuthoringError::RetainedDocument(format!(
                "invalid retained authoring document: {error}"
            ))
        })?;
    let mut spec = retained_authoring_scene_spec(&legacy, wire.document, wire.tracks)
        .map_err(map_scene_spec_adapter_error)?;
    spec.family_animations = wire.family_animations;
    spec.validate().map_err(map_scene_spec_error)?;
    Ok(spec)
}

/// Wire-facing mixed retained scene.
///
/// Protocol v2 remains backwards-compatible while the consumer path is canonical:
///
/// `legacy geometry + retained sidecar -> SceneSpec -> RetainedScene -> compiler`.
///
/// The old split lowerer remains only as an equivalence oracle during #367 migration.
#[derive(Clone, Debug)]
pub struct MixedRetainedAuthoringScene {
    inner: retained_scene_spec_runtime::CanonicalRetainedAuthoringScene,
}

impl MixedRetainedAuthoringScene {
    pub fn from_json(
        legacy_scene_json: &str,
        retained_document_json: &str,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        Self::from_scene_spec(retained_authoring_scene_spec_from_json(
            legacy_scene_json,
            retained_document_json,
        )?)
    }

    pub fn from_parts(
        legacy: &SceneDefinition,
        retained: RetainedAuthoringDocument,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        Self::from_parts_with_tracks(legacy, retained, Vec::new())
    }

    pub fn from_parts_with_tracks(
        legacy: &SceneDefinition,
        retained: RetainedAuthoringDocument,
        retained_tracks: Vec<RetainedTrackAuthoringSpec>,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        let spec = retained_authoring_scene_spec(legacy, retained, retained_tracks)
            .map_err(map_scene_spec_adapter_error)?;
        Self::from_scene_spec(spec)
    }

    /// Consume the canonical mixed authoring document directly.
    ///
    /// This is the target consumer boundary for future Rust/Python/JavaScript producer
    /// migration. Source-level text is still compiled by the existing shared Rust
    /// backends and enters the ordinary retained resource/runtime path.
    pub fn from_scene_spec(spec: SceneSpec) -> Result<Self, MixedRetainedAuthoringError> {
        Ok(Self {
            inner: retained_scene_spec_runtime::CanonicalRetainedAuthoringScene::from_scene_spec(
                spec,
            )?,
        })
    }

    pub const fn scene(&self) -> &RetainedScene {
        self.inner.scene()
    }

    pub fn tracks(&self) -> &[TrackDefinition] {
        self.inner.tracks()
    }

    pub fn compile(&self) -> Result<RetainedCompiledScene, MixedRetainedAuthoringError> {
        self.inner.compile()
    }

    pub fn into_scene(self) -> RetainedScene {
        self.inner.into_scene()
    }

    pub const fn camera_object(&self) -> Option<ObjectId> {
        self.inner.camera_object()
    }
}

fn map_scene_spec_adapter_error(
    error: RetainedAuthoringSceneSpecError,
) -> MixedRetainedAuthoringError {
    match error {
        RetainedAuthoringSceneSpecError::RetainedDocument(error) => {
            MixedRetainedAuthoringError::RetainedDocument(error)
        }
        RetainedAuthoringSceneSpecError::TrackMaterialization(error) => {
            MixedRetainedAuthoringError::TrackMaterialization(error)
        }
        RetainedAuthoringSceneSpecError::SceneSpec(error) => map_scene_spec_error(error),
    }
}

fn map_scene_spec_error(error: SceneSpecError) -> MixedRetainedAuthoringError {
    match error {
        SceneSpecError::PainterOrderOutOfRange {
            order,
            object_count,
        } => MixedRetainedAuthoringError::PainterOrderOutOfRange {
            order,
            object_count,
        },
        SceneSpecError::ObjectCountOverflow => MixedRetainedAuthoringError::ObjectCountOverflow,
        SceneSpecError::DuplicateObject(object) => {
            MixedRetainedAuthoringError::ObjectIdentityCollision(object)
        }
        error => MixedRetainedAuthoringError::RetainedDocument(format!(
            "invalid canonical mixed SceneSpec: {error}"
        )),
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(js_name = canonicalRetainedSceneSpecJson)]
    pub fn wasm_canonical_retained_scene_spec_json(
        legacy_scene_json: &str,
        retained_document_json: &str,
    ) -> Result<String, JsValue> {
        super::canonical_retained_scene_spec_json(legacy_scene_json, retained_document_json)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{
        FamilyAnimationLeafBinding, FamilyAnimationMode, FamilyAnimationSpec, GeometryRef,
        Property, RateFunction, SemanticNodeId, TrackTiming, TrackValues, Vec2,
    };
    use noon_ir::{encode_scene, ObjectSpecContent};
    use serde_json::json;

    use crate::{RetainedAuthoringTextObject, RetainedTextAuthoringSpec};

    fn retained_document(object: ObjectId) -> RetainedAuthoringDocument {
        RetainedAuthoringDocument::new(vec![RetainedAuthoringTextObject {
            object,
            order: 0,
            text: RetainedTextAuthoringSpec::native(
                "Animated Noon",
                noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY,
                48.0,
                -1.0,
            )
            .unwrap(),
        }])
        .unwrap()
    }

    #[test]
    fn protocol_v2_object_only_document_remains_backwards_compatible() {
        let legacy = SceneDefinition::new();
        let object = ObjectId::new(1_u64 << 52);
        let document = retained_document(object);
        let mixed = MixedRetainedAuthoringScene::from_json(
            &encode_scene(&legacy).unwrap(),
            &document.to_json().unwrap(),
        )
        .unwrap();
        assert!(mixed.tracks().is_empty());
        assert_eq!(mixed.scene().objects()[0].id, object);
    }

    #[test]
    fn protocol_v2_normalizes_geometry_text_and_tracks_through_scene_spec() {
        let mut legacy = SceneDefinition::new();
        let geometry = legacy.add(GeometryRef::circle(0.5));
        let object = ObjectId::new(1_u64 << 52);
        let document = RetainedAuthoringDocument::new(vec![RetainedAuthoringTextObject {
            object,
            order: 1,
            text: RetainedTextAuthoringSpec::native(
                "Animated Noon",
                noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY,
                48.0,
                -1.0,
            )
            .unwrap(),
        }])
        .unwrap();
        let mut wire = serde_json::to_value(document).unwrap();
        wire["tracks"] = json!([RetainedTrackAuthoringSpec::new(
            object,
            Property::Scale,
            TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::ZERO,
            },
            TrackTiming::new(0.0, 1.0, RateFunction::Smooth),
        )]);

        let mixed = MixedRetainedAuthoringScene::from_json(
            &encode_scene(&legacy).unwrap(),
            &serde_json::to_string(&wire).unwrap(),
        )
        .unwrap();
        assert_eq!(
            mixed
                .scene()
                .objects()
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>(),
            vec![geometry, object]
        );
        assert_eq!(mixed.tracks().len(), 1);
        assert_eq!(mixed.tracks()[0].property, Property::Scale);
        assert_eq!(mixed.tracks()[0].object, object);
        assert_eq!(
            mixed.tracks()[0].values,
            TrackValues::Vec2 {
                from: Vec2::new(
                    noon::NATIVE_POINT_TO_SCENE_SCALE,
                    noon::NATIVE_POINT_TO_SCENE_SCALE,
                ),
                to: Vec2::ZERO,
            }
        );
        assert_eq!(mixed.compile().unwrap().object_index(object), Some(1));
    }

    #[test]
    fn browser_normalizer_emits_one_valid_canonical_scene_spec() {
        let mut legacy = SceneDefinition::new();
        let geometry = legacy.add(GeometryRef::circle(0.5));
        let object = ObjectId::new(1_u64 << 52);
        let document = RetainedAuthoringDocument::new(vec![RetainedAuthoringTextObject {
            object,
            order: 1,
            text: RetainedTextAuthoringSpec::native(
                "Canonical transport",
                noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY,
                48.0,
                -1.0,
            )
            .unwrap(),
        }])
        .unwrap();

        let json = canonical_retained_scene_spec_json(
            &encode_scene(&legacy).unwrap(),
            &document.to_json().unwrap(),
        )
        .unwrap();
        assert!(!json.contains("noon.authoring.retained"));

        let spec = SceneSpec::from_json(&json).unwrap();
        assert_eq!(spec.objects.len(), 2);
        assert_eq!(spec.objects[0].id, geometry);
        assert_eq!(spec.objects[1].id, object);
        assert!(matches!(
            spec.objects[0].content,
            ObjectSpecContent::Geometry(_)
        ));
        assert!(matches!(
            spec.objects[1].content,
            ObjectSpecContent::Text(_)
        ));
    }

    #[test]
    fn browser_normalizer_carries_family_animation_requests_into_scene_spec() {
        let legacy = SceneDefinition::new();
        let object = ObjectId::new(1_u64 << 52);
        let document = retained_document(object);
        let mut wire = serde_json::to_value(document).unwrap();
        let target = SemanticNodeId::new(7, 0);
        let leaf = SemanticNodeId::new(8, 0);
        let animation = FamilyAnimationRequest::new(
            target,
            vec![FamilyAnimationLeafBinding::new(leaf, object)],
            FamilyAnimationSpec::new(
                FamilyAnimationMode::Reveal,
                0.0,
                1.0,
                1.0,
                RateFunction::Smooth,
                false,
                false,
            )
            .unwrap(),
        )
        .unwrap();
        wire["family_animations"] = json!([animation]);

        let json = canonical_retained_scene_spec_json(
            &encode_scene(&legacy).unwrap(),
            &serde_json::to_string(&wire).unwrap(),
        )
        .unwrap();
        let spec = SceneSpec::from_json(&json).unwrap();
        assert_eq!(spec.family_animations.len(), 1);
        assert_eq!(spec.family_animations[0], animation);
    }

    #[test]
    fn invalid_object_document_is_rejected_before_canonical_lowering() {
        let legacy = SceneDefinition::new();
        let object = ObjectId::new(1_u64 << 52);
        let document = retained_document(object);
        let mut wire = serde_json::to_value(document).unwrap();
        wire["channel"] = json!("wrong.channel");
        wire["tracks"] = json!([RetainedTrackAuthoringSpec::new(
            object,
            Property::Scale,
            TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::ZERO,
            },
            TrackTiming::new(0.0, 1.0, RateFunction::Smooth),
        )]);

        let error = MixedRetainedAuthoringScene::from_json(
            &encode_scene(&legacy).unwrap(),
            &serde_json::to_string(&wire).unwrap(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            MixedRetainedAuthoringError::RetainedDocument(_)
        ));
    }
}
