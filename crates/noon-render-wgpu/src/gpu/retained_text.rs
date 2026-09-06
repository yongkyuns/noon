use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    mem::{size_of, size_of_val},
    sync::Arc,
};

use noon_core::{
    Color, FontResourceHandle, FontResourceLookup, GeometryRef, GeometryResource,
    GeometryResourceLookup, GlyphRun, ObjectContentRef, ObjectId, PathCommand, PublicationContext,
    StrokeCap, StrokeJoin, StrokeWidthMode, Style, TextAffineTransform, TextGlyphStroke,
    TextRenderItem, TextResourceLookup, TextVectorItem, Transform2D, Vec2, VectorPath,
};
#[cfg(test)]
use noon_core::{FontResourceArena, GeometryResourceArena, TextResourceArena, TextVectorStyle};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState, RendererPublication};
use noon_text_atlas::{GlyphAtlasPlane, GpuGlyphAtlas};
use noon_text_render_wgpu::{
    GlyphQuadInstance, PreparedRetainedTextFrame, PreparedTextItem, RetainedTextPrepareStats,
    RetainedTextQuadPreparer, TextCamera2D, TextDeviceMetrics, TextGlyphGpuRenderer,
    TextGpuDrawError, TextGpuDrawStats, TextGpuUploadStats, TextPrepareError,
};
#[cfg(test)]
use noon_typst::{compile_typst_resource, TypstMode};
use swash::{
    scale::ScaleContext,
    zeno::{self, Cap as ZenoCap, Command as ZenoCommand, Join as ZenoJoin, PathData},
    CacheKey, FontRef, GlyphId,
};

use super::{push_upload_write, Camera2D, DrawStats, GpuRenderer, UploadStats, PATH_SAMPLE_COUNT};
use crate::{
    FramePreparer, OrderedRenderBatch, PreparedFrame, RenderPrimitive, VisibleRenderError,
};

pub const DEFAULT_GLYPH_OUTLINE_CACHE_MAX_ENTRIES: usize = 4_096;
pub const DEFAULT_GLYPH_OUTLINE_CACHE_MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;

/// One item in the renderer's single global painter-order stream.
///
/// `object_id` is always the semantic retained object ID. Geometry packing uses
/// private scratch IDs only to recover packed instance locations; those IDs never
/// escape this adapter and never create a second semantic identity space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedRenderItem {
    Geometry {
        object_id: ObjectId,
        batch: OrderedRenderBatch,
    },
    Glyph {
        object_id: ObjectId,
        text_item_index: usize,
    },
}

impl RetainedRenderItem {
    pub const fn object_id(&self) -> ObjectId {
        match self {
            Self::Geometry { object_id, .. } | Self::Glyph { object_id, .. } => *object_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedPrepareStats {
    pub semantic_objects: usize,
    pub geometry_slots: usize,
    pub glyph_batches: usize,
    pub vector_items: usize,
    pub outline_runs: usize,
    pub outline_cache_hits: u64,
    pub outline_cache_misses: u64,
}

/// Cumulative counters for retained mixed-frame preparation locality.
///
/// A scratch reuse means the semantic object/text/vector/outline walk was skipped.
/// Text snapshot copies and mixed-order rebuilds are counted separately so a
/// transform/opacity-only text update can prove that both parent-level scans stay
/// out of the frame loop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedFrameIncrementalStats {
    pub scratch_rebuilds: u64,
    pub scratch_reuses: u64,
    pub text_snapshot_copies: u64,
    pub mixed_order_rebuilds: u64,
}

/// Cumulative work performed to derive camera-visible submission lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedVisibilityProjectionStats {
    pub projections: u64,
    pub candidates_projected: u64,
    pub render_items_projected: u64,
}

impl RetainedVisibilityProjectionStats {
    fn record(&mut self, candidates: usize, render_items: usize) {
        self.projections = self.projections.saturating_add(1);
        self.candidates_projected = self.candidates_projected.saturating_add(candidates as u64);
        self.render_items_projected = self
            .render_items_projected
            .saturating_add(render_items as u64);
    }
}

pub struct PreparedRetainedTextSnapshot<'a> {
    pub time: f64,
    pub mask_quads: &'a [GlyphQuadInstance],
    pub color_quads: &'a [GlyphQuadInstance],
    pub items: &'a [PreparedTextItem],
    pub stats: RetainedTextPrepareStats,
    pub atlas: &'a GpuGlyphAtlas,
    /// The text generation whose GPU contents a partial update assumes are resident.
    /// `None` means the snapshot requires the full-upload path.
    pub partial_upload_base_generation: Option<u64>,
    pub dirty_mask_ranges: &'a [std::ops::Range<u32>],
    pub dirty_color_ranges: &'a [std::ops::Range<u32>],
}

impl PreparedRetainedTextSnapshot<'_> {
    fn as_prepared_frame(&self) -> PreparedRetainedTextFrame<'_> {
        PreparedRetainedTextFrame {
            time: self.time,
            mask_quads: self.mask_quads,
            color_quads: self.color_quads,
            items: self.items,
            stats: self.stats,
        }
    }
}

/// Prepared mixed geometry/text frame. The geometry frame is intentionally kept
/// private so its renderer-internal scratch IDs cannot be mistaken for semantic IDs.
pub struct PreparedRetainedGpuFrame<'a> {
    applied_publication: &'a mut Option<PublicationContext>,
    geometry: PreparedFrame<'a>,
    geometry_only: bool,
    text_generation: u64,
    pub text: PreparedRetainedTextSnapshot<'a>,
    pub render_items: &'a [RetainedRenderItem],
    pub stats: RetainedPrepareStats,
    source_geometry_slots: Option<&'a [Option<usize>]>,
    render_item_ranges: Option<&'a HashMap<ObjectId, std::ops::Range<usize>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedPreparedObjectKind {
    Geometry,
    Text,
    Mixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedGlyphPlane {
    Mask,
    Color,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedPreparedGlyphRange {
    pub plane: RetainedGlyphPlane,
    pub page: u32,
    pub instance_range: std::ops::Range<u32>,
    pub instance_dirty: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetainedPreparedObjectObservation {
    pub object: ObjectId,
    pub kind: RetainedPreparedObjectKind,
    pub geometry: Option<crate::PreparedGeometryObjectObservation>,
    pub render_item_start: Option<usize>,
    pub render_item_end: Option<usize>,
    pub render_item_count: usize,
    pub glyph_item_count: usize,
    pub glyph_ranges: Vec<RetainedPreparedGlyphRange>,
    pub submission_membership: bool,
    pub full_rebuilds: usize,
    pub instances_repacked: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedPreparedObjectOutcome {
    Absent,
    Unsupported(ObjectId),
    VisibilityProjectionUnavailable,
    FamilyProjectionUnavailable,
    /// A geometry-only frame compacted its source paths into the shared mega-path
    /// stream. The prepared source slot does not retain an exact object-to-mega
    /// instance range, so upload observation must not guess one.
    MegaPathMappingUnavailable,
}

impl PreparedRetainedGpuFrame<'_> {
    pub const fn time(&self) -> f64 {
        self.geometry.time
    }

    pub const fn geometry_stats(&self) -> crate::RenderStats {
        self.geometry.stats
    }

    /// Painter-ordered geometry batches for a geometry-only prepared frame.
    ///
    /// Mixed frames use `render_items` because geometry and glyphs interleave.
    /// This exposes only renderer-derived batch order, never private scratch IDs.
    pub fn geometry_render_batches(&self) -> &[OrderedRenderBatch] {
        self.geometry.render_batches
    }

    /// Observe one retained prepared object without scanning unrelated draw items.
    ///
    /// The source frame index is translated through the existing mixed scratch-slot
    /// table, while painter membership comes from the existing per-object item range.
    /// Visibility-projected frames report an explicit unavailable result because the
    /// canonical full-order range does not prove projected submission membership.
    pub fn observe_object(
        &self,
        frame_index: usize,
        object: ObjectId,
    ) -> Result<RetainedPreparedObjectObservation, RetainedPreparedObjectOutcome> {
        if self.geometry_only {
            let geometry = self
                .geometry
                .observe_object(frame_index)
                .map_err(|outcome| match outcome {
                    crate::PreparedGeometryObjectOutcome::Absent => {
                        RetainedPreparedObjectOutcome::Absent
                    }
                    crate::PreparedGeometryObjectOutcome::Unsupported(object) => {
                        RetainedPreparedObjectOutcome::Unsupported(object)
                    }
                })?;
            if geometry.object != object {
                return Err(RetainedPreparedObjectOutcome::Absent);
            }
            if geometry_path_mapping_is_compacted(&self.geometry, &geometry) {
                return Err(RetainedPreparedObjectOutcome::MegaPathMappingUnavailable);
            }
            let Some(submission_membership) = geometry.submission_membership else {
                return Err(RetainedPreparedObjectOutcome::VisibilityProjectionUnavailable);
            };
            return Ok(RetainedPreparedObjectObservation {
                object,
                kind: RetainedPreparedObjectKind::Geometry,
                geometry: Some(geometry),
                render_item_start: None,
                render_item_end: None,
                render_item_count: usize::from(submission_membership),
                glyph_item_count: 0,
                glyph_ranges: Vec::new(),
                submission_membership,
                full_rebuilds: self.geometry.stats.full_rebuilds,
                instances_repacked: self.geometry.stats.instances_repacked,
            });
        }

        if self.source_geometry_slots.is_none() {
            return Err(RetainedPreparedObjectOutcome::FamilyProjectionUnavailable);
        }
        let Some(ranges) = self.render_item_ranges else {
            return Err(RetainedPreparedObjectOutcome::VisibilityProjectionUnavailable);
        };
        let range = ranges.get(&object).cloned().unwrap_or(0..0);
        let items = self
            .render_items
            .get(range.clone())
            .ok_or(RetainedPreparedObjectOutcome::Absent)?;
        let glyph_item_count = items
            .iter()
            .filter(|item| matches!(item, RetainedRenderItem::Glyph { .. }))
            .count();
        let glyph_ranges = observed_glyph_ranges(&self.text, items);
        let geometry_item_count = items.len().saturating_sub(glyph_item_count);
        let geometry = self
            .source_geometry_slots
            .and_then(|slots| slots.get(frame_index))
            .copied()
            .flatten()
            .map(|scratch_index| self.geometry.observe_object(scratch_index))
            .transpose()
            .map_err(|outcome| match outcome {
                crate::PreparedGeometryObjectOutcome::Absent => {
                    RetainedPreparedObjectOutcome::Absent
                }
                crate::PreparedGeometryObjectOutcome::Unsupported(object) => {
                    RetainedPreparedObjectOutcome::Unsupported(object)
                }
            })?;
        if geometry
            .as_ref()
            .is_some_and(|prepared| prepared.object != object)
        {
            return Err(RetainedPreparedObjectOutcome::Absent);
        }
        if items.is_empty() && geometry.is_none() {
            return Err(RetainedPreparedObjectOutcome::Absent);
        }
        let kind = match (geometry_item_count > 0, glyph_item_count > 0) {
            (true, true) => RetainedPreparedObjectKind::Mixed,
            (true, false) => RetainedPreparedObjectKind::Geometry,
            (false, true) => RetainedPreparedObjectKind::Text,
            (false, false) => return Err(RetainedPreparedObjectOutcome::Absent),
        };
        Ok(RetainedPreparedObjectObservation {
            object,
            kind,
            geometry,
            render_item_start: Some(range.start),
            render_item_end: Some(range.end),
            render_item_count: items.len(),
            glyph_item_count,
            glyph_ranges,
            submission_membership: !items.is_empty(),
            full_rebuilds: self.geometry.stats.full_rebuilds,
            instances_repacked: self.geometry.stats.instances_repacked,
        })
    }
}

fn geometry_path_mapping_is_compacted(
    frame: &crate::PreparedFrame<'_>,
    geometry: &crate::PreparedGeometryObjectObservation,
) -> bool {
    matches!(geometry.primitive, RenderPrimitive::Path { .. })
        && frame
            .render_batches
            .iter()
            .any(|batch| matches!(batch.primitive, RenderPrimitive::MegaPath { .. }))
}

fn observed_glyph_ranges(
    text: &PreparedRetainedTextSnapshot<'_>,
    items: &[RetainedRenderItem],
) -> Vec<RetainedPreparedGlyphRange> {
    items
        .iter()
        .filter_map(|item| {
            let RetainedRenderItem::Glyph {
                text_item_index, ..
            } = item
            else {
                return None;
            };
            let PreparedTextItem::GlyphBatch {
                plane,
                page,
                instance_range,
                ..
            } = text.items.get(*text_item_index)?
            else {
                return None;
            };
            let dirty_ranges = match plane {
                noon_text_atlas::GlyphAtlasPlane::Mask => text.dirty_mask_ranges,
                noon_text_atlas::GlyphAtlasPlane::Color => text.dirty_color_ranges,
            };
            Some(RetainedPreparedGlyphRange {
                plane: match plane {
                    noon_text_atlas::GlyphAtlasPlane::Mask => RetainedGlyphPlane::Mask,
                    noon_text_atlas::GlyphAtlasPlane::Color => RetainedGlyphPlane::Color,
                },
                page: *page,
                instance_range: instance_range.clone(),
                instance_dirty: sorted_ranges_overlap(dirty_ranges, instance_range),
            })
        })
        .collect()
}

fn sorted_ranges_overlap(ranges: &[std::ops::Range<u32>], target: &std::ops::Range<u32>) -> bool {
    let candidate = ranges.partition_point(|range| range.end <= target.start);
    ranges
        .get(candidate)
        .is_some_and(|range| range.start < target.end)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedPrepareError {
    StalePublication {
        received: PublicationContext,
        applied: PublicationContext,
    },
    MissingTextResource,
    MissingGeometryResource,
    MissingFontResource,
    InvalidFontData(FontResourceHandle),
    InvalidGlyphId(u32),
    InvalidFontSize,
    InvalidVariation,
    Visibility(VisibleRenderError),
    Text(TextPrepareError),
}

impl std::fmt::Display for RetainedPrepareError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StalePublication { received, applied } => write!(
                formatter,
                "renderer publication frame epoch {} is stale relative to applied epoch {}",
                received.frame_epoch().get(),
                applied.frame_epoch().get()
            ),
            Self::MissingTextResource => formatter.write_str("retained text resource is missing"),
            Self::MissingGeometryResource => {
                formatter.write_str("retained vector geometry resource is missing")
            }
            Self::MissingFontResource => formatter.write_str("retained font resource is missing"),
            Self::InvalidFontData(handle) => write!(
                formatter,
                "retained font resource {}:{} does not contain a valid face",
                handle.id.get(),
                handle.version
            ),
            Self::InvalidGlyphId(id) => {
                write!(formatter, "glyph id {id} exceeds the font glyph-id range")
            }
            Self::InvalidFontSize => {
                formatter.write_str("glyph outline font size must be finite and positive")
            }
            Self::InvalidVariation => formatter.write_str("glyph outline variation must be finite"),
            Self::Visibility(error) => error.fmt(formatter),
            Self::Text(error) => write!(formatter, "retained text preparation failed: {error}"),
        }
    }
}

