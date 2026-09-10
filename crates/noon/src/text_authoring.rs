//! Retained native Text / Typst / MathTypst authoring over Noon's text resource model.
//!
//! The remaining authoring adapter is owned for deletion by #959. Its text
//! objects now enter the shared compiler/runtime as `ObjectContentRef::Text` and
//! keep shaped glyph/vector resources in explicit arenas; no placeholder geometry,
//! SVG payload, or frontend-owned glyph state is introduced at the authoring boundary.

mod semantic;
#[cfg(feature = "native-text")]
pub(crate) use semantic::native_text_state;
#[cfg(feature = "typst")]
pub(crate) use semantic::{math_typst_state, typst_state};

use std::sync::Arc;

use noon_compile::{CompileError, CompiledObject, CompiledScene};
#[cfg(feature = "typst")]
use noon_core::GeometryResource;
use noon_core::{
    Color, FontResourceArena, FontResourceError, GeometryResourceArena, ObjectId, Rect, Style,
    TextResource, TextResourceArena, TextResourceValidationError, TextSourceKind, Transform2D,
    Vec2, WHITE,
};
#[cfg(feature = "native-text")]
pub use noon_text::shaping::NativeFontFace;
#[cfg(feature = "native-text")]
use noon_text::shaping::{
    NativeTextCompiler, NativeTextError, NativeTextOptions, NativeTextResourceArtifact,
};
#[cfg(feature = "typst")]
pub use noon_typst::TypstBackendError;
#[cfg(feature = "typst")]
use noon_typst::{
    compile_typst_resource, compile_typst_resource_with_fonts, TypstMode, TypstResourceArtifact,
};
#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
use swash::{FontRef, StringId};

/// Typst's retained artifact is authored at 10pt, so its public Manim-style font size
/// remains an object transform and does not alter glyph/cluster identity.
#[cfg(feature = "typst")]
pub const SCALE_FACTOR_PER_FONT_POINT: f32 = 1.0 / 960.0;
/// Swash reports its requested size in device pixels while Manim's public native
/// Text font size is point-based. Convert 72 typographic points per scene-inch here;
/// the renderer must not compensate for this semantic unit conversion.
#[cfg(feature = "native-text")]
pub const NATIVE_POINT_TO_SCENE_SCALE: f32 = 1.0 / 72.0;
#[cfg(feature = "typst")]
pub const DEFAULT_TYPST_FONT_SIZE: f32 = 48.0;
#[cfg(feature = "native-text")]
pub const DEFAULT_NATIVE_TEXT_FONT_SIZE: f32 = 48.0;
#[cfg(feature = "native-text")]
pub const DEFAULT_NATIVE_TEXT_FONT_FAMILY: &str = "DejaVu Sans Mono";

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
struct TextPresentation {
    color: Color,
    opacity: f32,
    transform: Transform2D,
}

impl Default for TextPresentation {
    fn default() -> Self {
        Self {
            color: WHITE,
            opacity: 1.0,
            transform: Transform2D::default(),
        }
    }
}

impl TextPresentation {
    fn style(&self) -> Style {
        Style {
            fill: Some(self.color),
            stroke: None,
            stroke_width: 0.0,
            opacity: self.opacity,
            ..Style::default()
        }
    }

