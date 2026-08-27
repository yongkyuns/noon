//! Retained Typst / MathTypst authoring over Noon's text resource model.
//!
//! This module is deliberately separate from the legacy geometry-only `Scene`.
//! Typst-backed objects enter the retained compiler as `ObjectContentRef::Text`
//! and keep their shaped glyph/vector resources in explicit arenas; no placeholder
//! geometry or SVG payload is introduced at the public authoring boundary.

use std::sync::Arc;

use noon_compile::{RetainedCompileError, RetainedCompiledScene};
use noon_core::{
    Color, FontResourceArena, FontResourceError, GeometryResource, GeometryResourceArena, ObjectId,
    RetainedObjectDefinition, Style, TextResource, TextResourceArena, TextResourceValidationError,
    TextSourceKind, TrackDefinition, Transform2D, Vec2, WHITE,
};
use noon_typst::{compile_typst_resource, TypstBackendError, TypstMode, TypstResourceArtifact};

/// Manim's public text APIs scale imported layout geometry by font points rather
/// than changing the layout backend's source. Keeping this as an object transform
/// also keeps glyph/cluster identity stable when font size changes geometrically.
pub const SCALE_FACTOR_PER_FONT_POINT: f32 = 1.0 / 960.0;
pub const DEFAULT_TYPST_FONT_SIZE: f32 = 48.0;

/// Stable handle to one semantic object in a [`RetainedScene`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RetainedMobject {
    id: ObjectId,
}

