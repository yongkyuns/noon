#[cfg(test)]
use std::collections::{BTreeMap, HashSet};

use noon::TextAuthoringError;
#[cfg(test)]
use noon::{MathTypst, RetainedScene, Text as NativeText, Typst};
use noon_compile::CompileError;
#[cfg(test)]
use noon_compile::CompiledScene;
use noon_core::ObjectId;
#[cfg(test)]
use noon_core::{SceneDefinition, TrackDefinition, Vec2};

use crate::RetainedTrackMaterializationError;
#[cfg(test)]
use crate::{
    materialize_retained_tracks, RetainedAuthoringDocument, RetainedAuthoringTextObject,
    RetainedTextAuthoringSpec, RetainedTextBackendSpec, RetainedTrackAuthoringSpec,
};

/// Previous split-input retained lowerer, kept only as a migration oracle for #367.
///
/// Production consumers now normalize through canonical `SceneSpec` before retained
/// lowering. Keeping this implementation test-only lets equivalence coverage compare
/// against the proven path without shipping two runtime architectures.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct MixedRetainedAuthoringScene {
    scene: RetainedScene,
    tracks: Vec<TrackDefinition>,
    camera_object: Option<ObjectId>,
}

#[cfg(test)]
impl MixedRetainedAuthoringScene {
    pub(crate) fn from_json(
        legacy_scene_json: &str,
        retained_document_json: &str,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        let legacy = noon_ir::decode_scene(legacy_scene_json)?;
        let retained = RetainedAuthoringDocument::from_json(retained_document_json)
            .map_err(MixedRetainedAuthoringError::RetainedDocument)?;
        Self::from_parts(&legacy, retained)
    }

    pub(crate) fn from_parts(
        legacy: &SceneDefinition,
        retained: RetainedAuthoringDocument,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        Self::from_parts_with_tracks(legacy, retained, Vec::new())
    }

    /// Merge source-level retained tracks with the legacy scene after semantic object
    /// identity and painter order have been unified.
    ///
    /// Frontends deliberately do not assign `TrackId`s. IDs are materialized after
    /// the legacy range, then the exact object/track set is compiled before this
    /// constructor commits a mixed scene. Invalid or text-incompatible tracks therefore
    /// fail transactionally without introducing a second animation validator.
    pub(crate) fn from_parts_with_tracks(
        legacy: &SceneDefinition,
        retained: RetainedAuthoringDocument,
        retained_tracks: Vec<RetainedTrackAuthoringSpec>,
    ) -> Result<Self, MixedRetainedAuthoringError> {
        retained
            .validate()
            .map_err(MixedRetainedAuthoringError::RetainedDocument)?;
        preflight_merge(legacy, &retained)?;

        let retained_scale_factors = retained_scale_factors(&retained.objects);
        let camera_object = legacy.camera_object();
        let mut scene = RetainedScene::from_legacy(legacy)?;
        let mut text_objects = retained.objects;
        text_objects.sort_by_key(|object| object.order);
        for object in text_objects {
            insert_text_object(&mut scene, object)?;
        }

        let tracks =
            materialize_retained_tracks(legacy.tracks(), retained_tracks, &retained_scale_factors)?;
        crate::retained_resource_transport::compile_retained_scene(&scene, &tracks)?;

        Ok(Self {
            scene,
            tracks,
            camera_object,
        })
    }

    pub(crate) const fn scene(&self) -> &RetainedScene {
        &self.scene
    }

    pub(crate) fn tracks(&self) -> &[TrackDefinition] {
        &self.tracks
    }

    pub(crate) fn compile(&self) -> Result<CompiledScene, MixedRetainedAuthoringError> {
        Ok(crate::retained_resource_transport::compile_retained_scene(
            &self.scene,
            &self.tracks,
        )?)
    }

    pub(crate) const fn camera_object(&self) -> Option<ObjectId> {
        self.camera_object
    }
}