    fn validate(&self) -> Result<(), TextAuthoringError> {
        if !self.opacity.is_finite() || !(0.0..=1.0).contains(&self.opacity) {
            return Err(TextAuthoringError::InvalidOpacity(self.opacity));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg(feature = "typst")]
struct TypstSpec {
    source: Arc<str>,
    fonts: Option<Arc<[Arc<[u8]>]>>,
    font_size: f32,
    presentation: TextPresentation,
}

#[cfg(feature = "typst")]
impl TypstSpec {
    fn new(source: impl Into<Arc<str>>) -> Self {
        Self {
            source: source.into(),
            font_size: DEFAULT_TYPST_FONT_SIZE,
            fonts: None,
            presentation: TextPresentation::default(),
        }
    }

    fn compile_artifact(
        &self,
        mode: TypstMode,
    ) -> Result<TypstResourceArtifact, TextAuthoringError> {
        match &self.fonts {
            Some(fonts) => Ok(compile_typst_resource_with_fonts(
                self.source.as_ref(),
                mode,
                fonts.iter(),
            )?),
            None => Ok(compile_typst_resource(self.source.as_ref(), mode)?),
        }
    }

    fn authored_transform(&self) -> Transform2D {
        let mut transform = self.presentation.transform;
        let font_scale = self.font_size * SCALE_FACTOR_PER_FONT_POINT;
        transform.scale = transform
            .scale
            .component_mul(Vec2::new(font_scale, font_scale));
        transform
    }
}

#[cfg(feature = "typst")]
macro_rules! typst_object {
    ($name:ident, $mode:expr, $kind:expr) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name(TypstSpec);

        impl $name {
            pub fn new(source: impl Into<Arc<str>>) -> Self {
                Self(TypstSpec::new(source))
            }

            /// Use only these font buffers instead of bundled fonts. An empty or
            /// invalid set fails explicitly; it never falls back to bundled assets.
            pub fn with_fonts(mut self, fonts: impl IntoIterator<Item = Arc<[u8]>>) -> Self {
                self.0.fonts = Some(fonts.into_iter().collect::<Vec<_>>().into());
                self
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
                self.0.presentation.color = color;
                self
            }

            pub fn set_opacity(mut self, opacity: f32) -> Self {
                self.0.presentation.opacity = opacity;
                self
            }

            pub fn shift(mut self, offset: Vec2) -> Self {
                self.0.presentation.transform.translation += offset;
                self
            }

            pub fn move_to(mut self, point: Vec2) -> Self {
                self.0.presentation.transform.translation = point;
                self
            }

            pub fn scale(mut self, factor: f32) -> Self {
                self.0.presentation.transform.scale = Vec2::new(
                    self.0.presentation.transform.scale.x * factor,
                    self.0.presentation.transform.scale.y * factor,
                );
                self
            }

            pub fn scale_xy(mut self, factor: Vec2) -> Self {
                self.0.presentation.transform.scale =
                    self.0.presentation.transform.scale.component_mul(factor);
                self
            }

            pub fn rotate(mut self, angle: f32) -> Self {
                self.0.presentation.transform.rotation += angle;
                self
            }

            fn validate(&self) -> Result<(), TextAuthoringError> {
                if !self.0.font_size.is_finite() || self.0.font_size <= 0.0 {
                    return Err(TextAuthoringError::InvalidFontSize(self.0.font_size));
                }
                self.0.presentation.validate()
            }

            fn compile(
                self,
                scene: &mut RetainedScene,
            ) -> Result<CompiledObject, TextAuthoringError> {
                self.validate()?;
                let artifact = self.0.compile_artifact($mode)?;
                debug_assert_eq!(artifact.resource.kind, $kind);
                let bounds = artifact.resource.bounds;
                let handle = scene.import_typst_artifact(artifact)?;
                let id = scene.allocate_object_id()?;
                Ok(self.compiled_object(id, handle, bounds))
            }

            fn compiled_object(
                &self,
                id: ObjectId,
                handle: noon_core::TextResourceHandle,
                bounds: Rect,
            ) -> CompiledObject {
                let mut object = CompiledObject::new(
                    id,
                    handle,
                    self.0.authored_transform(),
                    self.0.presentation.style(),
                );
                object.text_bounds = Some(bounds);
                object
            }
        }
    };
}

#[cfg(feature = "typst")]
typst_object!(Typst, TypstMode::Markup, TextSourceKind::Typst);
#[cfg(feature = "typst")]
typst_object!(MathTypst, TypstMode::Math, TextSourceKind::MathTypst);

/// Native plain text authored through the same retained resource contract as Typst.
///
/// This first public slice intentionally exposes deterministic plain/multiline text.
/// Styled spans, fallback chains, bidi/script itemization, and MarkupText remain
/// backend follow-ups rather than being approximated in frontend wrappers.
#[derive(Clone, Debug, PartialEq)]
#[cfg(feature = "native-text")]
pub struct Text {
    source: Arc<str>,
    font_family: Arc<str>,
    font_face: Option<NativeFontFace>,
    font_size: f32,
    line_spacing: f32,
    presentation: TextPresentation,
}

#[cfg(feature = "native-text")]
impl Text {
    pub fn new(source: impl Into<Arc<str>>) -> Self {
        Self {
            source: source.into(),
            font_family: Arc::from(DEFAULT_NATIVE_TEXT_FONT_FAMILY),
            font_face: None,
            font_size: DEFAULT_NATIVE_TEXT_FONT_SIZE,
            line_spacing: -1.0,
            presentation: TextPresentation::default(),
        }
    }

