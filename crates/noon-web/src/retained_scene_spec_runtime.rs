use noon::{MathTypst, RetainedScene, Text as NativeText, Typst};
use noon_compile::CompiledScene;
use noon_core::{
    Color, ObjectDefinition, ObjectId, SceneDefinition, Style, TrackDefinition, Transform2D,
};
use noon_ir::{ObjectSpec, ObjectSpecContent, SceneSpec, TextSpec, TextSpecKind, TextSpecOptions};

use crate::retained_authoring_wire_scene::MixedRetainedAuthoringError;

/// Canonical `SceneSpec` lowered into the existing retained runtime/resource model.
///
/// This is the consumer-side convergence point for #367. Geometry and source-level
/// text arrive in one painter-ordered object vector, while compilation still reuses
/// the existing native/Typst text authoring, resource arenas, retained compiler, and
/// renderer. No frontend payload or renderer representation is introduced here.
#[derive(Clone, Debug)]
pub(crate) struct CanonicalRetainedAuthoringScene {
    scene: RetainedScene,
    tracks: Vec<TrackDefinition>,
    camera_object: Option<ObjectId>,
}

impl CanonicalRetainedAuthoringScene {
    pub(crate) fn from_scene_spec(spec: SceneSpec) -> Result<Self, MixedRetainedAuthoringError> {
        spec.validate().map_err(invalid_scene_spec)?;

        let SceneSpec {
            objects,
            tracks,
            camera_object,
            ..
        } = spec;

        // `RetainedScene` already owns the correct resource-backed text insertion
        // APIs. Seed it with only the geometry subset, preserving relative geometry
        // order, then insert text at its structural global painter slots.
        let geometry_objects = objects
            .iter()
            .filter_map(|object| {
                let ObjectSpecContent::Geometry(geometry) = &object.content else {
                    return None;
                };
                Some(ObjectDefinition {
                    id: object.id,
                    geometry: geometry.clone(),
                    transform: object.transform,
                    style: object.style,
                })
            })
            .collect::<Vec<_>>();
        let geometry_scene = SceneDefinition::from_parts(geometry_objects, Vec::new())
            .map_err(|error| invalid_scene_spec(error.to_string()))?;
        let mut scene = RetainedScene::from_legacy(&geometry_scene)?;

        for (order, object) in objects.into_iter().enumerate() {
            let ObjectSpec {
                id,
                content,
                transform,
                style,
            } = object;
            if let ObjectSpecContent::Text(text) = content {
                insert_text_object(&mut scene, order, id, text, transform, style)?;
            }
        }

        // Keep the mature retained compiler as the semantic/timeline validator.
        // This also proves every canonical object ID and normalized track reaches the
        // same dense runtime domain before the scene is committed.
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

    pub(crate) fn into_scene(self) -> RetainedScene {
        self.scene
    }

    pub(crate) const fn camera_object(&self) -> Option<ObjectId> {
        self.camera_object
    }
}

fn insert_text_object(
    scene: &mut RetainedScene,
    order: usize,
    id: ObjectId,
    text: TextSpec,
    transform: Transform2D,
    style: Style,
) -> Result<(), MixedRetainedAuthoringError> {
    let color = canonical_text_color(id, transform, style)?;
    let TextSpec {
        kind,
        source,
        font_size,
        options,
    } = text;

    match kind {
        TextSpecKind::Plain => {
            let (font_family, line_spacing) = match options {
                TextSpecOptions::Default => {
                    (noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY.to_owned(), -1.0)
                }
                TextSpecOptions::NativePlain {
                    font_family,
                    line_spacing,
                } => (font_family, line_spacing),
            };
            let text = NativeText::new(source)
                .with_font(font_family)
                .with_font_size(font_size)
                .with_line_spacing(line_spacing)
                .color(color)
                .set_opacity(style.opacity)
                .move_to(transform.translation)
                .scale_xy(transform.scale)
                .rotate(transform.rotation);
            scene.insert_native_text_at(order, id, text)?;
        }
        TextSpecKind::Typst | TextSpecKind::MathTypst => {
            debug_assert!(matches!(options, TextSpecOptions::Default));
            if kind == TextSpecKind::MathTypst {
                let text = MathTypst::new(source)
                    .with_font_size(font_size)
                    .color(color)
                    .set_opacity(style.opacity)
                    .move_to(transform.translation)
                    .scale_xy(transform.scale)
                    .rotate(transform.rotation);
                scene.insert_math_typst_at(order, id, text)?;
            } else {
                let text = Typst::new(source)
                    .with_font_size(font_size)
                    .color(color)
                    .set_opacity(style.opacity)
                    .move_to(transform.translation)
                    .scale_xy(transform.scale)
                    .rotate(transform.rotation);
                scene.insert_typst_at(order, id, text)?;
            }
        }
        TextSpecKind::Markup | TextSpecKind::Tex | TextSpecKind::MathTex => {
            return Err(invalid_scene_spec(format!(
                "text object {} uses unsupported source kind {kind:?}",
                id.get()
            )));
        }
    }

    Ok(())
}

fn canonical_text_color(
    id: ObjectId,
    transform: Transform2D,
    style: Style,
) -> Result<Color, MixedRetainedAuthoringError> {
    let Some(color) = style.fill else {
        return Err(invalid_scene_spec(format!(
            "text object {} has no fill color",
            id.get()
        )));
    };
    if style.stroke.is_some() {
        return Err(invalid_scene_spec(format!(
            "text object {} requests text stroke before canonical stroke lowering is available",
            id.get()
        )));
    }
    if !style.opacity.is_finite() || !(0.0..=1.0).contains(&style.opacity) {
        return Err(invalid_scene_spec(format!(
            "text object {} has invalid opacity {}",
            id.get(),
            style.opacity
        )));
    }

    let values = [
        transform.translation.x,
        transform.translation.y,
        transform.scale.x,
        transform.scale.y,
        transform.rotation,
        color.red,
        color.green,
        color.blue,
        color.alpha,
    ];
    if values.iter().any(|value| !value.is_finite()) {
        return Err(invalid_scene_spec(format!(
            "text object {} has non-finite transform/color state",
            id.get()
        )));
    }
    Ok(color)
}

fn invalid_scene_spec(error: impl std::fmt::Display) -> MixedRetainedAuthoringError {
    MixedRetainedAuthoringError::RetainedDocument(format!(
        "invalid canonical mixed SceneSpec: {error}"
    ))
}

#[cfg(test)]
mod tests {
    use noon_core::{
        Color, GeometryRef, ObjectContentRef, Property, RateFunction, TrackTiming, TrackValues,
        Transform2D, Vec2,
    };

