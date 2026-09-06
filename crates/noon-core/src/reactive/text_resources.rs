use std::{
    mem::{size_of, size_of_val},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use crate::{Color, GeometryResourceHandle, Rect, StrokeCap, StrokeJoin, Vec2};

static NEXT_TEXT_RESOURCE_ARENA: AtomicU64 = AtomicU64::new(1);

fn next_text_resource_arena() -> u64 {
    NEXT_TEXT_RESOURCE_ARENA
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("text resource arena identity exhausted")
}

const TEXT_RESOURCE_SLOT_BITS: u32 = 32;
const TEXT_RESOURCE_SLOT_MASK: u64 = u32::MAX as u64;

/// Stable identity for one retained text/math resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextResourceId(u64);

impl TextResourceId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Versioned reference to immutable shaped text/math data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextResourceHandle {
    /// Owning resource namespace; equal slot/version values from another arena are unrelated.
    pub arena: u64,
    pub id: TextResourceId,
    pub version: u64,
}

/// UTF-8 byte range in the original authoring source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextSourceSpan {
    pub start: u32,
    pub end: u32,
}

impl TextSourceSpan {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// Source-language identity is intentionally separate from layout backend identity.
/// In particular, `Tex`/`MathTex` must remain real LaTeX semantics for Manim parity;
/// they must never be silently translated to Typst.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextSourceKind {
    Plain,
    Markup,
    Typst,
    MathTypst,
    Tex,
    MathTex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextDirection {
    LeftToRight,
    RightToLeft,
}

/// Backend-independent identity for the system that produced a retained layout.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TextLayoutBackendKind {
    NativeText,
    Typst,
    Latex,
    Other(Arc<str>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextLayoutBackend {
    pub kind: TextLayoutBackendKind,
    pub version: Arc<str>,
}

/// Deterministic identity for a backend layout artifact.
///
/// Backend-owned DOM/frame/SVG/DVI payloads live outside `noon-core`; the semantic
/// core stores only stable fingerprints and normalized retained output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextLayoutArtifact {
    pub backend: TextLayoutBackend,
    pub template_fingerprint: Arc<str>,
    pub artifact_fingerprint: Arc<str>,
    pub backend_payload_key: Option<Arc<str>>,
}

/// Compatibility alias retained while #65 migrates callers from the older name.
pub type MathLayoutArtifact = TextLayoutArtifact;

/// Renderer-independent font face identity emitted by a shaping backend.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontFaceIdentity {
    pub family: Arc<str>,
    pub face_key: Arc<str>,
    pub face_index: u32,
    /// Stable cache/identity key for the exact font instance. Renderer-facing
    /// variation values are retained separately on `GlyphRun`; callers must never
    /// parse this string to recover axis coordinates.
    pub variation_key: Arc<str>,
}

/// Exact OpenType variation setting used to shape and later rasterize a glyph run.
///
/// Values use the font's design-space coordinates (for example `wght=520.5`). The
/// renderer may normalize these for its raster backend, but it must not substitute
/// default-axis values when this list is non-empty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontVariationSetting {
    pub tag: [u8; 4],
    pub value: f32,
}

/// Stable identity for one shaped cluster/part across ordinary transforms.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextClusterIdentity {
    pub source_span: TextSourceSpan,
    pub cluster_ordinal: u32,
    pub semantic_key: Option<Arc<str>>,
}

/// General 2D affine transform used by retained text layout.
///
/// Text backends can legitimately emit skewed or otherwise non-TRS groups. Keeping
/// the complete matrix at this boundary avoids either baking transforms into glyph
/// identities or silently dropping layout that cannot be represented by `Transform2D`.
/// The matrix maps `(x, y)` to `(xx*x + xy*y + tx, yx*x + yy*y + ty)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextAffineTransform {
    pub xx: f32,
    pub yx: f32,
    pub xy: f32,
    pub yy: f32,
    pub tx: f32,
    pub ty: f32,
}