    pub fn source(&self) -> &str {
        self.source.as_ref()
    }

    pub fn font_family(&self) -> &str {
        self.font_family.as_ref()
    }

    pub const fn font_size(&self) -> f32 {
        self.font_size
    }

    pub const fn line_spacing(&self) -> f32 {
        self.line_spacing
    }

    pub fn with_font(mut self, family: impl Into<Arc<str>>) -> Self {
        self.font_family = family.into();
        self.font_face = None;
        self
    }

    /// Supply an immutable font face without requiring bundled font assets.
    /// A subsequent `with_font` call deliberately returns to family lookup.
    pub fn with_font_face(mut self, font: NativeFontFace) -> Self {
        self.font_family = Arc::clone(&font.family);
        self.font_face = Some(font);
        self
    }

    pub fn with_font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn with_line_spacing(mut self, line_spacing: f32) -> Self {
        self.line_spacing = line_spacing;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.presentation.color = color;
        self
    }

    pub fn set_opacity(mut self, opacity: f32) -> Self {
        self.presentation.opacity = opacity;
        self
    }

    pub fn shift(mut self, offset: Vec2) -> Self {
        self.presentation.transform.translation += offset;
        self
    }

    pub fn move_to(mut self, point: Vec2) -> Self {
        self.presentation.transform.translation = point;
        self
    }

    pub fn scale(mut self, factor: f32) -> Self {
        self.presentation.transform.scale = Vec2::new(
            self.presentation.transform.scale.x * factor,
            self.presentation.transform.scale.y * factor,
        );
        self
    }

    pub fn scale_xy(mut self, factor: Vec2) -> Self {
        self.presentation.transform.scale = self.presentation.transform.scale.component_mul(factor);
        self
    }

    pub fn rotate(mut self, angle: f32) -> Self {
        self.presentation.transform.rotation += angle;
        self
    }

    fn validate(&self) -> Result<(), TextAuthoringError> {
        if !self.font_size.is_finite() || self.font_size <= 0.0 {
            return Err(TextAuthoringError::InvalidFontSize(self.font_size));
        }
        self.presentation.validate()
    }

    fn compile_artifact(&self) -> Result<NativeTextResourceArtifact, TextAuthoringError> {
        self.compile_artifact_with_fill(Some(self.presentation.color))
    }

    fn compile_artifact_with_fill(
        &self,
        fill: Option<Color>,
    ) -> Result<NativeTextResourceArtifact, TextAuthoringError> {
        self.validate()?;
        let font = match &self.font_face {
            Some(font) => font.clone(),
            None => bundled_native_font(self.font_family.as_ref())?,
        };
        let mut options = NativeTextOptions::new(self.font_size);
        options.line_spacing = self.line_spacing;
        options.fill = fill;
        let mut compiler = NativeTextCompiler::new();
        let artifact = compiler.compile_plain(self.source.as_ref(), &font, &options)?;
        debug_assert_eq!(artifact.resource.kind, TextSourceKind::Plain);
        Ok(artifact)
    }