    use super::*;
    use crate::{
        retained_authoring_scene, retained_authoring_scene_spec, RetainedAuthoringDocument,
        RetainedAuthoringTextObject, RetainedTextAuthoringSpec, RetainedTrackAuthoringSpec,
    };

    #[test]
    fn canonical_lowering_matches_split_runtime_and_compiled_output() {
        let mut legacy = SceneDefinition::new();
        let camera = legacy.add(GeometryRef::rectangle(14.0, 8.0));
        let circle = legacy.add(GeometryRef::circle(0.5));
        assert!(legacy.set_camera_object(camera));

        let text_id = ObjectId::new(1_u64 << 52);
        let mut text = RetainedTextAuthoringSpec::native(
            "Canonical Noon",
            noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY,
            48.0,
            -1.0,
        )
        .unwrap();
        text.transform = Transform2D {
            translation: Vec2::new(1.25, -0.5),
            rotation: 0.2,
            scale: Vec2::new(1.5, 0.75),
        };
        text.set_color(Color::rgba(0.2, 0.5, 0.8, 0.9)).unwrap();
        text.set_opacity(0.65).unwrap();

        let retained = RetainedAuthoringDocument::new(vec![RetainedAuthoringTextObject {
            object: text_id,
            order: 1,
            text,
        }])
        .unwrap();
        let track = RetainedTrackAuthoringSpec::new(
            text_id,
            Property::Scale,
            TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::ZERO,
            },
            TrackTiming::new(0.0, 1.0, RateFunction::Smooth),
        );

        let canonical_spec =
            retained_authoring_scene_spec(&legacy, retained.clone(), vec![track.clone()]).unwrap();
        let canonical = CanonicalRetainedAuthoringScene::from_scene_spec(canonical_spec).unwrap();
        let split = retained_authoring_scene::MixedRetainedAuthoringScene::from_parts_with_tracks(
            &legacy,
            retained,
            vec![track],
        )
        .unwrap();

        assert_eq!(
            canonical
                .scene()
                .objects()
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>(),
            vec![camera, text_id, circle]
        );
        assert_eq!(canonical.tracks(), split.tracks());
        assert_eq!(canonical.camera_object(), split.camera_object());
        // Independent lowering owns distinct arena namespaces; compare the
        // executable observations and resolved content, not process-local handles.
        let mut canonical_runtime = noon_runtime::SceneInstance::new(canonical.compile().unwrap());
        let mut split_runtime = noon_runtime::SceneInstance::new(split.compile().unwrap());
        for time in [0.0, 0.5, 1.0] {
            canonical_runtime.seek(time).unwrap();
            split_runtime.seek(time).unwrap();
            assert_eq!(
                crate::determinism::normalized_frame_value(canonical_runtime.frame()),
                crate::determinism::normalized_frame_value(split_runtime.frame()),
            );
        }

        let canonical_handle = canonical.scene().objects()[1].content.text().unwrap();
        let split_handle = split.scene().objects()[1].content.text().unwrap();
        assert_ne!(canonical_handle.arena, split_handle.arena);
        assert_eq!(
            canonical.scene().texts().get(canonical_handle),
            split.scene().texts().get(split_handle)
        );
        assert!(matches!(
            canonical.scene().objects()[0].content,
            ObjectContentRef::Geometry(_)
        ));
        assert!(matches!(
            canonical.scene().objects()[1].content,
            ObjectContentRef::Text(_)
        ));
    }

    #[test]
    fn direct_scene_spec_rejects_unimplemented_text_backends_without_fallback() {
        let object = ObjectId::new(7);
        let spec = SceneSpec::new(
            vec![ObjectSpec::text(
                object,
                TextSpec::new(TextSpecKind::Tex, "x^2", 48.0),
            )],
            Vec::new(),
        )
        .unwrap();

        let error = CanonicalRetainedAuthoringScene::from_scene_spec(spec).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("unsupported source kind Tex"));
        assert!(message.contains("7"));
    }
}
