use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt,
    sync::Arc,
};

use noon_core::{
    Color, FontFaceIdentity, FontResourceArena, FontResourceLookup, FontVariationSetting,
    GeometryRef, GeometryResource, GeometryResourceArena, GeometryResourceHandle,
    GeometryResourceLookup, GlyphRun, PositionedGlyph, Rect, StrokeCap, StrokeJoin, Style,
    TextAffineTransform, TextClusterIdentity, TextDirection, TextGlyphStroke, TextLayoutArtifact,
    TextLayoutBackend, TextLayoutBackendKind, TextPart, TextRenderItem, TextResource,
    TextResourceArena, TextResourceHandle, TextResourceLookup, TextSourceKind, TextSourceSpan,
    TextVectorItem, TextVectorStyle, Transform2D, Vec2, VectorPath,
};
use serde::{Deserialize, Serialize};

/// Lower the compatibility retained scene at the web boundary into the unified
/// compiled runtime input. Text bounds are immutable resource metadata, captured
/// from the same scene-owned arena that supplies the renderer bundle.
pub(crate) fn compile_retained_scene(
    scene: &noon::RetainedScene,
    tracks: &[noon_core::TrackDefinition],
) -> Result<noon_compile::CompiledScene, noon_compile::CompileError> {
    let objects = scene
        .objects()
        .iter()
        .map(|object| {
            let mut compiled = noon_compile::CompiledObject::new(
                object.id,
                object.content.clone(),
                object.transform,
                object.style,
            );
            compiled.text_bounds = object
                .content
                .text()
                .and_then(|handle| scene.texts().get(handle).map(|resource| resource.bounds));
            compiled
        })
        .collect();
    noon_compile::CompiledScene::compile_objects(objects, tracks)
}

use crate::TransportTextResourceHandle;

/// One-shot resource channel paired with `noon.execution.retained`.
///
/// Frame deltas carry only small text handles. This bundle transfers the immutable
/// shaped text, vector-decoration geometry, and exact OpenType buffers once when a
/// retained scene is installed. Python never owns or serializes these payloads.
pub const RETAINED_RESOURCE_TRANSPORT_CHANNEL: &str = "noon.execution.retained.resources";
pub const RETAINED_RESOURCE_TRANSPORT_VERSION: u32 = 4;

/// Immutable compiled render geometry at the genuine cross-worker boundary.
/// Indices are scoped to the player session and this installed resource bundle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportRenderGeometryResources {
    session: u32,
    geometries: Vec<GeometryRef>,
    preparations: Vec<RenderGeometryPreparation>,
}

/// Derived renderer inputs, transferred once with the installed geometry table.
/// Actual playback still resolves its full mesh key; these are preparation hints.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct RenderGeometryPreparation {
    pub resource: u32,
    pub style: Style,
    pub transform: Transform2D,
}

impl RenderGeometryPreparation {
    fn is_finite(&self) -> bool {
        let finite_color = |color: Color| {
            [color.red, color.green, color.blue, color.alpha]
                .into_iter()
                .all(f32::is_finite)
        };
        [
            self.transform.translation.x,
            self.transform.translation.y,
            self.transform.scale.x,
            self.transform.scale.y,
            self.transform.rotation,
            self.style.stroke_width,
            self.style.opacity,
        ]
        .into_iter()
        .all(f32::is_finite)
            && self.style.fill.is_none_or(finite_color)
            && self.style.stroke.is_none_or(finite_color)
    }
}

