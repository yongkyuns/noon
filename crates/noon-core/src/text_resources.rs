use std::{
    mem::{size_of, size_of_val},
    sync::Arc,
};

use crate::{GeometryResourceHandle, Rect, Vec2};

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
///
/// The stable ID is suitable for reconciliation. The version changes whenever the
/// immutable payload is replaced so renderer caches and old snapshots cannot observe
/// new glyph/layout data through an old handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextResourceHandle {
    pub id: TextResourceId,
    pub version: u64,
}

/// UTF-8 byte range in the original authoring source.
///
/// Byte offsets are used so every frontend can preserve exactly the same source
/// identity without depending on host-language string indexing rules.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextSourceKind {
    Plain,
    Markup,
    Tex,
    MathTex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextDirection {
    LeftToRight,
    RightToLeft,
}

/// Renderer-independent font face identity emitted by a shaping backend.
///
/// `face_key` is backend-defined but deterministic for the exact resolved face. It
/// may be a content hash, packaged-font key, or another stable identity. The core
/// deliberately does not depend on FreeType/HarfBuzz/browser font objects.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontFaceIdentity {
    pub family: Arc<str>,
    pub face_key: Arc<str>,
    pub face_index: u32,
    pub variation_key: Arc<str>,
}

/// Stable identity for one shaped cluster/part across ordinary transforms.
///
/// The source span anchors identity to authoring text. `cluster_ordinal` disambiguates
/// repeated/expanded clusters with the same source span. `semantic_key` allows a math
/// backend to preserve token/part identity needed by `get_part_by_tex`, coloring and
/// `TransformMatchingTex` without exposing backend-specific node objects.
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

#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    pub font: FontFaceIdentity,
    pub font_size: f32,
    pub direction: TextDirection,
    pub glyphs: Arc<[PositionedGlyph]>,
}

/// Logical source part independently addressable by public text/math APIs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextPart {
    pub source_span: TextSourceSpan,
    pub first_cluster: u32,
    pub cluster_count: u32,
    pub semantic_key: Option<Arc<str>>,
}

/// Identity of a math-layout artifact produced by an external backend.
///
/// Noon stores only the deterministic artifact identity here; concrete compiler DOM,
/// DVI/XDV, SVG, MathJax nodes, or other backend payloads remain outside `noon-core`.
/// The shaped runs/parts in [`TextResource`] are the language-neutral observable
/// result consumed by semantics and rendering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MathLayoutArtifact {
    pub backend: Arc<str>,
    pub backend_version: Arc<str>,
    pub template_fingerprint: Arc<str>,
    pub artifact_fingerprint: Arc<str>,
}

/// Immutable renderer-independent shaped text/math payload.
#[derive(Clone, Debug, PartialEq)]
pub struct TextResource {
    pub source: Arc<str>,
    pub kind: TextSourceKind,
    pub runs: Arc<[GlyphRun]>,
    pub parts: Arc<[TextPart]>,
    pub bounds: Rect,
    pub baseline: f32,
    pub math_layout: Option<MathLayoutArtifact>,
}

impl TextResource {
    /// Deterministic retained-memory estimate used for architecture/perf tests.
    ///
    /// Allocator bookkeeping and shared-Arc deduplication are intentionally excluded,
    /// matching the approximate accounting policy of `GeometryResourceArena`.
    pub fn retained_bytes(&self) -> usize {
        let mut bytes = size_of::<Self>() + self.source.len();
        bytes = bytes.saturating_add(size_of_val(self.runs.as_ref()));
        bytes = bytes.saturating_add(size_of_val(self.parts.as_ref()));

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

        for part in self.parts.iter() {
            if let Some(key) = &part.semantic_key {
                bytes = bytes.saturating_add(key.len());
            }
        }

        if let Some(layout) = &self.math_layout {
            bytes = bytes
                .saturating_add(layout.backend.len())
                .saturating_add(layout.backend_version.len())
                .saturating_add(layout.template_fingerprint.len())
                .saturating_add(layout.artifact_fingerprint.len());
        }
        bytes
    }

    pub fn glyph_count(&self) -> usize {
        self.runs.iter().map(|run| run.glyphs.len()).sum()
    }
}

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

