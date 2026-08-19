//! Versioned language-neutral interchange for Noon authoring and live control.
//!
//! JSON is the initial transport because it is easy to inspect from Python and
//! browser tooling. The versioned envelope keeps the runtime protocol independent
//! of that encoding so a compact binary representation can be added later.

#![forbid(unsafe_code)]

use noon_core::{ObjectDefinition, PatchError, SceneDefinition, ScenePatch, TrackDefinition};
use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneDocument {
    pub version: u32,
    pub objects: Vec<ObjectDefinition>,
    pub tracks: Vec<TrackDefinition>,
}

impl SceneDocument {
    pub fn from_scene(scene: &SceneDefinition) -> Self {
        Self {
            version: FORMAT_VERSION,
            objects: scene.objects().to_vec(),
            tracks: scene.tracks().to_vec(),
        }
    }

    pub fn into_scene(self) -> Result<SceneDefinition, IrError> {
        ensure_version(self.version)?;
        let mut scene = SceneDefinition::new();
        for object in self.objects {
            scene
                .apply_patch(ScenePatch::CreateObject(object))
                .map_err(IrError::Patch)?;
        }
        for track in self.tracks {
            scene
                .apply_patch(ScenePatch::AddTrack(track))
                .map_err(IrError::Patch)?;
        }
        Ok(scene)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PatchBatch {
    pub version: u32,
    pub sequence: u64,
    pub patches: Vec<ScenePatch>,
}

impl PatchBatch {
    pub fn new(sequence: u64, patches: Vec<ScenePatch>) -> Self {
        Self {
            version: FORMAT_VERSION,
            sequence,
            patches,
        }
    }

    pub fn validate(&self) -> Result<(), IrError> {
        ensure_version(self.version)
    }
}

#[derive(Debug)]
pub enum IrError {
    UnsupportedVersion(u32),
    Json(serde_json::Error),
    Patch(PatchError),
}

impl std::fmt::Display for IrError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported Noon IR version {version}")
            }
            Self::Json(error) => write!(formatter, "invalid Noon JSON: {error}"),
            Self::Patch(error) => write!(formatter, "invalid Noon scene document: {error}"),
        }
    }
}

impl std::error::Error for IrError {}

impl From<serde_json::Error> for IrError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

pub fn encode_scene(scene: &SceneDefinition) -> Result<String, IrError> {
    Ok(serde_json::to_string(&SceneDocument::from_scene(scene))?)
}

pub fn decode_scene(json: &str) -> Result<SceneDefinition, IrError> {
    let document: SceneDocument = serde_json::from_str(json)?;
    document.into_scene()
}

pub fn encode_patch_batch(batch: &PatchBatch) -> Result<String, IrError> {
    batch.validate()?;
    Ok(serde_json::to_string(batch)?)
}

pub fn decode_patch_batch(json: &str) -> Result<PatchBatch, IrError> {
    let batch: PatchBatch = serde_json::from_str(json)?;
    batch.validate()?;
    Ok(batch)
}

fn ensure_version(version: u32) -> Result<(), IrError> {
    if version == FORMAT_VERSION {
        Ok(())
    } else {
        Err(IrError::UnsupportedVersion(version))
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        Easing, GeometryRef, ObjectId, Property, Style, TrackTiming, Transform2D, Vec2,
    };

    use super::*;

    fn sample_scene() -> SceneDefinition {
        let mut scene = SceneDefinition::new();
        let circle = scene.add(GeometryRef::circle(1.5));
        scene.object_mut(circle).expect("object exists").transform = Transform2D {
            translation: Vec2::new(2.0, -1.0),
            ..Transform2D::IDENTITY
        };
        scene
            .animate_position(
                circle,
                Vec2::new(2.0, -1.0),
                Vec2::new(5.0, 3.0),
                TrackTiming::new(0.5, 2.0, Easing::EaseInOutCubic),
            )
            .expect("valid track");
        scene
    }

    #[test]
    fn scene_json_round_trip_preserves_semantics_and_stable_ids() {
        let scene = sample_scene();
        let json = encode_scene(&scene).expect("scene must serialize");
        let decoded = decode_scene(&json).expect("scene must deserialize");

        assert_eq!(decoded.objects(), scene.objects());
        assert_eq!(decoded.tracks(), scene.tracks());
        assert_eq!(decoded.add(GeometryRef::circle(1.0)), ObjectId::new(1));
    }

    #[test]
    fn identical_scene_produces_identical_json() {
        let first = encode_scene(&sample_scene()).expect("scene must serialize");
        let second = encode_scene(&sample_scene()).expect("scene must serialize");
        assert_eq!(first, second);
    }

    #[test]
    fn patch_batch_round_trip_preserves_order_and_sequence() {
        let batch = PatchBatch::new(
            42,
            vec![
                ScenePatch::SetTransform {
                    object: ObjectId::new(7),
                    transform: Transform2D {
                        translation: Vec2::new(3.0, 4.0),
                        ..Transform2D::IDENTITY
                    },
                },
                ScenePatch::SetStyle {
                    object: ObjectId::new(8),
                    style: Style {
                        opacity: 0.25,
                        ..Style::default()
                    },
                },
            ],
        );

        let json = encode_patch_batch(&batch).expect("batch must serialize");
        let decoded = decode_patch_batch(&json).expect("batch must deserialize");
        assert_eq!(decoded, batch);
    }

    #[test]
    fn unsupported_versions_are_rejected_before_application() {
        let json = r#"{"version":999,"sequence":1,"patches":[]}"#;
        assert!(matches!(
            decode_patch_batch(json),
            Err(IrError::UnsupportedVersion(999))
        ));
    }

    #[test]
    fn malformed_scene_references_are_rejected_deterministically() {
        let mut document = SceneDocument::from_scene(&sample_scene());
        document.tracks[0].object = ObjectId::new(999);
        let json = serde_json::to_string(&document).expect("document must serialize");

        assert!(matches!(decode_scene(&json), Err(IrError::Patch(_))));
    }

    #[test]
    fn json_uses_human_readable_operation_names() {
        let batch = PatchBatch::new(
            1,
            vec![ScenePatch::SetStyle {
                object: ObjectId::new(2),
                style: Style::default(),
            }],
        );
        let json = encode_patch_batch(&batch).expect("batch must serialize");

        assert!(json.contains("set_style"));
        assert!(json.contains("sequence"));
        assert!(!json.contains("Property"));
        let _ = Property::Opacity;
    }
}
