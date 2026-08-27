use std::collections::HashSet;

use noon_core::{
    GeometryRef, ObjectDefinition, ObjectId, Style, TrackDefinition, Transform2D,
};
use serde::{Deserialize, Serialize};

use crate::{IrError, SceneDocument};

/// Version of the canonical mixed authoring scene contract.
///
/// This is intentionally distinct from the legacy geometry-only `SceneDocument`
/// version. Compatibility adapters may consume the legacy document while migration
/// is in progress, but new frontend semantics should converge on `SceneSpec`.
pub const SCENE_SPEC_VERSION: u32 = 1;

/// Source-language identity at the canonical authoring boundary.
///
/// This mirrors the semantic source kinds without making the IR depend on a
/// backend-specific compiler or retained `TextResource` handle. Adding typed layout
/// options is additive to `TextSpec`; renderer state never belongs here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextSpecKind {
    Plain,
    Markup,
    Typst,
    MathTypst,
    Tex,
    MathTex,
}

/// Backend-source text definition before compilation into immutable retained resources.
///
/// Presentation is deliberately absent. Transform, color, opacity and painter order
/// belong to the owning `ObjectSpec`, allowing identical text content to share a
/// compiled artifact while remaining independent scene objects.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextSpec {
    pub kind: TextSpecKind,
    pub source: String,
    pub font_size: f32,
}

impl TextSpec {
    pub fn new(kind: TextSpecKind, source: impl Into<String>, font_size: f32) -> Self {
        Self {
            kind,
            source: source.into(),
            font_size,
        }
    }

    pub fn typst(source: impl Into<String>, font_size: f32) -> Self {
        Self::new(TextSpecKind::Typst, source, font_size)
    }

    pub fn math_typst(source: impl Into<String>, font_size: f32) -> Self {
        Self::new(TextSpecKind::MathTypst, source, font_size)
    }

    pub fn validate(&self) -> Result<(), SceneSpecError> {
        if self.source.is_empty() {
            return Err(SceneSpecError::EmptyTextSource);
        }
        if !self.font_size.is_finite() || self.font_size <= 0.0 {
            return Err(SceneSpecError::InvalidTextFontSize(self.font_size));
        }
        Ok(())
    }
}

/// Source content for one canonical scene object.
///
/// Geometry and text occupy the same object identity and painter-order domain.
/// Text remains source-level here and is compiled later through the shared text
/// compiler/resource boundary; no glyphs, font bytes, SVG, or atlas state cross it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ObjectSpecContent {
    Geometry(GeometryRef),
    Text(TextSpec),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectSpec {
    pub id: ObjectId,
    pub content: ObjectSpecContent,
    #[serde(default)]
    pub transform: Transform2D,
    #[serde(default)]
    pub style: Style,
}

impl ObjectSpec {
    pub fn geometry(id: ObjectId, geometry: GeometryRef) -> Self {
        Self {
            id,
            content: ObjectSpecContent::Geometry(geometry),
            transform: Transform2D::default(),
            style: Style::default(),
        }
    }

    pub fn text(id: ObjectId, text: TextSpec) -> Self {
        Self {
            id,
            content: ObjectSpecContent::Text(text),
            transform: Transform2D::default(),
            style: Style::default(),
        }
    }

    pub fn from_legacy(value: &ObjectDefinition) -> Self {
        Self {
            id: value.id,
            content: ObjectSpecContent::Geometry(value.geometry.clone()),
            transform: value.transform,
            style: value.style,
        }
    }
}

/// Transitional adapter input for the current retained-text sidecar.
///
/// `order` exists only at this compatibility edge. Once merged, painter order is the
/// position of the object in `SceneSpec::objects`; the canonical document therefore
/// has no second order allocator or text-specific ID/order space.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderedTextObjectSpec {
    pub order: u32,
    pub object: ObjectSpec,
}

impl OrderedTextObjectSpec {
    pub fn new(order: u32, object: ObjectSpec) -> Result<Self, SceneSpecError> {
        if !matches!(&object.content, ObjectSpecContent::Text(_)) {
            return Err(SceneSpecError::OrderedObjectIsNotText(object.id));
        }
        Ok(Self { order, object })
    }
}