impl TextAffineTransform {
    pub const IDENTITY: Self = Self {
        xx: 1.0,
        yx: 0.0,
        xy: 0.0,
        yy: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub const fn translation(x: f32, y: f32) -> Self {
        Self {
            tx: x,
            ty: y,
            ..Self::IDENTITY
        }
    }

    pub fn transform_point(self, point: Vec2) -> Vec2 {
        Vec2::new(
            self.xx * point.x + self.xy * point.y + self.tx,
            self.yx * point.x + self.yy * point.y + self.ty,
        )
    }

    pub fn transform_vector(self, vector: Vec2) -> Vec2 {
        Vec2::new(
            self.xx * vector.x + self.xy * vector.y,
            self.yx * vector.x + self.yy * vector.y,
        )
    }

    /// Compose transforms such that `self.then(next)` applies `self` first and
    /// `next` second.
    pub fn then(self, next: Self) -> Self {
        Self {
            xx: next.xx * self.xx + next.xy * self.yx,
            yx: next.yx * self.xx + next.yy * self.yx,
            xy: next.xx * self.xy + next.xy * self.yy,
            yy: next.yx * self.xy + next.yy * self.yy,
            tx: next.xx * self.tx + next.xy * self.ty + next.tx,
            ty: next.yx * self.tx + next.yy * self.ty + next.ty,
        }
    }
}

impl Default for TextAffineTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PositionedGlyph {
    pub glyph_id: u32,
    pub cluster: TextClusterIdentity,
    pub origin: Vec2,
    pub advance: Vec2,
    pub bounds: Rect,
}

/// Exact outline-stroke semantics for one shaped glyph run.
///
/// `paint == None` means inherit the owning mobject color, mirroring `GlyphRun::fill`.
/// Stroked runs require path-outline rendering: bitmap/atlas rasterization cannot
/// reproduce dash, cap, join, or miter behavior without changing observable output.
#[derive(Clone, Debug, PartialEq)]
pub struct TextGlyphStroke {
    pub paint: Option<Color>,
    pub width: f32,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub dash_array: Arc<[f32]>,
    pub dash_phase: f32,
    pub miter_limit: f32,
}

/// One shaped run. `fill == None` means inherit the owning mobject's color;
/// `Some` preserves an intrinsic backend color (for example styled Typst content).
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    pub font: FontFaceIdentity,
    /// Exact variable-font design coordinates used by the shaping backend.
    pub variations: Arc<[FontVariationSetting]>,
    pub font_size: f32,
    pub direction: TextDirection,
    pub fill: Option<Color>,
    /// Exact outline stroke requested by the layout backend. Any `Some` value
    /// routes the run through lazy glyph outlines instead of the steady atlas path.
    pub stroke: Option<TextGlyphStroke>,
    /// Maps the run's backend-local glyph coordinates into the resource coordinate
    /// system. Ordinary native text uses identity; math/layout engines can retain
    /// arbitrary group transforms without outlining every glyph.
    pub transform: TextAffineTransform,
    pub glyphs: Arc<[PositionedGlyph]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextVectorStyle {
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_width: f32,
}

impl Default for TextVectorStyle {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: None,
            stroke_width: 0.0,
        }
    }
}

/// Non-glyph vector content emitted by a text/math layout backend.
///
/// Fraction rules, radical decorations, Typst shapes, and similar content are
/// immutable geometry resources rather than fake glyphs. This keeps rendering and
/// path animation/morphing on Noon's shared geometry pipeline.
#[derive(Clone, Debug, PartialEq)]
pub struct TextVectorItem {
    pub geometry: GeometryResourceHandle,
    pub transform: TextAffineTransform,
    pub style: TextVectorStyle,
    pub source_span: Option<TextSourceSpan>,
    pub semantic_key: Option<Arc<str>>,
}

/// One entry in the layout backend's original painter-order stream.
///
/// Runs and vectors live in separate immutable arrays for efficient batching and
/// semantic lookup, while this compact stream preserves their observable z-order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextRenderItem {
    GlyphRun(u32),
    Vector(u32),
}

/// Logical source part independently addressable by public text/math APIs.
/// Ranges refer to flattened glyph-cluster and vector-item order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextPart {
    pub source_span: TextSourceSpan,
    pub first_cluster: u32,
    pub cluster_count: u32,
    pub first_vector: u32,
    pub vector_count: u32,
    pub semantic_key: Option<Arc<str>>,
}

/// Immutable renderer-independent shaped text/math payload.
#[derive(Clone, Debug, PartialEq)]
pub struct TextResource {
    pub source: Arc<str>,
    pub kind: TextSourceKind,
    pub runs: Arc<[GlyphRun]>,
    pub vector_items: Arc<[TextVectorItem]>,
    /// Backend painter order across shaped runs and non-glyph vector items.
    pub render_items: Arc<[TextRenderItem]>,
    pub parts: Arc<[TextPart]>,
    pub bounds: Rect,
    pub baseline: f32,
    pub layout_artifact: Option<TextLayoutArtifact>,
}

impl TextResource {
    pub fn glyph_count(&self) -> usize {
        self.runs.iter().map(|run| run.glyphs.len()).sum()
    }

    pub fn cluster_count(&self) -> usize {
        self.glyph_count()
    }

    pub fn vector_count(&self) -> usize {
        self.vector_items.len()
    }