#[derive(Debug)]
pub enum MixedRetainedAuthoringError {
    LegacyScene(noon_ir::IrError),
    RetainedDocument(String),
    Text(TextAuthoringError),
    TrackMaterialization(RetainedTrackMaterializationError),
    Compile(CompileError),
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
            Self::TrackMaterialization(error) => error.fmt(formatter),
            Self::Compile(error) => error.fmt(formatter),
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

impl From<RetainedTrackMaterializationError> for MixedRetainedAuthoringError {
    fn from(value: RetainedTrackMaterializationError) -> Self {
        Self::TrackMaterialization(value)
    }
}

impl From<CompileError> for MixedRetainedAuthoringError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}

#[cfg(test)]
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

#[cfg(test)]
fn retained_scale_factors(objects: &[RetainedAuthoringTextObject]) -> BTreeMap<ObjectId, Vec2> {
    objects
        .iter()
        .map(|object| {
            let factor = match &object.text.backend {
                RetainedTextBackendSpec::Native { .. } => noon::NATIVE_POINT_TO_SCENE_SCALE,
                RetainedTextBackendSpec::Typst { .. } => {
                    object.text.font_size * noon::SCALE_FACTOR_PER_FONT_POINT
                }
            };
            (object.object, Vec2::new(factor, factor))
        })
        .collect()
}

