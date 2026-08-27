use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
};

use noon_core::{
    Color, FontResourceArena, FontResourceHandle, GeometryRef, GeometryResource,
    GeometryResourceArena, GlyphRun, ObjectContentRef, ObjectId, PathCommand, StrokeCap,
    StrokeJoin, StrokeWidthMode, Style, TextAffineTransform, TextGlyphStroke, TextRenderItem,
    TextResourceArena, TextVectorItem, Transform2D, Vec2, VectorPath,
};
use noon_runtime::{FrameObjectState, FrameState, RetainedFrameState};
use noon_text_atlas::GpuGlyphAtlas;
use noon_text_render_wgpu::{
    GlyphQuadInstance, PreparedRetainedTextFrame, PreparedTextItem, RetainedTextPrepareStats,
    RetainedTextQuadPreparer, TextCamera2D, TextDeviceMetrics, TextGlyphGpuRenderer,
    TextGpuDrawError, TextGpuDrawStats, TextGpuUploadStats, TextPrepareError,
};
use swash::{
    scale::ScaleContext,
    zeno::{self, Cap as ZenoCap, Command as ZenoCommand, Join as ZenoJoin},
    CacheKey, FontRef, GlyphId,
};

use super::{Camera2D, DrawStats, GpuRenderer, UploadStats, PATH_SAMPLE_COUNT};
use crate::{
    FramePreparer, OrderedRenderBatch, PreparedFrame, RenderPrimitive,
};

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

#[derive(Debug)]
pub struct PreparedRetainedTextSnapshot<'a> {
    pub time: f64,
    pub mask_quads: &'a [GlyphQuadInstance],
    pub color_quads: &'a [GlyphQuadInstance],
    pub items: &'a [PreparedTextItem],
    pub stats: RetainedTextPrepareStats,
    pub atlas: &'a GpuGlyphAtlas,
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
#[derive(Debug)]
pub struct PreparedRetainedGpuFrame<'a> {
    geometry: PreparedFrame<'a>,
    pub text: PreparedRetainedTextSnapshot<'a>,
    pub render_items: &'a [RetainedRenderItem],
    pub stats: RetainedPrepareStats,
}

