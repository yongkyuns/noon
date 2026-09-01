use noon::RetainedScene;
use noon_compile::RetainedCompiledScene;
use noon_core::{ObjectId, SceneDefinition, TextFamilyAnimationDefinition, TrackDefinition};
use serde::Deserialize;

use crate::{
    retained_authoring_scene, MixedRetainedAuthoringError, RetainedAuthoringDocument,
    RetainedTrackAuthoringSpec,
};

/// One protocol-v2 retained payload.
///
/// The established object document remains the canonical source-level text schema;
/// animation tracks and Text-family animations are additive wire fields. Flattening
/// the object document here lets the complete payload deserialize exactly once while
/// preserving backwards compatibility with object-only v2 JSON.
#[derive(Debug, Deserialize)]
struct RetainedAuthoringWireDocument {
    #[serde(flatten)]
    document: RetainedAuthoringDocument,
    #[serde(default)]
    tracks: Vec<RetainedTrackAuthoringSpec>,
    #[serde(default)]
    text_family_animations: Vec<TextFamilyAnimationDefinition>,
}

/// Wire-facing mixed retained scene.
///
/// Protocol v2 remains backwards-compatible with object-only retained documents while
/// allowing additive `tracks` and `text_family_animations` fields. Ordinary property
/// tracks compile into the unified retained runtime. Text-family definitions stay as a
/// separate semantic sidecar until `RetainedAuthoringPlayer` installs the shared family
/// scheduler, preserving raw overall progress until per-member lag evaluation.
#[derive(Clone, Debug)]
pub struct MixedRetainedAuthoringScene {
    inner: retained_authoring_scene::MixedRetainedAuthoringScene,
    text_family_animations: Vec<TextFamilyAnimationDefinition>,
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
        Self::from_parts_with_tracks_and_text_family_animations(
            &legacy,
            wire.document,
            wire.tracks,
            wire.text_family_animations,
        )
    }

    pub fn from_parts(
        legacy: &SceneDefinition,
        retained: RetainedAuthoringDocument,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        Self::from_parts_with_tracks_and_text_family_animations(
            legacy,
            retained,
            Vec::new(),
            Vec::new(),
        )
    }

    pub fn from_parts_with_tracks(
        legacy: &SceneDefinition,
        retained: RetainedAuthoringDocument,
        retained_tracks: Vec<RetainedTrackAuthoringSpec>,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        Self::from_parts_with_tracks_and_text_family_animations(
            legacy,
            retained,
            retained_tracks,
            Vec::new(),
        )
    }

    pub fn from_parts_with_tracks_and_text_family_animations(
        legacy: &SceneDefinition,
        retained: RetainedAuthoringDocument,
        retained_tracks: Vec<RetainedTrackAuthoringSpec>,
        text_family_animations: Vec<TextFamilyAnimationDefinition>,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        Ok(Self {
            inner: retained_authoring_scene::MixedRetainedAuthoringScene::from_parts_with_tracks(
                legacy,
                retained,
                retained_tracks,
            )?,
            text_family_animations,
        })
    }

    pub const fn scene(&self) -> &RetainedScene {
        self.inner.scene()
    }

    pub fn tracks(&self) -> &[TrackDefinition] {
        self.inner.tracks()
    }

    pub fn text_family_animations(&self) -> &[TextFamilyAnimationDefinition] {
        &self.text_family_animations
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
    use noon_core::{
        Property, RateFunction, TextFamilyAnimationMode, TrackTiming, TrackValues, Vec2,
    };
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
        assert!(mixed.text_family_animations().is_empty());
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
    fn protocol_v2_text_family_field_preserves_raw_family_timing() {
        let legacy = SceneDefinition::new();
        let object = ObjectId::new(1_u64 << 52);
        let document = retained_document(object);
        let animation = TextFamilyAnimationDefinition::new(
            object,
            TextFamilyAnimationMode::Reveal,
            0.25,
            2.0,
            1.0,
            RateFunction::Smooth,
            true,
            false,
        )
        .unwrap();
        let mut wire = serde_json::to_value(document).unwrap();
        wire["text_family_animations"] = json!([animation]);

        let mixed = MixedRetainedAuthoringScene::from_json(
            &encode_scene(&legacy).unwrap(),
            &serde_json::to_string(&wire).unwrap(),
        )
        .unwrap();
        assert_eq!(mixed.text_family_animations(), &[animation]);
        assert!(mixed.tracks().is_empty());
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
