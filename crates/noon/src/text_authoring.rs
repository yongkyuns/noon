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
    RetainedObjectDefinition, SceneDefinition, Style, TextResource, TextResourceArena,
    TextResourceValidationError, TextSourceKind, TrackDefinition, Transform2D, Vec2, WHITE,
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

            fn validate(&self) -> Result<(), TextAuthoringError> {
                if !self.0.font_size.is_finite() || self.0.font_size <= 0.0 {
                    return Err(TextAuthoringError::InvalidFontSize(self.0.font_size));
                }
                if !self.0.opacity.is_finite() || !(0.0..=1.0).contains(&self.0.opacity) {
                    return Err(TextAuthoringError::InvalidOpacity(self.0.opacity));
                }
                Ok(())
            }

            fn compile(
                self,
                scene: &mut RetainedScene,
            ) -> Result<RetainedObjectDefinition, TextAuthoringError> {
                self.validate()?;
                let artifact = compile_typst_resource(self.0.source.as_ref(), $mode)?;
                debug_assert_eq!(artifact.resource.kind, $kind);
                let handle = scene.import_typst_artifact(artifact)?;
                let id = scene.allocate_object_id()?;
                Ok(self.retained_definition(id, handle))
            }

            fn compile_with_id(
                self,
                scene: &mut RetainedScene,
                id: ObjectId,
            ) -> Result<RetainedObjectDefinition, TextAuthoringError> {
                self.validate()?;
                let artifact = compile_typst_resource(self.0.source.as_ref(), $mode)?;
                debug_assert_eq!(artifact.resource.kind, $kind);
                let handle = scene.import_typst_artifact(artifact)?;
                Ok(self.retained_definition(id, handle))
            }

            fn retained_definition(
                &self,
                id: ObjectId,
                handle: noon_core::TextResourceHandle,
            ) -> RetainedObjectDefinition {
                let mut object = RetainedObjectDefinition::text(id, handle);
                object.transform = self.0.authored_transform();
                object.style = self.0.style();
                object
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
    DuplicateObject(ObjectId),
    InvalidPainterOrder { order: usize, object_count: usize },
    ObjectIdSpaceExhausted,
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
            Self::DuplicateObject(id) => {
                write!(formatter, "duplicate retained object id {}", id.get())
            }
            Self::InvalidPainterOrder {
                order,
                object_count,
            } => write!(
                formatter,
                "retained painter order {order} is invalid for {object_count} existing objects"
            ),
            Self::ObjectIdSpaceExhausted => {
                formatter.write_str("retained object ID space is exhausted")
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

    /// Lift an existing geometry-only scene into the retained object domain.
    ///
    /// Object IDs, painter order, and tracks are preserved exactly. Resource-backed
    /// text can then be inserted into explicit painter slots without changing the
    /// legacy scene serialization or its track references.
    pub fn from_legacy(scene: &SceneDefinition) -> Result<Self, TextAuthoringError> {
        let objects = scene
            .objects()
            .iter()
            .map(RetainedObjectDefinition::from)
            .collect::<Vec<_>>();
        let next_object_id =
            objects
                .iter()
                .map(|object| object.id.get())
                .max()
                .map_or(Ok(0), |id| {
                    id.checked_add(1)
                        .ok_or(TextAuthoringError::ObjectIdSpaceExhausted)
                })?;
        Ok(Self {
            objects,
            tracks: scene.tracks().to_vec(),
            texts: TextResourceArena::default(),
            geometries: GeometryResourceArena::default(),
            fonts: FontResourceArena::default(),
            next_object_id,
        })
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

    /// Insert Typst at an exact global painter slot using a caller-owned semantic ID.
    pub fn insert_typst_at(
        &mut self,
        order: usize,
        id: ObjectId,
        object: Typst,
    ) -> Result<RetainedMobject, TextAuthoringError> {
        self.insert_text_at(order, id, |scene| object.compile_with_id(scene, id))
    }

    /// Insert MathTypst at an exact global painter slot using a caller-owned semantic ID.
    pub fn insert_math_typst_at(
        &mut self,
        order: usize,
        id: ObjectId,
        object: MathTypst,
    ) -> Result<RetainedMobject, TextAuthoringError> {
        self.insert_text_at(order, id, |scene| object.compile_with_id(scene, id))
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

    fn insert_text_at<F>(
        &mut self,
        order: usize,
        id: ObjectId,
        compile: F,
    ) -> Result<RetainedMobject, TextAuthoringError>
    where
        F: FnOnce(&mut Self) -> Result<RetainedObjectDefinition, TextAuthoringError>,
    {
        if order > self.objects.len() {
            return Err(TextAuthoringError::InvalidPainterOrder {
                order,
                object_count: self.objects.len(),
            });
        }
        if self.objects.iter().any(|object| object.id == id) {
            return Err(TextAuthoringError::DuplicateObject(id));
        }
        let next_object_id = self.next_object_id_after(id)?;
        let object = compile(self)?;
        self.objects.insert(order, object);
        self.next_object_id = next_object_id;
        Ok(RetainedMobject { id })
    }

    fn push_object(&mut self, object: RetainedObjectDefinition) -> RetainedMobject {
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

    fn next_object_id_after(&self, id: ObjectId) -> Result<u64, TextAuthoringError> {
        if id.get() < self.next_object_id {
            return Ok(self.next_object_id);
        }
        id.get()
            .checked_add(1)
            .ok_or(TextAuthoringError::ObjectIdSpaceExhausted)
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
    use noon_core::{GeometryRef, ObjectContentRef, RateFunction, TrackTiming};

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
    fn legacy_scene_lifts_with_identity_tracks_and_order_intact() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.5));
        let square = legacy.add(GeometryRef::rectangle(1.0, 1.0));
        legacy
            .animate_position(
                circle,
                Vec2::ZERO,
                Vec2::new(2.0, 0.0),
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .unwrap();

        let retained = RetainedScene::from_legacy(&legacy).unwrap();
        assert_eq!(retained.objects().len(), 2);
        assert_eq!(retained.objects()[0].id, circle);
        assert_eq!(retained.objects()[1].id, square);
        assert_eq!(retained.tracks(), legacy.tracks());
        assert!(retained
            .objects()
            .iter()
            .all(|object| object.content.geometry().is_some()));

        let compiled = retained.compile().unwrap();
        assert_eq!(compiled.object_index(circle), Some(0));
        assert_eq!(compiled.object_index(square), Some(1));
        assert_eq!(compiled.track_count(), legacy.tracks().len());
    }

    #[test]
    fn explicit_typst_insertion_reconstructs_mixed_global_painter_order() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        let square = legacy.add(GeometryRef::rectangle(0.5, 0.5));
        let text_id = ObjectId::new(1_u64 << 52);

        let mut retained = RetainedScene::from_legacy(&legacy).unwrap();
        let text = retained
            .insert_typst_at(1, text_id, Typst::new("middle").with_font_size(48.0))
            .unwrap();

        assert_eq!(text.id(), text_id);
        assert_eq!(
            retained
                .objects()
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>(),
            vec![circle, text_id, square]
        );
        assert!(retained.objects()[0].content.geometry().is_some());
        assert!(retained.objects()[1].content.text().is_some());
        assert!(retained.objects()[2].content.geometry().is_some());
        assert_eq!(retained.texts().len(), 1);

        let compiled = retained.compile().unwrap();
        assert_eq!(compiled.object_index(circle), Some(0));
        assert_eq!(compiled.object_index(text_id), Some(1));
        assert_eq!(compiled.object_index(square), Some(2));
    }

    #[test]
    fn explicit_math_typst_keeps_math_resource_identity() {
        let mut legacy = SceneDefinition::new();
        legacy.add(GeometryRef::circle(0.25));
        let math_id = ObjectId::new((1_u64 << 52) + 1);
        let mut retained = RetainedScene::from_legacy(&legacy).unwrap();
        retained
            .insert_math_typst_at(
                1,
                math_id,
                MathTypst::new("sum_(k=1)^n k").with_font_size(72.0),
            )
            .unwrap();

        let handle = retained.objects()[1].content.text().unwrap();
        assert_eq!(
            retained.texts().get(handle).unwrap().kind,
            TextSourceKind::MathTypst
        );
    }

    #[test]
    fn rejected_explicit_insertion_does_not_compile_text_resources() {
        let mut legacy = SceneDefinition::new();
        let existing = legacy.add(GeometryRef::circle(0.25));
        let mut retained = RetainedScene::from_legacy(&legacy).unwrap();

        let order_error = retained
            .insert_typst_at(2, ObjectId::new(1_u64 << 52), Typst::new("bad order"))
            .unwrap_err();
        assert_eq!(
            order_error,
            TextAuthoringError::InvalidPainterOrder {
                order: 2,
                object_count: 1,
            }
        );
        assert!(retained.texts().is_empty());
        assert!(retained.fonts().is_empty());

        let duplicate_error = retained
            .insert_typst_at(1, existing, Typst::new("duplicate"))
            .unwrap_err();
        assert_eq!(
            duplicate_error,
            TextAuthoringError::DuplicateObject(existing)
        );
        assert!(retained.texts().is_empty());
        assert!(retained.fonts().is_empty());
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