    fn compile(self, scene: &mut RetainedScene) -> Result<CompiledObject, TextAuthoringError> {
        let artifact = self.compile_artifact()?;
        let bounds = artifact.resource.bounds;
        let handle = scene.import_native_text_artifact(artifact)?;
        let id = scene.allocate_object_id()?;
        Ok(self.compiled_object(id, handle, bounds))
    }

    fn compiled_object(
        &self,
        id: ObjectId,
        handle: noon_core::TextResourceHandle,
        bounds: Rect,
    ) -> CompiledObject {
        let mut transform = self.presentation.transform;
        transform.scale = transform.scale.component_mul(Vec2::new(
            NATIVE_POINT_TO_SCENE_SCALE,
            NATIVE_POINT_TO_SCENE_SCALE,
        ));
        let mut object = CompiledObject::new(id, handle, transform, self.presentation.style());
        object.text_bounds = Some(bounds);
        object
    }
}

#[cfg(feature = "native-text")]
fn bundled_native_font(family: &str) -> Result<NativeFontFace, TextAuthoringError> {
    #[cfg(feature = "bundled-fonts")]
    for data in typst_assets::fonts() {
        let Some(font) = FontRef::from_index(data, 0) else {
            continue;
        };
        let matches = font.localized_strings().any(|name| {
            matches!(
                name.id(),
                StringId::Family | StringId::TypographicFamily | StringId::WwsFamily
            ) && name.to_string().eq_ignore_ascii_case(family)
        });
        if matches {
            return NativeFontFace::new(Arc::<str>::from(family), Arc::<[u8]>::from(data), 0)
                .map_err(TextAuthoringError::NativeText);
        }
    }
    Err(TextAuthoringError::FontUnavailable(Arc::from(family)))
}

#[derive(Clone, Debug, PartialEq)]
pub enum TextAuthoringError {
    InvalidFontSize(f32),
    InvalidOpacity(f32),
    FontUnavailable(Arc<str>),
    MissingGeometryResource,
    MissingFontResource,
    DuplicateObject(ObjectId),
    ObjectIdSpaceExhausted,
    #[cfg(feature = "native-text")]
    NativeText(NativeTextError),
    #[cfg(feature = "typst")]
    Typst(TypstBackendError),
    Font(FontResourceError),
    Text(TextResourceValidationError),
    Compile(CompileError),
    Semantic(crate::AuthoringError),
    Import(noon_core::SemanticTextImportError),
}

impl std::fmt::Display for TextAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFontSize(value) => write!(formatter, "invalid text font size {value}"),
            Self::InvalidOpacity(value) => write!(formatter, "invalid text opacity {value}"),
            Self::FontUnavailable(family) => {
                write!(
                    formatter,
                    "bundled native font family {family:?} is unavailable"
                )
            }
            Self::MissingGeometryResource => {
                formatter.write_str("text artifact references missing vector geometry")
            }
            Self::MissingFontResource => {
                formatter.write_str("text artifact references missing font data")
            }
            Self::DuplicateObject(id) => {
                write!(formatter, "duplicate retained object id {}", id.get())
            }
            Self::ObjectIdSpaceExhausted => {
                formatter.write_str("retained object ID space is exhausted")
            }
            #[cfg(feature = "native-text")]
            Self::NativeText(error) => error.fmt(formatter),
            #[cfg(feature = "typst")]
            Self::Typst(error) => error.fmt(formatter),
            Self::Font(error) => error.fmt(formatter),
            Self::Text(error) => error.fmt(formatter),
            Self::Compile(error) => error.fmt(formatter),
            Self::Semantic(error) => error.fmt(formatter),
            Self::Import(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TextAuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            #[cfg(feature = "native-text")]
            Self::NativeText(error) => Some(error),
            #[cfg(feature = "typst")]
            Self::Typst(error) => Some(error),
            Self::Font(error) => Some(error),
            Self::Text(error) => Some(error),
            Self::Compile(error) => Some(error),
            Self::Semantic(error) => Some(error),
            Self::Import(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(feature = "native-text")]
impl From<NativeTextError> for TextAuthoringError {
    fn from(value: NativeTextError) -> Self {
        Self::NativeText(value)
    }
}

#[cfg(feature = "typst")]
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

impl From<CompileError> for TextAuthoringError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}

#[cfg(feature = "native-text")]
impl From<&str> for Text {
    fn from(source: &str) -> Self {
        Self::new(source)
    }
}
#[cfg(feature = "native-text")]
impl From<String> for Text {
    fn from(source: String) -> Self {
        Self::new(source)
    }
}

/// Public retained authoring container for resource-backed text/math objects.
#[derive(Clone, Debug, Default)]
pub struct RetainedScene {
    objects: Vec<CompiledObject>,
    texts: TextResourceArena,
    geometries: GeometryResourceArena,
    fonts: FontResourceArena,
    next_object_id: u64,
}

impl RetainedScene {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(feature = "native-text")]
    pub fn add_text(&mut self, object: Text) -> Result<RetainedMobject, TextAuthoringError> {
        let object = object.compile(self)?;
        Ok(self.push_object(object))
    }