impl std::error::Error for RetainedPrepareError {}

impl From<TextPrepareError> for RetainedPrepareError {
    fn from(value: TextPrepareError) -> Self {
        Self::Text(value)
    }
}

impl From<VisibleRenderError> for RetainedPrepareError {
    fn from(value: VisibleRenderError) -> Self {
        Self::Visibility(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct OutlineKey {
    font: FontResourceHandle,
    glyph_id: GlyphId,
    size_bits: u32,
    variation_fingerprint: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct StrokedOutlineKey {
    outline: OutlineKey,
    stroke_fingerprint: u64,
}

#[derive(Clone, Copy)]
struct SwashFace {
    offset: u32,
    key: CacheKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphOutlineCacheLimits {
    pub max_entries: usize,
    pub max_retained_bytes: usize,
}

impl GlyphOutlineCacheLimits {
    pub const fn new(max_entries: usize, max_retained_bytes: usize) -> Self {
        Self {
            max_entries,
            max_retained_bytes,
        }
    }
}

impl Default for GlyphOutlineCacheLimits {
    fn default() -> Self {
        Self::new(
            DEFAULT_GLYPH_OUTLINE_CACHE_MAX_ENTRIES,
            DEFAULT_GLYPH_OUTLINE_CACHE_MAX_RETAINED_BYTES,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphOutlineCacheStats {
    pub outline_entries: usize,
    pub stroked_entries: usize,
    pub retained_bytes: usize,
    pub font_faces: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub rejected_admissions: u64,
}

#[derive(Clone)]
struct CachedOutline {
    path: Arc<VectorPath>,
    retained_bytes: usize,
    last_used: u64,
}

#[derive(Clone, Copy)]
enum OutlineResidencyKey {
    Outline(OutlineKey),
    Stroked(StrokedOutlineKey),
}

struct GlyphOutlineCache {
    scale_context: ScaleContext,
    faces: HashMap<FontResourceHandle, SwashFace>,
    outlines: HashMap<OutlineKey, CachedOutline>,
    stroked: HashMap<StrokedOutlineKey, CachedOutline>,
    limits: GlyphOutlineCacheLimits,
    retained_bytes: usize,
    access_clock: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
    rejected_admissions: u64,
}

impl Default for GlyphOutlineCache {
    fn default() -> Self {
        Self::with_limits(GlyphOutlineCacheLimits::default())
    }
}

impl GlyphOutlineCache {
    fn with_limits(limits: GlyphOutlineCacheLimits) -> Self {
        Self {
            scale_context: ScaleContext::new(),
            faces: HashMap::new(),
            outlines: HashMap::new(),
            stroked: HashMap::new(),
            limits,
            retained_bytes: 0,
            access_clock: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            rejected_admissions: 0,
        }
    }

    fn limits(&self) -> GlyphOutlineCacheLimits {
        self.limits
    }

    fn set_limits(&mut self, limits: GlyphOutlineCacheLimits) {
        self.limits = limits;
        self.enforce_limits();
    }

    fn stats(&self) -> GlyphOutlineCacheStats {
        GlyphOutlineCacheStats {
            outline_entries: self.outlines.len(),
            stroked_entries: self.stroked.len(),
            retained_bytes: self.retained_bytes,
            font_faces: self.faces.len(),
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            rejected_admissions: self.rejected_admissions,
        }
    }

    fn total_entries(&self) -> usize {
        self.outlines.len().saturating_add(self.stroked.len())
    }

    fn next_access(&mut self) -> u64 {
        self.access_clock = self.access_clock.saturating_add(1);
        self.access_clock
    }

    fn cached_outline(&mut self, key: OutlineKey) -> Option<Arc<VectorPath>> {
        if !self.outlines.contains_key(&key) {
            return None;
        }
        let access = self.next_access();
        let entry = self
            .outlines
            .get_mut(&key)
            .expect("glyph outline cache entry must still exist");
        entry.last_used = access;
        self.hits = self.hits.saturating_add(1);
        Some(entry.path.clone())
    }

    fn cached_stroked(&mut self, key: StrokedOutlineKey) -> Option<Arc<VectorPath>> {
        if !self.stroked.contains_key(&key) {
            return None;
        }
        let access = self.next_access();
        let entry = self
            .stroked
            .get_mut(&key)
            .expect("stroked glyph outline cache entry must still exist");
        entry.last_used = access;
        self.hits = self.hits.saturating_add(1);
        Some(entry.path.clone())
    }

    fn admit_outline(&mut self, key: OutlineKey, path: Arc<VectorPath>) {
        self.admit(OutlineResidencyKey::Outline(key), path);
    }

    fn admit_stroked(&mut self, key: StrokedOutlineKey, path: Arc<VectorPath>) {
        self.admit(OutlineResidencyKey::Stroked(key), path);
    }

    fn admit(&mut self, key: OutlineResidencyKey, path: Arc<VectorPath>) {
        let retained_bytes = vector_path_retained_bytes(path.as_ref());
        if self.limits.max_entries == 0 || retained_bytes > self.limits.max_retained_bytes {
            self.rejected_admissions = self.rejected_admissions.saturating_add(1);
            return;
        }
        let access = self.next_access();
        let entry = CachedOutline {
            path,
            retained_bytes,
            last_used: access,
        };
        let previous = match key {
            OutlineResidencyKey::Outline(key) => self.outlines.insert(key, entry),
            OutlineResidencyKey::Stroked(key) => self.stroked.insert(key, entry),
        };
        if let Some(previous) = previous {
            self.retained_bytes = self.retained_bytes.saturating_sub(previous.retained_bytes);
        }
        self.retained_bytes = self.retained_bytes.saturating_add(retained_bytes);
        self.enforce_limits();
    }

    fn enforce_limits(&mut self) {
        while self.total_entries() > self.limits.max_entries
            || self.retained_bytes > self.limits.max_retained_bytes
        {
            let Some(oldest) = self.oldest_entry() else {
                break;
            };
            let removed = match oldest {
                OutlineResidencyKey::Outline(key) => self.outlines.remove(&key),
                OutlineResidencyKey::Stroked(key) => self.stroked.remove(&key),
            }
            .expect("selected glyph outline cache entry must still exist");
            self.retained_bytes = self.retained_bytes.saturating_sub(removed.retained_bytes);
            self.evictions = self.evictions.saturating_add(1);
        }
    }

    fn oldest_entry(&self) -> Option<OutlineResidencyKey> {
        let outline = self
            .outlines
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, entry)| (OutlineResidencyKey::Outline(*key), entry.last_used));
        let stroked = self
            .stroked
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, entry)| (OutlineResidencyKey::Stroked(*key), entry.last_used));
        match (outline, stroked) {
            (Some((key, _)), None) | (None, Some((key, _))) => Some(key),
            (Some((outline_key, outline_access)), Some((stroked_key, stroked_access))) => {
                if outline_access <= stroked_access {
                    Some(outline_key)
                } else {
                    Some(stroked_key)
                }
            }
            (None, None) => None,
        }
    }

    fn outline(
        &mut self,
        fonts: &(impl FontResourceLookup + ?Sized),
        run: &GlyphRun,
        glyph_id: u32,
    ) -> Result<(OutlineKey, Arc<VectorPath>), RetainedPrepareError> {
        if !run.font_size.is_finite() || run.font_size <= 0.0 {
            return Err(RetainedPrepareError::InvalidFontSize);
        }
        if run
            .variations
            .iter()
            .any(|setting| !setting.value.is_finite())
        {
            return Err(RetainedPrepareError::InvalidVariation);
        }
        let glyph_id = GlyphId::try_from(glyph_id)
            .map_err(|_| RetainedPrepareError::InvalidGlyphId(glyph_id))?;
        let font_handle = fonts
            .handle_for_face(&run.font)
            .ok_or(RetainedPrepareError::MissingFontResource)?;
        let key = OutlineKey {
            font: font_handle,
            glyph_id,
            size_bits: run.font_size.to_bits(),
            variation_fingerprint: variation_fingerprint(run),
        };
        if let Some(path) = self.cached_outline(key) {
            return Ok((key, path));
        }

        self.misses = self.misses.saturating_add(1);
        let resource = fonts
            .get(font_handle)
            .ok_or(RetainedPrepareError::MissingFontResource)?;
        let face = if let Some(face) = self.faces.get(&font_handle).copied() {
            face
        } else {
            let font =
                FontRef::from_index(resource.data.as_ref(), resource.key.face_index as usize)
                    .ok_or(RetainedPrepareError::InvalidFontData(font_handle))?;
            let face = SwashFace {
                offset: font.offset,
                key: font.key,
            };
            self.faces.insert(font_handle, face);
            face
        };
        let font = FontRef {
            data: resource.data.as_ref(),
            offset: face.offset,
            key: face.key,
        };
        let variations: Vec<([u8; 4], f32)> = run
            .variations
            .iter()
            .map(|setting| (setting.tag, setting.value))
            .collect();
        let mut scaler = self
            .scale_context
            .builder(font)
            .size(run.font_size)
            .hint(false)
            .variations(variations.iter())
            .build();
        let path = scaler
            .scale_outline(glyph_id)
            .map(|outline| zeno_to_noon(outline.path().commands()))
            .unwrap_or_default();
        let path = Arc::new(path);
        self.admit_outline(key, path.clone());
        Ok((key, path))
    }

    fn stroked_outline(
        &mut self,
        outline_key: OutlineKey,
        outline: &VectorPath,
        stroke: &TextGlyphStroke,
    ) -> Arc<VectorPath> {
        let key = StrokedOutlineKey {
            outline: outline_key,
            stroke_fingerprint: stroke_fingerprint(stroke),
        };
        if let Some(path) = self.cached_stroked(key) {
            return path;
        }
        self.misses = self.misses.saturating_add(1);
        let path = Arc::new(expand_stroke(outline, stroke));
        self.admit_stroked(key, path.clone());
        path
    }
}

fn vector_path_retained_bytes(path: &VectorPath) -> usize {
    let own = size_of::<VectorPath>() + size_of_val(path.commands());
    own.saturating_add(
        path.morph_target()
            .map(vector_path_retained_bytes)
            .unwrap_or(0),
    )
}

#[derive(Clone, Debug)]
enum SourceItem {
    Geometry {
        object_id: ObjectId,
        scratch_id: ObjectId,
    },
    FastGlyphRun {
        object_id: ObjectId,
        object_index: u32,
        run_index: u32,
    },
}

/// Persistent preparation state for the mixed retained renderer.
///
/// Semantic text stays as `ObjectContentRef::Text`. Only vector decorations and
/// outline-required glyphs are materialized as renderer-local paths, and every
/// emitted painter item keeps the owning retained `ObjectId`.
pub struct RetainedFramePreparer {
    geometry: FramePreparer,
    text: RetainedTextQuadPreparer,
    outlines: GlyphOutlineCache,
    scratch: FrameState,
    scratch_ready: bool,
    scratch_object_count: usize,
    geometry_only_classification: Option<bool>,
    scratch_slots: Vec<Option<usize>>,
    incremental_stats: RetainedFrameIncrementalStats,
    sources: Vec<SourceItem>,
    render_items: Vec<RetainedRenderItem>,
    render_item_ranges: HashMap<ObjectId, std::ops::Range<usize>>,
    visible_render_items: Vec<RetainedRenderItem>,
    visible_projection_ready: bool,
    visible_projection_candidates: Vec<usize>,
    visibility_stats: RetainedVisibilityProjectionStats,
    snapshot_mask_quads: Vec<GlyphQuadInstance>,
    snapshot_color_quads: Vec<GlyphQuadInstance>,
    snapshot_text_items: Vec<PreparedTextItem>,
    snapshot_text_stats: RetainedTextPrepareStats,
    text_item_ranges: Vec<std::ops::Range<usize>>,
    fast_text_only: Vec<bool>,
    dirty_mask_ranges: Vec<std::ops::Range<u32>>,
    dirty_color_ranges: Vec<std::ops::Range<u32>>,
    snapshot_prepare_stats: RetainedPrepareStats,
    snapshot_metrics: Option<TextDeviceMetrics>,
    prepared_generation_ready: bool,
    prepared_generation_reuses: u64,
    text_generation: u64,
    last_applied_publication: Option<PublicationContext>,
}

impl Default for RetainedFramePreparer {
    fn default() -> Self {
        Self {
            geometry: FramePreparer::for_individual_path_draws(),
            text: RetainedTextQuadPreparer::default(),
            outlines: GlyphOutlineCache::default(),
            scratch: FrameState {
                time: 0.0,
                objects: Vec::new(),
                presences: Vec::new(),
                reveals: Vec::new(),
                morphs: Vec::new(),
                render_geometries: Vec::new(),
                render_transforms: Vec::new(),
            },
            scratch_ready: false,
            scratch_object_count: 0,
            geometry_only_classification: None,
            scratch_slots: Vec::new(),
            incremental_stats: RetainedFrameIncrementalStats::default(),
            sources: Vec::new(),
            render_items: Vec::new(),
            render_item_ranges: HashMap::new(),
            visible_render_items: Vec::new(),
            visible_projection_ready: false,
            visible_projection_candidates: Vec::new(),
            visibility_stats: RetainedVisibilityProjectionStats::default(),
            snapshot_mask_quads: Vec::new(),
            snapshot_color_quads: Vec::new(),
            snapshot_text_items: Vec::new(),
            snapshot_text_stats: RetainedTextPrepareStats::default(),
            text_item_ranges: Vec::new(),
            fast_text_only: Vec::new(),
            dirty_mask_ranges: Vec::new(),
            dirty_color_ranges: Vec::new(),
            snapshot_prepare_stats: RetainedPrepareStats::default(),
            snapshot_metrics: None,
            prepared_generation_ready: false,
            prepared_generation_reuses: 0,
            text_generation: 0,
            last_applied_publication: None,
        }
    }
}

impl RetainedFramePreparer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Prepare and upload a replacement resident resource set, then install its
    /// CPU preparation state. Tessellation or device-limit failure leaves the
    /// previous CPU/GPU installation untouched. The host owns queue submission;
    /// this operation neither draws nor advances scene time.
    pub fn preload_path_meshes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut GpuRenderer,
        requests: &[crate::PathMeshPreload<'_>],
    ) -> Result<crate::PathMeshPreloadStats, crate::PathMeshPreloadError> {
        let mut geometry = FramePreparer::for_individual_path_draws();
        geometry.set_path_mesh_cache_limit(self.geometry.path_mesh_cache_limit());
        geometry.preload_paths(requests)?;
        let frame = geometry.preloaded_frame();
        let stats = crate::PathMeshPreloadStats {
            geometry: frame.stats,
            upload: renderer.upload_preloaded_paths(device, queue, &frame)?,
        };
        let mut replacement = Self::with_outline_cache_limits(self.outline_cache_limits());
        replacement.geometry = geometry;
        replacement.text_generation = self.text_generation;
        *self = replacement;
        Ok(stats)
    }

    /// Budget the path cache for the currently installed immutable scene resources.
    ///
    /// Compiled morph pairs need room across timeline phases, in addition to the
    /// normal allowance for base geometry and transient outlines. This is an entry
    /// budget, not eager preparation or pinning; recompute it when replacing the
    /// installed resource set so capacity does not grow with historical scenes.
    pub fn set_scene_path_mesh_cache_budget(
        &mut self,
        compiled_geometry_count: usize,
        installed_geometry_count: usize,
    ) {
        self.geometry
            .set_path_mesh_cache_limit(compiled_geometry_count.saturating_add(
                installed_geometry_count.max(crate::DEFAULT_PATH_MESH_CACHE_LIMIT),
            ));
    }

    pub fn with_outline_cache_limits(limits: GlyphOutlineCacheLimits) -> Self {
        let mut preparer = Self::default();
        preparer.outlines.set_limits(limits);
        preparer
    }

    pub fn outline_cache_limits(&self) -> GlyphOutlineCacheLimits {
        self.outlines.limits()
    }

    pub fn set_outline_cache_limits(&mut self, limits: GlyphOutlineCacheLimits) {
        self.outlines.set_limits(limits);
    }

    pub fn outline_cache_stats(&self) -> GlyphOutlineCacheStats {
        self.outlines.stats()
    }

    pub const fn incremental_stats(&self) -> RetainedFrameIncrementalStats {
        self.incremental_stats
    }

    pub const fn visibility_stats(&self) -> RetainedVisibilityProjectionStats {
        self.visibility_stats
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &FrameState,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedPrepareError> {
        let changes = FrameChanges::all();
        self.prepare_with_changes(
            device, queue, frame, &changes, texts, fonts, geometries, metrics,
        )
    }

    /// Prepare one coherent runtime publication and retain its applied revision
    /// metadata for later stale-publication rejection.
    pub fn prepare_publication<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        publication: &RendererPublication<'_>,
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedPrepareError> {
        let received = publication.context();
        let previous = self.last_applied_publication;
        if let Some(applied) = previous {
            if publication_is_stale(received, applied) {
                return Err(RetainedPrepareError::StalePublication { received, applied });
            }
        }

        let prepared = self.prepare_with_changes(
            device,
            queue,
            publication.frame(),
            publication.changes(),
            publication.text_resources(),
            publication.font_resources(),
            publication.geometry_resources(),
            metrics,
        )?;
        *prepared.applied_publication = Some(received);
        Ok(prepared)
    }

    /// Prepare one coherent runtime publication while projecting draw submission
    /// to painter-ordered viewport candidates. Resident geometry, glyph snapshots,
    /// and immutable resources remain canonical and are updated from the publication's
    /// dirty set before this candidate-sized renderer projection is returned.
    pub fn prepare_publication_visible<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        publication: &RendererPublication<'_>,
        visible_object_indices: &[usize],
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedPrepareError> {
        let received = publication.context();
        if let Some(applied) = self.last_applied_publication {
            if publication_is_stale(received, applied) {
                return Err(RetainedPrepareError::StalePublication { received, applied });
            }
        }
        validate_visible_object_indices(publication.frame(), visible_object_indices)?;

        let prepared = self.prepare_with_changes_inner(
            device,
            queue,
            publication.frame(),
            publication.changes(),
            publication.text_resources(),
            publication.font_resources(),
            publication.geometry_resources(),
            metrics,
            true,
            Some(visible_object_indices),
        )?;
        *prepared.applied_publication = Some(received);
        Ok(prepared)
    }

    /// Last successfully applied runtime publication, if this preparer has one.
    pub fn last_applied_publication(&self) -> Option<PublicationContext> {
        self.last_applied_publication
    }

    /// Reset retained renderer state before deliberately binding this preparer to a
    /// different runtime instance. Frame epochs order one runtime only, and retained
    /// geometry/text caches cannot be shared across independent resource projections.
    pub fn reset_publication_context(&mut self) {
        let outline_limits = self.outlines.limits();
        *self = Self::default();
        self.outlines.set_limits(outline_limits);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_with_changes<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &FrameState,
        changes: &FrameChanges,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedPrepareError> {
        self.prepare_with_changes_inner(
            device, queue, frame, changes, texts, fonts, geometries, metrics, true, None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_with_changes_inner<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &FrameState,
        changes: &FrameChanges,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
        allow_geometry_only: bool,
        visible_object_indices: Option<&[usize]>,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedPrepareError> {
        if changes.is_all() || changes.is_structural() {
            self.geometry_only_classification = None;
        }
        if allow_geometry_only {
            match self.geometry_only_classification {
                Some(true) if self.can_prepare_geometry_only(frame, changes) => {
                    return self.prepare_geometry_only(
                        frame,
                        changes,
                        metrics,
                        visible_object_indices,
                    );
                }
                Some(false) => {}
                Some(true) => {}
                None if self.frame_is_geometry_only(frame) => {
                    self.geometry_only_classification = Some(true);
                    return self.prepare_geometry_only(
                        frame,
                        changes,
                        metrics,
                        visible_object_indices,
                    );
                }
                None => self.geometry_only_classification = Some(false),
            }
        } else {
            self.geometry_only_classification = Some(false);
        }
        if self.can_update_mixed_geometry_locally(frame, changes, metrics) {
            return self.prepare_mixed_geometry_locally(
                frame,
                changes,
                geometries,
                metrics,
                visible_object_indices,
            );
        }

        let text_can_update_locally = if changes.is_empty() {
            false
        } else {
            self.text
                .can_update_objects_locally(frame, changes, texts, metrics)?
                && self.changes_are_fast_text_only(frame, changes)
        };
        let scratch_reused = self.prepare_scratch_with_changes(
            frame,
            changes,
            text_can_update_locally,
            texts,
            fonts,
            geometries,
        )?;

        if scratch_reused
            && changes.is_empty()
            && self.prepared_generation_ready
            && self.snapshot_metrics == Some(metrics)
        {
            // The child preparer is privately owned and this generation was recorded
            // only after a complete successful parent preparation. Matching metrics
            // and an empty semantic change set therefore select its O(1) reuse path.
            // Invalidate the parent generation first so any future fallible change in
            // that contract cannot expose stale snapshots.
            self.prepared_generation_ready = false;
            let prepared = self
                .text
                .prepare_with_changes(device, queue, frame, changes, texts, fonts, metrics)?;
            debug_assert_eq!(prepared.mask_quads.len(), self.snapshot_mask_quads.len());
            debug_assert_eq!(prepared.color_quads.len(), self.snapshot_color_quads.len());
            debug_assert_eq!(prepared.items.len(), self.snapshot_text_items.len());
            debug_assert_eq!(prepared.stats, self.snapshot_text_stats);

            self.project_mixed_visibility(frame, visible_object_indices);
            let no_changes = FrameChanges::default();
            let geometry = self
                .geometry
                .prepare_incremental(&self.scratch, &no_changes);
            self.prepared_generation_reuses = self.prepared_generation_reuses.saturating_add(1);
            self.prepared_generation_ready = true;
            let text = PreparedRetainedTextSnapshot {
                time: frame.time,
                mask_quads: &self.snapshot_mask_quads,
                color_quads: &self.snapshot_color_quads,
                items: &self.snapshot_text_items,
                stats: self.snapshot_text_stats,
                atlas: self.text.atlas(),
                partial_upload_base_generation: None,
                dirty_mask_ranges: &self.dirty_mask_ranges,
                dirty_color_ranges: &self.dirty_color_ranges,
            };
            return Ok(PreparedRetainedGpuFrame {
                applied_publication: &mut self.last_applied_publication,
                geometry,
                geometry_only: false,
                text_generation: self.text_generation,
                text,
                render_items: if visible_object_indices.is_some() {
                    &self.visible_render_items
                } else {
                    &self.render_items
                },
                stats: self.snapshot_prepare_stats,
                source_geometry_slots: Some(&self.scratch_slots),
                render_item_ranges: visible_object_indices
                    .is_none()
                    .then_some(&self.render_item_ranges),
            });
        }

        if text_can_update_locally {
            self.prepared_generation_ready = false;
            let partial_upload_base_generation = self.text_generation;
            let prepared = self
                .text
                .prepare_with_changes(device, queue, frame, changes, texts, fonts, metrics)?;
            copy_local_text_snapshot_updates(
                &mut self.snapshot_mask_quads,
                &mut self.snapshot_color_quads,
                &self.text_item_ranges,
                prepared.items,
                prepared.mask_quads,
                prepared.color_quads,
                changes,
                &mut self.dirty_mask_ranges,
                &mut self.dirty_color_ranges,
            );
            self.text_generation = self
                .text_generation
                .checked_add(1)
                .expect("retained text generation counter exhausted");
            self.project_mixed_visibility(frame, visible_object_indices);
            let no_changes = FrameChanges::default();
            let geometry = self
                .geometry
                .prepare_incremental(&self.scratch, &no_changes);
            self.prepared_generation_ready = true;
            let text = PreparedRetainedTextSnapshot {
                time: frame.time,
                mask_quads: &self.snapshot_mask_quads,
                color_quads: &self.snapshot_color_quads,
                items: &self.snapshot_text_items,
                stats: self.snapshot_text_stats,
                atlas: self.text.atlas(),
                partial_upload_base_generation: Some(partial_upload_base_generation),
                dirty_mask_ranges: &self.dirty_mask_ranges,
                dirty_color_ranges: &self.dirty_color_ranges,
            };
            return Ok(PreparedRetainedGpuFrame {
                applied_publication: &mut self.last_applied_publication,
                geometry,
                geometry_only: false,
                text_generation: self.text_generation,
                text,
                render_items: if visible_object_indices.is_some() {
                    &self.visible_render_items
                } else {
                    &self.render_items
                },
                stats: self.snapshot_prepare_stats,
                source_geometry_slots: Some(&self.scratch_slots),
                render_item_ranges: visible_object_indices
                    .is_none()
                    .then_some(&self.render_item_ranges),
            });
        }

        // Snapshot and painter-order state is only reusable after a complete parent
        // preparation. Clear validity before the fallible text step so errors cannot
        // leave an older successful generation eligible for empty-frame reuse.
        self.prepared_generation_ready = false;

        // #339/#341 intentionally keep the atlas inside the retained text preparer.
        // Snapshot the lightweight prepared records once so that borrow can end and
        // the atlas can be borrowed alongside them for the parent GPU renderer.
        {
            let prepared = self
                .text
                .prepare_with_changes(device, queue, frame, changes, texts, fonts, metrics)?;
            self.snapshot_mask_quads.clear();
            self.snapshot_mask_quads
                .extend_from_slice(prepared.mask_quads);
            self.snapshot_color_quads.clear();
            self.snapshot_color_quads
                .extend_from_slice(prepared.color_quads);
            self.snapshot_text_items.clear();
            self.snapshot_text_items.extend_from_slice(prepared.items);
            self.snapshot_text_stats = prepared.stats;
            self.text_item_ranges = text_item_ranges(prepared.items, frame.objects.len());
        }
        self.dirty_mask_ranges.clear();
        self.dirty_color_ranges.clear();
        self.incremental_stats.text_snapshot_copies = self
            .incremental_stats
            .text_snapshot_copies
            .saturating_add(1);
        self.text_generation = self
            .text_generation
            .checked_add(1)
            .expect("retained text generation counter exhausted");

        let geometry = if scratch_reused {
            let no_changes = FrameChanges::default();
            self.geometry
                .prepare_incremental(&self.scratch, &no_changes)
        } else {
            self.geometry.prepare(&self.scratch)
        };
        self.render_items.clear();
        rebuild_mixed_order(
            &mut self.render_items,
            &self.sources,
            &self.snapshot_text_items,
            &geometry,
        );
        rebuild_render_item_ranges(&mut self.render_item_ranges, &self.render_items);
        if let Some(indices) = visible_object_indices {
            if let Some(projected) = project_mixed_visibility_cached(
                frame,
                indices,
                &self.render_items,
                &self.render_item_ranges,
                &mut self.visible_projection_ready,
                &mut self.visible_projection_candidates,
                &mut self.visible_render_items,
            ) {
                self.visibility_stats.record(indices.len(), projected);
            }
        }
        self.incremental_stats.mixed_order_rebuilds = self
            .incremental_stats
            .mixed_order_rebuilds
            .saturating_add(1);
        let glyph_batches = self
            .render_items
            .iter()
            .filter(|item| matches!(item, RetainedRenderItem::Glyph { .. }))
            .count();
        let outline_cache = self.outlines.stats();
        let stats = RetainedPrepareStats {
            semantic_objects: frame.objects.len(),
            geometry_slots: self.scratch.objects.len(),
            glyph_batches,
            vector_items: self.snapshot_text_stats.vector_items,
            outline_runs: self.snapshot_text_stats.outline_runs,
            outline_cache_hits: outline_cache.hits,
            outline_cache_misses: outline_cache.misses,
        };
        self.snapshot_prepare_stats = stats;
        self.snapshot_metrics = Some(metrics);
        self.prepared_generation_ready = true;
        let text = PreparedRetainedTextSnapshot {
            time: frame.time,
            mask_quads: &self.snapshot_mask_quads,
            color_quads: &self.snapshot_color_quads,
            items: &self.snapshot_text_items,
            stats: self.snapshot_text_stats,
            atlas: self.text.atlas(),
            partial_upload_base_generation: None,
            dirty_mask_ranges: &self.dirty_mask_ranges,
            dirty_color_ranges: &self.dirty_color_ranges,
        };
        Ok(PreparedRetainedGpuFrame {
            applied_publication: &mut self.last_applied_publication,
            geometry,
            geometry_only: false,
            text_generation: self.text_generation,
            text,
            render_items: if visible_object_indices.is_some() {
                &self.visible_render_items
            } else {
                &self.render_items
            },
            stats,
            source_geometry_slots: Some(&self.scratch_slots),
            render_item_ranges: visible_object_indices
                .is_none()
                .then_some(&self.render_item_ranges),
        })
    }

    /// Build the canonical scratch baseline required by family realization.
    ///
    /// Family operations substitute renderer-local scratch records even when the
    /// source frame happens to contain geometry only.  The geometry-only fast path
    /// deliberately does not build that scratch state, so it cannot be used as a
    /// family baseline.  Clear the cached classification afterwards: family-local
    /// scratch is never a valid classification for the next ordinary frame.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_canonical_mixed_baseline(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &FrameState,
        changes: &FrameChanges,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
    ) -> Result<(), RetainedPrepareError> {
        // Keep full and structural publications intact for cache invalidation; only
        // suppress the geometry-only fast path while this family baseline is built.
        let result = self
            .prepare_with_changes_inner(
                device, queue, frame, changes, texts, fonts, geometries, metrics, false, None,
            )
            .map(|_| ());
        self.geometry_only_classification = None;
        result
    }

    fn prepare_scratch_with_changes(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        text_only_local: bool,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
    ) -> Result<bool, RetainedPrepareError> {
        if self.scratch_ready
            && (changes.is_empty() || text_only_local)
            && frame.objects.len() == self.scratch_object_count
        {
            self.scratch.time = frame.time;
            self.incremental_stats.scratch_reuses =
                self.incremental_stats.scratch_reuses.saturating_add(1);
            return Ok(true);
        }

        // `build_scratch_frame` is fallible and mutates its destination as it walks.
        // Mark the current generation invalid first so a later empty change set can
        // never reuse a partially rebuilt scratch scene.
        self.scratch_ready = false;
        self.build_scratch_frame(frame, texts, fonts, geometries)?;
        self.scratch_object_count = frame.objects.len();
        self.scratch_ready = true;
        self.incremental_stats.scratch_rebuilds =
            self.incremental_stats.scratch_rebuilds.saturating_add(1);
        Ok(false)
    }

    fn frame_is_geometry_only(&self, frame: &FrameState) -> bool {
        frame.objects.iter().enumerate().all(|(index, object)| {
            frame.is_present(index)
                && !matches!(object.geometry(), Some(GeometryRef::External(_)))
                && object.geometry().is_some()
                && !matches!(frame.render_geometry(index), Some(GeometryRef::External(_)))
        })
    }

    fn can_prepare_geometry_only(&self, frame: &FrameState, changes: &FrameChanges) -> bool {
        if self.geometry_only_classification != Some(true)
            || changes.is_all()
            || changes.is_structural()
        {
            return false;
        }
        changes.object_indices().iter().all(|&index| {
            frame.objects.get(index).is_some_and(|object| {
                object.geometry().is_some()
                    && !matches!(object.geometry(), Some(GeometryRef::External(_)))
                    && !matches!(frame.render_geometry(index), Some(GeometryRef::External(_)))
            })
        })
    }

    fn prepare_geometry_only<'a>(
        &'a mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        metrics: TextDeviceMetrics,
        visible_object_indices: Option<&[usize]>,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedPrepareError> {
        self.prepared_generation_ready = false;
        if let Some(indices) = visible_object_indices {
            self.record_visibility_projection(indices.len(), indices.len());
        }
        let geometry = match visible_object_indices {
            Some(indices) => self
                .geometry
                .prepare_incremental_visible(frame, changes, indices)?,
            None => self.geometry.prepare_incremental(frame, changes),
        };
        let stats = RetainedPrepareStats {
            semantic_objects: frame.objects.len(),
            geometry_slots: frame.objects.len(),
            glyph_batches: 0,
            vector_items: 0,
            outline_runs: 0,
            outline_cache_hits: self.outlines.stats().hits,
            outline_cache_misses: self.outlines.stats().misses,
        };
        self.snapshot_prepare_stats = stats;
        self.snapshot_metrics = Some(metrics);
        self.prepared_generation_ready = true;
        let text = PreparedRetainedTextSnapshot {
            time: frame.time,
            mask_quads: &self.snapshot_mask_quads,
            color_quads: &self.snapshot_color_quads,
            items: &self.snapshot_text_items,
            stats: self.snapshot_text_stats,
            atlas: self.text.atlas(),
            partial_upload_base_generation: None,
            dirty_mask_ranges: &self.dirty_mask_ranges,
            dirty_color_ranges: &self.dirty_color_ranges,
        };
        Ok(PreparedRetainedGpuFrame {
            applied_publication: &mut self.last_applied_publication,
            geometry,
            geometry_only: true,
            text_generation: self.text_generation,
            text,
            render_items: &[],
            stats,
            source_geometry_slots: None,
            render_item_ranges: None,
        })
    }

    fn can_update_mixed_geometry_locally(
        &self,
        frame: &FrameState,
        changes: &FrameChanges,
        metrics: TextDeviceMetrics,
    ) -> bool {
        if self.geometry_only_classification != Some(false)
            || !self.scratch_ready
            || !self.prepared_generation_ready
            || self.snapshot_metrics != Some(metrics)
            || changes.is_all()
            || changes.is_structural()
            || changes.is_empty()
        {
            return false;
        }
        changes.object_indices().iter().all(|&index| {
            let Some(object) = frame.objects.get(index) else {
                return false;
            };
            let Some(scratch_slot) = self.scratch_slots.get(index).and_then(|slot| *slot) else {
                return false;
            };
            let rendered_geometry = frame.render_geometry(index).or_else(|| object.geometry());
            frame.is_present(index)
                && self
                    .scratch
                    .objects
                    .get(scratch_slot)
                    .is_some_and(|scratch| {
                        same_analytic_geometry_kind(rendered_geometry, scratch.geometry())
                            && frame.reveal(index) == self.scratch.reveal(scratch_slot)
                            && frame.morph(index) == self.scratch.morph(scratch_slot)
                    })
        })
    }

    fn prepare_mixed_geometry_locally<'a>(
        &'a mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
        visible_object_indices: Option<&[usize]>,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedPrepareError> {
        self.prepared_generation_ready = false;
        let mut scratch_changes = Vec::with_capacity(changes.object_indices().len());
        for &index in changes.object_indices() {
            let object = &frame.objects[index];
            let scratch_slot = self.scratch_slots[index].expect("validated mixed geometry slot");
            let source_geometry = frame
                .render_geometry(index)
                .unwrap_or_else(|| object.geometry().expect("validated mixed geometry object"));
            let geometry = resolve_geometry_ref(source_geometry, geometries)?;
            let scratch = &mut self.scratch.objects[scratch_slot];
            scratch.content = ObjectContentRef::Geometry(geometry);
            scratch.text_bounds = None;
            scratch.transform = frame.render_transform(index);
            scratch.style = object.style;
            scratch.appearance = object.appearance;
            self.scratch.reveals[scratch_slot] = frame.reveal(index);
            self.scratch.morphs[scratch_slot] = frame.morph(index);
            scratch_changes.push(scratch_slot);
        }
        self.scratch.time = frame.time;
        self.incremental_stats.scratch_reuses =
            self.incremental_stats.scratch_reuses.saturating_add(1);
        let scratch_changes = FrameChanges::objects(scratch_changes);
        let geometry = self
            .geometry
            .prepare_incremental(&self.scratch, &scratch_changes);
        if geometry.stats.full_rebuilds > 0 {
            self.render_items.clear();
            rebuild_mixed_order(
                &mut self.render_items,
                &self.sources,
                &self.snapshot_text_items,
                &geometry,
            );
            rebuild_render_item_ranges(&mut self.render_item_ranges, &self.render_items);
            self.incremental_stats.mixed_order_rebuilds = self
                .incremental_stats
                .mixed_order_rebuilds
                .saturating_add(1);
        }
        if let Some(indices) = visible_object_indices {
            if let Some(projected) = project_mixed_visibility_cached(
                frame,
                indices,
                &self.render_items,
                &self.render_item_ranges,
                &mut self.visible_projection_ready,
                &mut self.visible_projection_candidates,
                &mut self.visible_render_items,
            ) {
                self.visibility_stats.record(indices.len(), projected);
            }
        }
        self.snapshot_metrics = Some(metrics);
        self.prepared_generation_ready = true;
        let text = PreparedRetainedTextSnapshot {
            time: frame.time,
            mask_quads: &self.snapshot_mask_quads,
            color_quads: &self.snapshot_color_quads,
            items: &self.snapshot_text_items,
            stats: self.snapshot_text_stats,
            atlas: self.text.atlas(),
            partial_upload_base_generation: None,
            dirty_mask_ranges: &self.dirty_mask_ranges,
            dirty_color_ranges: &self.dirty_color_ranges,
        };
        Ok(PreparedRetainedGpuFrame {
            applied_publication: &mut self.last_applied_publication,
            geometry,
            geometry_only: false,
            text_generation: self.text_generation,
            text,
            render_items: if visible_object_indices.is_some() {
                &self.visible_render_items
            } else {
                &self.render_items
            },
            stats: self.snapshot_prepare_stats,
            source_geometry_slots: Some(&self.scratch_slots),
            render_item_ranges: visible_object_indices
                .is_none()
                .then_some(&self.render_item_ranges),
        })
    }

    fn project_mixed_visibility(
        &mut self,
        frame: &FrameState,
        visible_object_indices: Option<&[usize]>,
    ) {
        let Some(indices) = visible_object_indices else {
            return;
        };
        let projected = project_mixed_visibility_cached(
            frame,
            indices,
            &self.render_items,
            &self.render_item_ranges,
            &mut self.visible_projection_ready,
            &mut self.visible_projection_candidates,
            &mut self.visible_render_items,
        );
        if let Some(projected) = projected {
            self.record_visibility_projection(indices.len(), projected);
        }
    }

    fn record_visibility_projection(&mut self, candidates: usize, render_items: usize) {
        self.visibility_stats.record(candidates, render_items);
    }
}

fn rebuild_render_item_ranges(
    ranges: &mut HashMap<ObjectId, std::ops::Range<usize>>,
    render_items: &[RetainedRenderItem],
) {
    ranges.clear();
    for (index, item) in render_items.iter().enumerate() {
        ranges
            .entry(item.object_id())
            .and_modify(|range| range.end = index + 1)
            .or_insert(index..index + 1);
    }
}

fn project_mixed_visibility_cached(
    frame: &FrameState,
    visible_object_indices: &[usize],
    render_items: &[RetainedRenderItem],
    ranges: &HashMap<ObjectId, std::ops::Range<usize>>,
    ready: &mut bool,
    cached_candidates: &mut Vec<usize>,
    output: &mut Vec<RetainedRenderItem>,
) -> Option<usize> {
    if *ready
        && cached_candidates == visible_object_indices
        && mixed_visibility_topology_matches(
            frame,
            visible_object_indices,
            render_items,
            ranges,
            output,
        )
    {
        return None;
    }

    cached_candidates.clear();
    cached_candidates.extend_from_slice(visible_object_indices);
    output.clear();
    for &index in visible_object_indices {
        let object = &frame.objects[index];
        if let Some(range) = ranges.get(&object.id) {
            output.extend_from_slice(&render_items[range.clone()]);
        }
    }
    *ready = true;
    Some(output.len())
}

fn mixed_visibility_topology_matches(
    frame: &FrameState,
    visible_object_indices: &[usize],
    render_items: &[RetainedRenderItem],
    ranges: &HashMap<ObjectId, std::ops::Range<usize>>,
    projected: &[RetainedRenderItem],
) -> bool {
    let mut projected_start = 0;
    for &index in visible_object_indices {
        let object = &frame.objects[index];
        let Some(range) = ranges.get(&object.id) else {
            continue;
        };
        let projected_end = projected_start + range.len();
        if projected.get(projected_start..projected_end) != Some(&render_items[range.clone()]) {
            return false;
        }
        projected_start = projected_end;
    }
    projected_start == projected.len()
}

fn validate_visible_object_indices(
    frame: &FrameState,
    visible_object_indices: &[usize],
) -> Result<(), VisibleRenderError> {
    let mut seen = std::collections::HashSet::with_capacity(visible_object_indices.len());
    for &index in visible_object_indices {
        if index >= frame.objects.len() {
            return Err(VisibleRenderError::ObjectIndexOutOfRange {
                index,
                objects: frame.objects.len(),
            });
        }
        if !seen.insert(index) {
            return Err(VisibleRenderError::DuplicateObjectIndex(index));
        }
    }
    Ok(())
}

impl RetainedFramePreparer {
    fn changes_are_fast_text_only(&self, frame: &FrameState, changes: &FrameChanges) -> bool {
        if !self.scratch_ready || changes.is_all() || changes.is_structural() || changes.is_empty()
        {
            return false;
        }
        changes.object_indices().iter().all(|&index| {
            let Some(object) = frame.objects.get(index) else {
                return false;
            };
            if !frame.is_present(index) || !matches!(&object.content, ObjectContentRef::Text(_)) {
                return false;
            }
            self.fast_text_only.get(index).copied().unwrap_or(false)
        })
    }

    fn build_scratch_frame(
        &mut self,
        frame: &FrameState,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
    ) -> Result<(), RetainedPrepareError> {
        self.scratch.time = frame.time;
        self.scratch.objects.clear();
        self.scratch.presences.clear();
        self.scratch.reveals.clear();
        self.scratch.morphs.clear();
        self.scratch.render_geometries.clear();
        self.scratch.render_transforms.clear();
        self.sources.clear();
        self.fast_text_only.clear();
        self.fast_text_only.resize(frame.objects.len(), false);
        self.scratch_slots.clear();
        self.scratch_slots.resize(frame.objects.len(), None);
        let mut geometry_only = true;

        for (object_index, object) in frame.objects.iter().enumerate() {
            if !frame.is_present(object_index) {
                geometry_only = false;
                continue;
            }
            if object.text().is_some() {
                geometry_only = false;
            }

            match &object.content {
                ObjectContentRef::Geometry(semantic_geometry) => {
                    let source_geometry = frame
                        .render_geometry(object_index)
                        .unwrap_or(semantic_geometry);
                    if matches!(source_geometry, GeometryRef::External(_)) {
                        geometry_only = false;
                    }
                    let geometry = resolve_geometry_ref(source_geometry, geometries)?;
                    let scratch_slot = self.push_geometry(
                        object.id,
                        geometry,
                        frame.render_transform(object_index),
                        object.style,
                        object.appearance,
                        frame.reveal(object_index),
                        frame.morph(object_index),
                    );
                    self.scratch_slots[object_index] = Some(scratch_slot);
                }
                ObjectContentRef::Text(handle) => {
                    let mut fast_text_only = true;
                    let resource = texts
                        .get(*handle)
                        .ok_or(RetainedPrepareError::MissingTextResource)?;
                    let object_index_u32 = u32::try_from(object_index)
                        .expect("retained object count exceeds u32 painter-order limits");
                    let reveal = frame.reveal(object_index);
                    let morph = frame.morph(object_index);
                    for item in resource.render_items.iter().copied() {
                        match item {
                            TextRenderItem::GlyphRun(run_index) => {
                                let run = &resource.runs[run_index as usize];
                                if run.stroke.is_some() || reveal < 1.0 || morph != 0.0 {
                                    fast_text_only = false;
                                    self.push_outline_run(
                                        object.id,
                                        object.transform,
                                        object.style,
                                        object.appearance,
                                        reveal,
                                        morph,
                                        run,
                                        fonts,
                                    )?;
                                } else {
                                    self.sources.push(SourceItem::FastGlyphRun {
                                        object_id: object.id,
                                        object_index: object_index_u32,
                                        run_index,
                                    });
                                }
                            }
                            TextRenderItem::Vector(vector_index) => {
                                fast_text_only = false;
                                let vector = &resource.vector_items[vector_index as usize];
                                self.push_text_vector(
                                    object.id,
                                    object.transform,
                                    object.style,
                                    object.appearance,
                                    reveal,
                                    morph,
                                    vector,
                                    geometries,
                                )?;
                            }
                        }
                    }
                    self.fast_text_only[object_index] = fast_text_only;
                }
            }
        }
        self.geometry_only_classification = Some(geometry_only);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn push_geometry(
        &mut self,
        object_id: ObjectId,
        geometry: GeometryRef,
        transform: Transform2D,
        style: Style,
        appearance: f32,
        reveal: f32,
        morph: f32,
    ) -> usize {
        let scratch_slot = self.scratch.objects.len();
        let scratch_id = ObjectId::new(scratch_slot as u64);
        self.scratch.objects.push(FrameObjectState {
            id: scratch_id,
            content: ObjectContentRef::Geometry(geometry),
            text_bounds: None,
            transform,
            style,
            appearance,
        });
        self.scratch.presences.push(true);
        self.scratch.reveals.push(reveal);
        self.scratch.morphs.push(morph);
        self.scratch.render_geometries.push(None);
        self.scratch.render_transforms.push(None);
        self.sources.push(SourceItem::Geometry {
            object_id,
            scratch_id,
        });
        scratch_slot
    }

    #[allow(clippy::too_many_arguments)]
    fn push_text_vector(
        &mut self,
        object_id: ObjectId,
        object_transform: Transform2D,
        object_style: Style,
        appearance: f32,
        reveal: f32,
        morph: f32,
        vector: &TextVectorItem,
        geometries: &(impl GeometryResourceLookup + ?Sized),
    ) -> Result<(), RetainedPrepareError> {
        let GeometryResource::VectorPath(path) = geometries
            .get(vector.geometry)
            .ok_or(RetainedPrepareError::MissingGeometryResource)?;
        let path = transform_path(path, vector.transform, Vec2::ZERO);
        let has_stroke = vector.style.stroke_width > 0.0;
        let style = Style {
            fill: if vector.style.fill.is_some() || !has_stroke {
                vector.style.fill.or(object_style.fill)
            } else {
                None
            },
            stroke: has_stroke
                .then(|| vector.style.stroke.or(object_style.fill))
                .flatten(),
            stroke_width: vector.style.stroke_width,
            stroke_width_mode: StrokeWidthMode::ScaleWithObject,
            stroke_join: object_style.stroke_join,
            stroke_cap: object_style.stroke_cap,
            opacity: object_style.opacity,
        };
        self.push_geometry(
            object_id,
            GeometryRef::VectorPath(path),
            object_transform,
            style,
            appearance,
            reveal,
            morph,
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn push_outline_run(
        &mut self,
        object_id: ObjectId,
        object_transform: Transform2D,
        object_style: Style,
        appearance: f32,
        reveal: f32,
        morph: f32,
        run: &GlyphRun,
        fonts: &(impl FontResourceLookup + ?Sized),
    ) -> Result<(), RetainedPrepareError> {
        let mut fill_path = VectorPath::new();
        let mut stroke_path = VectorPath::new();
        for glyph in run.glyphs.iter() {
            let (key, outline) = self.outlines.outline(fonts, run, glyph.glyph_id)?;
            fill_path =
                append_transformed_path(fill_path, outline.as_ref(), run.transform, glyph.origin);
            if let Some(stroke) = run.stroke.as_ref() {
                let expanded = self.outlines.stroked_outline(key, outline.as_ref(), stroke);
                stroke_path = append_transformed_path(
                    stroke_path,
                    expanded.as_ref(),
                    run.transform,
                    glyph.origin,
                );
            }
        }

        if !fill_path.is_empty() {
            let fill = run.fill.or(object_style.fill).unwrap_or(Color::WHITE);
            self.push_geometry(
                object_id,
                GeometryRef::VectorPath(fill_path),
                object_transform,
                Style {
                    fill: Some(fill),
                    stroke: None,
                    stroke_width: 0.0,
                    stroke_width_mode: StrokeWidthMode::ScaleWithObject,
                    stroke_join: StrokeJoin::Round,
                    stroke_cap: StrokeCap::Round,
                    opacity: object_style.opacity,
                },
                appearance,
                reveal,
                morph,
            );
        }

        if let Some(stroke) = run.stroke.as_ref() {
            if !stroke_path.is_empty() {
                let color = stroke.paint.or(object_style.fill).unwrap_or(Color::WHITE);
                self.push_geometry(
                    object_id,
                    GeometryRef::VectorPath(stroke_path),
                    object_transform,
                    Style {
                        fill: Some(color),
                        stroke: None,
                        stroke_width: 0.0,
                        stroke_width_mode: StrokeWidthMode::ScaleWithObject,
                        stroke_join: StrokeJoin::Round,
                        stroke_cap: StrokeCap::Round,
                        opacity: object_style.opacity,
                    },
                    appearance,
                    reveal,
                    morph,
                );
            }
        }
        Ok(())
    }
}

fn publication_is_stale(received: PublicationContext, applied: PublicationContext) -> bool {
    received.frame_epoch().get() < applied.frame_epoch().get()
        || (received.frame_epoch() == applied.frame_epoch() && received != applied)
}

fn text_item_ranges(
    items: &[PreparedTextItem],
    object_count: usize,
) -> Vec<std::ops::Range<usize>> {
    let mut ranges = vec![0..0; object_count];
    for (item_index, item) in items.iter().enumerate() {
        let object_index = item.object_index() as usize;
        let Some(range) = ranges.get_mut(object_index) else {
            continue;
        };
        if range.start == range.end {
            range.start = item_index;
        }
        range.end = item_index + 1;
    }
    ranges
}

#[allow(clippy::too_many_arguments)]
fn copy_local_text_snapshot_updates(
    mask_destination: &mut [GlyphQuadInstance],
    color_destination: &mut [GlyphQuadInstance],
    item_ranges: &[std::ops::Range<usize>],
    items: &[PreparedTextItem],
    mask_source: &[GlyphQuadInstance],
    color_source: &[GlyphQuadInstance],
    changes: &FrameChanges,
    dirty_mask_ranges: &mut Vec<std::ops::Range<u32>>,
    dirty_color_ranges: &mut Vec<std::ops::Range<u32>>,
) {
    dirty_mask_ranges.clear();
    dirty_color_ranges.clear();
    for &object_index in changes.object_indices() {
        let Some(item_range) = item_ranges.get(object_index) else {
            continue;
        };
        for item in &items[item_range.clone()] {
            let PreparedTextItem::GlyphBatch {
                plane,
                instance_range,
                ..
            } = item
            else {
                continue;
            };
            let start = instance_range.start as usize;
            let end = instance_range.end as usize;
            match plane {
                noon_text_atlas::GlyphAtlasPlane::Mask => {
                    mask_destination[start..end].copy_from_slice(&mask_source[start..end]);
                    push_coalesced_range(dirty_mask_ranges, instance_range.clone());
                }
                noon_text_atlas::GlyphAtlasPlane::Color => {
                    color_destination[start..end].copy_from_slice(&color_source[start..end]);
                    push_coalesced_range(dirty_color_ranges, instance_range.clone());
                }
            }
        }
    }
}

fn push_coalesced_range(ranges: &mut Vec<std::ops::Range<u32>>, range: std::ops::Range<u32>) {
    if range.start == range.end {
        return;
    }
    if let Some(previous) = ranges.last_mut() {
        if range.start <= previous.end {
            previous.end = previous.end.max(range.end);
            return;
        }
    }
    ranges.push(range);
}

fn same_analytic_geometry_kind(left: Option<&GeometryRef>, right: Option<&GeometryRef>) -> bool {
    matches!(
        (left, right),
        (
            Some(GeometryRef::Circle { .. }),
            Some(GeometryRef::Circle { .. })
        ) | (
            Some(GeometryRef::Rectangle { .. }),
            Some(GeometryRef::Rectangle { .. })
        ) | (
            Some(GeometryRef::Line { .. }),
            Some(GeometryRef::Line { .. })
        )
    )
}

fn resolve_geometry_ref(
    geometry: &GeometryRef,
    geometries: &(impl GeometryResourceLookup + ?Sized),
) -> Result<GeometryRef, RetainedPrepareError> {
    let GeometryRef::External(id) = geometry else {
        return Ok(geometry.clone());
    };
    let handle = geometries
        .current_handle(*id)
        .ok_or(RetainedPrepareError::MissingGeometryResource)?;
    let GeometryResource::VectorPath(path) = geometries
        .get(handle)
        .ok_or(RetainedPrepareError::MissingGeometryResource)?;
    Ok(GeometryRef::VectorPath(path.as_ref().clone()))
}

fn rebuild_mixed_order(
    output: &mut Vec<RetainedRenderItem>,
    sources: &[SourceItem],
    text_items: &[PreparedTextItem],
    geometry: &PreparedFrame<'_>,
) {
    let mut circle_indices = HashMap::new();
    for (index, id) in geometry.circle_ids.iter().copied().enumerate() {
        circle_indices.insert(id, index);
    }
    let mut rectangle_indices = HashMap::new();
    for (index, id) in geometry.rectangle_ids.iter().copied().enumerate() {
        rectangle_indices.insert(id, index);
    }
    let mut line_indices: HashMap<ObjectId, Vec<usize>> = HashMap::new();
    for (index, id) in geometry.line_ids.iter().copied().enumerate() {
        line_indices.entry(id).or_default().push(index);
    }
    let mut path_indices = HashMap::new();
    for (index, id) in geometry.path_ids.iter().copied().enumerate() {
        path_indices.insert(id, index);
    }

    let mut glyph_items: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (item_index, item) in text_items.iter().enumerate() {
        if let PreparedTextItem::GlyphBatch {
            object_index,
            run_index,
            ..
        } = item
        {
            glyph_items
                .entry((*object_index, *run_index))
                .or_default()
                .push(item_index);
        }
    }

    for source in sources {
        match source {
            SourceItem::FastGlyphRun {
                object_id,
                object_index,
                run_index,
            } => {
                if let Some(items) = glyph_items.get(&(*object_index, *run_index)) {
                    for &text_item_index in items {
                        output.push(RetainedRenderItem::Glyph {
                            object_id: *object_id,
                            text_item_index,
                        });
                    }
                }
            }
            SourceItem::Geometry {
                object_id,
                scratch_id,
            } => {
                // Preparation is allowed to change the renderer primitive for one
                // semantic object. In particular analytic Create/Uncreate lowers a
                // circle/rectangle/line to a temporary path. Recover painter order
                // from the primitive that was actually prepared rather than from the
                // source geometry kind captured before preparation.
                if let Some(&index) = path_indices.get(scratch_id) {
                    if let Some((batch, _)) = geometry
                        .path_batches
                        .iter()
                        .enumerate()
                        .find(|(_, batch)| batch.instance_range.contains(&(index as u32)))
                    {
                        push_geometry_item(
                            output,
                            *object_id,
                            RenderPrimitive::Path { batch },
                            index,
                        );
                    }
                    // Create reveal heads are packed as lines sharing the scratch ID
                    // and must remain immediately above the path body.
                    if let Some(indices) = line_indices.get(scratch_id) {
                        for &index in indices {
                            push_geometry_item(output, *object_id, RenderPrimitive::Line, index);
                        }
                    }
                } else if let Some(&index) = circle_indices.get(scratch_id) {
                    push_geometry_item(output, *object_id, RenderPrimitive::Circle, index);
                } else if let Some(&index) = rectangle_indices.get(scratch_id) {
                    push_geometry_item(output, *object_id, RenderPrimitive::Rectangle, index);
                } else if let Some(indices) = line_indices.get(scratch_id) {
                    for &index in indices {
                        push_geometry_item(output, *object_id, RenderPrimitive::Line, index);
                    }
                }
            }
        }
    }
}

fn push_geometry_item(
    output: &mut Vec<RetainedRenderItem>,
    object_id: ObjectId,
    primitive: RenderPrimitive,
    index: usize,
) {
    let start = u32::try_from(index).expect("retained render instance exceeds u32 limits");
    let end = start
        .checked_add(1)
        .expect("retained render instance exceeds u32 limits");
    if let Some(RetainedRenderItem::Geometry {
        object_id: last_object,
        batch,
    }) = output.last_mut()
    {
        if *last_object == object_id
            && batch.primitive == primitive
            && batch.instance_range.end == start
        {
            batch.instance_range.end = end;
            return;
        }
    }
    output.push(RetainedRenderItem::Geometry {
        object_id,
        batch: OrderedRenderBatch {
            primitive,
            instance_range: start..end,
        },
    });
}

fn transform_path(path: &VectorPath, transform: TextAffineTransform, offset: Vec2) -> VectorPath {
    let mut result = append_transformed_path(VectorPath::new(), path, transform, offset);
    if let Some(target) = path.morph_target() {
        result = result.with_morph_target(transform_path(target, transform, offset));
    }
    result
}

fn append_transformed_path(
    mut target: VectorPath,
    source: &VectorPath,
    transform: TextAffineTransform,
    offset: Vec2,
) -> VectorPath {
    let point = |value: Vec2| transform.transform_point(value + offset);
    for command in source.commands() {
        target = match *command {
            PathCommand::MoveTo { to } => target.move_to(point(to)),
            PathCommand::LineTo { to } => target.line_to(point(to)),
            PathCommand::QuadraticTo { control, to } => {
                target.quadratic_to(point(control), point(to))
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => target.cubic_to(point(control1), point(control2), point(to)),
            PathCommand::Close => target.close(),
        };
    }
    target
}

fn variation_fingerprint(run: &GlyphRun) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for setting in run.variations.iter() {
        for byte in setting.tag {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for byte in setting.value.to_bits().to_be_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn stroke_fingerprint(stroke: &TextGlyphStroke) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    stroke.width.to_bits().hash(&mut hasher);
    stroke.cap.hash(&mut hasher);
    stroke.join.hash(&mut hasher);
    stroke.dash_phase.to_bits().hash(&mut hasher);
    stroke.miter_limit.to_bits().hash(&mut hasher);
    for value in stroke.dash_array.iter() {
        value.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn noon_to_zeno(path: &VectorPath) -> Vec<ZenoCommand> {
    path.commands()
        .iter()
        .map(|command| match *command {
            PathCommand::MoveTo { to } => ZenoCommand::MoveTo(zeno::Point::new(to.x, to.y)),
            PathCommand::LineTo { to } => ZenoCommand::LineTo(zeno::Point::new(to.x, to.y)),
            PathCommand::QuadraticTo { control, to } => ZenoCommand::QuadTo(
                zeno::Point::new(control.x, control.y),
                zeno::Point::new(to.x, to.y),
            ),
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => ZenoCommand::CurveTo(
                zeno::Point::new(control1.x, control1.y),
                zeno::Point::new(control2.x, control2.y),
                zeno::Point::new(to.x, to.y),
            ),
            PathCommand::Close => ZenoCommand::Close,
        })
        .collect()
}

fn zeno_to_noon(commands: impl Iterator<Item = ZenoCommand>) -> VectorPath {
    let mut path = VectorPath::new();
    for command in commands {
        path = match command {
            ZenoCommand::MoveTo(point) => path.move_to(Vec2::new(point.x, point.y)),
            ZenoCommand::LineTo(point) => path.line_to(Vec2::new(point.x, point.y)),
            ZenoCommand::QuadTo(control, point) => {
                path.quadratic_to(Vec2::new(control.x, control.y), Vec2::new(point.x, point.y))
            }
            ZenoCommand::CurveTo(control1, control2, point) => path.cubic_to(
                Vec2::new(control1.x, control1.y),
                Vec2::new(control2.x, control2.y),
                Vec2::new(point.x, point.y),
            ),
            ZenoCommand::Close => path.close(),
        };
    }
    path
}

fn expand_stroke(path: &VectorPath, stroke: &TextGlyphStroke) -> VectorPath {
    let source = noon_to_zeno(path);
    let mut style = zeno::Stroke::new(stroke.width);
    style
        .join(match stroke.join {
            StrokeJoin::Round => ZenoJoin::Round,
            StrokeJoin::Miter => ZenoJoin::Miter,
            StrokeJoin::Bevel => ZenoJoin::Bevel,
        })
        .miter_limit(stroke.miter_limit)
        .cap(match stroke.cap {
            StrokeCap::Round => ZenoCap::Round,
            StrokeCap::Butt => ZenoCap::Butt,
            StrokeCap::Square => ZenoCap::Square,
        })
        .dash(stroke.dash_array.as_ref(), stroke.dash_phase);
    let mut output = Vec::<ZenoCommand>::new();
    zeno::apply(source.as_slice(), style, None, &mut output);
    zeno_to_noon(output.into_iter())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedUploadStats {
    pub geometry: UploadStats,
    pub text: TextGpuUploadStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedDrawStats {
    pub geometry: DrawStats,
    pub text: TextGpuDrawStats,
}

pub struct RetainedTextGpuState {
    glyphs: TextGlyphGpuRenderer,
    last_uploaded_generation: Option<u64>,
}

impl RetainedTextGpuState {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        camera: Camera2D,
    ) -> Self {
        Self {
            glyphs: TextGlyphGpuRenderer::new(device, queue, target_format, text_camera(camera)),
            last_uploaded_generation: None,
        }
    }
}

fn text_upload_needed(last_uploaded_generation: Option<u64>, generation: u64) -> bool {
    last_uploaded_generation != Some(generation)
}

impl GpuRenderer {
    pub fn create_retained_text_state(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> RetainedTextGpuState {
        RetainedTextGpuState::new(device, queue, self.target_format, self.camera)
    }

    pub fn upload_retained(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedRetainedGpuFrame<'_>,
        text_state: &mut RetainedTextGpuState,
    ) -> RetainedUploadStats {
        self.upload_retained_inner(device, queue, prepared, text_state, None)
    }

    /// Upload a retained frame while recording only writes that intersect one
    /// prepared target. The caller owns the opt-in trace allocation.
    pub fn upload_retained_with_trace(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedRetainedGpuFrame<'_>,
        text_state: &mut RetainedTextGpuState,
        observed: &RetainedPreparedObjectObservation,
        upload_writes: &mut Vec<crate::UploadWrite>,
    ) -> RetainedUploadStats {
        self.upload_retained_inner(
            device,
            queue,
            prepared,
            text_state,
            Some((observed, upload_writes)),
        )
    }

    fn upload_retained_inner(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &PreparedRetainedGpuFrame<'_>,
        text_state: &mut RetainedTextGpuState,
        mut observed_uploads: Option<(
            &RetainedPreparedObjectObservation,
            &mut Vec<crate::UploadWrite>,
        )>,
    ) -> RetainedUploadStats {
        let geometry = if let Some((observed, writes)) = observed_uploads.as_mut() {
            let mut trace = |buffer, range, offset, bytes: &[u8]| {
                if observed_geometry_write_matches(observed, buffer, &range) {
                    push_upload_write(writes, buffer, range, offset, bytes);
                }
            };
            self.upload_with_write_trace(device, queue, &prepared.geometry, &mut trace)
        } else {
            self.upload(device, queue, &prepared.geometry)
        };
        text_state
            .glyphs
            .set_camera(queue, text_camera(self.camera));
        let text = if text_upload_needed(
            text_state.last_uploaded_generation,
            prepared.text_generation,
        ) {
            let text_frame = prepared.text.as_prepared_frame();
            let partial = prepared.text.partial_upload_base_generation.is_some()
                && prepared.text.partial_upload_base_generation
                    == text_state.last_uploaded_generation;
            let uploaded = if let Some((observed, writes)) = observed_uploads.as_mut() {
                let mut trace = |plane, range, offset, bytes: &[u8]| {
                    let buffer = match plane {
                        GlyphAtlasPlane::Mask => "text_mask",
                        GlyphAtlasPlane::Color => "text_color",
                    };
                    if observed_text_write_matches(observed, plane, &range) {
                        push_upload_write(writes, buffer, range, offset, bytes);
                    }
                };
                if partial {
                    text_state.glyphs.upload_ranges_with_trace(
                        device,
                        queue,
                        &text_frame,
                        prepared.text.atlas,
                        prepared.text.dirty_mask_ranges,
                        prepared.text.dirty_color_ranges,
                        &mut trace,
                    )
                } else {
                    text_state.glyphs.upload_with_trace(
                        device,
                        queue,
                        &text_frame,
                        prepared.text.atlas,
                        &mut trace,
                    )
                }
            } else if partial {
                text_state.glyphs.upload_ranges(
                    device,
                    queue,
                    &text_frame,
                    prepared.text.atlas,
                    prepared.text.dirty_mask_ranges,
                    prepared.text.dirty_color_ranges,
                )
            } else {
                text_state
                    .glyphs
                    .upload(device, queue, &text_frame, prepared.text.atlas)
            };
            text_state.last_uploaded_generation = Some(prepared.text_generation);
            uploaded
        } else {
            TextGpuUploadStats::default()
        };
        RetainedUploadStats { geometry, text }
    }

    pub fn encode_retained(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedRetainedGpuFrame<'_>,
        text_state: &RetainedTextGpuState,
        clear_color: wgpu::Color,
    ) -> Result<RetainedDrawStats, TextGpuDrawError> {
        if prepared.geometry_only {
            return Ok(RetainedDrawStats {
                geometry: self.encode(encoder, view, &prepared.geometry, clear_color),
                text: TextGpuDrawStats::default(),
            });
        }
        let scene_view = self.presentation.scene_view(view);
        let sample_count = retained_sample_count(prepared.render_items);
        let color_attachments = if sample_count == 1 {
            [Some(wgpu::RenderPassColorAttachment {
                view: scene_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
            })]
        } else {
            [Some(wgpu::RenderPassColorAttachment {
                view: &self.path_msaa_view,
                depth_slice: None,
                resolve_target: Some(scene_view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Discard,
                },
            })]
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Noon retained geometry/text painter-order pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let mut stats = RetainedDrawStats::default();
        for item in prepared.render_items {
            match item {
                RetainedRenderItem::Geometry { batch, .. } => {
                    stats.geometry += self.draw_retained_geometry_batch(
                        &mut pass,
                        &prepared.geometry,
                        batch,
                        sample_count == 1,
                    );
                }
                RetainedRenderItem::Glyph {
                    text_item_index, ..
                } => {
                    stats.text += text_state.glyphs.draw_item(
                        &mut pass,
                        &prepared.text.items[*text_item_index],
                        sample_count,
                    )?;
                }
            }
        }
        drop(pass);
        self.presentation.encode_present(encoder, view);
        Ok(stats)
    }

    fn draw_retained_geometry_batch<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
        batch: &OrderedRenderBatch,
        single_sample_analytics: bool,
    ) -> DrawStats {
        let mut stats = DrawStats::default();
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        match batch.primitive {
            RenderPrimitive::Circle => {
                pass.set_pipeline(if single_sample_analytics {
                    &self.circle_pipeline_single_sample
                } else {
                    &self.circle_pipeline
                });
                pass.set_vertex_buffer(0, self.quad_buffer.slice(..));
                pass.set_vertex_buffer(1, self.circle_buffer.slice(..));
                pass.draw(0..6, batch.instance_range.clone());
            }
            RenderPrimitive::Rectangle => {
                pass.set_pipeline(if single_sample_analytics {
                    &self.rectangle_pipeline_single_sample
                } else {
                    &self.rectangle_pipeline
                });
                pass.set_vertex_buffer(0, self.quad_buffer.slice(..));
                pass.set_vertex_buffer(1, self.rectangle_buffer.slice(..));
                pass.draw(0..6, batch.instance_range.clone());
            }
            RenderPrimitive::Line => {
                pass.set_pipeline(if single_sample_analytics {
                    &self.line_pipeline_single_sample
                } else {
                    &self.line_pipeline
                });
                pass.set_vertex_buffer(0, self.quad_buffer.slice(..));
                pass.set_vertex_buffer(1, self.line_buffer.slice(..));
                pass.draw(0..6, batch.instance_range.clone());
            }
            RenderPrimitive::Path {
                batch: path_batch_index,
            } => {
                let path_batch = &prepared.path_batches[path_batch_index];
                if path_batch.index_range.is_empty() {
                    return stats;
                }
                pass.set_pipeline(&self.path_pipeline);
                pass.set_vertex_buffer(0, self.path_vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.path_instance_buffer.slice(..));
                pass.set_index_buffer(self.path_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(
                    path_batch.index_range.clone(),
                    0,
                    batch.instance_range.clone(),
                );
            }
            RenderPrimitive::MegaPath {
                batch: mega_batch_index,
            } => {
                let mega_batch = &prepared.mega_path_batches[mega_batch_index];
                if mega_batch.index_range.is_empty() {
                    return stats;
                }
                pass.set_pipeline(&self.mega_path_pipeline);
                pass.set_vertex_buffer(0, self.path_vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.mega_path_vertex_instance_buffer.slice(..));
                pass.set_index_buffer(
                    self.mega_path_index_buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                pass.draw_indexed(mega_batch.index_range.clone(), 0, 0..1);
                stats.draw_calls = 1;
                stats.instances_drawn = mega_batch.path_count;
                return stats;
            }
        }
        stats.draw_calls = 1;
        stats.instances_drawn = batch.instance_range.len();
        stats
    }
}

fn observed_geometry_write_matches(
    observed: &RetainedPreparedObjectObservation,
    buffer: &'static str,
    range: &std::ops::Range<usize>,
) -> bool {
    observed.geometry.as_ref().is_some_and(|geometry| {
        let target_buffer = match geometry.primitive {
            RenderPrimitive::Circle => "circle",
            RenderPrimitive::Rectangle => "rectangle",
            RenderPrimitive::Line => "line",
            RenderPrimitive::Path { .. } | RenderPrimitive::MegaPath { .. } => "path_instance",
        };
        buffer == target_buffer && range.contains(&geometry.instance_index)
    })
}

fn observed_text_write_matches(
    observed: &RetainedPreparedObjectObservation,
    plane: GlyphAtlasPlane,
    range: &std::ops::Range<usize>,
) -> bool {
    observed.glyph_ranges.iter().any(|target| {
        let target_plane = match target.plane {
            RetainedGlyphPlane::Mask => GlyphAtlasPlane::Mask,
            RetainedGlyphPlane::Color => GlyphAtlasPlane::Color,
        };
        target_plane == plane
            && range.start < target.instance_range.end as usize
            && (target.instance_range.start as usize) < range.end
    })
}

impl std::ops::AddAssign for DrawStats {
    fn add_assign(&mut self, rhs: Self) {
        self.draw_calls = self.draw_calls.saturating_add(rhs.draw_calls);
        self.instances_drawn = self.instances_drawn.saturating_add(rhs.instances_drawn);
    }
}

fn text_camera(camera: Camera2D) -> TextCamera2D {
    TextCamera2D {
        center: camera.center,
        world_size: camera.world_size,
    }
}

fn retained_sample_count(items: &[RetainedRenderItem]) -> u32 {
    if items.iter().any(|item| {
        matches!(
            item,
            RetainedRenderItem::Geometry {
                batch: OrderedRenderBatch {
                    primitive: RenderPrimitive::Path { .. } | RenderPrimitive::MegaPath { .. },
                    ..
                },
                ..
            }
        )
    }) {
        PATH_SAMPLE_COUNT
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::{CompiledObject, CompiledScene};
    use noon_core::FontResourceId;
    use noon_runtime::{FrameObjectState, SceneInstance};

    use super::*;

    fn mixed_text_frame() -> (
        FrameState,
        TextResourceArena,
        FontResourceArena,
        GeometryResourceArena,
    ) {
        let mut artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let bounds = artifact.resource.bounds;
        let fonts = artifact.fonts;
        let mut geometries = GeometryResourceArena::new();
        let vector = geometries.insert_path(
            VectorPath::new()
                .move_to(Vec2::new(-0.25, -0.05))
                .line_to(Vec2::new(0.25, -0.05))
                .line_to(Vec2::new(0.25, 0.05))
                .line_to(Vec2::new(-0.25, 0.05))
                .close(),
        );
        let mut vector_items = artifact.resource.vector_items.to_vec();
        let vector_index = vector_items.len() as u32;
        vector_items.push(TextVectorItem {
            geometry: vector,
            transform: TextAffineTransform::IDENTITY,
            style: TextVectorStyle::default(),
            source_span: None,
            semantic_key: Some(Arc::from("test-rule")),
        });
        artifact.resource.vector_items = vector_items.into();
        let mut render_items = artifact.resource.render_items.to_vec();
        render_items.push(TextRenderItem::Vector(vector_index));
        artifact.resource.render_items = render_items.into();
        let mut texts = TextResourceArena::new();
        let text = texts.insert(artifact.resource).unwrap();
        (
            FrameState {
                time: 0.0,
                objects: vec![
                    FrameObjectState {
                        id: ObjectId::new(1),
                        content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                        text_bounds: None,
                        transform: Transform2D::default(),
                        style: Style::default(),
                        appearance: 1.0,
                    },
                    FrameObjectState {
                        id: ObjectId::new(2),
                        content: ObjectContentRef::Text(text),
                        text_bounds: Some(bounds),
                        transform: Transform2D::default(),
                        style: Style::default(),
                        appearance: 1.0,
                    },
                ],
                presences: vec![true, true],
                reveals: vec![1.0, 1.0],
                morphs: vec![0.0, 0.0],
                render_geometries: vec![None, None],
                render_transforms: vec![None, None],
            },
            texts,
            fonts,
            geometries,
        )
    }

    fn geometry_only_mega_path_frame() -> FrameState {
        let path = |id, y| {
            let style = Style {
                fill: None,
                stroke: Some(Color::WHITE),
                stroke_width: 0.01,
                ..Style::default()
            };
            FrameObjectState {
                id: ObjectId::new(id),
                content: ObjectContentRef::Geometry(GeometryRef::path(
                    VectorPath::new()
                        .move_to(Vec2::new(-0.5, y))
                        .line_to(Vec2::new(0.5, y)),
                )),
                text_bounds: None,
                transform: Transform2D::IDENTITY,
                style,
                appearance: 1.0,
            }
        };
        FrameState {
            time: 0.0,
            objects: vec![path(1, 0.0), path(2, 0.2)],
            presences: vec![true, true],
            reveals: vec![1.0, 1.0],
            morphs: vec![0.0, 0.0],
            render_geometries: vec![None, None],
            render_transforms: vec![None, None],
        }
    }

    fn outline_key(glyph_id: GlyphId) -> OutlineKey {
        OutlineKey {
            font: FontResourceHandle {
                arena: 0,
                id: FontResourceId::new(1),
                version: 0,
            },
            glyph_id,
            size_bits: 24.0_f32.to_bits(),
            variation_fingerprint: 0,
        }
    }

    fn stroked_key(outline: OutlineKey, stroke_fingerprint: u64) -> StrokedOutlineKey {
        StrokedOutlineKey {
            outline,
            stroke_fingerprint,
        }
    }

    fn test_path(seed: f32, segments: usize) -> Arc<VectorPath> {
        let mut path = VectorPath::new().move_to(Vec2::new(seed, seed));
        for index in 0..segments {
            path = path.line_to(Vec2::new(seed + index as f32, seed - index as f32));
        }
        Arc::new(path)
    }

    #[test]
    fn outline_entry_budget_is_shared_with_stroked_paths() {
        let mut cache = GlyphOutlineCache::with_limits(GlyphOutlineCacheLimits::new(2, usize::MAX));
        let first = outline_key(1);
        let second = outline_key(2);
        cache.admit_outline(first, test_path(1.0, 2));
        cache.admit_stroked(stroked_key(first, 11), test_path(2.0, 3));
        cache.admit_outline(second, test_path(3.0, 2));

        let stats = cache.stats();
        assert_eq!(stats.outline_entries + stats.stroked_entries, 2);
        assert_eq!(stats.evictions, 1);
        assert!(!cache.outlines.contains_key(&first));
        assert!(cache.stroked.contains_key(&stroked_key(first, 11)));
        assert!(cache.outlines.contains_key(&second));
    }

    #[test]
    fn outline_lru_hit_preserves_recent_entry() {
        let mut cache = GlyphOutlineCache::with_limits(GlyphOutlineCacheLimits::new(2, usize::MAX));
        let first = outline_key(1);
        let second = outline_key(2);
        let third = outline_key(3);
        cache.admit_outline(first, test_path(1.0, 2));
        cache.admit_outline(second, test_path(2.0, 2));
        assert!(cache.cached_outline(first).is_some());
        cache.admit_outline(third, test_path(3.0, 2));

        assert!(cache.outlines.contains_key(&first));
        assert!(!cache.outlines.contains_key(&second));
        assert!(cache.outlines.contains_key(&third));
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().evictions, 1);
    }

    #[test]
    fn oversized_outline_is_returnable_but_not_resident() {
        let path = test_path(1.0, 8);
        let bytes = vector_path_retained_bytes(path.as_ref());
        assert!(bytes > 0);
        let mut cache = GlyphOutlineCache::with_limits(GlyphOutlineCacheLimits::new(8, bytes - 1));
        cache.admit_outline(outline_key(1), path);

        let stats = cache.stats();
        assert_eq!(stats.outline_entries, 0);
        assert_eq!(stats.retained_bytes, 0);
        assert_eq!(stats.rejected_admissions, 1);
    }

    #[test]
    fn tightening_outline_limits_evicts_immediately() {
        let mut cache = GlyphOutlineCache::with_limits(GlyphOutlineCacheLimits::new(4, usize::MAX));
        cache.admit_outline(outline_key(1), test_path(1.0, 2));
        cache.admit_outline(outline_key(2), test_path(2.0, 2));
        cache.admit_stroked(stroked_key(outline_key(2), 5), test_path(3.0, 3));
        assert_eq!(cache.total_entries(), 3);

        cache.set_limits(GlyphOutlineCacheLimits::new(1, usize::MAX));
        assert_eq!(cache.total_entries(), 1);
        assert_eq!(cache.stats().evictions, 2);
    }

    fn runtime_publication_scene() -> SceneInstance {
        let object = CompiledObject::new(
            ObjectId::new(0),
            GeometryRef::circle(1.0),
            Transform2D::default(),
            Style::default(),
        );
        SceneInstance::new(
            CompiledScene::compile_objects(vec![object], &[]).expect("scene must compile"),
        )
    }

    #[test]
    fn publication_order_rejects_stale_or_ambiguous_frame_contexts() {
        use noon_core::{ExecutionRevision, FrameEpoch, SceneRevision};

        let applied = PublicationContext::new(
            SceneRevision::new(3),
            ExecutionRevision::new(5),
            FrameEpoch::new(11),
        );
        assert!(!publication_is_stale(applied, applied));
        assert!(publication_is_stale(
            applied.with_frame_epoch(FrameEpoch::new(10)),
            applied
        ));
        assert!(publication_is_stale(
            PublicationContext::new(
                SceneRevision::new(4),
                ExecutionRevision::new(6),
                FrameEpoch::new(11),
            ),
            applied,
        ));
        assert!(!publication_is_stale(
            PublicationContext::new(
                SceneRevision::new(4),
                ExecutionRevision::new(6),
                FrameEpoch::new(12),
            ),
            applied,
        ));
    }

    #[test]
    fn prepare_publication_applies_current_runtime_and_rejects_stale_runtime_view() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let mut preparer = RetainedFramePreparer::new();

        let mut stale_instance = runtime_publication_scene();
        let stale = stale_instance.take_renderer_publication();
        let stale_context = stale.context();

        let mut current_instance = runtime_publication_scene();
        current_instance.advance_to(1.0).unwrap();
        let current = current_instance.take_renderer_publication();
        let current_context = current.context();
        assert!(current_context.frame_epoch().get() > stale_context.frame_epoch().get());
        {
            let prepared = preparer
                .prepare_publication(&device, &queue, &current, metrics)
                .unwrap();
            assert_eq!(prepared.time(), 1.0);
        }
        assert_eq!(preparer.last_applied_publication(), Some(current_context));

        assert!(matches!(
            preparer.prepare_publication(&device, &queue, &stale, metrics),
            Err(RetainedPrepareError::StalePublication { received, applied })
                if received == stale_context && applied == current_context
        ));
        assert_eq!(preparer.last_applied_publication(), Some(current_context));

        preparer.reset_publication_context();
        {
            let prepared = preparer
                .prepare_publication(&device, &queue, &stale, metrics)
                .unwrap();
            assert_eq!(prepared.time(), 0.0);
            assert_eq!(prepared.geometry_stats().full_rebuilds, 1);
        }
        assert_eq!(preparer.last_applied_publication(), Some(stale_context));
    }

    #[test]
    fn visible_publication_keeps_resident_geometry_but_filters_submission() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let mut preparer = RetainedFramePreparer::new();
        let mut instance = runtime_publication_scene();
        let publication = instance.take_renderer_publication();
        let context = publication.context();

        {
            let prepared = preparer
                .prepare_publication_visible(&device, &queue, &publication, &[], metrics)
                .unwrap();
            assert_eq!(prepared.geometry.circles.len(), 1);
            assert!(prepared.geometry_render_batches().is_empty());
        }
        assert_eq!(preparer.last_applied_publication(), Some(context));
        assert_eq!(
            preparer.visibility_stats(),
            RetainedVisibilityProjectionStats {
                projections: 1,
                candidates_projected: 0,
                render_items_projected: 0,
            }
        );
    }

    #[test]
    fn new_or_changed_text_generation_requires_gpu_upload() {
        assert!(text_upload_needed(None, 1));
        assert!(!text_upload_needed(Some(1), 1));
        assert!(text_upload_needed(Some(1), 2));
    }

    #[test]
    fn observed_text_object_reuses_its_existing_glyph_instance_ranges() {
        let (frame, texts, fonts, geometries) = mixed_text_frame();
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();
        let prepared = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();

        let observed = prepared.observe_object(1, ObjectId::new(2)).unwrap();
        assert_eq!(observed.kind, RetainedPreparedObjectKind::Mixed);
        assert!(observed.geometry.is_some());
        assert_eq!(observed.glyph_ranges.len(), observed.glyph_item_count);
        assert!(!observed.glyph_ranges.is_empty());
        for range in &observed.glyph_ranges {
            assert!(!range.instance_range.is_empty());
            assert!(prepared.render_items
                [observed.render_item_start.unwrap()..observed.render_item_end.unwrap()]
                .iter()
                .filter_map(|item| {
                    let RetainedRenderItem::Glyph {
                        text_item_index, ..
                    } = item
                    else {
                        return None;
                    };
                    let PreparedTextItem::GlyphBatch {
                        plane,
                        page,
                        instance_range,
                        ..
                    } = &prepared.text.items[*text_item_index]
                    else {
                        return None;
                    };
                    Some((plane, page, instance_range))
                })
                .any(|(plane, page, instance_range)| {
                    let plane = match plane {
                        GlyphAtlasPlane::Mask => RetainedGlyphPlane::Mask,
                        GlyphAtlasPlane::Color => RetainedGlyphPlane::Color,
                    };
                    plane == range.plane
                        && *page == range.page
                        && instance_range == &range.instance_range
                }));
        }
    }

    #[test]
    fn actual_mixed_text_preparation_traces_target_uploads_and_draws() {
        let (frame, texts, fonts, geometries) = mixed_text_frame();
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();
        let prepared = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        let observed = prepared.observe_object(1, ObjectId::new(2)).unwrap();
        assert_eq!(observed.kind, RetainedPreparedObjectKind::Mixed);
        assert!(observed.geometry.is_some());
        assert!(!observed.glyph_ranges.is_empty());

        let format = wgpu::TextureFormat::Rgba8Unorm;
        let mut renderer = GpuRenderer::new(&device, format);
        renderer.set_viewport(&device, &queue, 64, 64);
        let mut text_state = renderer.create_retained_text_state(&device, &queue);
        let mut writes = Vec::new();
        let upload = renderer.upload_retained_with_trace(
            &device,
            &queue,
            &prepared,
            &mut text_state,
            &observed,
            &mut writes,
        );
        assert!(upload.geometry.bytes_uploaded > 0);
        assert!(upload.text.bytes_uploaded > 0);
        assert!(writes.iter().any(|write| write.buffer == "path_instance"));
        assert!(writes
            .iter()
            .any(|write| matches!(write.buffer, "text_mask" | "text_color")));
        assert!(writes.iter().all(|write| write.buffer != "circle"));

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon mixed text observation target"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Noon mixed text observation encoder"),
        });
        let draw = renderer
            .encode_retained(
                &mut encoder,
                &view,
                &prepared,
                &text_state,
                wgpu::Color::TRANSPARENT,
            )
            .unwrap();
        queue.submit(Some(encoder.finish()));
        assert!(draw.geometry.draw_calls > 0);
        assert!(draw.text.draw_calls > 0);
    }

    #[test]
    fn observed_geometry_only_mega_path_is_explicitly_unavailable() {
        let frame = geometry_only_mega_path_frame();
        let texts = TextResourceArena::new();
        let fonts = FontResourceArena::new();
        let geometries = GeometryResourceArena::new();
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();
        let prepared = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();

        assert!(prepared.geometry_only);
        assert!(prepared.geometry.render_batches.iter().any(|batch| {
            matches!(batch.primitive, RenderPrimitive::MegaPath { .. })
        }));
        assert!(matches!(
            prepared.observe_object(0, ObjectId::new(1)),
            Err(RetainedPreparedObjectOutcome::MegaPathMappingUnavailable)
        ));
    }

    #[test]
    fn unchanged_frame_reuses_semantic_scratch_generation() {
        let (mut frame, texts, fonts, geometries) = mixed_text_frame();
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();
        let first_text_generation;

        {
            let prepared = preparer
                .prepare_with_changes(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::all(),
                    &texts,
                    &fonts,
                    &geometries,
                    metrics,
                )
                .unwrap();
            assert_eq!(prepared.time(), 0.0);
            assert_eq!(prepared.geometry_stats().full_rebuilds, 1);
            first_text_generation = prepared.text_generation;
            assert_eq!(first_text_generation, 1);
        }
        assert_eq!(preparer.prepared_generation_reuses, 0);
        assert_eq!(
            preparer.incremental_stats(),
            RetainedFrameIncrementalStats {
                scratch_rebuilds: 1,
                scratch_reuses: 0,
                text_snapshot_copies: 1,
                mixed_order_rebuilds: 1,
            }
        );
        let outline_stats = preparer.outline_cache_stats();

        frame.time = 1.0;
        {
            let prepared = preparer
                .prepare_with_changes(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::default(),
                    &texts,
                    &fonts,
                    &geometries,
                    metrics,
                )
                .unwrap();
            assert_eq!(prepared.time(), 1.0);
            assert_eq!(prepared.text_generation, first_text_generation);
            let geometry_stats = prepared.geometry_stats();
            assert_eq!(geometry_stats.full_rebuilds, 0);
            assert_eq!(geometry_stats.instances_repacked, 0);
            assert_eq!(geometry_stats.dirty_instance_count, 0);
        }
        assert_eq!(preparer.prepared_generation_reuses, 1);
        assert_eq!(
            preparer.incremental_stats(),
            RetainedFrameIncrementalStats {
                scratch_rebuilds: 1,
                scratch_reuses: 1,
                text_snapshot_copies: 1,
                mixed_order_rebuilds: 1,
            }
        );
        assert_eq!(preparer.outline_cache_stats(), outline_stats);
    }

    #[test]
    fn mixed_visibility_projects_geometry_and_text_enter_exit_in_painter_order() {
        let (frame, texts, fonts, geometries) = mixed_text_frame();
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();

        {
            let prepared = preparer
                .prepare_with_changes_inner(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::all(),
                    &texts,
                    &fonts,
                    &geometries,
                    metrics,
                    true,
                    Some(&[0, 1]),
                )
                .unwrap();
            assert!(!prepared.geometry_only);
            assert_eq!(
                prepared.render_items.first().unwrap().object_id(),
                ObjectId::new(1)
            );
            assert_eq!(
                prepared.render_items.last().unwrap().object_id(),
                ObjectId::new(2)
            );
        }
        let baseline_incremental = preparer.incremental_stats();

        {
            let prepared = preparer
                .prepare_with_changes_inner(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::default(),
                    &texts,
                    &fonts,
                    &geometries,
                    metrics,
                    true,
                    Some(&[1]),
                )
                .unwrap();
            assert!(!prepared.render_items.is_empty());
            assert!(prepared
                .render_items
                .iter()
                .all(|item| item.object_id() == ObjectId::new(2)));
        }
        {
            let prepared = preparer
                .prepare_with_changes_inner(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::default(),
                    &texts,
                    &fonts,
                    &geometries,
                    metrics,
                    true,
                    Some(&[0]),
                )
                .unwrap();
            assert_eq!(prepared.render_items.len(), 1);
            assert_eq!(prepared.render_items[0].object_id(), ObjectId::new(1));
        }

        let visibility = preparer.visibility_stats();
        assert_eq!(visibility.projections, 3);
        assert_eq!(visibility.candidates_projected, 4);
        assert_eq!(
            preparer.incremental_stats().mixed_order_rebuilds,
            baseline_incremental.mixed_order_rebuilds,
            "camera-only visibility changes must reuse canonical painter order"
        );
    }

    #[test]
    fn unchanged_text_candidate_reuses_projection_across_clean_and_offscreen_changes() {
        let (mut frame, texts, fonts, geometries) = mixed_text_frame();
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();

        preparer
            .prepare_with_changes_inner(
                &device,
                &queue,
                &frame,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                metrics,
                true,
                Some(&[1]),
            )
            .unwrap();
        let projected_once = preparer.visibility_stats();
        {
            let prepared = preparer
                .prepare_with_changes_inner(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::default(),
                    &texts,
                    &fonts,
                    &geometries,
                    metrics,
                    true,
                    Some(&[1]),
                )
                .unwrap();
            assert!(prepared
                .render_items
                .iter()
                .all(|item| item.object_id() == ObjectId::new(2)));
        }
        assert_eq!(preparer.visibility_stats(), projected_once);
        let baseline = preparer.incremental_stats();
        frame.objects[0].transform.translation = Vec2::new(20.0, 0.0);

        {
            let prepared = preparer
                .prepare_with_changes_inner(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::objects(vec![0]),
                    &texts,
                    &fonts,
                    &geometries,
                    metrics,
                    true,
                    Some(&[1]),
                )
                .unwrap();
            assert!(prepared
                .render_items
                .iter()
                .all(|item| item.object_id() == ObjectId::new(2)));
            assert_eq!(prepared.geometry_stats().instances_repacked, 1);
            assert_eq!(prepared.geometry_stats().full_rebuilds, 0);
        }
        let after = preparer.incremental_stats();
        assert_eq!(after.scratch_rebuilds, baseline.scratch_rebuilds);
        assert_eq!(after.mixed_order_rebuilds, baseline.mixed_order_rebuilds);
        assert_eq!(preparer.visibility_stats(), projected_once);
    }

    #[test]
    fn metric_change_does_not_reuse_parent_generation() {
        let (mut frame, texts, fonts, geometries) = mixed_text_frame();
        let first_metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let second_metrics = TextDeviceMetrics::uniform(200.0).unwrap();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();

        let first_generation = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                first_metrics,
            )
            .unwrap()
            .text_generation;
        frame.time = 1.0;
        let prepared = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::default(),
                &texts,
                &fonts,
                &geometries,
                second_metrics,
            )
            .unwrap();

        assert_eq!(prepared.time(), 1.0);
        assert_eq!(prepared.geometry_stats().full_rebuilds, 0);
        assert_ne!(prepared.text_generation, first_generation);
        assert_eq!(preparer.prepared_generation_reuses, 0);
    }

    #[test]
    fn retained_order_never_merges_geometry_across_glyphs() {
        let object = ObjectId::new(7);
        let mut output = Vec::new();
        push_geometry_item(&mut output, object, RenderPrimitive::Circle, 0);
        output.push(RetainedRenderItem::Glyph {
            object_id: object,
            text_item_index: 3,
        });
        push_geometry_item(&mut output, object, RenderPrimitive::Circle, 1);
        assert_eq!(output.len(), 3);
        assert!(matches!(output[1], RetainedRenderItem::Glyph { .. }));
    }

    #[test]
    fn affine_path_baking_preserves_quadratic_and_cubic_commands() {
        let path = VectorPath::new()
            .move_to(Vec2::new(1.0, 2.0))
            .quadratic_to(Vec2::new(2.0, 3.0), Vec2::new(4.0, 5.0))
            .cubic_to(
                Vec2::new(5.0, 6.0),
                Vec2::new(7.0, 8.0),
                Vec2::new(9.0, 10.0),
            )
            .close();
        let transform = TextAffineTransform {
            xx: 2.0,
            yx: 0.25,
            xy: -0.5,
            yy: 3.0,
            tx: 4.0,
            ty: -2.0,
        };
        let transformed = transform_path(&path, transform, Vec2::new(0.5, -1.0));
        assert_eq!(transformed.commands().len(), path.commands().len());
        assert!(matches!(
            transformed.commands()[1],
            PathCommand::QuadraticTo { .. }
        ));
        assert!(matches!(
            transformed.commands()[2],
            PathCommand::CubicTo { .. }
        ));
    }
}
