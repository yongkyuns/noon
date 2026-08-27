use std::collections::HashSet;

use noon::{MathTypst, RetainedScene, TextAuthoringError, Typst};
use noon_core::{ObjectId, SceneDefinition};

use crate::{RetainedAuthoringDocument, RetainedAuthoringTextObject, RetainedTypstAuthoringSpec};

/// One resource-aware scene assembled from the legacy geometry document and the
/// retained text sidecar emitted by Python authoring.
///
/// The merge happens before runtime compilation. Geometry and text therefore share
/// one semantic object list, one painter order, and one retained runtime; the browser
/// never needs a second scene model or overlay renderer.
#[derive(Clone, Debug)]
pub struct MixedRetainedAuthoringScene {
    scene: RetainedScene,
    camera_object: Option<ObjectId>,
}

impl MixedRetainedAuthoringScene {
    pub fn from_json(
        legacy_scene_json: &str,
        retained_document_json: &str,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        let legacy = noon_ir::decode_scene(legacy_scene_json)?;
        let retained = RetainedAuthoringDocument::from_json(retained_document_json)
            .map_err(MixedRetainedAuthoringError::RetainedDocument)?;
        Self::from_parts(&legacy, retained)
    }

    pub fn from_parts(
        legacy: &SceneDefinition,
        retained: RetainedAuthoringDocument,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        retained
            .validate()
            .map_err(MixedRetainedAuthoringError::RetainedDocument)?;
        preflight_merge(legacy, &retained)?;

        let camera_object = legacy.camera_object();
        let mut scene = RetainedScene::from_legacy(legacy)?;
        let mut text_objects = retained.objects;
        text_objects.sort_by_key(|object| object.order);
        for object in text_objects {
            insert_text_object(&mut scene, object)?;
        }

        Ok(Self {
            scene,
            camera_object,
        })
    }

    pub const fn scene(&self) -> &RetainedScene {
        &self.scene
    }

    pub fn into_scene(self) -> RetainedScene {
        self.scene
    }

    pub const fn camera_object(&self) -> Option<ObjectId> {
        self.camera_object
    }
}

#[derive(Debug)]
pub enum MixedRetainedAuthoringError {
    LegacyScene(noon_ir::IrError),
    RetainedDocument(String),
    Text(TextAuthoringError),
    ObjectCountOverflow,
    PainterOrderOutOfRange { order: u32, object_count: usize },
    ObjectIdentityCollision(ObjectId),
}

impl std::fmt::Display for MixedRetainedAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LegacyScene(error) => error.fmt(formatter),
            Self::RetainedDocument(error) => formatter.write_str(error),
            Self::Text(error) => error.fmt(formatter),
            Self::ObjectCountOverflow => {
                formatter.write_str("mixed retained authoring object count overflow")
            }
            Self::PainterOrderOutOfRange {
                order,
                object_count,
            } => write!(
                formatter,
                "retained painter order {order} is outside mixed object count {object_count}"
            ),
            Self::ObjectIdentityCollision(object) => write!(
                formatter,
                "retained text object {} collides with a legacy scene object",
                object.get()
            ),
        }
    }
}

impl std::error::Error for MixedRetainedAuthoringError {}

impl From<noon_ir::IrError> for MixedRetainedAuthoringError {
    fn from(value: noon_ir::IrError) -> Self {
        Self::LegacyScene(value)
    }
}

impl From<TextAuthoringError> for MixedRetainedAuthoringError {
    fn from(value: TextAuthoringError) -> Self {
        Self::Text(value)
    }
}

fn preflight_merge(
    legacy: &SceneDefinition,
    retained: &RetainedAuthoringDocument,
) -> Result<(), MixedRetainedAuthoringError> {
    let object_count = legacy
        .objects()
        .len()
        .checked_add(retained.objects.len())
        .ok_or(MixedRetainedAuthoringError::ObjectCountOverflow)?;
    for object in &retained.objects {
        if object.order as usize >= object_count {
            return Err(MixedRetainedAuthoringError::PainterOrderOutOfRange {
                order: object.order,
                object_count,
            });
        }
    }

    let legacy_ids = legacy
        .objects()
        .iter()
        .map(|object| object.id)
        .collect::<HashSet<_>>();
    if let Some(object) = retained
        .objects
        .iter()
        .find(|object| legacy_ids.contains(&object.object))
    {
        return Err(MixedRetainedAuthoringError::ObjectIdentityCollision(
            object.object,
        ));
    }
    Ok(())
}

fn insert_text_object(
    scene: &mut RetainedScene,
    object: RetainedAuthoringTextObject,
) -> Result<(), MixedRetainedAuthoringError> {
    let order = object.order as usize;
    let id = object.object;
    let spec = object.text;
    if spec.math {
        scene.insert_math_typst_at(order, id, math_typst_from_spec(spec))?;
    } else {
        scene.insert_typst_at(order, id, typst_from_spec(spec))?;
    }
    Ok(())
}

fn typst_from_spec(spec: RetainedTypstAuthoringSpec) -> Typst {
    Typst::new(spec.source)
        .with_font_size(spec.font_size)
        .color(spec.color)
        .set_opacity(spec.opacity)
        .move_to(spec.transform.translation)
        .scale_xy(spec.transform.scale)
        .rotate(spec.transform.rotation)
}