    /// Validate backend-normalized references before the resource enters the arena.
    pub fn validate(&self) -> Result<(), TextResourceValidationError> {
        let source_len = u32::try_from(self.source.len()).unwrap_or(u32::MAX);
        let clusters = u32::try_from(self.cluster_count()).unwrap_or(u32::MAX);
        let vectors = u32::try_from(self.vector_count()).unwrap_or(u32::MAX);

        for run in self.runs.iter() {
            if run
                .variations
                .iter()
                .any(|setting| !setting.value.is_finite())
            {
                return Err(TextResourceValidationError::InvalidFontVariation);
            }
            if let Some(stroke) = &run.stroke {
                let invalid = !stroke.width.is_finite()
                    || stroke.width < 0.0
                    || !stroke.dash_phase.is_finite()
                    || !stroke.miter_limit.is_finite()
                    || stroke.miter_limit < 0.0
                    || stroke
                        .dash_array
                        .iter()
                        .any(|length| !length.is_finite() || *length < 0.0);
                if invalid {
                    return Err(TextResourceValidationError::InvalidGlyphStroke);
                }
            }
            for glyph in run.glyphs.iter() {
                validate_span(glyph.cluster.source_span, source_len)?;
            }
        }

        for item in self.vector_items.iter() {
            if let Some(span) = item.source_span {
                validate_span(span, source_len)?;
            }
        }

        let mut seen_runs = vec![false; self.runs.len()];
        let mut seen_vectors = vec![false; self.vector_items.len()];
        for item in self.render_items.iter().copied() {
            let seen = match item {
                TextRenderItem::GlyphRun(index) => seen_runs.get_mut(index as usize),
                TextRenderItem::Vector(index) => seen_vectors.get_mut(index as usize),
            }
            .ok_or(TextResourceValidationError::InvalidRenderItem)?;
            if std::mem::replace(seen, true) {
                return Err(TextResourceValidationError::DuplicateRenderItem);
            }
        }
        if seen_runs.iter().any(|seen| !seen) || seen_vectors.iter().any(|seen| !seen) {
            return Err(TextResourceValidationError::MissingRenderItem);
        }

        for part in self.parts.iter() {
            validate_span(part.source_span, source_len)?;
            validate_range(part.first_cluster, part.cluster_count, clusters)
                .map_err(|_| TextResourceValidationError::InvalidClusterRange)?;
            validate_range(part.first_vector, part.vector_count, vectors)
                .map_err(|_| TextResourceValidationError::InvalidVectorRange)?;
        }

        Ok(())
    }

    /// Deterministic retained-memory estimate used for architecture/perf tests.
    pub fn retained_bytes(&self) -> usize {
        let mut bytes = size_of::<Self>() + self.source.len();
        bytes = bytes
            .saturating_add(size_of_val(self.runs.as_ref()))
            .saturating_add(size_of_val(self.vector_items.as_ref()))
            .saturating_add(size_of_val(self.render_items.as_ref()))
            .saturating_add(size_of_val(self.parts.as_ref()));

        for run in self.runs.iter() {
            bytes = bytes
                .saturating_add(run.font.family.len())
                .saturating_add(run.font.face_key.len())
                .saturating_add(run.font.variation_key.len())
                .saturating_add(size_of_val(run.variations.as_ref()))
                .saturating_add(size_of_val(run.glyphs.as_ref()));
            if let Some(stroke) = &run.stroke {
                bytes = bytes.saturating_add(size_of_val(stroke.dash_array.as_ref()));
            }
            for glyph in run.glyphs.iter() {
                if let Some(key) = &glyph.cluster.semantic_key {
                    bytes = bytes.saturating_add(key.len());
                }
            }
        }

        for item in self.vector_items.iter() {
            if let Some(key) = &item.semantic_key {
                bytes = bytes.saturating_add(key.len());
            }
        }

        for part in self.parts.iter() {
            if let Some(key) = &part.semantic_key {
                bytes = bytes.saturating_add(key.len());
            }
        }

        if let Some(layout) = &self.layout_artifact {
            bytes = bytes
                .saturating_add(layout.backend.version.len())
                .saturating_add(layout.template_fingerprint.len())
                .saturating_add(layout.artifact_fingerprint.len());
            if let TextLayoutBackendKind::Other(name) = &layout.backend.kind {
                bytes = bytes.saturating_add(name.len());
            }
            if let Some(key) = &layout.backend_payload_key {
                bytes = bytes.saturating_add(key.len());
            }
        }

        bytes
    }
}

fn validate_span(span: TextSourceSpan, source_len: u32) -> Result<(), TextResourceValidationError> {
    if span.start > span.end || span.end > source_len {
        return Err(TextResourceValidationError::InvalidSourceSpan);
    }
    Ok(())
}

