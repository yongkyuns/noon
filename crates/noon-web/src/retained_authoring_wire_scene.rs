use noon::RetainedScene;
use noon_compile::RetainedCompiledScene;
use noon_core::{ObjectId, SceneDefinition, TrackDefinition};
use serde::Deserialize;

use crate::{
    retained_authoring_scene, MixedRetainedAuthoringError, RetainedAuthoringDocument,
    RetainedTrackAuthoringSpec,
};

/// One protocol-v2 retained payload.
///
/// The established object document remains the canonical source-level text schema;
/// animation tracks are an additive wire field. Flattening the object document here
/// lets the complete payload deserialize exactly once while preserving backwards
/// compatibility with object-only v2 JSON.
#[derive(Debug, Deserialize)]
struct RetainedAuthoringWireDocument {
    #[serde(flatten)]
    document: RetainedAuthoringDocument,
    #[serde(default)]
    tracks: Vec<RetainedTrackAuthoringSpec>,
}

/// Wire-facing mixed retained scene.
///
/// Protocol v2 remains backwards-compatible with object-only retained documents while
/// allowing an additive `tracks` field. Object validation stays source-level; track
/// semantics are compiled only after legacy geometry and retained text share one object
/// domain.
#[derive(Clone, Debug)]
pub struct MixedRetainedAuthoringScene {
    inner: retained_authoring_scene::MixedRetainedAuthoringScene,
}

impl MixedRetainedAuthoringScene {
    pub fn from_json(
        legacy_scene_json: &str,
        retained_document_json: &str,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        let legacy = noon_ir::decode_scene(legacy_scene_json)?;
        let wire: RetainedAuthoringWireDocument = serde_json::from_str(retained_document_json)
            .map_err(|error| {
                MixedRetainedAuthoringError::RetainedDocument(format!(
                    "invalid retained authoring document: {error}"
                ))
            })?;
        wire.document
            .validate()
            .map_err(MixedRetainedAuthoringError::RetainedDocument)?;
        Self::from_parts_with_tracks(&legacy, wire.document, wire.tracks)
    }

    pub fn from_parts(
        legacy: &SceneDefinition,
        retained: RetainedAuthoringDocument,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        Ok(Self {
            inner: retained_authoring_scene::MixedRetainedAuthoringScene::from_parts(
                legacy, retained,
            )?,
        })
    }

    pub fn from_parts_with_tracks(
        legacy: &SceneDefinition,
        retained: RetainedAuthoringDocument,
        retained_tracks: Vec<RetainedTrackAuthoringSpec>,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        Ok(Self {
            inner: retained_authoring_scene::MixedRetainedAuthoringScene::from_parts_with_tracks(
                legacy,
                retained,
                retained_tracks,
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

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{Property, RateFunction, TrackTiming, TrackValues, Vec2};
    use noon_ir::encode_scene;
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
    fn protocol_v2_track_field_reaches_unified_retained_compiler() {
        let legacy = SceneDefinition::new();
        let object = ObjectId::new(1_u64 << 52);
        let document = retained_document(object);
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
    }

    #[test]
    fn invalid_object_document_is_rejected_before_track_materialization() {
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