impl PreparedRetainedGpuFrame<'_> {
    pub const fn time(&self) -> f64 {
        self.geometry.time
    }

    pub const fn geometry_stats(&self) -> crate::RenderStats {
        self.geometry.stats
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedPrepareError {
    MissingTextResource,
    MissingGeometryResource,
    MissingFontResource,
    InvalidFontData(FontResourceHandle),
    InvalidGlyphId(u32),
    InvalidFontSize,
    InvalidVariation,
    Text(TextPrepareError),
}

impl std::fmt::Display for RetainedPrepareError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
            Self::InvalidGlyphId(id) => write!(formatter, "glyph id {id} exceeds the font glyph-id range"),
            Self::InvalidFontSize => formatter.write_str("glyph outline font size must be finite and positive"),
            Self::InvalidVariation => formatter.write_str("glyph outline variation must be finite"),
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

struct GlyphOutlineCache {
    scale_context: ScaleContext,
    faces: HashMap<FontResourceHandle, SwashFace>,
    outlines: HashMap<OutlineKey, Arc<VectorPath>>,
    stroked: HashMap<StrokedOutlineKey, Arc<VectorPath>>,
    hits: u64,
    misses: u64,
}

impl Default for GlyphOutlineCache {
    fn default() -> Self {
        Self {
            scale_context: ScaleContext::new(),
            faces: HashMap::new(),
            outlines: HashMap::new(),
            stroked: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }
}

impl GlyphOutlineCache {
    fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }

    fn outline(
        &mut self,
        fonts: &FontResourceArena,
        run: &GlyphRun,
        glyph_id: u32,
    ) -> Result<(OutlineKey, Arc<VectorPath>), RetainedPrepareError> {
        if !run.font_size.is_finite() || run.font_size <= 0.0 {
            return Err(RetainedPrepareError::InvalidFontSize);
        }
        if run.variations.iter().any(|setting| !setting.value.is_finite()) {
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
        if let Some(path) = self.outlines.get(&key) {
            self.hits = self.hits.saturating_add(1);
            return Ok((key, path.clone()));
        }

        self.misses = self.misses.saturating_add(1);
        let resource = fonts
            .get(font_handle)
            .ok_or(RetainedPrepareError::MissingFontResource)?;
        let face = if let Some(face) = self.faces.get(&font_handle).copied() {
            face
        } else {
            let font = FontRef::from_index(resource.data.as_ref(), resource.key.face_index as usize)
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
        self.outlines.insert(key, path.clone());
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
        if let Some(path) = self.stroked.get(&key) {
            self.hits = self.hits.saturating_add(1);
            return path.clone();
        }
        self.misses = self.misses.saturating_add(1);
        let path = Arc::new(expand_stroke(outline, stroke));
        self.stroked.insert(key, path.clone());
        path
    }
}

#[derive(Clone, Debug)]
enum SourceItem {
    Geometry {
        object_id: ObjectId,
        scratch_id: ObjectId,
        kind: ScratchGeometryKind,
    },
    FastGlyphRun {
        object_id: ObjectId,
        object_index: u32,
        run_index: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScratchGeometryKind {
    Circle,
    Rectangle,
    Line,
    Path,
    Unsupported,
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
    sources: Vec<SourceItem>,
    render_items: Vec<RetainedRenderItem>,
    snapshot_mask_quads: Vec<GlyphQuadInstance>,
    snapshot_color_quads: Vec<GlyphQuadInstance>,
    snapshot_text_items: Vec<PreparedTextItem>,
    snapshot_text_stats: RetainedTextPrepareStats,
}

impl Default for RetainedFramePreparer {
    fn default() -> Self {
        Self {
            geometry: FramePreparer::new(),
            text: RetainedTextQuadPreparer::default(),
            outlines: GlyphOutlineCache::default(),
            scratch: FrameState {
                time: 0.0,
                objects: Vec::new(),
                presences: Vec::new(),
                reveals: Vec::new(),
                morphs: Vec::new(),
                render_geometries: Vec::new(),
            },
            sources: Vec::new(),
            render_items: Vec::new(),
            snapshot_mask_quads: Vec::new(),
            snapshot_color_quads: Vec::new(),
            snapshot_text_items: Vec::new(),
            snapshot_text_stats: RetainedTextPrepareStats::default(),
        }
    }
}

impl RetainedFramePreparer {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedFrameState,
        texts: &TextResourceArena,
        fonts: &FontResourceArena,
        geometries: &GeometryResourceArena,
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedPrepareError> {
        self.build_scratch_frame(frame, texts, fonts, geometries)?;

        // #339/#341 intentionally keep the atlas inside the retained text preparer.
        // Snapshot the lightweight prepared records once so that borrow can end and
        // the atlas can be borrowed alongside them for the parent GPU renderer.
        {
            let prepared = self
                .text
                .prepare(device, queue, frame, texts, fonts, metrics)?;
            self.snapshot_mask_quads.clear();
            self.snapshot_mask_quads.extend_from_slice(prepared.mask_quads);
            self.snapshot_color_quads.clear();
            self.snapshot_color_quads
                .extend_from_slice(prepared.color_quads);
            self.snapshot_text_items.clear();
            self.snapshot_text_items.extend_from_slice(prepared.items);
            self.snapshot_text_stats = prepared.stats;
        }

        let geometry = self.geometry.prepare(&self.scratch);
        self.render_items.clear();
        rebuild_mixed_order(
            &mut self.render_items,
            &self.sources,
            &self.snapshot_text_items,
            &geometry,
        );
        let glyph_batches = self
            .render_items
            .iter()
            .filter(|item| matches!(item, RetainedRenderItem::Glyph { .. }))
            .count();
        let (outline_cache_hits, outline_cache_misses) = self.outlines.stats();
        let stats = RetainedPrepareStats {
            semantic_objects: frame.objects.len(),
            geometry_slots: self.scratch.objects.len(),
            glyph_batches,
            vector_items: self.snapshot_text_stats.vector_items,
            outline_runs: self.snapshot_text_stats.outline_runs,
            outline_cache_hits,
            outline_cache_misses,
        };
        let text = PreparedRetainedTextSnapshot {
            time: frame.time,
            mask_quads: &self.snapshot_mask_quads,
            color_quads: &self.snapshot_color_quads,
            items: &self.snapshot_text_items,
            stats: self.snapshot_text_stats,
            atlas: self.text.atlas(),
        };
        Ok(PreparedRetainedGpuFrame {
            geometry,
            text,
            render_items: &self.render_items,
            stats,
        })
    }

    fn build_scratch_frame(
        &mut self,
        frame: &RetainedFrameState,
        texts: &TextResourceArena,
        fonts: &FontResourceArena,
        geometries: &GeometryResourceArena,
    ) -> Result<(), RetainedPrepareError> {
        self.scratch.time = frame.time;
        self.scratch.objects.clear();
        self.scratch.presences.clear();
        self.scratch.reveals.clear();
        self.scratch.morphs.clear();
        self.scratch.render_geometries.clear();
        self.sources.clear();

        for (object_index, object) in frame.objects.iter().enumerate() {
            if !frame.is_present(object_index) {
                continue;
            }
            match &object.content {
                ObjectContentRef::Geometry(geometry) => {
                    let geometry = resolve_geometry_ref(geometry, geometries)?;
                    self.push_geometry(
                        object.id,
                        geometry,
                        object.transform,
                        object.style,
                        object.appearance,
                        frame.reveal(object_index),
                        frame.morph(object_index),
                    );
                }
                ObjectContentRef::Text(handle) => {
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
                }
            }
        }
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
    ) {
        let scratch_id = ObjectId::new(self.scratch.objects.len() as u64);
        let kind = geometry_kind(&geometry);
        self.scratch.objects.push(FrameObjectState {
            id: scratch_id,
            geometry,
            transform,
            style,
            appearance,
        });
        self.scratch.presences.push(true);
        self.scratch.reveals.push(reveal);
        self.scratch.morphs.push(morph);
        self.scratch.render_geometries.push(None);
        self.sources.push(SourceItem::Geometry {
            object_id,
            scratch_id,
            kind,
        });
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
        geometries: &GeometryResourceArena,
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
        fonts: &FontResourceArena,
    ) -> Result<(), RetainedPrepareError> {
        let mut fill_path = VectorPath::new();
        let mut stroke_path = VectorPath::new();
        for glyph in run.glyphs.iter() {
            let (key, outline) = self.outlines.outline(fonts, run, glyph.glyph_id)?;
            fill_path = append_transformed_path(
                fill_path,
                outline.as_ref(),
                run.transform,
                glyph.origin,
            );
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

fn geometry_kind(geometry: &GeometryRef) -> ScratchGeometryKind {
    match geometry {
        GeometryRef::Circle { .. } => ScratchGeometryKind::Circle,
        GeometryRef::Rectangle { .. } => ScratchGeometryKind::Rectangle,
        GeometryRef::Line { .. } => ScratchGeometryKind::Line,
        GeometryRef::VectorPath(_) => ScratchGeometryKind::Path,
        GeometryRef::External(_) => ScratchGeometryKind::Unsupported,
    }
}

fn resolve_geometry_ref(
    geometry: &GeometryRef,
    geometries: &GeometryResourceArena,
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
                kind,
            } => match kind {
                ScratchGeometryKind::Circle => {
                    if let Some(&index) = circle_indices.get(scratch_id) {
                        push_geometry_item(output, *object_id, RenderPrimitive::Circle, index);
                    }
                }
                ScratchGeometryKind::Rectangle => {
                    if let Some(&index) = rectangle_indices.get(scratch_id) {
                        push_geometry_item(output, *object_id, RenderPrimitive::Rectangle, index);
                    }
                }
                ScratchGeometryKind::Line => {
                    if let Some(indices) = line_indices.get(scratch_id) {
                        for &index in indices {
                            push_geometry_item(output, *object_id, RenderPrimitive::Line, index);
                        }
                    }
                }
                ScratchGeometryKind::Path => {
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
                    }
                    // `Create` reveal heads are packed as a line sharing the scratch
                    // ID and must remain immediately above this path body.
                    if let Some(indices) = line_indices.get(scratch_id) {
                        for &index in indices {
                            push_geometry_item(output, *object_id, RenderPrimitive::Line, index);
                        }
                    }
                }
                ScratchGeometryKind::Unsupported => {}
            },
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
    let end = start.checked_add(1).expect("retained render instance exceeds u32 limits");
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
            ZenoCommand::QuadTo(control, point) => path.quadratic_to(
                Vec2::new(control.x, control.y),
                Vec2::new(point.x, point.y),
            ),
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
        }
    }
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
        let geometry = self.upload(device, queue, &prepared.geometry);
        text_state.glyphs.set_camera(queue, text_camera(self.camera));
        let text_frame = prepared.text.as_prepared_frame();
        let text = text_state
            .glyphs
            .upload(device, queue, &text_frame, prepared.text.atlas);
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
            RenderPrimitive::Path { batch: path_batch_index } => {
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
            RenderPrimitive::MegaPath { batch: mega_batch_index } => {
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
    use super::*;

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