/// Lazily extracted outline backed by the common immutable geometry arena.
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
    pub parts: usize,
}

/// Stable-ID arena for immutable shaped text/math payloads.
#[derive(Clone, Debug, Default)]
pub struct TextResourceArena {
    entries: Vec<TextResourceEntry>,
    live_resources: usize,
    retained_bytes: usize,
    glyphs: usize,
    parts: usize,
}

impl TextResourceArena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, resource: TextResource) -> TextResourceHandle {
        let id = TextResourceId::new(
            u64::try_from(self.entries.len()).expect("Noon text resource ID space exhausted"),
        );
        self.add_stats(&resource);
        self.entries.push(TextResourceEntry {
            version: 0,
            value: Some(Arc::new(resource)),
        });
        self.live_resources += 1;
        TextResourceHandle { id, version: 0 }
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
        self.parts = self.parts.saturating_add(resource.parts.len());
    }

    fn subtract_stats(&mut self, resource: &TextResource) {
        self.retained_bytes = self
            .retained_bytes
            .saturating_sub(resource.retained_bytes());
        self.glyphs = self.glyphs.saturating_sub(resource.glyph_count());
        self.parts = self.parts.saturating_sub(resource.parts.len());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextResourceError {
    UnknownResource(TextResourceId),
    VersionExhausted(TextResourceId),
}

impl std::fmt::Display for TextResourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownResource(id) => write!(formatter, "unknown text resource {}", id.get()),
            Self::VersionExhausted(id) => {
                write!(formatter, "text resource {} version space exhausted", id.get())
            }
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
                glyphs,
            }]),
            parts: Arc::from([TextPart {
                source_span: TextSourceSpan::new(0, source.len() as u32),
                first_cluster: 0,
                cluster_count: source.chars().count() as u32,
                semantic_key: None,
            }]),
            bounds: Rect::new(Vec2::new(0.0, -0.2), Vec2::new(source.len() as f32, 0.8)),
            baseline: 0.0,
            math_layout: None,
        }
    }

    #[test]
    fn many_snapshots_share_one_small_text_handle() {
        let mut arena = TextResourceArena::new();
        let handle = arena.insert(sample_text("hello"));
        let snapshots = vec![handle; 100_000];
        assert_eq!(snapshots.len(), 100_000);
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.stats().glyphs, 5);
        assert_eq!(arena.get(handle).unwrap().source.as_ref(), "hello");
    }

    #[test]
    fn replacement_preserves_id_and_invalidates_old_version() {
        let mut arena = TextResourceArena::new();
        let first = arena.insert(sample_text("x"));
        let second = arena.replace(first.id, sample_text("x^2")).unwrap();
        assert_eq!(first.id, second.id);
        assert_ne!(first.version, second.version);
        assert!(arena.get(first).is_none());
        assert_eq!(arena.get(second).unwrap().source.as_ref(), "x^2");
        assert_eq!(arena.stats().glyphs, 3);
    }

    #[test]
    fn removal_does_not_renumber_unrelated_resources() {
        let mut arena = TextResourceArena::new();
        let first = arena.insert(sample_text("a"));
        let second = arena.insert(sample_text("b"));
        arena.remove(first.id).unwrap();
        assert!(arena.get(first).is_none());
        assert_eq!(arena.current_handle(second.id), Some(second));
    }

    #[test]
    fn source_spans_and_semantic_keys_define_matching_identity() {
        let identity = TextClusterIdentity {
            source_span: TextSourceSpan::new(3, 8),
            cluster_ordinal: 2,
            semantic_key: Some(Arc::from("x^2")),
        };
        let same = identity.clone();
        assert_eq!(identity, same);
    }

    #[test]
    fn glyph_outlines_reuse_common_geometry_resources() {
        let text = TextResourceHandle {
            id: TextResourceId::new(4),
            version: 2,
        };
        let geometry = GeometryResourceHandle {
            id: GeometryId::new(7),
            version: 1,
        };
        let outline = GlyphOutlineResource {
            key: GlyphOutlineKey {
                text,
                run_index: 0,
                glyph_index: 3,
            },
            geometry,
        };
        assert_eq!(outline.geometry, geometry);
        assert_eq!(outline.key.text, text);
    }
}