/// Canonical mixed authoring scene shared by Python, Rust and JavaScript frontends.
///
/// The object vector is the single painter-order stream. Tracks continue to target
/// stable `ObjectId`s, so geometry and text can share timeline semantics without a
/// parallel text scene or fake geometry representation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneSpec {
    pub version: u32,
    pub objects: Vec<ObjectSpec>,
    pub tracks: Vec<TrackDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_object: Option<ObjectId>,
}

impl SceneSpec {
    pub fn new(objects: Vec<ObjectSpec>, tracks: Vec<TrackDefinition>) -> Result<Self, SceneSpecError> {
        let spec = Self {
            version: SCENE_SPEC_VERSION,
            objects,
            tracks,
            camera_object: None,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn from_legacy(legacy: &SceneDocument) -> Result<Self, SceneSpecError> {
        // Reuse the mature legacy validator before adapting its geometry objects.
        legacy.clone().into_scene().map_err(SceneSpecError::Legacy)?;
        let spec = Self {
            version: SCENE_SPEC_VERSION,
            objects: legacy.objects.iter().map(ObjectSpec::from_legacy).collect(),
            tracks: legacy.tracks.clone(),
            camera_object: legacy.camera_object,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Merge the current geometry-only document plus ordered text sidecar into the
    /// canonical one-vector painter stream.
    ///
    /// Text objects claim their existing global slots first. Legacy geometry then
    /// fills every remaining slot in original geometry order. This is equivalent to
    /// the current sorted insertion behavior, but order becomes structural after the
    /// adapter instead of remaining a text-only field.
    pub fn from_legacy_with_ordered_text(
        legacy: &SceneDocument,
        text_objects: Vec<OrderedTextObjectSpec>,
    ) -> Result<Self, SceneSpecError> {
        legacy.clone().into_scene().map_err(SceneSpecError::Legacy)?;
        let object_count = legacy
            .objects
            .len()
            .checked_add(text_objects.len())
            .ok_or(SceneSpecError::ObjectCountOverflow)?;

        let legacy_ids = legacy
            .objects
            .iter()
            .map(|object| object.id)
            .collect::<HashSet<_>>();
        let mut text_ids = HashSet::with_capacity(text_objects.len());
        let mut orders = HashSet::with_capacity(text_objects.len());
        let mut slots = vec![None; object_count];

        for text in text_objects {
            if text.order as usize >= object_count {
                return Err(SceneSpecError::PainterOrderOutOfRange {
                    order: text.order,
                    object_count,
                });
            }
            if !orders.insert(text.order) {
                return Err(SceneSpecError::DuplicatePainterOrder(text.order));
            }
            if legacy_ids.contains(&text.object.id) || !text_ids.insert(text.object.id) {
                return Err(SceneSpecError::DuplicateObject(text.object.id));
            }
            if !matches!(&text.object.content, ObjectSpecContent::Text(_)) {
                return Err(SceneSpecError::OrderedObjectIsNotText(text.object.id));
            }
            slots[text.order as usize] = Some(text.object);
        }

        let mut legacy_objects = legacy.objects.iter();
        for slot in &mut slots {
            if slot.is_none() {
                let object = legacy_objects
                    .next()
                    .expect("mixed scene slot count must match legacy plus text objects");
                *slot = Some(ObjectSpec::from_legacy(object));
            }
        }
        debug_assert!(legacy_objects.next().is_none());

        let spec = Self {
            version: SCENE_SPEC_VERSION,
            objects: slots
                .into_iter()
                .map(|object| object.expect("every mixed scene painter slot must be filled"))
                .collect(),
            tracks: legacy.tracks.clone(),
            camera_object: legacy.camera_object,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<(), SceneSpecError> {
        if self.version != SCENE_SPEC_VERSION {
            return Err(SceneSpecError::UnsupportedVersion(self.version));
        }

        let mut ids = HashSet::with_capacity(self.objects.len());
        for object in &self.objects {
            if !ids.insert(object.id) {
                return Err(SceneSpecError::DuplicateObject(object.id));
            }
            if let ObjectSpecContent::Text(text) = &object.content {
                text.validate()?;
            }
        }

        if let Some(camera) = self.camera_object {
            if !ids.contains(&camera) {
                return Err(SceneSpecError::UnknownCameraObject(camera));
            }
        }
        Ok(())
    }

    pub fn from_json(json: &str) -> Result<Self, SceneSpecError> {
        let spec: Self = serde_json::from_str(json).map_err(SceneSpecError::Json)?;
        spec.validate()?;
        Ok(spec)
    }

    pub fn to_json(&self) -> Result<String, SceneSpecError> {
        self.validate()?;
        serde_json::to_string(self).map_err(SceneSpecError::Json)
    }
}

#[derive(Debug)]
pub enum SceneSpecError {
    UnsupportedVersion(u32),
    EmptyTextSource,
    InvalidTextFontSize(f32),
    DuplicateObject(ObjectId),
    DuplicatePainterOrder(u32),
    PainterOrderOutOfRange { order: u32, object_count: usize },
    OrderedObjectIsNotText(ObjectId),
    UnknownCameraObject(ObjectId),
    ObjectCountOverflow,
    Legacy(IrError),
    Json(serde_json::Error),
}

impl std::fmt::Display for SceneSpecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported mixed SceneSpec version {version}")
            }
            Self::EmptyTextSource => formatter.write_str("text source must not be empty"),
            Self::InvalidTextFontSize(size) => {
                write!(formatter, "text font_size must be finite and positive, got {size}")
            }
            Self::DuplicateObject(object) => {
                write!(formatter, "duplicate SceneSpec object {}", object.get())
            }
            Self::DuplicatePainterOrder(order) => {
                write!(formatter, "duplicate transitional text painter order {order}")
            }
            Self::PainterOrderOutOfRange {
                order,
                object_count,
            } => write!(
                formatter,
                "transitional text painter order {order} is outside mixed object count {object_count}"
            ),
            Self::OrderedObjectIsNotText(object) => write!(
                formatter,
                "ordered text adapter object {} does not contain text",
                object.get()
            ),
            Self::UnknownCameraObject(object) => {
                write!(formatter, "unknown SceneSpec camera object {}", object.get())
            }
            Self::ObjectCountOverflow => formatter.write_str("mixed SceneSpec object count overflow"),
            Self::Legacy(error) => error.fmt(formatter),
            Self::Json(error) => write!(formatter, "invalid mixed SceneSpec JSON: {error}"),
        }
    }
}

impl std::error::Error for SceneSpecError {}

#[cfg(test)]
mod tests {
    use noon_core::{Color, GeometryRef, SceneDefinition, Vec2};

    use super::*;
    use crate::SceneDocument;

    fn legacy_document() -> SceneDocument {
        let mut scene = SceneDefinition::new();
        let first = scene.add(GeometryRef::circle(1.0));
        let second = scene.add(GeometryRef::rectangle(2.0, 1.0));
        scene.object_mut(first).unwrap().transform.translation = Vec2::new(-2.0, 0.0);
        scene.object_mut(second).unwrap().style.opacity = 0.6;
        SceneDocument::from_scene(&scene)
    }

    #[test]
    fn legacy_geometry_adapts_without_changing_order_or_presentation() {
        let legacy = legacy_document();
        let mixed = SceneSpec::from_legacy(&legacy).unwrap();

        assert_eq!(mixed.objects.len(), 2);
        assert_eq!(mixed.objects[0].id, legacy.objects[0].id);
        assert_eq!(mixed.objects[1].id, legacy.objects[1].id);
        assert_eq!(mixed.objects[0].transform, legacy.objects[0].transform);
        assert_eq!(mixed.objects[1].style, legacy.objects[1].style);
        assert!(matches!(
            &mixed.objects[0].content,
            ObjectSpecContent::Geometry(_)
        ));
    }

    #[test]
    fn ordered_text_fills_global_slots_and_removes_parallel_order_domain() {
        let legacy = legacy_document();
        let text_id = ObjectId::new(1_u64 << 52);
        let mut text = ObjectSpec::text(text_id, TextSpec::typst("middle", 48.0));
        text.transform.translation = Vec2::new(0.5, -1.0);
        text.style.fill = Some(Color::rgba(0.2, 0.4, 0.8, 1.0));
        text.style.opacity = 0.75;

        let mixed = SceneSpec::from_legacy_with_ordered_text(
            &legacy,
            vec![OrderedTextObjectSpec::new(1, text.clone()).unwrap()],
        )
        .unwrap();

        assert_eq!(
            mixed.objects.iter().map(|object| object.id).collect::<Vec<_>>(),
            vec![legacy.objects[0].id, text_id, legacy.objects[1].id]
        );
        assert_eq!(mixed.objects[1].transform, text.transform);
        assert_eq!(mixed.objects[1].style, text.style);
        let ObjectSpecContent::Text(spec) = &mixed.objects[1].content else {
            panic!("middle object must remain source-level text");
        };
        assert_eq!(spec.source, "middle");
        assert_eq!(spec.kind, TextSpecKind::Typst);
    }

    #[test]
    fn multiple_text_slots_match_sorted_insertion_semantics() {
        let legacy = legacy_document();
        let left = ObjectSpec::text(ObjectId::new(100), TextSpec::typst("left", 24.0));
        let right = ObjectSpec::text(ObjectId::new(101), TextSpec::math_typst("x^2", 24.0));

        let mixed = SceneSpec::from_legacy_with_ordered_text(
            &legacy,
            vec![
                OrderedTextObjectSpec::new(0, left).unwrap(),
                OrderedTextObjectSpec::new(2, right).unwrap(),
            ],
        )
        .unwrap();

        assert_eq!(
            mixed.objects.iter().map(|object| object.id).collect::<Vec<_>>(),
            vec![ObjectId::new(100), legacy.objects[0].id, ObjectId::new(101), legacy.objects[1].id]
        );
    }

    #[test]
    fn duplicate_or_out_of_range_transitional_orders_are_rejected() {
        let legacy = legacy_document();
        let first = ObjectSpec::text(ObjectId::new(100), TextSpec::typst("a", 24.0));
        let second = ObjectSpec::text(ObjectId::new(101), TextSpec::typst("b", 24.0));

        assert!(matches!(
            SceneSpec::from_legacy_with_ordered_text(
                &legacy,
                vec![
                    OrderedTextObjectSpec::new(1, first.clone()).unwrap(),
                    OrderedTextObjectSpec::new(1, second).unwrap(),
                ],
            ),
            Err(SceneSpecError::DuplicatePainterOrder(1))
        ));
        assert!(matches!(
            SceneSpec::from_legacy_with_ordered_text(
                &legacy,
                vec![OrderedTextObjectSpec::new(3, first).unwrap()],
            ),
            Err(SceneSpecError::PainterOrderOutOfRange { order: 3, .. })
        ));
    }

    #[test]
    fn text_and_geometry_share_one_identity_domain() {
        let legacy = legacy_document();
        let collision = ObjectSpec::text(
            legacy.objects[0].id,
            TextSpec::typst("collision", 24.0),
        );
        assert!(matches!(
            SceneSpec::from_legacy_with_ordered_text(
                &legacy,
                vec![OrderedTextObjectSpec::new(1, collision).unwrap()],
            ),
            Err(SceneSpecError::DuplicateObject(_))
        ));
    }

    #[test]
    fn canonical_json_round_trip_preserves_mixed_painter_order() {
        let legacy = legacy_document();
        let text = ObjectSpec::text(ObjectId::new(100), TextSpec::typst("hello", 36.0));
        let mixed = SceneSpec::from_legacy_with_ordered_text(
            &legacy,
            vec![OrderedTextObjectSpec::new(1, text).unwrap()],
        )
        .unwrap();

        let json = mixed.to_json().unwrap();
        let decoded = SceneSpec::from_json(&json).unwrap();
        assert_eq!(decoded, mixed);
        assert_eq!(decoded.objects[1].id, ObjectId::new(100));
    }

    #[test]
    fn all_public_text_source_families_have_canonical_identity() {
        let kinds = [
            TextSpecKind::Plain,
            TextSpecKind::Markup,
            TextSpecKind::Typst,
            TextSpecKind::MathTypst,
            TextSpecKind::Tex,
            TextSpecKind::MathTex,
        ];
        for kind in kinds {
            TextSpec::new(kind, "source", 48.0).validate().unwrap();
        }
    }
}