fn validate_range(first: u32, count: u32, total: u32) -> Result<(), ()> {
    first
        .checked_add(count)
        .filter(|end| *end <= total)
        .map(|_| ())
        .ok_or(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextResourceValidationError {
    InvalidSourceSpan,
    InvalidClusterRange,
    InvalidVectorRange,
    InvalidFontVariation,
    InvalidGlyphStroke,
    InvalidRenderItem,
    DuplicateRenderItem,
    MissingRenderItem,
}

impl std::fmt::Display for TextResourceValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSourceSpan => write!(formatter, "invalid text source span"),
            Self::InvalidClusterRange => write!(formatter, "invalid text cluster range"),
            Self::InvalidVectorRange => write!(formatter, "invalid text vector range"),
            Self::InvalidFontVariation => write!(formatter, "invalid font variation value"),
            Self::InvalidGlyphStroke => write!(formatter, "invalid glyph stroke value"),
            Self::InvalidRenderItem => write!(formatter, "invalid text render item reference"),
            Self::DuplicateRenderItem => write!(formatter, "duplicate text render item reference"),
            Self::MissingRenderItem => write!(formatter, "missing text render item reference"),
        }
    }
}

impl std::error::Error for TextResourceValidationError {}

/// Key for lazy path-outline extraction of one glyph.
///
/// Normal text rendering must not require this. `Write`, `Create`, path morphing and
/// matching can request/cache outlines separately and store those paths in the shared
/// geometry arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GlyphOutlineKey {
    pub text: TextResourceHandle,
    pub run_index: u32,
    pub glyph_index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphOutlineResource {
    pub key: GlyphOutlineKey,
    pub geometry: GeometryResourceHandle,
}

#[derive(Clone, Debug)]
struct TextResourceEntry {
    generation: u32,
    version: u64,
    value: Option<Arc<TextResource>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextResourceStats {
    pub live_resources: usize,
    pub retained_bytes: usize,
    pub glyphs: usize,
    pub vectors: usize,
    pub parts: usize,
}

/// Stable-ID arena for immutable shaped text/math payloads.
///
/// `TextResourceId` encodes a physical slot plus its generation. This lets the arena
/// reuse storage at the live-working-set high-water mark without allowing a stale
/// bare ID to alias a later occupant of the same slot.
#[derive(Clone, Debug)]
pub struct TextResourceArena {
    namespace: u64,
    entries: Vec<TextResourceEntry>,
    free_slots: Vec<u32>,
    live_resources: usize,
    retained_bytes: usize,
    glyphs: usize,
    vectors: usize,
    parts: usize,
}

impl Default for TextResourceArena {
    fn default() -> Self {
        Self {
            namespace: next_text_resource_arena(),
            entries: Vec::new(),
            free_slots: Vec::new(),
            live_resources: 0,
            retained_bytes: 0,
            glyphs: 0,
            vectors: 0,
            parts: 0,
        }
    }
}

impl TextResourceArena {
    /// Assign a cloned independent store a distinct resource namespace while retaining
    /// the immutable payload allocations shared by its `Arc` entries.
    pub(crate) fn fork_namespace(&mut self) -> u64 {
        self.namespace = next_text_resource_arena();
        self.namespace
    }

    /// Rebind resource dependencies only while cloning an independent store.
    /// Live resource replacement continues to use versioned publication.
    pub(crate) fn remap_geometry_handles(
        &mut self,
        mut remap: impl FnMut(&mut crate::GeometryResourceHandle),
    ) {
        for entry in &mut self.entries {
            let Some(resource) = entry.value.as_mut() else {
                continue;
            };
            if resource.vector_items.is_empty() {
                continue;
            }
            for item in
                std::sync::Arc::make_mut(&mut std::sync::Arc::make_mut(resource).vector_items)
            {
                remap(&mut item.geometry);
            }
        }
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        resource: TextResource,
    ) -> Result<TextResourceHandle, TextResourceValidationError> {
        resource.validate()?;
        let resource = Arc::new(resource);
        let handle = if let Some(slot) = self.free_slots.pop() {
            let index = slot as usize;
            let entry = &mut self.entries[index];
            debug_assert!(entry.value.is_none());
            entry.version = 0;
            entry.value = Some(resource.clone());
            TextResourceHandle {
                arena: self.namespace,
                id: text_resource_id(index, entry.generation),
                version: 0,
            }
        } else {
            let index = self.entries.len();
            let id = text_resource_id(index, 0);
            self.entries.push(TextResourceEntry {
                generation: 0,
                version: 0,
                value: Some(resource.clone()),
            });
            TextResourceHandle {
                arena: self.namespace,
                id,
                version: 0,
            }
        };

        self.add_stats(resource.as_ref());
        self.live_resources += 1;
        Ok(handle)
    }

    pub fn get(&self, handle: TextResourceHandle) -> Option<&TextResource> {
        if handle.arena != self.namespace {
            return None;
        }
        let index = text_resource_slot(handle.id);
        let entry = self.entries.get(index)?;
        if text_resource_id(index, entry.generation) != handle.id || entry.version != handle.version
        {
            return None;
        }
        entry.value.as_deref()
    }