    #[cfg(feature = "typst")]
    pub fn add_typst(&mut self, object: Typst) -> Result<RetainedMobject, TextAuthoringError> {
        let object = object.compile(self)?;
        Ok(self.push_object(object))
    }

    #[cfg(feature = "typst")]
    pub fn add_math_typst(
        &mut self,
        object: MathTypst,
    ) -> Result<RetainedMobject, TextAuthoringError> {
        let object = object.compile(self)?;
        Ok(self.push_object(object))
    }

    pub fn objects(&self) -> &[CompiledObject] {
        &self.objects
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

    pub fn compile(&self) -> Result<CompiledScene, TextAuthoringError> {
        Ok(CompiledScene::compile_objects(self.objects.clone(), &[])?)
    }

    fn push_object(&mut self, object: CompiledObject) -> RetainedMobject {
        let id = object.id;
        self.objects.push(object);
        RetainedMobject { id }
    }

    fn allocate_object_id(&mut self) -> Result<ObjectId, TextAuthoringError> {
        let id = ObjectId::new(self.next_object_id);
        if self.objects.iter().any(|object| object.id == id) {
            return Err(TextAuthoringError::DuplicateObject(id));
        }
        self.next_object_id = self
            .next_object_id
            .checked_add(1)
            .ok_or(TextAuthoringError::ObjectIdSpaceExhausted)?;
        Ok(id)
    }

    #[cfg(feature = "native-text")]
    fn import_native_text_artifact(
        &mut self,
        artifact: NativeTextResourceArtifact,
    ) -> Result<noon_core::TextResourceHandle, TextAuthoringError> {
        self.import_font_dependencies(&artifact.resource, &artifact.fonts)?;
        Ok(self.texts.insert(artifact.resource)?)
    }

    #[cfg(feature = "typst")]
    fn import_typst_artifact(
        &mut self,
        artifact: TypstResourceArtifact,
    ) -> Result<noon_core::TextResourceHandle, TextAuthoringError> {
        self.import_font_dependencies(&artifact.resource, &artifact.fonts)?;

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

    fn import_font_dependencies(
        &mut self,
        resource: &TextResource,
        fonts: &FontResourceArena,
    ) -> Result<(), TextAuthoringError> {
        for run in resource.runs.iter() {
            let font = fonts
                .get_for_face(&run.font)
                .ok_or(TextAuthoringError::MissingFontResource)?;
            self.fonts.intern_face(&run.font, font.data.clone())?;
        }
        Ok(())
    }
}

#[cfg(all(
    test,
    feature = "native-text",
    feature = "typst",
    feature = "bundled-fonts"
))]
mod tests {
    use super::*;
    use noon_core::ObjectContentRef;