pub(crate) fn compiled_render_geometry_preparations(
    compiled: &noon_compile::CompiledScene,
    geometries: &[Arc<GeometryRef>],
) -> Result<Vec<RenderGeometryPreparation>, RetainedResourceTransportError> {
    let indices = geometries
        .iter()
        .enumerate()
        .map(|(index, geometry)| {
            u32::try_from(index)
                .map(|index| (Arc::as_ptr(geometry) as usize, index))
                .map_err(|_| {
                    RetainedResourceTransportError::Encode("too many render resources".into())
                })
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(compiled
        .tracks_iter()
        .filter_map(|track| {
            let noon_compile::TransformGeometryPlan::PathPair {
                geometry,
                render_transform,
            } = track.transform_geometry_plan.as_ref()?
            else {
                return None;
            };
            let noon_core::TrackValues::Object { from, to } = &track.values else {
                return None;
            };
            // Mixed stroke modes and current-relative paths can change their mesh
            // key during playback. Keep them on the normal lazy preparation path.
            if from.style.stroke_width_mode != to.style.stroke_width_mode
                || (render_transform.is_none()
                    && from.style.stroke_width_mode == noon_core::StrokeWidthMode::ScreenSpace)
            {
                return None;
            }
            Some(RenderGeometryPreparation {
                resource: *indices.get(&(Arc::as_ptr(geometry) as usize))?,
                style: from.style,
                transform: render_transform.unwrap_or(Transform2D::IDENTITY),
            })
        })
        .collect())
}

pub(crate) fn compiled_render_geometries(
    compiled: &noon_compile::CompiledScene,
) -> Arc<[Arc<GeometryRef>]> {
    let mut seen = std::collections::HashSet::new();
    compiled
        .tracks_iter()
        .filter_map(|track| {
            let noon_compile::TransformGeometryPlan::PathPair {
                geometry,
                render_transform,
            } = track.transform_geometry_plan.as_ref()?
            else {
                return None;
            };
            // Current-relative screen-space fallback paths are rebuilt rather than
            // published by identity. Local ScaleWithObject pairs remain stable.
            if render_transform.is_none()
                && matches!(&track.values,
                noon_core::TrackValues::Object { from, to }
                    if from.style.stroke_width_mode == noon_core::StrokeWidthMode::ScreenSpace
                    && to.style.stroke_width_mode == noon_core::StrokeWidthMode::ScreenSpace)
            {
                return None;
            }
            seen.insert(Arc::as_ptr(geometry) as usize)
                .then(|| geometry.clone())
        })
        .collect::<Vec<_>>()
        .into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TransportGeometryResourceHandle {
    pub arena: u64,
    pub id: u64,
    pub version: u64,
}

impl From<GeometryResourceHandle> for TransportGeometryResourceHandle {
    fn from(value: GeometryResourceHandle) -> Self {
        Self {
            arena: value.arena,
            id: value.id.get(),
            version: value.version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedResourceBundle {
    pub channel: String,
    pub protocol_version: u32,
    texts: Vec<TransportTextEntry>,
    geometries: Vec<TransportGeometryEntry>,
    fonts: Vec<TransportFontEntry>,
    render_geometry_resources: Option<TransportRenderGeometryResources>,
}

impl RetainedResourceBundle {
    pub fn capture(
        text_handles: impl IntoIterator<Item = TextResourceHandle>,
        texts: &impl TextResourceLookup,
        geometries: &impl GeometryResourceLookup,
        fonts: &impl FontResourceLookup,
    ) -> Result<Self, RetainedResourceTransportError> {
        let text_handles = text_handles.into_iter().collect::<BTreeSet<_>>();
        let mut geometry_handles = BTreeSet::new();
        let mut font_entries = BTreeMap::<(String, u32), TransportFontEntry>::new();
        let mut text_entries = Vec::with_capacity(text_handles.len());

        for handle in text_handles {
            let resource = texts.get(handle).ok_or_else(|| {
                RetainedResourceTransportError::UnknownText(
                    TransportTextResourceHandle::from_source_handle(handle),
                )
            })?;
            for vector in resource.vector_items.iter() {
                geometry_handles.insert(vector.geometry);
            }
            for run in resource.runs.iter() {
                let font = fonts.get_for_face(&run.font).ok_or_else(|| {
                    RetainedResourceTransportError::MissingFont {
                        face_key: run.font.face_key.to_string(),
                        face_index: run.font.face_index,
                    }
                })?;
                let key = (font.key.face_key.to_string(), font.key.face_index);
                font_entries
                    .entry(key.clone())
                    .or_insert_with(|| TransportFontEntry {
                        face_key: key.0,
                        face_index: key.1,
                        data: font.data.as_ref().to_vec(),
                    });
            }
            text_entries.push(TransportTextEntry {
                handle: TransportTextResourceHandle::from_source_handle(handle),
                resource: TransportTextResource::from_core(resource),
            });
        }

        let mut geometry_entries = Vec::with_capacity(geometry_handles.len());
        for handle in geometry_handles {
            let resource = geometries
                .get(handle)
                .ok_or_else(|| RetainedResourceTransportError::UnknownGeometry(handle.into()))?;
            let GeometryResource::VectorPath(path) = resource;
            geometry_entries.push(TransportGeometryEntry {
                handle: handle.into(),
                path: path.as_ref().clone(),
            });
        }

        Ok(Self {
            channel: RETAINED_RESOURCE_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: RETAINED_RESOURCE_TRANSPORT_VERSION,
            texts: text_entries,
            geometries: geometry_entries,
            fonts: font_entries.into_values().collect(),
            render_geometry_resources: None,
        })
    }

    pub fn text_count(&self) -> usize {
        self.texts.len()
    }

    pub(crate) fn set_render_geometries(
        &mut self,
        session: u32,
        geometries: Arc<[Arc<GeometryRef>]>,
        preparations: Vec<RenderGeometryPreparation>,
    ) {
        self.render_geometry_resources = Some(TransportRenderGeometryResources {
            session,
            geometries: geometries
                .iter()
                .map(|geometry| geometry.as_ref().clone())
                .collect(),
            preparations,
        });
    }

    pub fn geometry_count(&self) -> usize {
        self.geometries.len()
    }

    pub fn font_count(&self) -> usize {
        self.fonts.len()
    }

    pub fn font_bytes(&self) -> usize {
        self.fonts.iter().map(|font| font.data.len()).sum()
    }

    pub fn encode_binary(&self) -> Result<Vec<u8>, RetainedResourceTransportError> {
        let mut bytes = Vec::new();
        ciborium::ser::into_writer(self, &mut bytes)
            .map_err(|error| RetainedResourceTransportError::Encode(error.to_string()))?;
        Ok(bytes)
    }

    pub fn decode_binary(bytes: &[u8]) -> Result<Self, RetainedResourceTransportError> {
        let bundle: Self = ciborium::de::from_reader(bytes)
            .map_err(|error| RetainedResourceTransportError::Decode(error.to_string()))?;
        bundle.validate_protocol()?;
        Ok(bundle)
    }

    pub fn install(self) -> Result<InstalledRetainedResources, RetainedResourceTransportError> {
        self.validate_protocol()?;
        if let Some(resources) = &self.render_geometry_resources {
            for (index, geometry) in resources.geometries.iter().enumerate() {
                if !matches!(geometry, GeometryRef::VectorPath(_)) || !geometry.is_finite() {
                    return Err(RetainedResourceTransportError::InvalidRenderGeometry(index));
                }
            }
            for (index, preparation) in resources.preparations.iter().enumerate() {
                if preparation.resource as usize >= resources.geometries.len()
                    || !preparation.is_finite()
                {
                    return Err(RetainedResourceTransportError::InvalidRenderPreparation(
                        index,
                    ));
                }
            }
        }

        let mut geometries = GeometryResourceArena::new();
        let mut geometry_handles = HashMap::with_capacity(self.geometries.len());
        for entry in self.geometries {
            if geometry_handles.contains_key(&entry.handle) {
                return Err(RetainedResourceTransportError::DuplicateGeometry(
                    entry.handle,
                ));
            }
            let local = geometries.insert_path(entry.path);
            geometry_handles.insert(entry.handle, local);
        }

        let font_bytes = self
            .fonts
            .into_iter()
            .map(|entry| {
                (
                    (entry.face_key, entry.face_index),
                    Arc::<[u8]>::from(entry.data),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut fonts = FontResourceArena::new();
        let mut texts = TextResourceArena::new();
        let mut text_handles = HashMap::with_capacity(self.texts.len());

        for entry in self.texts {
            if text_handles.contains_key(&entry.handle) {
                return Err(RetainedResourceTransportError::DuplicateText(entry.handle));
            }
            let resource = entry.resource.into_core(&geometry_handles)?;
            for run in resource.runs.iter() {
                let key = (run.font.face_key.to_string(), run.font.face_index);
                let bytes = font_bytes.get(&key).ok_or_else(|| {
                    RetainedResourceTransportError::MissingFont {
                        face_key: key.0.clone(),
                        face_index: key.1,
                    }
                })?;
                fonts
                    .intern_face(&run.font, bytes.clone())
                    .map_err(|error| {
                        RetainedResourceTransportError::InvalidFont(error.to_string())
                    })?;
            }
            let local = texts
                .insert(resource)
                .map_err(|error| RetainedResourceTransportError::InvalidText(error.to_string()))?;
            text_handles.insert(entry.handle, local);
        }

        Ok(InstalledRetainedResources {
            texts,
            geometries,
            fonts,
            text_handles,
            render_geometry_session: self
                .render_geometry_resources
                .as_ref()
                .map(|resources| resources.session),
            render_geometry_preparations: self
                .render_geometry_resources
                .as_ref()
                .map(|resources| resources.preparations.clone())
                .unwrap_or_default(),
            render_geometries: self
                .render_geometry_resources
                .map(|resources| {
                    resources
                        .geometries
                        .into_iter()
                        .map(Arc::new)
                        .collect::<Vec<_>>()
                        .into()
                })
                .unwrap_or_default(),
        })
    }

    fn validate_protocol(&self) -> Result<(), RetainedResourceTransportError> {
        if self.channel != RETAINED_RESOURCE_TRANSPORT_CHANNEL {
            return Err(RetainedResourceTransportError::InvalidChannel(
                self.channel.clone(),
            ));
        }
        if self.protocol_version != RETAINED_RESOURCE_TRANSPORT_VERSION {
            return Err(RetainedResourceTransportError::UnsupportedVersion(
                self.protocol_version,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct InstalledRetainedResources {
    texts: TextResourceArena,
    geometries: GeometryResourceArena,
    fonts: FontResourceArena,
    text_handles: HashMap<TransportTextResourceHandle, TextResourceHandle>,
    render_geometry_session: Option<u32>,
    render_geometries: Arc<[Arc<GeometryRef>]>,
    render_geometry_preparations: Vec<RenderGeometryPreparation>,
}

impl InstalledRetainedResources {
    pub(crate) fn render_geometry_session(&self) -> Option<u32> {
        self.render_geometry_session
    }

    pub(crate) fn render_geometries(&self) -> Arc<[Arc<GeometryRef>]> {
        self.render_geometries.clone()
    }

    pub(crate) fn render_geometry_preparations(&self) -> &[RenderGeometryPreparation] {
        &self.render_geometry_preparations
    }

    pub fn render_geometry_preparation_count(&self) -> usize {
        self.render_geometry_preparations().len()
    }

    pub fn texts(&self) -> &TextResourceArena {
        &self.texts
    }

    pub fn geometries(&self) -> &GeometryResourceArena {
        &self.geometries
    }

    pub fn fonts(&self) -> &FontResourceArena {
        &self.fonts
    }

    pub fn resolve_text_handle(
        &self,
        transport: TransportTextResourceHandle,
    ) -> Option<TextResourceHandle> {
        self.text_handles.get(&transport).copied()
    }

    pub(crate) fn text_handle_remap(
        &self,
    ) -> HashMap<TransportTextResourceHandle, TextResourceHandle> {
        self.text_handles.clone()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedResourceTransportError {
    InvalidChannel(String),
    UnsupportedVersion(u32),
    UnknownText(TransportTextResourceHandle),
    UnknownGeometry(TransportGeometryResourceHandle),
    DuplicateText(TransportTextResourceHandle),
    DuplicateGeometry(TransportGeometryResourceHandle),
    MissingGeometry(TransportGeometryResourceHandle),
    MissingFont { face_key: String, face_index: u32 },
    InvalidText(String),
    InvalidFont(String),
    InvalidRenderGeometry(usize),
    InvalidRenderPreparation(usize),
    Encode(String),
    Decode(String),
}

impl fmt::Display for RetainedResourceTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRenderPreparation(index) => {
                write!(formatter, "invalid render geometry preparation {index}")
            }
            Self::InvalidRenderGeometry(index) => write!(
                formatter,
                "invalid compiled render geometry resource {index}"
            ),
            Self::InvalidChannel(channel) => {
                write!(formatter, "invalid retained resource channel {channel}")
            }
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported retained resource version {version}")
            }
            Self::UnknownText(handle) => write!(
                formatter,
                "unknown retained text resource {}@{}",
                handle.id, handle.version
            ),
            Self::UnknownGeometry(handle) => write!(
                formatter,
                "unknown retained geometry resource {}@{}",
                handle.id, handle.version
            ),
            Self::DuplicateText(handle) => write!(
                formatter,
                "duplicate retained text resource {}@{}",
                handle.id, handle.version
            ),
            Self::DuplicateGeometry(handle) => write!(
                formatter,
                "duplicate retained geometry resource {}@{}",
                handle.id, handle.version
            ),
            Self::MissingGeometry(handle) => write!(
                formatter,
                "missing retained geometry dependency {}@{}",
                handle.id, handle.version
            ),
            Self::MissingFont {
                face_key,
                face_index,
            } => write!(formatter, "missing retained font {face_key}#{face_index}"),
            Self::InvalidText(message) => write!(formatter, "invalid retained text: {message}"),
            Self::InvalidFont(message) => write!(formatter, "invalid retained font: {message}"),
            Self::Encode(message) => {
                write!(formatter, "retained resource encode failed: {message}")
            }
            Self::Decode(message) => {
                write!(formatter, "retained resource decode failed: {message}")
            }
        }
    }
}

impl std::error::Error for RetainedResourceTransportError {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportTextEntry {
    handle: TransportTextResourceHandle,
    resource: TransportTextResource,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportGeometryEntry {
    handle: TransportGeometryResourceHandle,
    path: VectorPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportFontEntry {
    face_key: String,
    face_index: u32,
    data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportTextResource {
    source: String,
    kind: TransportTextSourceKind,
    runs: Vec<TransportGlyphRun>,
    vector_items: Vec<TransportTextVectorItem>,
    render_items: Vec<TransportTextRenderItem>,
    parts: Vec<TransportTextPart>,
    bounds: Rect,
    baseline: f32,
    layout_artifact: Option<TransportTextLayoutArtifact>,
}

impl TransportTextResource {
    fn from_core(resource: &TextResource) -> Self {
        Self {
            source: resource.source.to_string(),
            kind: resource.kind.into(),
            runs: resource
                .runs
                .iter()
                .map(TransportGlyphRun::from_core)
                .collect(),
            vector_items: resource
                .vector_items
                .iter()
                .map(TransportTextVectorItem::from_core)
                .collect(),
            render_items: resource
                .render_items
                .iter()
                .copied()
                .map(TransportTextRenderItem::from)
                .collect(),
            parts: resource
                .parts
                .iter()
                .map(TransportTextPart::from_core)
                .collect(),
            bounds: resource.bounds,
            baseline: resource.baseline,
            layout_artifact: resource
                .layout_artifact
                .as_ref()
                .map(TransportTextLayoutArtifact::from_core),
        }
    }

    fn into_core(
        self,
        geometry_handles: &HashMap<TransportGeometryResourceHandle, GeometryResourceHandle>,
    ) -> Result<TextResource, RetainedResourceTransportError> {
        Ok(TextResource {
            source: Arc::from(self.source),
            kind: self.kind.into(),
            runs: self
                .runs
                .into_iter()
                .map(TransportGlyphRun::into_core)
                .collect::<Vec<_>>()
                .into(),
            vector_items: self
                .vector_items
                .into_iter()
                .map(|item| item.into_core(geometry_handles))
                .collect::<Result<Vec<_>, _>>()?
                .into(),
            render_items: self
                .render_items
                .into_iter()
                .map(TextRenderItem::from)
                .collect::<Vec<_>>()
                .into(),
            parts: self
                .parts
                .into_iter()
                .map(TransportTextPart::into_core)
                .collect::<Vec<_>>()
                .into(),
            bounds: self.bounds,
            baseline: self.baseline,
            layout_artifact: self
                .layout_artifact
                .map(TransportTextLayoutArtifact::into_core),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransportTextSourceKind {
    Plain,
    Markup,
    Typst,
    MathTypst,
    Tex,
    MathTex,
}

impl From<TextSourceKind> for TransportTextSourceKind {
    fn from(value: TextSourceKind) -> Self {
        match value {
            TextSourceKind::Plain => Self::Plain,
            TextSourceKind::Markup => Self::Markup,
            TextSourceKind::Typst => Self::Typst,
            TextSourceKind::MathTypst => Self::MathTypst,
            TextSourceKind::Tex => Self::Tex,
            TextSourceKind::MathTex => Self::MathTex,
        }
    }
}

impl From<TransportTextSourceKind> for TextSourceKind {
    fn from(value: TransportTextSourceKind) -> Self {
        match value {
            TransportTextSourceKind::Plain => Self::Plain,
            TransportTextSourceKind::Markup => Self::Markup,
            TransportTextSourceKind::Typst => Self::Typst,
            TransportTextSourceKind::MathTypst => Self::MathTypst,
            TransportTextSourceKind::Tex => Self::Tex,
            TransportTextSourceKind::MathTex => Self::MathTex,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportGlyphRun {
    font: TransportFontFaceIdentity,
    variations: Vec<TransportFontVariationSetting>,
    font_size: f32,
    direction: TransportTextDirection,
    fill: Option<Color>,
    stroke: Option<TransportTextGlyphStroke>,
    transform: TransportTextAffineTransform,
    glyphs: Vec<TransportPositionedGlyph>,
}

impl TransportGlyphRun {
    fn from_core(run: &GlyphRun) -> Self {
        Self {
            font: TransportFontFaceIdentity::from_core(&run.font),
            variations: run
                .variations
                .iter()
                .copied()
                .map(TransportFontVariationSetting::from)
                .collect(),
            font_size: run.font_size,
            direction: run.direction.into(),
            fill: run.fill,
            stroke: run.stroke.as_ref().map(TransportTextGlyphStroke::from_core),
            transform: run.transform.into(),
            glyphs: run
                .glyphs
                .iter()
                .map(TransportPositionedGlyph::from_core)
                .collect(),
        }
    }

    fn into_core(self) -> GlyphRun {
        GlyphRun {
            font: self.font.into_core(),
            variations: self
                .variations
                .into_iter()
                .map(FontVariationSetting::from)
                .collect::<Vec<_>>()
                .into(),
            font_size: self.font_size,
            direction: self.direction.into(),
            fill: self.fill,
            stroke: self.stroke.map(TransportTextGlyphStroke::into_core),
            transform: self.transform.into(),
            glyphs: self
                .glyphs
                .into_iter()
                .map(TransportPositionedGlyph::into_core)
                .collect::<Vec<_>>()
                .into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportFontFaceIdentity {
    family: String,
    face_key: String,
    face_index: u32,
    variation_key: String,
}

impl TransportFontFaceIdentity {
    fn from_core(face: &FontFaceIdentity) -> Self {
        Self {
            family: face.family.to_string(),
            face_key: face.face_key.to_string(),
            face_index: face.face_index,
            variation_key: face.variation_key.to_string(),
        }
    }

    fn into_core(self) -> FontFaceIdentity {
        FontFaceIdentity {
            family: Arc::from(self.family),
            face_key: Arc::from(self.face_key),
            face_index: self.face_index,
            variation_key: Arc::from(self.variation_key),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct TransportFontVariationSetting {
    tag: [u8; 4],
    value: f32,
}

impl From<FontVariationSetting> for TransportFontVariationSetting {
    fn from(value: FontVariationSetting) -> Self {
        Self {
            tag: value.tag,
            value: value.value,
        }
    }
}

impl From<TransportFontVariationSetting> for FontVariationSetting {
    fn from(value: TransportFontVariationSetting) -> Self {
        Self {
            tag: value.tag,
            value: value.value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransportTextDirection {
    LeftToRight,
    RightToLeft,
}

impl From<TextDirection> for TransportTextDirection {
    fn from(value: TextDirection) -> Self {
        match value {
            TextDirection::LeftToRight => Self::LeftToRight,
            TextDirection::RightToLeft => Self::RightToLeft,
        }
    }
}

impl From<TransportTextDirection> for TextDirection {
    fn from(value: TransportTextDirection) -> Self {
        match value {
            TransportTextDirection::LeftToRight => Self::LeftToRight,
            TransportTextDirection::RightToLeft => Self::RightToLeft,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportTextGlyphStroke {
    paint: Option<Color>,
    width: f32,
    cap: StrokeCap,
    join: StrokeJoin,
    dash_array: Vec<f32>,
    dash_phase: f32,
    miter_limit: f32,
}

impl TransportTextGlyphStroke {
    fn from_core(stroke: &TextGlyphStroke) -> Self {
        Self {
            paint: stroke.paint,
            width: stroke.width,
            cap: stroke.cap,
            join: stroke.join,
            dash_array: stroke.dash_array.as_ref().to_vec(),
            dash_phase: stroke.dash_phase,
            miter_limit: stroke.miter_limit,
        }
    }

    fn into_core(self) -> TextGlyphStroke {
        TextGlyphStroke {
            paint: self.paint,
            width: self.width,
            cap: self.cap,
            join: self.join,
            dash_array: self.dash_array.into(),
            dash_phase: self.dash_phase,
            miter_limit: self.miter_limit,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct TransportTextAffineTransform {
    xx: f32,
    yx: f32,
    xy: f32,
    yy: f32,
    tx: f32,
    ty: f32,
}

impl From<TextAffineTransform> for TransportTextAffineTransform {
    fn from(value: TextAffineTransform) -> Self {
        Self {
            xx: value.xx,
            yx: value.yx,
            xy: value.xy,
            yy: value.yy,
            tx: value.tx,
            ty: value.ty,
        }
    }
}

impl From<TransportTextAffineTransform> for TextAffineTransform {
    fn from(value: TransportTextAffineTransform) -> Self {
        Self {
            xx: value.xx,
            yx: value.yx,
            xy: value.xy,
            yy: value.yy,
            tx: value.tx,
            ty: value.ty,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportPositionedGlyph {
    glyph_id: u32,
    cluster: TransportTextClusterIdentity,
    origin: Vec2,
    advance: Vec2,
    bounds: Rect,
}

impl TransportPositionedGlyph {
    fn from_core(glyph: &PositionedGlyph) -> Self {
        Self {
            glyph_id: glyph.glyph_id,
            cluster: TransportTextClusterIdentity::from_core(&glyph.cluster),
            origin: glyph.origin,
            advance: glyph.advance,
            bounds: glyph.bounds,
        }
    }

    fn into_core(self) -> PositionedGlyph {
        PositionedGlyph {
            glyph_id: self.glyph_id,
            cluster: self.cluster.into_core(),
            origin: self.origin,
            advance: self.advance,
            bounds: self.bounds,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportTextClusterIdentity {
    source_span: TransportTextSourceSpan,
    cluster_ordinal: u32,
    semantic_key: Option<String>,
}

impl TransportTextClusterIdentity {
    fn from_core(cluster: &TextClusterIdentity) -> Self {
        Self {
            source_span: cluster.source_span.into(),
            cluster_ordinal: cluster.cluster_ordinal,
            semantic_key: cluster.semantic_key.as_deref().map(str::to_owned),
        }
    }

    fn into_core(self) -> TextClusterIdentity {
        TextClusterIdentity {
            source_span: self.source_span.into(),
            cluster_ordinal: self.cluster_ordinal,
            semantic_key: self.semantic_key.map(Arc::from),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportTextSourceSpan {
    start: u32,
    end: u32,
}

impl From<TextSourceSpan> for TransportTextSourceSpan {
    fn from(value: TextSourceSpan) -> Self {
        Self {
            start: value.start,
            end: value.end,
        }
    }
}

impl From<TransportTextSourceSpan> for TextSourceSpan {
    fn from(value: TransportTextSourceSpan) -> Self {
        TextSourceSpan::new(value.start, value.end)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportTextVectorItem {
    geometry: TransportGeometryResourceHandle,
    transform: TransportTextAffineTransform,
    style: TransportTextVectorStyle,
    source_span: Option<TransportTextSourceSpan>,
    semantic_key: Option<String>,
}

impl TransportTextVectorItem {
    fn from_core(item: &TextVectorItem) -> Self {
        Self {
            geometry: item.geometry.into(),
            transform: item.transform.into(),
            style: item.style.into(),
            source_span: item.source_span.map(Into::into),
            semantic_key: item.semantic_key.as_deref().map(str::to_owned),
        }
    }

    fn into_core(
        self,
        geometry_handles: &HashMap<TransportGeometryResourceHandle, GeometryResourceHandle>,
    ) -> Result<TextVectorItem, RetainedResourceTransportError> {
        let geometry = geometry_handles.get(&self.geometry).copied().ok_or(
            RetainedResourceTransportError::MissingGeometry(self.geometry),
        )?;
        Ok(TextVectorItem {
            geometry,
            transform: self.transform.into(),
            style: self.style.into(),
            source_span: self.source_span.map(Into::into),
            semantic_key: self.semantic_key.map(Arc::from),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct TransportTextVectorStyle {
    fill: Option<Color>,
    stroke: Option<Color>,
    stroke_width: f32,
}

impl From<TextVectorStyle> for TransportTextVectorStyle {
    fn from(value: TextVectorStyle) -> Self {
        Self {
            fill: value.fill,
            stroke: value.stroke,
            stroke_width: value.stroke_width,
        }
    }
}

impl From<TransportTextVectorStyle> for TextVectorStyle {
    fn from(value: TransportTextVectorStyle) -> Self {
        Self {
            fill: value.fill,
            stroke: value.stroke,
            stroke_width: value.stroke_width,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransportTextRenderItem {
    GlyphRun(u32),
    Vector(u32),
}

impl From<TextRenderItem> for TransportTextRenderItem {
    fn from(value: TextRenderItem) -> Self {
        match value {
            TextRenderItem::GlyphRun(index) => Self::GlyphRun(index),
            TextRenderItem::Vector(index) => Self::Vector(index),
        }
    }
}

impl From<TransportTextRenderItem> for TextRenderItem {
    fn from(value: TransportTextRenderItem) -> Self {
        match value {
            TransportTextRenderItem::GlyphRun(index) => Self::GlyphRun(index),
            TransportTextRenderItem::Vector(index) => Self::Vector(index),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportTextPart {
    source_span: TransportTextSourceSpan,
    first_cluster: u32,
    cluster_count: u32,
    first_vector: u32,
    vector_count: u32,
    semantic_key: Option<String>,
}

impl TransportTextPart {
    fn from_core(part: &TextPart) -> Self {
        Self {
            source_span: part.source_span.into(),
            first_cluster: part.first_cluster,
            cluster_count: part.cluster_count,
            first_vector: part.first_vector,
            vector_count: part.vector_count,
            semantic_key: part.semantic_key.as_deref().map(str::to_owned),
        }
    }

    fn into_core(self) -> TextPart {
        TextPart {
            source_span: self.source_span.into(),
            first_cluster: self.first_cluster,
            cluster_count: self.cluster_count,
            first_vector: self.first_vector,
            vector_count: self.vector_count,
            semantic_key: self.semantic_key.map(Arc::from),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportTextLayoutArtifact {
    backend: TransportTextLayoutBackend,
    template_fingerprint: String,
    artifact_fingerprint: String,
    backend_payload_key: Option<String>,
}

impl TransportTextLayoutArtifact {
    fn from_core(artifact: &TextLayoutArtifact) -> Self {
        Self {
            backend: TransportTextLayoutBackend::from_core(&artifact.backend),
            template_fingerprint: artifact.template_fingerprint.to_string(),
            artifact_fingerprint: artifact.artifact_fingerprint.to_string(),
            backend_payload_key: artifact.backend_payload_key.as_deref().map(str::to_owned),
        }
    }

    fn into_core(self) -> TextLayoutArtifact {
        TextLayoutArtifact {
            backend: self.backend.into_core(),
            template_fingerprint: Arc::from(self.template_fingerprint),
            artifact_fingerprint: Arc::from(self.artifact_fingerprint),
            backend_payload_key: self.backend_payload_key.map(Arc::from),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportTextLayoutBackend {
    kind: TransportTextLayoutBackendKind,
    version: String,
}

impl TransportTextLayoutBackend {
    fn from_core(backend: &TextLayoutBackend) -> Self {
        Self {
            kind: TransportTextLayoutBackendKind::from_core(&backend.kind),
            version: backend.version.to_string(),
        }
    }

    fn into_core(self) -> TextLayoutBackend {
        TextLayoutBackend {
            kind: self.kind.into_core(),
            version: Arc::from(self.version),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransportTextLayoutBackendKind {
    NativeText,
    Typst,
    Latex,
    Other(String),
}

impl TransportTextLayoutBackendKind {
    fn from_core(kind: &TextLayoutBackendKind) -> Self {
        match kind {
            TextLayoutBackendKind::NativeText => Self::NativeText,
            TextLayoutBackendKind::Typst => Self::Typst,
            TextLayoutBackendKind::Latex => Self::Latex,
            TextLayoutBackendKind::Other(name) => Self::Other(name.to_string()),
        }
    }

    fn into_core(self) -> TextLayoutBackendKind {
        match self {
            Self::NativeText => TextLayoutBackendKind::NativeText,
            Self::Typst => TextLayoutBackendKind::Typst,
            Self::Latex => TextLayoutBackendKind::Latex,
            Self::Other(name) => TextLayoutBackendKind::Other(Arc::from(name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon::{MathTypst, RetainedScene, Typst};
    use noon_core::TextSourceKind;

    fn text_handles(scene: &RetainedScene) -> Vec<TextResourceHandle> {
        scene
            .objects()
            .iter()
            .filter_map(|object| object.content.text())
            .collect()
    }

    #[test]
    fn typst_resource_bundle_round_trips_without_python_or_placeholder_geometry() {
        let mut scene = RetainedScene::new();
        scene
            .add_typst(Typst::new("*Hello* from _Typst!_"))
            .unwrap();
        scene
            .add_math_typst(MathTypst::new("frac(x, 2)").with_font_size(72.0))
            .unwrap();

        let original_handles = text_handles(&scene);
        let bundle = RetainedResourceBundle::capture(
            original_handles.iter().copied(),
            scene.texts(),
            scene.geometries(),
            scene.fonts(),
        )
        .unwrap();
        assert_eq!(bundle.text_count(), 2);
        assert!(bundle.geometry_count() >= 1);
        assert!(bundle.font_count() >= 1);
        assert!(bundle.font_bytes() > 0);

        let bytes = bundle.encode_binary().unwrap();
        let decoded = RetainedResourceBundle::decode_binary(&bytes).unwrap();
        let installed = decoded.install().unwrap();

        for original in original_handles {
            let transport = TransportTextResourceHandle::from_source_handle(original);
            let local = installed.resolve_text_handle(transport).unwrap();
            assert_ne!(original.arena, local.arena);
            assert!(installed.texts().get(original).is_none());
            let resource = installed.texts().get(local).unwrap();
            assert!(matches!(
                resource.kind,
                TextSourceKind::Typst | TextSourceKind::MathTypst
            ));
            for run in resource.runs.iter() {
                assert!(installed.fonts().get_for_face(&run.font).is_some());
            }
            for vector in resource.vector_items.iter() {
                assert!(installed.geometries().get(vector.geometry).is_some());
            }
        }
    }

    #[test]
    fn capture_is_dependency_closed_and_deduplicates_shared_font_bytes() {
        let mut scene = RetainedScene::new();
        scene.add_typst(Typst::new("A")).unwrap();
        scene.add_typst(Typst::new("B")).unwrap();
        let handles = text_handles(&scene);

        let bundle = RetainedResourceBundle::capture(
            handles,
            scene.texts(),
            scene.geometries(),
            scene.fonts(),
        )
        .unwrap();

        assert_eq!(bundle.text_count(), 2);
        assert_eq!(bundle.font_count(), 1);
    }

    #[test]
    fn protocol_rejects_wrong_channel_before_installing_resources() {
        let bundle = RetainedResourceBundle {
            channel: "noon.execution".to_owned(),
            protocol_version: RETAINED_RESOURCE_TRANSPORT_VERSION,
            texts: Vec::new(),
            geometries: Vec::new(),
            fonts: Vec::new(),
            render_geometry_resources: None,
        };
        assert!(matches!(
            bundle.install(),
            Err(RetainedResourceTransportError::InvalidChannel(_))
        ));
    }

    #[test]
    fn compiled_morph_bundle_installs_geometry_once_and_reuses_local_arcs() {
        let geometry = Arc::new(GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1.0, 0.0)),
        ));
        let mut bundle = RetainedResourceBundle::capture(
            [],
            &TextResourceArena::new(),
            &GeometryResourceArena::new(),
            &FontResourceArena::new(),
        )
        .unwrap();
        let preparations = vec![
            RenderGeometryPreparation {
                resource: 0,
                style: Style::default(),
                transform: Transform2D::IDENTITY,
            },
            RenderGeometryPreparation {
                resource: 0,
                style: Style {
                    stroke_width: 3.0,
                    ..Style::default()
                },
                transform: Transform2D::IDENTITY,
            },
        ];
        bundle.set_render_geometries(17, vec![geometry.clone()].into(), preparations.clone());
        let installed = RetainedResourceBundle::decode_binary(&bundle.encode_binary().unwrap())
            .unwrap()
            .install()
            .unwrap();
        assert_eq!(installed.render_geometry_session(), Some(17));
        assert_eq!(installed.render_geometry_preparations(), preparations);
        assert_eq!(installed.render_geometry_preparation_count(), 2);
        for invalid in [
            RenderGeometryPreparation {
                resource: 1,
                ..preparations[0].clone()
            },
            RenderGeometryPreparation {
                style: Style {
                    stroke_width: f32::NAN,
                    ..Style::default()
                },
                ..preparations[0].clone()
            },
            RenderGeometryPreparation {
                transform: Transform2D {
                    rotation: f32::INFINITY,
                    ..Transform2D::IDENTITY
                },
                ..preparations[0].clone()
            },
        ] {
            let mut invalid_bundle = bundle.clone();
            invalid_bundle
                .render_geometry_resources
                .as_mut()
                .unwrap()
                .preparations
                .push(invalid);
            let decoded =
                RetainedResourceBundle::decode_binary(&invalid_bundle.encode_binary().unwrap())
                    .unwrap();
            assert!(matches!(
                decoded.install(),
                Err(RetainedResourceTransportError::InvalidRenderPreparation(2))
            ));
        }
        let first = installed.render_geometries();
        let again = installed.render_geometries();
        assert_eq!(first[0], geometry);
        assert!(Arc::ptr_eq(&first[0], &again[0]));
        assert!(
            !Arc::ptr_eq(&first[0], &geometry),
            "cross-worker decode owns its local resource allocation"
        );
    }

    #[test]
    fn nonfinite_compiled_resource_is_rejected_before_installation() {
        let mut bundle = RetainedResourceBundle::capture(
            [],
            &TextResourceArena::new(),
            &GeometryResourceArena::new(),
            &FontResourceArena::new(),
        )
        .unwrap();
        bundle.set_render_geometries(
            17,
            vec![Arc::new(GeometryRef::path(
                VectorPath::new().move_to(Vec2::new(f32::NAN, 0.0)),
            ))]
            .into(),
            Vec::new(),
        );
        let decoded =
            RetainedResourceBundle::decode_binary(&bundle.encode_binary().unwrap()).unwrap();
        assert!(matches!(
            decoded.install(),
            Err(RetainedResourceTransportError::InvalidRenderGeometry(0))
        ));
    }
}

#[cfg(test)]
mod morph_tests;