    /// Share one immutable payload with a derived compiled resource snapshot.
    pub fn get_shared(&self, handle: TextResourceHandle) -> Option<Arc<TextResource>> {
        if handle.arena != self.namespace {
            return None;
        }
        let index = text_resource_slot(handle.id);
        let entry = self.entries.get(index)?;
        if text_resource_id(index, entry.generation) != handle.id || entry.version != handle.version
        {
            return None;
        }
        entry.value.clone()
    }

    pub fn current_handle(&self, id: TextResourceId) -> Option<TextResourceHandle> {
        let index = text_resource_slot(id);
        let entry = self.entries.get(index)?;
        if text_resource_id(index, entry.generation) != id {
            return None;
        }
        entry.value.as_ref()?;
        Some(TextResourceHandle {
            arena: self.namespace,
            id,
            version: entry.version,
        })
    }

    pub fn replace(
        &mut self,
        id: TextResourceId,
        resource: TextResource,
    ) -> Result<TextResourceHandle, TextResourceError> {
        resource
            .validate()
            .map_err(TextResourceError::InvalidResource)?;
        let index = text_resource_slot(id);
        let (version, previous) = {
            let entry = self
                .entries
                .get_mut(index)
                .filter(|entry| text_resource_id(index, entry.generation) == id)
                .ok_or(TextResourceError::UnknownResource(id))?;
            entry
                .value
                .as_ref()
                .ok_or(TextResourceError::UnknownResource(id))?;
            let version = entry
                .version
                .checked_add(1)
                .ok_or(TextResourceError::VersionExhausted(id))?;
            let previous = entry
                .value
                .take()
                .expect("text resource presence was preflighted");
            entry.version = version;
            (version, previous)
        };

        self.subtract_stats(previous.as_ref());
        self.add_stats(&resource);
        self.entries[index].value = Some(Arc::new(resource));
        Ok(TextResourceHandle {
            arena: self.namespace,
            id,
            version,
        })
    }

    pub fn remove(&mut self, id: TextResourceId) -> Result<Arc<TextResource>, TextResourceError> {
        let index = text_resource_slot(id);
        let (resource, next_generation) = {
            let entry = self
                .entries
                .get_mut(index)
                .filter(|entry| text_resource_id(index, entry.generation) == id)
                .ok_or(TextResourceError::UnknownResource(id))?;
            entry
                .value
                .as_ref()
                .ok_or(TextResourceError::UnknownResource(id))?;
            let version = entry
                .version
                .checked_add(1)
                .ok_or(TextResourceError::VersionExhausted(id))?;
            let resource = entry
                .value
                .take()
                .expect("text resource presence was preflighted");
            entry.version = version;
            (resource, entry.generation.checked_add(1))
        };

        self.subtract_stats(resource.as_ref());
        self.live_resources -= 1;

        // Generation wrap must never make an old bare TextResourceId valid again.
        // A fully exhausted physical slot is therefore retired instead of recycled.
        if let Some(next_generation) = next_generation {
            self.entries[index].generation = next_generation;
            self.free_slots
                .push(u32::try_from(index).expect("Noon text resource slot space exhausted"));
        }

        Ok(resource)
    }

    pub const fn stats(&self) -> TextResourceStats {
        TextResourceStats {
            live_resources: self.live_resources,
            retained_bytes: self.retained_bytes,
            glyphs: self.glyphs,
            vectors: self.vectors,
            parts: self.parts,
        }
    }

    /// Number of physical arena slots retained at the current high-water mark.
    pub fn slot_capacity(&self) -> usize {
        self.entries.len()
    }

    pub const fn len(&self) -> usize {
        self.live_resources
    }

    pub const fn is_empty(&self) -> bool {
        self.live_resources == 0
    }

    fn add_stats(&mut self, resource: &TextResource) {
        self.retained_bytes = self
            .retained_bytes
            .saturating_add(resource.retained_bytes());
        self.glyphs = self.glyphs.saturating_add(resource.glyph_count());
        self.vectors = self.vectors.saturating_add(resource.vector_count());
        self.parts = self.parts.saturating_add(resource.parts.len());
    }