#[cfg(test)]
fn insert_text_object(
    scene: &mut RetainedScene,
    object: RetainedAuthoringTextObject,
) -> Result<(), MixedRetainedAuthoringError> {
    let order = object.order as usize;
    let id = object.object;
    let RetainedTextAuthoringSpec {
        source,
        backend,
        font_size,
        transform,
        color,
        opacity,
    } = object.text;

    match backend {
        RetainedTextBackendSpec::Native {
            font_family,
            line_spacing,
        } => {
            let text = NativeText::new(source)
                .with_font(font_family)
                .with_font_size(font_size)
                .with_line_spacing(line_spacing)
                .color(color)
                .set_opacity(opacity)
                .move_to(transform.translation)
                .scale_xy(transform.scale)
                .rotate(transform.rotation);
            scene.insert_native_text_at(order, id, text)?;
        }
        RetainedTextBackendSpec::Typst { math } => {
            if math {
                let text = MathTypst::new(source)
                    .with_font_size(font_size)
                    .color(color)
                    .set_opacity(opacity)
                    .move_to(transform.translation)
                    .scale_xy(transform.scale)
                    .rotate(transform.rotation);
                scene.insert_math_typst_at(order, id, text)?;
            } else {
                let text = Typst::new(source)
                    .with_font_size(font_size)
                    .color(color)
                    .set_opacity(opacity)
                    .move_to(transform.translation)
                    .scale_xy(transform.scale)
                    .rotate(transform.rotation);
                scene.insert_typst_at(order, id, text)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{
        Color, GeometryRef, ObjectContentRef, Property, RateFunction, TextSourceKind, TrackId,
        TrackTiming, TrackValues, Transform2D,
    };
    use noon_ir::encode_scene;

    fn retained_document(objects: Vec<RetainedAuthoringTextObject>) -> RetainedAuthoringDocument {
        RetainedAuthoringDocument::new(objects).unwrap()
    }

    fn typst_spec(source: &str, math: bool, font_size: f32) -> RetainedTextAuthoringSpec {
        RetainedTextAuthoringSpec::new(source, math, font_size).unwrap()
    }

    fn native_spec(source: &str, font_size: f32) -> RetainedTextAuthoringSpec {
        RetainedTextAuthoringSpec::native(
            source,
            noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY,
            font_size,
            -1.0,
        )
        .unwrap()
    }

    fn scale_track(object: ObjectId, from: Vec2, to: Vec2) -> RetainedTrackAuthoringSpec {
        RetainedTrackAuthoringSpec::new(
            object,
            Property::Scale,
            TrackValues::Vec2 { from, to },
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        )
    }

    #[test]
    fn native_text_only_authoring_decodes_into_one_retained_plain_text_object() {
        let legacy = SceneDefinition::new();
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = retained_document(vec![RetainedAuthoringTextObject {
            object: text_id,
            order: 0,
            text: native_spec("Native Noon", 48.0),
        }]);

        let mixed = MixedRetainedAuthoringScene::from_json(
            &encode_scene(&legacy).unwrap(),
            &retained.to_json().unwrap(),
        )
        .unwrap();
        assert_eq!(mixed.scene().objects().len(), 1);
        assert_eq!(mixed.scene().objects()[0].id, text_id);
        let handle = mixed.scene().objects()[0].content.text().unwrap();
        let resource = mixed.scene().texts().get(handle).unwrap();
        assert_eq!(resource.kind, TextSourceKind::Plain);
        assert_eq!(resource.source.as_ref(), "Native Noon");
        assert!(!mixed.scene().fonts().is_empty());
    }

    #[test]
    fn mixed_document_reconstructs_geometry_native_text_geometry_painter_order() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        let square = legacy.add(GeometryRef::rectangle(0.5, 0.5));
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = retained_document(vec![RetainedAuthoringTextObject {
            object: text_id,
            order: 1,
            text: native_spec("middle", 48.0),
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
        let handle = mixed.scene().objects()[1].content.text().unwrap();
        assert_eq!(
            mixed.scene().texts().get(handle).unwrap().kind,
            TextSourceKind::Plain
        );

        let compiled = mixed.compile().unwrap();
        assert_eq!(compiled.object_index(circle), Some(0));
        assert_eq!(compiled.object_index(text_id), Some(1));
        assert_eq!(compiled.object_index(square), Some(2));
    }

    #[test]
    fn retained_scale_track_compiles_against_native_text_without_geometry_snapshot() {
        let legacy = SceneDefinition::new();
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = retained_document(vec![RetainedAuthoringTextObject {
            object: text_id,
            order: 0,
            text: native_spec("Shrink", 48.0),
        }]);
        let mixed = MixedRetainedAuthoringScene::from_parts_with_tracks(
            &legacy,
            retained,
            vec![scale_track(text_id, Vec2::ONE, Vec2::ZERO)],
        )
        .unwrap();

        assert_eq!(mixed.tracks().len(), 1);
        assert_eq!(mixed.tracks()[0].id, TrackId::new(0));
        assert_eq!(mixed.tracks()[0].property, Property::Scale);
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
        let source_handle = mixed.scene().objects()[0].content.text().unwrap();
        let compiled = mixed.compile().unwrap();
        assert_eq!(compiled.objects()[0].text(), Some(source_handle));
        assert!(compiled.objects()[0].dynamic.scale);
        assert!(compiled.objects()[0].geometry().is_none());
    }

    #[test]
    fn typst_scale_track_uses_font_size_owned_intrinsic_scale() {
        let legacy = SceneDefinition::new();
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = retained_document(vec![RetainedAuthoringTextObject {
            object: text_id,
            order: 0,
            text: typst_spec("Noon", false, 72.0),
        }]);
        let mixed = MixedRetainedAuthoringScene::from_parts_with_tracks(
            &legacy,
            retained,
            vec![scale_track(text_id, Vec2::ONE, Vec2::ZERO)],
        )
        .unwrap();
        let expected = 72.0 * noon::SCALE_FACTOR_PER_FONT_POINT;
        assert_eq!(
            mixed.tracks()[0].values,
            TrackValues::Vec2 {
                from: Vec2::new(expected, expected),
                to: Vec2::ZERO,
            }
        );
    }

    #[test]
    fn retained_track_unknown_object_fails_transactionally_during_merge() {
        let legacy = SceneDefinition::new();
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = retained_document(vec![RetainedAuthoringTextObject {
            object: text_id,
            order: 0,
            text: native_spec("Known", 48.0),
        }]);
        let error = MixedRetainedAuthoringScene::from_parts_with_tracks(
            &legacy,
            retained,
            vec![scale_track(
                ObjectId::new(text_id.get() + 1),
                Vec2::ONE,
                Vec2::ZERO,
            )],
        )
        .unwrap_err();
        assert!(matches!(
            error,
            MixedRetainedAuthoringError::Compile(CompileError::UnknownObject(object))
                if object == ObjectId::new(text_id.get() + 1)
        ));
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
            text: native_spec("outside", 48.0),
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
            text: native_spec("collision", 48.0),
        }]);

        let error = MixedRetainedAuthoringScene::from_parts(&legacy, retained).unwrap_err();
        assert!(matches!(
            error,
            MixedRetainedAuthoringError::ObjectIdentityCollision(object) if object == circle
        ));
    }
}
