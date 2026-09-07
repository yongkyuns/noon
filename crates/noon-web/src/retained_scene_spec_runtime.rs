use noon::{MathTypst, RetainedScene, Text as NativeText, Typst};
use noon_compile::{CompileError, CompiledScene};
use noon_core::{
    Color, ObjectDefinition, ObjectId, SceneDefinition, Style, TrackDefinition, Transform2D,
};
use noon_ir::{ObjectSpec, ObjectSpecContent, SceneSpec, TextSpec, TextSpecKind, TextSpecOptions};

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
    MixedRetainedAuthoringError::InvalidInput(format!("invalid mixed scene input: {error}"))
}

#[derive(Debug)]
pub enum MixedRetainedAuthoringError {
    Text(noon::TextAuthoringError),
    Compile(CompileError),
    InvalidInput(String),
}

impl std::fmt::Display for MixedRetainedAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(error) => error.fmt(formatter),
            Self::Compile(error) => error.fmt(formatter),
            Self::InvalidInput(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for MixedRetainedAuthoringError {}

impl From<noon::TextAuthoringError> for MixedRetainedAuthoringError {
    fn from(value: noon::TextAuthoringError) -> Self {
        Self::Text(value)
    }
}

impl From<CompileError> for MixedRetainedAuthoringError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use noon_core::{Color, ObjectContentRef, Vec2};

    use super::*;

    #[test]
    fn scene_spec_materializes_one_painter_order_and_text_state() {
        let camera = ObjectId::new(1);
        let text_id = ObjectId::new(1_u64 << 52);
        let circle = ObjectId::new(2);
        let mut authored = noon::Scene::new();
        let camera_handle = authored.camera_frame().unwrap();
        let text_handle = authored
            .text(
                noon::Text::new("Canonical Noon")
                    .with_font_size(48.0)
                    .color(Color::rgba(0.2, 0.5, 0.8, 0.9))
                    .set_opacity(0.65)
                    .move_to(Vec2::new(1.25, -0.5))
                    .scale_xy(Vec2::new(1.5, 0.75))
                    .rotate(0.2),
            )
            .unwrap();
        let circle_handle = authored.circle(0.5).unwrap();
        let mut context = crate::CanonicalAuthoringScene::with_store(Rc::clone(authored.store()));
        context.bind_mobject(camera, &camera_handle).unwrap();
        context.bind_mobject(text_id, &text_handle).unwrap();
        context.bind_mobject(circle, &circle_handle).unwrap();
        let spec = context
            .finalize(Vec::new(), Vec::new(), Some(camera))
            .unwrap();

        let canonical = CanonicalRetainedAuthoringScene::from_scene_spec(spec).unwrap();

        assert_eq!(
            canonical
                .scene()
                .objects()
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>(),
            vec![camera, text_id, circle]
        );
        let canonical_handle = canonical.scene().objects()[1].content.text().unwrap();
        assert!(canonical.scene().texts().get(canonical_handle).is_some());
        assert_eq!(canonical.camera_object(), Some(camera));
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