    fn subtract_stats(&mut self, resource: &TextResource) {
        self.retained_bytes = self
            .retained_bytes
            .saturating_sub(resource.retained_bytes());
        self.glyphs = self.glyphs.saturating_sub(resource.glyph_count());
        self.vectors = self.vectors.saturating_sub(resource.vector_count());
        self.parts = self.parts.saturating_sub(resource.parts.len());
    }
}

fn text_resource_id(slot: usize, generation: u32) -> TextResourceId {
    let slot = u32::try_from(slot).expect("Noon text resource slot space exhausted");
    TextResourceId::new((u64::from(generation) << TEXT_RESOURCE_SLOT_BITS) | u64::from(slot))
}

fn text_resource_slot(id: TextResourceId) -> usize {
    (id.get() & TEXT_RESOURCE_SLOT_MASK) as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextResourceError {
    UnknownResource(TextResourceId),
    VersionExhausted(TextResourceId),
    InvalidResource(TextResourceValidationError),
}

impl std::fmt::Display for TextResourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownResource(id) => write!(formatter, "unknown text resource {}", id.get()),
            Self::VersionExhausted(id) => {
                write!(
                    formatter,
                    "text resource {} version space exhausted",
                    id.get()
                )
            }
            Self::InvalidResource(error) => write!(formatter, "invalid text resource: {error}"),
        }
    }
}