fn math_typst_from_spec(spec: RetainedTypstAuthoringSpec) -> MathTypst {
    MathTypst::new(spec.source)
        .with_font_size(spec.font_size)
        .color(spec.color)
        .set_opacity(spec.opacity)
        .move_to(spec.transform.translation)
        .scale_xy(spec.transform.scale)
        .rotate(spec.transform.rotation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{Color, GeometryRef, ObjectContentRef, TextSourceKind, Transform2D, Vec2};
    use noon_ir::encode_scene;

    fn retained_document(objects: Vec<RetainedAuthoringTextObject>) -> RetainedAuthoringDocument {
        RetainedAuthoringDocument::new(objects).unwrap()
    }

    fn typst_spec(source: &str, math: bool, font_size: f32) -> RetainedTypstAuthoringSpec {
        RetainedTypstAuthoringSpec::new(source, math, font_size).unwrap()
    }

    #[test]
    fn text_only_authoring_decodes_into_one_retained_text_object() {
        let legacy = SceneDefinition::new();
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = retained_document(vec![RetainedAuthoringTextObject {
            object: text_id,
            order: 0,
            text: typst_spec("*Hello* from _Typst!_", false, 96.0),
        }]);

        let mixed = MixedRetainedAuthoringScene::from_json(
            &encode_scene(&legacy).unwrap(),
            &retained.to_json().unwrap(),
        )
        .unwrap();
        assert_eq!(mixed.scene().objects().len(), 1);
        assert_eq!(mixed.scene().objects()[0].id, text_id);
        let handle = mixed.scene().objects()[0].content.text().unwrap();
        assert_eq!(
            mixed.scene().texts().get(handle).unwrap().kind,
            TextSourceKind::Typst
        );
    }

    #[test]
    fn mixed_document_reconstructs_geometry_text_geometry_painter_order() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        let square = legacy.add(GeometryRef::rectangle(0.5, 0.5));
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = retained_document(vec![RetainedAuthoringTextObject {
            object: text_id,
            order: 1,
            text: typst_spec("middle", false, 48.0),
        }]);

        let mixed = MixedRetainedAuthoringScene::from_parts(&legacy, retained).unwrap();
        assert_eq!(
            mixed
                .scene()
                .objects()
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>(),
            vec![circle, text_id, square]
        );
        assert!(matches!(
            mixed.scene().objects()[0].content,
            ObjectContentRef::Geometry(_)
        ));
        assert!(matches!(
            mixed.scene().objects()[1].content,
            ObjectContentRef::Text(_)
        ));
        assert!(matches!(
            mixed.scene().objects()[2].content,
            ObjectContentRef::Geometry(_)
        ));

        let compiled = mixed.scene().compile().unwrap();
        assert_eq!(compiled.object_index(circle), Some(0));
        assert_eq!(compiled.object_index(text_id), Some(1));
        assert_eq!(compiled.object_index(square), Some(2));
    }

    #[test]
    fn retained_transform_style_and_math_mode_survive_merge() {
        let legacy = SceneDefinition::new();
        let text_id = ObjectId::new(1_u64 << 52);
        let mut spec = typst_spec("sum_(k=1)^n k", true, 72.0);
        spec.transform = Transform2D {
            translation: Vec2::new(1.5, -0.75),
            rotation: 0.25,
            scale: Vec2::new(2.0, 0.5),
        };
        spec.color = Color::rgba(0.2, 0.4, 0.8, 0.6);
        spec.opacity = 0.7;
        let retained = retained_document(vec![RetainedAuthoringTextObject {
            object: text_id,
            order: 0,
            text: spec,
        }]);

        let mixed = MixedRetainedAuthoringScene::from_parts(&legacy, retained).unwrap();
        let object = &mixed.scene().objects()[0];
        assert_eq!(object.transform.translation, Vec2::new(1.5, -0.75));
        assert!((object.transform.rotation - 0.25).abs() < 1.0e-6);
        assert!((object.transform.scale.x - 0.15).abs() < 1.0e-6);
        assert!((object.transform.scale.y - 0.0375).abs() < 1.0e-6);
        assert_eq!(object.style.fill, Some(Color::rgba(0.2, 0.4, 0.8, 0.6)));
        assert!((object.style.opacity - 0.7).abs() < 1.0e-6);
        let handle = object.content.text().unwrap();
        assert_eq!(
            mixed.scene().texts().get(handle).unwrap().kind,
            TextSourceKind::MathTypst
        );
    }

    #[test]
    fn camera_identity_is_preserved_from_legacy_scene() {
        let mut legacy = SceneDefinition::new();
        let camera = legacy.add(GeometryRef::rectangle(14.0, 8.0));
        assert!(legacy.set_camera_object(camera));
        let mixed = MixedRetainedAuthoringScene::from_parts(&legacy, retained_document(Vec::new()))
            .unwrap();
        assert_eq!(mixed.camera_object(), Some(camera));
    }

    #[test]
    fn invalid_painter_order_is_rejected_before_any_text_is_compiled() {
        let mut legacy = SceneDefinition::new();
        legacy.add(GeometryRef::circle(0.25));
        let retained = retained_document(vec![RetainedAuthoringTextObject {
            object: ObjectId::new(1_u64 << 52),
            order: 2,
            text: typst_spec("outside", false, 48.0),
        }]);

        let error = MixedRetainedAuthoringScene::from_parts(&legacy, retained).unwrap_err();
        assert!(matches!(
            error,
            MixedRetainedAuthoringError::PainterOrderOutOfRange {
                order: 2,
                object_count: 2
            }
        ));
    }

    #[test]
    fn legacy_text_id_collision_is_rejected_during_preflight() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        let retained = retained_document(vec![RetainedAuthoringTextObject {
            object: circle,
            order: 1,
            text: typst_spec("collision", false, 48.0),
        }]);

        let error = MixedRetainedAuthoringScene::from_parts(&legacy, retained).unwrap_err();
        assert!(matches!(
            error,
            MixedRetainedAuthoringError::ObjectIdentityCollision(object) if object == circle
        ));
    }
}
