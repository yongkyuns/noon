use std::{
    mem::{size_of, size_of_val},
    sync::Arc,
};

use crate::{Color, GeometryResourceHandle, Rect, Transform2D, Vec2};

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
    pub variation_key: Arc<str>,
}

/// Stable identity for one shaped cluster/part across ordinary transforms.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextClusterIdentity {
    pub source_span: TextSourceSpan,
    pub cluster_ordinal: u32,
    pub semantic_key: Option<Arc<str>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PositionedGlyph {
    pub glyph_id: u32,
    pub cluster: TextClusterIdentity,
    pub origin: Vec2,
    pub advance: Vec2,
    pub bounds: Rect,
}

/// One shaped run. `fill == None` means inherit the owning mobject's color;
/// `Some` preserves an intrinsic backend color (for example styled Typst content).
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    pub font: FontFaceIdentity,
    pub font_size: f32,
    pub direction: TextDirection,
    pub fill: Option<Color>,
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
    pub transform: Transform2D,
    pub style: TextVectorStyle,
    pub source_span: Option<TextSourceSpan>,
    pub semantic_key: Option<Arc<str>>,
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
            for glyph in run.glyphs.iter() {
                validate_span(glyph.cluster.source_span, source_len)?;
            }
        }

        for item in self.vector_items.iter() {
            if let Some(span) = item.source_span {
                validate_span(span, source_len)?;
            }
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
            .saturating_add(size_of_val(self.parts.as_ref()));

        for run in self.runs.iter() {
            bytes = bytes
                .saturating_add(run.font.family.len())
                .saturating_add(run.font.face_key.len())
                .saturating_add(run.font.variation_key.len())
                .saturating_add(size_of_val(run.glyphs.as_ref()));
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
}

impl std::fmt::Display for TextResourceValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSourceSpan => write!(formatter, "invalid text source span"),
            Self::InvalidClusterRange => write!(formatter, "invalid text cluster range"),
            Self::InvalidVectorRange => write!(formatter, "invalid text vector range"),
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
#[derive(Clone, Debug, Default)]
pub struct TextResourceArena {
    entries: Vec<TextResourceEntry>,
    live_resources: usize,
    retained_bytes: usize,
    glyphs: usize,
    vectors: usize,
    parts: usize,
}

impl TextResourceArena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        resource: TextResource,
    ) -> Result<TextResourceHandle, TextResourceValidationError> {
        resource.validate()?;
        let id = TextResourceId::new(
            u64::try_from(self.entries.len()).expect("Noon text resource ID space exhausted"),
        );
        self.add_stats(&resource);
        self.entries.push(TextResourceEntry {
            version: 0,
            value: Some(Arc::new(resource)),
        });
        self.live_resources += 1;
        Ok(TextResourceHandle { id, version: 0 })
    }

    pub fn get(&self, handle: TextResourceHandle) -> Option<&TextResource> {
        let entry = self.entries.get(handle.id.get() as usize)?;
        if entry.version != handle.version {
            return None;
        }
        entry.value.as_deref()
    }

    pub fn current_handle(&self, id: TextResourceId) -> Option<TextResourceHandle> {
        let entry = self.entries.get(id.get() as usize)?;
        entry.value.as_ref()?;
        Some(TextResourceHandle {
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
        let index = id.get() as usize;
        let (version, previous) = {
            let entry = self
                .entries
                .get_mut(index)
                .ok_or(TextResourceError::UnknownResource(id))?;
            let previous = entry
                .value
                .take()
                .ok_or(TextResourceError::UnknownResource(id))?;
            entry.version = entry
                .version
                .checked_add(1)
                .ok_or(TextResourceError::VersionExhausted(id))?;
            (entry.version, previous)
        };

        self.subtract_stats(previous.as_ref());
        self.add_stats(&resource);
        self.entries[index].value = Some(Arc::new(resource));
        Ok(TextResourceHandle { id, version })
    }

    pub fn remove(&mut self, id: TextResourceId) -> Result<Arc<TextResource>, TextResourceError> {
        let index = id.get() as usize;
        let resource = {
            let entry = self
                .entries
                .get_mut(index)
                .ok_or(TextResourceError::UnknownResource(id))?;
            let resource = entry
                .value
                .take()
                .ok_or(TextResourceError::UnknownResource(id))?;
            entry.version = entry
                .version
                .checked_add(1)
                .ok_or(TextResourceError::VersionExhausted(id))?;
            resource
        };

        self.subtract_stats(resource.as_ref());
        self.live_resources -= 1;
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
                font_size: 48.0,
                direction: TextDirection::LeftToRight,
                fill: None,
                glyphs,
            }]),
            vector_items: Arc::from([]),
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
    fn vector_math_items_share_geometry_resource_handles() {
        let geometry = GeometryResourceHandle {
            id: GeometryId::new(7),
            version: 2,
        };
        let item = TextVectorItem {
            geometry,
            transform: Transform2D::IDENTITY,
            style: TextVectorStyle::default(),
            source_span: Some(TextSourceSpan::new(0, 1)),
            semantic_key: Some(Arc::from("fraction-rule")),
        };
        assert_eq!(item.geometry, geometry);
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
            id: TextResourceId::new(3),
            version: 1,
        };
        let geometry = GeometryResourceHandle {
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