    #[test]
    fn native_text_authors_retained_plain_text_without_geometry_placeholder() {
        let mut scene = RetainedScene::new();
        let object = scene
            .add_text(Text::new("Native Noon").color(noon_core::YELLOW))
            .unwrap();

        assert_eq!(scene.objects().len(), 1);
        let ObjectContentRef::Text(handle) = &scene.objects()[0].content else {
            panic!("Text must author retained text content");
        };
        assert_eq!(object.id(), scene.objects()[0].id);
        let resource = scene.texts().get(*handle).unwrap();
        assert_eq!(resource.kind, TextSourceKind::Plain);
        assert_eq!(resource.source.as_ref(), "Native Noon");
        assert!(!scene.fonts().is_empty());
        assert!(scene.objects()[0].content.geometry().is_none());
    }

    #[test]
    fn native_multiline_text_preserves_backend_runs_and_source_identity() {
        let mut scene = RetainedScene::new();
        scene
            .add_text(Text::new("first\nsecond").with_line_spacing(0.5))
            .unwrap();
        let handle = scene.objects()[0].content.text().unwrap();
        let resource = scene.texts().get(handle).unwrap();
        assert_eq!(resource.kind, TextSourceKind::Plain);
        assert_eq!(resource.runs.len(), 2);
        assert_eq!(resource.source.as_ref(), "first\nsecond");
    }

    #[test]
    fn unavailable_native_font_fails_without_consuming_scene_identity_or_resources() {
        let mut scene = RetainedScene::new();
        let error = scene
            .add_text(Text::new("Noon").with_font("Definitely Missing Font"))
            .unwrap_err();
        assert!(matches!(error, TextAuthoringError::FontUnavailable(_)));
        assert!(scene.objects().is_empty());
        assert!(scene.texts().is_empty());
        assert!(scene.fonts().is_empty());

        let object = scene.add_text(Text::new("first valid object")).unwrap();
        assert_eq!(object.id(), ObjectId::new(0));
    }

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
        assert!(scene.objects()[0].base_style.fill.is_some());
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
        assert!((scene.objects()[0].base_transform.scale.x - 0.075).abs() < 1e-6);
        assert!((scene.objects()[0].base_transform.scale.y - 0.075).abs() < 1e-6);
    }

    #[test]
    fn retained_scene_compiles_text_handles_without_copying_resources() {
        let mut scene = RetainedScene::new();
        scene.add_text(Text::new("Plain")).unwrap();
        scene.add_typst(Typst::new("Noon")).unwrap();
        scene
            .add_math_typst(MathTypst::new("sum_(k=1)^n k"))
            .unwrap();

        let compiled = scene.compile().unwrap();
        assert_eq!(compiled.objects().len(), 3);
        assert!(compiled
            .objects()
            .iter()
            .all(|object| object.text().is_some()));
        assert_eq!(scene.texts().len(), 3);
    }

    #[test]
    fn invalid_font_size_is_rejected_before_resource_insertion() {
        let mut scene = RetainedScene::new();
        let error = scene
            .add_text(Text::new("bad").with_font_size(0.0))
            .unwrap_err();
        assert_eq!(error, TextAuthoringError::InvalidFontSize(0.0));
        assert!(scene.objects().is_empty());
        assert!(scene.texts().is_empty());

        let error = scene
            .add_typst(Typst::new("bad").with_font_size(0.0))
            .unwrap_err();
        assert_eq!(error, TextAuthoringError::InvalidFontSize(0.0));
        assert!(scene.objects().is_empty());
        assert!(scene.texts().is_empty());
    }
}