impl RetainedMobject {
    pub const fn id(self) -> ObjectId {
        self.id
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TypstSpec {
    source: Arc<str>,
    font_size: f32,
    color: Color,
    opacity: f32,
    transform: Transform2D,
}

impl TypstSpec {
    fn new(source: impl Into<Arc<str>>) -> Self {
        Self {
            source: source.into(),
            font_size: DEFAULT_TYPST_FONT_SIZE,
            color: WHITE,
            opacity: 1.0,
            transform: Transform2D::default(),
        }
    }

    fn style(&self) -> Style {
        Style {
            fill: Some(self.color),
            stroke: None,
            stroke_width: 0.0,
            opacity: self.opacity,
            ..Style::default()
        }
    }

    fn authored_transform(&self) -> Transform2D {
        let mut transform = self.transform;
        let font_scale = self.font_size * SCALE_FACTOR_PER_FONT_POINT;
        transform.scale = transform
            .scale
            .component_mul(Vec2::new(font_scale, font_scale));
        transform
    }
}

macro_rules! typst_object {
    ($name:ident, $mode:expr, $kind:expr) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name(TypstSpec);

        impl $name {
            pub fn new(source: impl Into<Arc<str>>) -> Self {
                Self(TypstSpec::new(source))
            }

            pub fn source(&self) -> &str {
                self.0.source.as_ref()
            }

            pub const fn font_size(&self) -> f32 {
                self.0.font_size
            }

            pub fn with_font_size(mut self, font_size: f32) -> Self {
                self.0.font_size = font_size;
                self
            }

            pub fn color(mut self, color: Color) -> Self {
                self.0.color = color;
                self
            }

            pub fn set_opacity(mut self, opacity: f32) -> Self {
                self.0.opacity = opacity;
                self
            }

            pub fn shift(mut self, offset: Vec2) -> Self {
                self.0.transform.translation += offset;
                self
            }

            pub fn move_to(mut self, point: Vec2) -> Self {
                self.0.transform.translation = point;
                self
            }

            pub fn scale(mut self, factor: f32) -> Self {
                self.0.transform.scale = Vec2::new(
                    self.0.transform.scale.x * factor,
                    self.0.transform.scale.y * factor,
                );
                self
            }

            pub fn scale_xy(mut self, factor: Vec2) -> Self {
                self.0.transform.scale = self.0.transform.scale.component_mul(factor);
                self
            }

            pub fn rotate(mut self, angle: f32) -> Self {
                self.0.transform.rotation += angle;
                self
            }

            fn compile(
                self,
                scene: &mut RetainedScene,
            ) -> Result<RetainedObjectDefinition, TextAuthoringError> {
                if !self.0.font_size.is_finite() || self.0.font_size <= 0.0 {
                    return Err(TextAuthoringError::InvalidFontSize(self.0.font_size));
                }
                if !self.0.opacity.is_finite() || !(0.0..=1.0).contains(&self.0.opacity) {
                    return Err(TextAuthoringError::InvalidOpacity(self.0.opacity));
                }
                let artifact = compile_typst_resource(self.0.source.as_ref(), $mode)?;
                debug_assert_eq!(artifact.resource.kind, $kind);
                let handle = scene.import_typst_artifact(artifact)?;
                let id = scene.allocate_object_id();
                let mut object = RetainedObjectDefinition::text(id, handle);
                object.transform = self.0.authored_transform();
                object.style = self.0.style();
                Ok(object)
            }
        }
    };
}

typst_object!(Typst, TypstMode::Markup, TextSourceKind::Typst);
typst_object!(MathTypst, TypstMode::Math, TextSourceKind::MathTypst);

#[derive(Clone, Debug, PartialEq)]
pub enum TextAuthoringError {
    InvalidFontSize(f32),
    InvalidOpacity(f32),
    MissingGeometryResource,
    MissingFontResource,
    Typst(TypstBackendError),
    Font(FontResourceError),
    Text(TextResourceValidationError),
    Compile(RetainedCompileError),
}

impl std::fmt::Display for TextAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFontSize(value) => write!(formatter, "invalid Typst font size {value}"),
            Self::InvalidOpacity(value) => write!(formatter, "invalid Typst opacity {value}"),
            Self::MissingGeometryResource => {
                formatter.write_str("Typst artifact references missing vector geometry")
            }
            Self::MissingFontResource => {
                formatter.write_str("Typst artifact references missing font data")
            }
            Self::Typst(error) => error.fmt(formatter),
            Self::Font(error) => error.fmt(formatter),
            Self::Text(error) => error.fmt(formatter),
            Self::Compile(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TextAuthoringError {}

impl From<TypstBackendError> for TextAuthoringError {
    fn from(value: TypstBackendError) -> Self {
        Self::Typst(value)
    }
}

impl From<FontResourceError> for TextAuthoringError {
    fn from(value: FontResourceError) -> Self {
        Self::Font(value)
    }
}

impl From<TextResourceValidationError> for TextAuthoringError {
    fn from(value: TextResourceValidationError) -> Self {
        Self::Text(value)
    }
}

impl From<RetainedCompileError> for TextAuthoringError {
    fn from(value: RetainedCompileError) -> Self {
        Self::Compile(value)
    }
}

/// Public retained authoring container for resource-backed text/math objects.
///
/// The legacy `Scene` remains available for the serialized geometry path. This
/// retained scene is the resource-aware path consumed by the retained compiler and
/// mixed GPU renderer, and is intentionally backend-neutral after object insertion.
#[derive(Clone, Debug, Default)]
pub struct RetainedScene {
    objects: Vec<RetainedObjectDefinition>,
    tracks: Vec<TrackDefinition>,
    texts: TextResourceArena,
    geometries: GeometryResourceArena,
    fonts: FontResourceArena,
    next_object_id: u64,
}

impl RetainedScene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_typst(&mut self, object: Typst) -> Result<RetainedMobject, TextAuthoringError> {
        let object = object.compile(self)?;
        Ok(self.push_object(object))
    }

    pub fn add_math_typst(
        &mut self,
        object: MathTypst,
    ) -> Result<RetainedMobject, TextAuthoringError> {
        let object = object.compile(self)?;
        Ok(self.push_object(object))
    }

    pub fn objects(&self) -> &[RetainedObjectDefinition] {
        &self.objects
    }

    pub fn tracks(&self) -> &[TrackDefinition] {
        &self.tracks
    }

    pub const fn texts(&self) -> &TextResourceArena {
        &self.texts
    }

    pub const fn geometries(&self) -> &GeometryResourceArena {
        &self.geometries
    }

    pub const fn fonts(&self) -> &FontResourceArena {
        &self.fonts
    }

    pub fn compile(&self) -> Result<RetainedCompiledScene, TextAuthoringError> {
        Ok(RetainedCompiledScene::compile(&self.objects, &self.tracks)?)
    }

    fn push_object(&mut self, object: RetainedObjectDefinition) -> RetainedMobject {
        let id = object.id;
        self.objects.push(object);
        RetainedMobject { id }
    }

    fn allocate_object_id(&mut self) -> ObjectId {
        let id = ObjectId::new(self.next_object_id);
        self.next_object_id = self
            .next_object_id
            .checked_add(1)
            .expect("Noon retained object ID space exhausted");
        id
    }

    fn import_typst_artifact(
        &mut self,
        artifact: TypstResourceArtifact,
    ) -> Result<noon_core::TextResourceHandle, TextAuthoringError> {
        for run in artifact.resource.runs.iter() {
            let resource = artifact
                .fonts
                .get_for_face(&run.font)
                .ok_or(TextAuthoringError::MissingFontResource)?;
            self.fonts.intern_face(&run.font, resource.data.clone())?;
        }

        let mut resource: TextResource = artifact.resource;
        let mut vectors = Vec::with_capacity(resource.vector_items.len());
        for item in resource.vector_items.iter() {
            let GeometryResource::VectorPath(path) = artifact
                .geometry
                .get(item.geometry)
                .ok_or(TextAuthoringError::MissingGeometryResource)?;
            let mut imported = item.clone();
            imported.geometry = self.geometries.insert_path(path.as_ref().clone());
            vectors.push(imported);
        }
        resource.vector_items = vectors.into();
        Ok(self.texts.insert(resource)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::ObjectContentRef;

    #[test]
    fn typst_authors_one_retained_text_object_without_geometry_placeholder() {
        let mut scene = RetainedScene::new();
        let object = scene
            .add_typst(Typst::new("*Hello* from _Typst!_").color(noon_core::YELLOW))
            .unwrap();

        assert_eq!(scene.objects().len(), 1);
        let ObjectContentRef::Text(handle) = &scene.objects()[0].content else {
            panic!("Typst must author retained text content");
        };
        assert_eq!(object.id(), scene.objects()[0].id);
        assert_eq!(
            scene.texts().get(*handle).unwrap().kind,
            TextSourceKind::Typst
        );
        assert!(!scene.fonts().is_empty());
        assert!(scene.objects()[0].style.fill.is_some());
    }

    #[test]
    fn math_typst_keeps_math_source_identity_and_shared_vector_resources() {
        let mut scene = RetainedScene::new();
        scene
            .add_math_typst(MathTypst::new("frac(x, 2)").with_font_size(72.0))
            .unwrap();

        let ObjectContentRef::Text(handle) = &scene.objects()[0].content else {
            panic!("MathTypst must author retained text content");
        };
        let resource = scene.texts().get(*handle).unwrap();
        assert_eq!(resource.kind, TextSourceKind::MathTypst);
        assert_ne!(resource.kind, TextSourceKind::MathTex);
        assert!(resource.vector_count() >= 1);
        for vector in resource.vector_items.iter() {
            assert!(matches!(
                scene.geometries().get(vector.geometry),
                Some(GeometryResource::VectorPath(_))
            ));
        }
        assert!((scene.objects()[0].transform.scale.x - 0.075).abs() < 1e-6);
        assert!((scene.objects()[0].transform.scale.y - 0.075).abs() < 1e-6);
    }

    #[test]
    fn retained_scene_compiles_text_handles_without_copying_resources() {
        let mut scene = RetainedScene::new();
        scene.add_typst(Typst::new("Noon")).unwrap();
        scene
            .add_math_typst(MathTypst::new("sum_(k=1)^n k"))
            .unwrap();

        let compiled = scene.compile().unwrap();
        assert_eq!(compiled.objects().len(), 2);
        assert!(compiled
            .objects()
            .iter()
            .all(|object| object.text().is_some()));
        assert_eq!(scene.texts().len(), 2);
    }

    #[test]
    fn invalid_font_size_is_rejected_before_resource_insertion() {
        let mut scene = RetainedScene::new();
        let error = scene
            .add_typst(Typst::new("bad").with_font_size(0.0))
            .unwrap_err();
        assert_eq!(error, TextAuthoringError::InvalidFontSize(0.0));
        assert!(scene.objects().is_empty());
        assert!(scene.texts().is_empty());
    }
}