impl std::error::Error for TextResourceError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GeometryId, GeometryResourceHandle};

    fn sample_text(source: &str) -> TextResource {
        let glyphs: Arc<[PositionedGlyph]> = source
            .char_indices()
            .enumerate()
            .map(|(index, (byte, character))| {
                let end = byte + character.len_utf8();
                PositionedGlyph {
                    glyph_id: character as u32,
                    cluster: TextClusterIdentity {
                        source_span: TextSourceSpan::new(byte as u32, end as u32),
                        cluster_ordinal: index as u32,
                        semantic_key: None,
                    },
                    origin: Vec2::new(index as f32, 0.0),
                    advance: Vec2::new(1.0, 0.0),
                    bounds: Rect::new(
                        Vec2::new(index as f32, -0.2),
                        Vec2::new(index as f32 + 0.8, 0.8),
                    ),
                }
            })
            .collect::<Vec<_>>()
            .into();
        let glyph_count = glyphs.len() as u32;

        TextResource {
            source: Arc::from(source),
            kind: TextSourceKind::Plain,
            runs: Arc::from([GlyphRun {
                font: FontFaceIdentity {
                    family: Arc::from("Test Sans"),
                    face_key: Arc::from("test-sans-v1"),
                    face_index: 0,
                    variation_key: Arc::from(""),
                },
                variations: Arc::from([]),
                font_size: 48.0,
                direction: TextDirection::LeftToRight,
                fill: None,
                stroke: None,
                transform: TextAffineTransform::IDENTITY,
                glyphs,
            }]),
            vector_items: Arc::from([]),
            render_items: Arc::from([TextRenderItem::GlyphRun(0)]),
            parts: Arc::from([TextPart {
                source_span: TextSourceSpan::new(0, source.len() as u32),
                first_cluster: 0,
                cluster_count: glyph_count,
                first_vector: 0,
                vector_count: 0,
                semantic_key: None,
            }]),
            bounds: Rect::new(Vec2::new(0.0, -0.2), Vec2::new(source.len() as f32, 0.8)),
            baseline: 0.0,
            layout_artifact: None,
        }
    }

    #[test]
    fn many_snapshots_share_one_small_text_handle() {
        let mut arena = TextResourceArena::new();
        let handle = arena.insert(sample_text("hello")).unwrap();
        let snapshots = vec![handle; 100_000];
        assert_eq!(snapshots.len(), 100_000);
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.stats().glyphs, 5);
        assert_eq!(arena.get(handle).unwrap().source.as_ref(), "hello");
    }

    #[test]
    fn same_slot_handle_from_another_arena_is_rejected() {
        let mut first = TextResourceArena::new();
        let mut second = TextResourceArena::new();
        let a = first.insert(sample_text("first")).unwrap();
        let b = second.insert(sample_text("second")).unwrap();

        assert_eq!((a.id, a.version), (b.id, b.version));
        assert_ne!(a.arena, b.arena);
        assert!(first.get(b).is_none());
        assert!(second.get(a).is_none());
    }

    #[test]
    fn cloned_arena_requires_local_remap_but_shares_immutable_payloads() {
        let mut source = TextResourceArena::new();
        let source_handle = source.insert(sample_text("shared")).unwrap();
        let source_payload = source.get_shared(source_handle).unwrap();

        let mut cloned = source.clone();
        cloned.fork_namespace();
        let local_handle = cloned.current_handle(source_handle.id).unwrap();
        let local_payload = cloned.get_shared(local_handle).unwrap();

        assert_ne!(source_handle.arena, local_handle.arena);
        assert!(cloned.get(source_handle).is_none());
        assert!(source.get(local_handle).is_none());
        assert!(Arc::ptr_eq(&source_payload, &local_payload));
    }

    #[test]
    fn replacement_preserves_id_and_invalidates_old_version() {
        let mut arena = TextResourceArena::new();
        let first = arena.insert(sample_text("x")).unwrap();
        let second = arena.replace(first.id, sample_text("x^2")).unwrap();
        assert_eq!(first.id, second.id);
        assert_ne!(first.version, second.version);
        assert!(arena.get(first).is_none());
        assert_eq!(arena.get(second).unwrap().source.as_ref(), "x^2");
        assert_eq!(arena.stats().glyphs, 3);
    }

    #[test]
    fn removed_slot_is_reused_without_revalidating_stale_id() {
        let mut arena = TextResourceArena::new();
        let first = arena.insert(sample_text("old")).unwrap();
        assert_eq!(arena.slot_capacity(), 1);
        arena.remove(first.id).unwrap();

        let second = arena.insert(sample_text("new")).unwrap();
        assert_eq!(arena.slot_capacity(), 1);
        assert_ne!(first.id, second.id);
        assert!(arena.get(first).is_none());
        assert!(arena.current_handle(first.id).is_none());
        assert_eq!(arena.get(second).unwrap().source.as_ref(), "new");
    }

    #[test]
    fn replacement_version_exhaustion_leaves_resource_and_stats_unchanged() {
        let mut arena = TextResourceArena::new();
        let inserted = arena.insert(sample_text("x")).unwrap();
        arena.entries[text_resource_slot(inserted.id)].version = u64::MAX;
        let current = arena.current_handle(inserted.id).unwrap();
        let stats = arena.stats();

        assert_eq!(
            arena.replace(inserted.id, sample_text("replacement")),
            Err(TextResourceError::VersionExhausted(inserted.id))
        );
        assert_eq!(arena.current_handle(inserted.id), Some(current));
        assert_eq!(arena.stats(), stats);
        assert_eq!(arena.get(current).unwrap().source.as_ref(), "x");
    }

    #[test]
    fn removal_version_exhaustion_leaves_resource_and_stats_unchanged() {
        let mut arena = TextResourceArena::new();
        let inserted = arena.insert(sample_text("xyz")).unwrap();
        arena.entries[text_resource_slot(inserted.id)].version = u64::MAX;
        let current = arena.current_handle(inserted.id).unwrap();
        let stats = arena.stats();

        assert!(matches!(
            arena.remove(inserted.id),
            Err(TextResourceError::VersionExhausted(id)) if id == inserted.id
        ));
        assert_eq!(arena.current_handle(inserted.id), Some(current));
        assert_eq!(arena.stats(), stats);
        assert_eq!(arena.get(current).unwrap().source.as_ref(), "xyz");
    }

    #[test]
    fn exhausted_generation_retires_slot_instead_of_wrapping_identity() {
        let mut arena = TextResourceArena::new();
        let inserted = arena.insert(sample_text("old")).unwrap();
        let index = text_resource_slot(inserted.id);
        arena.entries[index].generation = u32::MAX;
        let exhausted_id = text_resource_id(index, u32::MAX);
        let exhausted_handle = arena.current_handle(exhausted_id).unwrap();

        arena.remove(exhausted_id).unwrap();
        assert!(arena.get(exhausted_handle).is_none());
        assert!(arena.current_handle(exhausted_id).is_none());

        let replacement = arena.insert(sample_text("new")).unwrap();
        assert_eq!(arena.slot_capacity(), 2);
        assert_ne!(replacement.id, exhausted_id);
        assert_eq!(arena.get(replacement).unwrap().source.as_ref(), "new");
    }

    #[test]
    fn typst_and_latex_are_distinct_source_and_backend_semantics() {
        let typst = TextLayoutArtifact {
            backend: TextLayoutBackend {
                kind: TextLayoutBackendKind::Typst,
                version: Arc::from("0.15.1"),
            },
            template_fingerprint: Arc::from("typst-template-v1"),
            artifact_fingerprint: Arc::from("artifact-a"),
            backend_payload_key: None,
        };
        let latex = TextLayoutArtifact {
            backend: TextLayoutBackend {
                kind: TextLayoutBackendKind::Latex,
                version: Arc::from("latex-profile"),
            },
            template_fingerprint: Arc::from("latex-template-v1"),
            artifact_fingerprint: Arc::from("artifact-b"),
            backend_payload_key: None,
        };
        assert_ne!(typst.backend.kind, latex.backend.kind);
        assert_ne!(TextSourceKind::MathTypst, TextSourceKind::MathTex);
    }

    #[test]
    fn affine_text_transform_preserves_skew() {
        let transform = TextAffineTransform {
            xx: 1.0,
            yx: 0.25,
            xy: -0.5,
            yy: 1.0,
            tx: 3.0,
            ty: -2.0,
        };
        assert_eq!(
            transform.transform_point(Vec2::new(2.0, 4.0)),
            Vec2::new(3.0, 2.5)
        );
        assert_eq!(
            transform.transform_vector(Vec2::new(2.0, 4.0)),
            Vec2::new(0.0, 4.5)
        );
    }

    #[test]
    fn vector_math_items_share_geometry_resource_handles() {
        let geometry = GeometryResourceHandle {
            arena: 0,
            id: GeometryId::new(7),
            version: 2,
        };
        let item = TextVectorItem {
            geometry,
            transform: TextAffineTransform::IDENTITY,
            style: TextVectorStyle::default(),
            source_span: Some(TextSourceSpan::new(0, 1)),
            semantic_key: Some(Arc::from("fraction-rule")),
        };
        assert_eq!(item.geometry, geometry);
    }

    #[test]
    fn render_items_preserve_interleaved_painter_order() {
        let mut resource = sample_text("x");
        resource.vector_items = Arc::from([TextVectorItem {
            geometry: GeometryResourceHandle {
                arena: 0,
                id: GeometryId::new(8),
                version: 0,
            },
            transform: TextAffineTransform::IDENTITY,
            style: TextVectorStyle::default(),
            source_span: None,
            semantic_key: Some(Arc::from("background")),
        }]);
        resource.render_items = Arc::from([TextRenderItem::Vector(0), TextRenderItem::GlyphRun(0)]);
        assert_eq!(resource.validate(), Ok(()));
        assert_eq!(
            resource.render_items.as_ref(),
            &[TextRenderItem::Vector(0), TextRenderItem::GlyphRun(0)]
        );
    }

    #[test]
    fn invalid_render_item_references_are_rejected() {
        let mut resource = sample_text("x");
        resource.render_items = Arc::from([TextRenderItem::GlyphRun(1)]);
        assert_eq!(
            resource.validate().unwrap_err(),
            TextResourceValidationError::InvalidRenderItem
        );
    }

    #[test]
    fn non_finite_font_variations_are_rejected() {
        let mut resource = sample_text("x");
        resource.runs = Arc::from([GlyphRun {
            variations: Arc::from([FontVariationSetting {
                tag: *b"wght",
                value: f32::NAN,
            }]),
            ..resource.runs[0].clone()
        }]);
        assert_eq!(
            resource.validate().unwrap_err(),
            TextResourceValidationError::InvalidFontVariation
        );
    }

    #[test]
    fn glyph_stroke_semantics_are_retained_and_validated() {
        let mut resource = sample_text("x");
        resource.runs = Arc::from([GlyphRun {
            stroke: Some(TextGlyphStroke {
                paint: Some(Color::RED),
                width: 1.5,
                cap: StrokeCap::Butt,
                join: StrokeJoin::Miter,
                dash_array: Arc::from([3.0, 2.0]),
                dash_phase: 0.5,
                miter_limit: 4.0,
            }),
            ..resource.runs[0].clone()
        }]);
        assert_eq!(resource.validate(), Ok(()));
        let stroke = resource.runs[0].stroke.as_ref().unwrap();
        assert_eq!(stroke.paint, Some(Color::RED));
        assert_eq!(stroke.dash_array.as_ref(), &[3.0, 2.0]);
    }

    #[test]
    fn non_finite_glyph_strokes_are_rejected() {
        let mut resource = sample_text("x");
        resource.runs = Arc::from([GlyphRun {
            stroke: Some(TextGlyphStroke {
                paint: None,
                width: 1.0,
                cap: StrokeCap::Round,
                join: StrokeJoin::Round,
                dash_array: Arc::from([f32::NAN]),
                dash_phase: 0.0,
                miter_limit: 4.0,
            }),
            ..resource.runs[0].clone()
        }]);
        assert_eq!(
            resource.validate().unwrap_err(),
            TextResourceValidationError::InvalidGlyphStroke
        );
    }

    #[test]
    fn invalid_part_ranges_are_rejected_before_arena_insertion() {
        let mut resource = sample_text("x");
        resource.parts = Arc::from([TextPart {
            source_span: TextSourceSpan::new(0, 1),
            first_cluster: 1,
            cluster_count: 1,
            first_vector: 0,
            vector_count: 0,
            semantic_key: None,
        }]);
        let mut arena = TextResourceArena::new();
        assert_eq!(
            arena.insert(resource).unwrap_err(),
            TextResourceValidationError::InvalidClusterRange
        );
    }

    #[test]
    fn glyph_outlines_are_separate_lazy_geometry_resources() {
        let text = TextResourceHandle {
            arena: 0,
            id: TextResourceId::new(3),
            version: 1,
        };
        let geometry = GeometryResourceHandle {
            arena: 0,
            id: GeometryId::new(12),
            version: 4,
        };
        let outline = GlyphOutlineResource {
            key: GlyphOutlineKey {
                text,
                run_index: 0,
                glyph_index: 2,
            },
            geometry,
        };
        assert_eq!(outline.key.text, text);
        assert_eq!(outline.geometry, geometry);
    }
}
