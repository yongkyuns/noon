#![forbid(unsafe_code)]

//! Retained text preparation between Noon's semantic text resources and the wgpu renderer.
//!
//! This layer owns device-scale raster selection, CPU glyph-raster reuse, GPU atlas
//! residency, and world-space glyph quads. It deliberately does not turn text into
//! `GeometryRef`: vector decorations and outline-required glyph runs remain explicit
//! painter-order entries for the shared geometry renderer to consume.

use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use noon_core::{
    Color, FontResourceArena, FontVariationSetting, GlyphRun, ObjectId, TextRenderItem,
    TextResourceArena, TextResourceHandle, Transform2D, Vec2,
};
use noon_runtime::{FrameChanges, RetainedFrameState};
use noon_text_atlas::{
    GlyphAtlasEntry, GlyphAtlasError, GlyphAtlasPlane, GlyphAtlasStats, GpuGlyphAtlas,
    DEFAULT_GLYPH_ATLAS_EXTENT,
};
use noon_text_raster::{
    GlyphRaster, GlyphRasterCache, GlyphRasterCacheLimits, GlyphRasterError, GlyphRasterKey,
    GlyphRasterStats,
};

pub const DEFAULT_GLYPH_RASTER_CACHE_MAX_ENTRIES: usize = 8_192;
pub const DEFAULT_GLYPH_RASTER_CACHE_MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
pub const GLYPH_RASTER_SIZE_BUCKET_RATIO: f32 = 1.125;
pub const GLYPH_RASTER_SIZE_BUCKET_START: f32 = 256.0;

pub const DEFAULT_GLYPH_RASTER_CACHE_LIMITS: GlyphRasterCacheLimits = GlyphRasterCacheLimits::new(
    DEFAULT_GLYPH_RASTER_CACHE_MAX_ENTRIES,
    DEFAULT_GLYPH_RASTER_CACHE_MAX_IMAGE_BYTES,
);

/// Device density needed to select a position-independent glyph raster size.
///
/// Components are physical backing pixels per Noon world unit. Keeping this input
/// explicit makes DPR/camera changes observable to preparation without coupling the
/// retained text resource to one viewport or display density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextDeviceMetrics {
    pub pixels_per_world: Vec2,
}

impl TextDeviceMetrics {
    pub fn new(pixels_per_world: Vec2) -> Result<Self, TextPrepareError> {
        let metrics = Self { pixels_per_world };
        metrics.validate()?;
        Ok(metrics)
    }

    pub fn uniform(pixels_per_world: f32) -> Result<Self, TextPrepareError> {
        Self::new(Vec2::new(pixels_per_world, pixels_per_world))
    }

    fn validate(self) -> Result<(), TextPrepareError> {
        if !self.pixels_per_world.x.is_finite()
            || !self.pixels_per_world.y.is_finite()
            || self.pixels_per_world.x <= 0.0
            || self.pixels_per_world.y <= 0.0
        {
            return Err(TextPrepareError::InvalidDeviceMetrics);
        }
        Ok(())
    }
}

/// One atlas-backed glyph bitmap in world coordinates.
///
/// `origin` is the bitmap's bottom-left corner. `axis_x` and `axis_y` span the
/// complete bitmap and can represent rotation, non-uniform scale, reflection, and
/// retained backend skew without baking those transforms into atlas identity.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GlyphQuadInstance {
    pub origin: [f32; 2],
    pub axis_x: [f32; 2],
    pub axis_y: [f32; 2],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    /// Alpha-mask glyphs use this as their tint. Color glyphs use RGB=1 and the
    /// alpha component as the owning object's opacity multiplier.
    pub color: [f32; 4],
}

/// Painter-ordered retained text item prepared for renderer integration.
///
/// Glyph batches index the plane-specific quad arrays. Vector and outline entries
/// deliberately remain semantic resource references so the shared geometry path can
/// render them without duplicating path storage in this crate.
#[derive(Clone, Debug, PartialEq)]
pub enum PreparedTextItem {
    GlyphBatch {
        object_index: u32,
        text: TextResourceHandle,
        run_index: u32,
        plane: GlyphAtlasPlane,
        page: u32,
        instance_range: Range<u32>,
    },
    Vector {
        object_index: u32,
        text: TextResourceHandle,
        vector_index: u32,
        reveal: f32,
        morph: f32,
    },
    OutlineRun {
        object_index: u32,
        text: TextResourceHandle,
        run_index: u32,
        reveal: f32,
        morph: f32,
    },
}

impl PreparedTextItem {
    pub const fn object_index(&self) -> u32 {
        match self {
            Self::GlyphBatch { object_index, .. }
            | Self::Vector { object_index, .. }
            | Self::OutlineRun { object_index, .. } => *object_index,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedTextPrepareStats {
    pub text_objects: usize,
    pub glyph_runs: usize,
    pub mask_quads: usize,
    pub color_quads: usize,
    pub empty_glyphs: usize,
    pub vector_items: usize,
    pub outline_runs: usize,
}

/// Cumulative counters for retained text preparation locality.
///
/// `reused_frames` return the existing prepared arrays untouched. Successful
/// `object_update_frames` mutate only already-resident quad records for changed text
/// objects and perform no raster or atlas lookups. Incompatible incremental changes
/// are counted as `fallback_rebuilds` before taking the conservative full path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedTextIncrementalStats {
    pub rebuild_attempts: u64,
    pub reused_frames: u64,
    pub object_update_frames: u64,
    pub objects_updated: u64,
    pub fallback_rebuilds: u64,
}

#[derive(Clone, Debug)]
struct PreparedTextObjectState {
    id: ObjectId,
    text: TextResourceHandle,
    transform: Transform2D,
    reveal: f32,
    morph: f32,
    item_range: Range<usize>,
}

#[derive(Debug)]
pub struct PreparedRetainedTextFrame<'a> {
    pub time: f64,
    pub mask_quads: &'a [GlyphQuadInstance],
    pub color_quads: &'a [GlyphQuadInstance],
    pub items: &'a [PreparedTextItem],
    pub stats: RetainedTextPrepareStats,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextPrepareError {
    InvalidDeviceMetrics,
    InvalidTextTransform,
    MissingTextResource(TextResourceHandle),
    Raster(GlyphRasterError),
    Atlas(GlyphAtlasError),
}

impl std::fmt::Display for TextPrepareError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDeviceMetrics => formatter.write_str(
                "text device metrics must contain finite positive pixels-per-world values",
            ),
            Self::InvalidTextTransform => {
                formatter.write_str("retained text transform produced a non-finite device scale")
            }
            Self::MissingTextResource(handle) => write!(
                formatter,
                "retained text resource {}:{} is not present in the text arena",
                handle.id.get(),
                handle.version
            ),
            Self::Raster(error) => write!(formatter, "glyph raster preparation failed: {error}"),
            Self::Atlas(error) => write!(formatter, "glyph atlas preparation failed: {error}"),
        }
    }
}

impl std::error::Error for TextPrepareError {}

impl From<GlyphRasterError> for TextPrepareError {
    fn from(value: GlyphRasterError) -> Self {
        Self::Raster(value)
    }
}

impl From<GlyphAtlasError> for TextPrepareError {
    fn from(value: GlyphAtlasError) -> Self {
        Self::Atlas(value)
    }
}

/// Persistent retained text preparation state.
///
/// Raster and atlas caches survive frame preparation. Stable object-local ranges let
/// translation and paint/opacity changes update already-resident quads without
/// rescanning unrelated text or probing those caches. Glyph raster identity excludes
/// position; ordinary display sizes preserve the legacy integer-pixel identity, while
/// very large device scales use conservative geometric residency buckets.
pub struct RetainedTextQuadPreparer {
    raster_cache: GlyphRasterCache,
    atlas: GpuGlyphAtlas,
    mask_quads: Vec<GlyphQuadInstance>,
    color_quads: Vec<GlyphQuadInstance>,
    items: Vec<PreparedTextItem>,
    object_states: Vec<Option<PreparedTextObjectState>>,
    stats: RetainedTextPrepareStats,
    incremental_stats: RetainedTextIncrementalStats,
    last_metrics: Option<TextDeviceMetrics>,
    prepared_once: bool,
}

impl RetainedTextQuadPreparer {
    pub fn new(atlas_extent: u32) -> Result<Self, GlyphAtlasError> {
        Self::with_raster_cache_limits(atlas_extent, DEFAULT_GLYPH_RASTER_CACHE_LIMITS)
    }

    pub fn with_raster_cache_limits(
        atlas_extent: u32,
        raster_limits: GlyphRasterCacheLimits,
    ) -> Result<Self, GlyphAtlasError> {
        Ok(Self {
            raster_cache: GlyphRasterCache::with_limits(raster_limits),
            atlas: GpuGlyphAtlas::new(atlas_extent)?,
            mask_quads: Vec::new(),
            color_quads: Vec::new(),
            items: Vec::new(),
            object_states: Vec::new(),
            stats: RetainedTextPrepareStats::default(),
            incremental_stats: RetainedTextIncrementalStats::default(),
            last_metrics: None,
            prepared_once: false,
        })
    }

    pub fn with_default_atlas() -> Self {
        Self::new(DEFAULT_GLYPH_ATLAS_EXTENT).expect("default glyph atlas extent is valid")
    }

    pub fn raster_stats(&self) -> GlyphRasterStats {
        self.raster_cache.stats()
    }

    pub fn raster_cache_limits(&self) -> GlyphRasterCacheLimits {
        self.raster_cache.limits()
    }

    pub fn set_raster_cache_limits(&mut self, limits: GlyphRasterCacheLimits) {
        self.raster_cache.set_limits(limits);
    }

    pub const fn atlas_stats(&self) -> GlyphAtlasStats {
        self.atlas.stats()
    }

    pub const fn incremental_stats(&self) -> RetainedTextIncrementalStats {
        self.incremental_stats
    }

    pub fn atlas(&self) -> &GpuGlyphAtlas {
        &self.atlas
    }

    /// Compatibility entry point for callers that do not yet preserve runtime
    /// dirtiness. Treats the frame as fully dirty, matching the pre-incremental
    /// behavior exactly.
    pub fn prepare<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedFrameState,
        texts: &TextResourceArena,
        fonts: &FontResourceArena,
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedTextFrame<'a>, TextPrepareError> {
        let changes = FrameChanges::all();
        self.prepare_with_changes(device, queue, frame, &changes, texts, fonts, metrics)
    }

    /// Prepare retained text while preserving runtime dirty-state locality.
    ///
    /// Empty change sets reuse the prepared arrays. Non-empty compatible change sets
    /// update only the changed text objects' stable quad ranges. Anything that can
    /// change raster identity or prepared structure falls back before mutation.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_with_changes<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedFrameState,
        changes: &FrameChanges,
        texts: &TextResourceArena,
        fonts: &FontResourceArena,
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedTextFrame<'a>, TextPrepareError> {
        metrics.validate()?;
        if self.prepared_once && changes.is_empty() && self.last_metrics == Some(metrics) {
            self.incremental_stats.reused_frames =
                self.incremental_stats.reused_frames.saturating_add(1);
            return Ok(self.prepared_frame(frame.time));
        }

        if self.can_update_objects(frame, changes, texts, metrics)? {
            let updated = self.update_objects(frame, changes, texts);
            self.incremental_stats.object_update_frames = self
                .incremental_stats
                .object_update_frames
                .saturating_add(1);
            self.incremental_stats.objects_updated = self
                .incremental_stats
                .objects_updated
                .saturating_add(updated as u64);
            return Ok(self.prepared_frame(frame.time));
        }

        if self.prepared_once && !changes.is_all() && !changes.is_empty() {
            self.incremental_stats.fallback_rebuilds =
                self.incremental_stats.fallback_rebuilds.saturating_add(1);
        }
        self.full_rebuild(device, queue, frame, texts, fonts, metrics)?;
        Ok(self.prepared_frame(frame.time))
    }

    fn prepared_frame(&self, time: f64) -> PreparedRetainedTextFrame<'_> {
        PreparedRetainedTextFrame {
            time,
            mask_quads: &self.mask_quads,
            color_quads: &self.color_quads,
            items: &self.items,
            stats: self.stats,
        }
    }

    fn can_update_objects(
        &self,
        frame: &RetainedFrameState,
        changes: &FrameChanges,
        texts: &TextResourceArena,
        metrics: TextDeviceMetrics,
    ) -> Result<bool, TextPrepareError> {
        if !self.prepared_once
            || changes.is_all()
            || changes.is_structural()
            || changes.is_empty()
            || self.last_metrics != Some(metrics)
            || frame.objects.len() != self.object_states.len()
        {
            return Ok(false);
        }

        for &index in changes.object_indices() {
            let Some(object) = frame.objects.get(index) else {
                return Ok(false);
            };
            match self.object_states.get(index).and_then(Option::as_ref) {
                None => {
                    if frame.is_present(index) && object.text().is_some() {
                        return Ok(false);
                    }
                }
                Some(state) => {
                    if object.id != state.id
                        || !frame.is_present(index)
                        || object.text() != Some(state.text)
                        || object.transform.scale != state.transform.scale
                        || object.transform.rotation != state.transform.rotation
                        || frame.reveal(index) != state.reveal
                        || frame.morph(index) != state.morph
                    {
                        return Ok(false);
                    }
                    texts
                        .get(state.text)
                        .ok_or(TextPrepareError::MissingTextResource(state.text))?;
                }
            }
        }
        Ok(true)
    }

    fn update_objects(
        &mut self,
        frame: &RetainedFrameState,
        changes: &FrameChanges,
        texts: &TextResourceArena,
    ) -> usize {
        let mut updated = 0_usize;
        for &index in changes.object_indices() {
            let Some(state) = self.object_states[index].clone() else {
                continue;
            };
            let object = &frame.objects[index];
            let resource = texts
                .get(state.text)
                .expect("validated text resource must remain present during object-local update");
            let delta = object.transform.translation - state.transform.translation;
            let object_opacity = object.style.opacity * object.appearance;

            for item in &self.items[state.item_range.clone()] {
                let PreparedTextItem::GlyphBatch {
                    run_index,
                    plane,
                    instance_range,
                    ..
                } = item
                else {
                    continue;
                };
                let run = &resource.runs[*run_index as usize];
                let color = match plane {
                    GlyphAtlasPlane::Mask => color_with_opacity(
                        run.fill.or(object.style.fill).unwrap_or(Color::WHITE),
                        object_opacity,
                    ),
                    GlyphAtlasPlane::Color => [1.0, 1.0, 1.0, object_opacity],
                };
                let start = instance_range.start as usize;
                let end = instance_range.end as usize;
                let quads = match plane {
                    GlyphAtlasPlane::Mask => &mut self.mask_quads,
                    GlyphAtlasPlane::Color => &mut self.color_quads,
                };
                for quad in &mut quads[start..end] {
                    quad.origin[0] += delta.x;
                    quad.origin[1] += delta.y;
                    quad.color = color;
                }
            }

            let stored = self.object_states[index]
                .as_mut()
                .expect("validated text object state must remain present");
            stored.transform = object.transform;
            stored.reveal = frame.reveal(index);
            stored.morph = frame.morph(index);
            updated = updated.saturating_add(1);
        }
        updated
    }

    #[allow(clippy::too_many_arguments)]
    fn full_rebuild(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedFrameState,
        texts: &TextResourceArena,
        fonts: &FontResourceArena,
        metrics: TextDeviceMetrics,
    ) -> Result<(), TextPrepareError> {
        self.incremental_stats.rebuild_attempts =
            self.incremental_stats.rebuild_attempts.saturating_add(1);
        self.atlas.begin_generation();
        // A fallible rebuild must never leave an old successful generation reusable.
        // Clear validity before mutating any prepared arrays and restore it only after
        // the complete frame has been rebuilt successfully.
        self.prepared_once = false;
        self.last_metrics = None;
        self.mask_quads.clear();
        self.color_quads.clear();
        self.items.clear();
        self.object_states.clear();
        self.object_states.resize_with(frame.objects.len(), || None);
        self.stats = RetainedTextPrepareStats::default();

        for (object_slot, object) in frame.objects.iter().enumerate() {
            if !frame.is_present(object_slot) {
                continue;
            }
            let Some(text_handle) = object.text() else {
                continue;
            };
            let resource = texts
                .get(text_handle)
                .ok_or(TextPrepareError::MissingTextResource(text_handle))?;
            let object_index = u32::try_from(object_slot)
                .expect("retained frame object count exceeds u32 painter-order limits");
            self.stats.text_objects += 1;
            let reveal = frame.reveal(object_slot);
            let morph = frame.morph(object_slot);
            let item_start = self.items.len();

            for render_item in resource.render_items.iter().copied() {
                match render_item {
                    TextRenderItem::GlyphRun(run_index) => {
                        let run = &resource.runs[run_index as usize];
                        self.stats.glyph_runs += 1;
                        if run.stroke.is_some() || reveal < 1.0 || morph != 0.0 {
                            self.items.push(PreparedTextItem::OutlineRun {
                                object_index,
                                text: text_handle,
                                run_index,
                                reveal,
                                morph,
                            });
                            self.stats.outline_runs += 1;
                            continue;
                        }
                        self.prepare_run(
                            device,
                            queue,
                            object_index,
                            text_handle,
                            run_index,
                            object.transform,
                            object.style.fill,
                            object.style.opacity * object.appearance,
                            run,
                            fonts,
                            metrics,
                        )?;
                    }
                    TextRenderItem::Vector(vector_index) => {
                        self.items.push(PreparedTextItem::Vector {
                            object_index,
                            text: text_handle,
                            vector_index,
                            reveal,
                            morph,
                        });
                        self.stats.vector_items += 1;
                    }
                }
            }

            self.object_states[object_slot] = Some(PreparedTextObjectState {
                id: object.id,
                text: text_handle,
                transform: object.transform,
                reveal,
                morph,
                item_range: item_start..self.items.len(),
            });
        }

        self.last_metrics = Some(metrics);
        self.prepared_once = true;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_run(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        object_index: u32,
        text_handle: TextResourceHandle,
        run_index: u32,
        object_transform: Transform2D,
        object_fill: Option<Color>,
        object_opacity: f32,
        run: &GlyphRun,
        fonts: &FontResourceArena,
        metrics: TextDeviceMetrics,
    ) -> Result<(), TextPrepareError> {
        let pixel_size = raster_pixel_size(run, object_transform, metrics)?;
        let pixels_to_local = run.font_size / pixel_size;
        let mask_color = run.fill.or(object_fill).unwrap_or(Color::WHITE);

        for glyph in run.glyphs.iter() {
            let raster =
                self.raster_cache
                    .get_or_rasterize(fonts, run, glyph.glyph_id, pixel_size)?;
            let key = raster_key(fonts, run, glyph.glyph_id, pixel_size)?;
            let entry = self.atlas.insert(device, queue, key, raster.as_ref())?;
            let GlyphAtlasEntry::Image(atlas_image) = entry else {
                self.stats.empty_glyphs += 1;
                continue;
            };
            let GlyphRaster::Image(image) = raster.as_ref() else {
                unreachable!("atlas image entry must originate from a raster image");
            };

            let local_origin = glyph.origin
                + Vec2::new(
                    image.placement.left as f32 * pixels_to_local,
                    (image.placement.top as f32 - image.placement.height as f32) * pixels_to_local,
                );
            let local_axis_x = Vec2::new(image.placement.width as f32 * pixels_to_local, 0.0);
            let local_axis_y = Vec2::new(0.0, image.placement.height as f32 * pixels_to_local);
            let resource_origin = run.transform.transform_point(local_origin);
            let resource_axis_x = run.transform.transform_vector(local_axis_x);
            let resource_axis_y = run.transform.transform_vector(local_axis_y);
            let world_origin = object_transform.transform_point(resource_origin);
            let world_axis_x = transform_vector(object_transform, resource_axis_x);
            let world_axis_y = transform_vector(object_transform, resource_axis_y);

            let color = match atlas_image.plane {
                GlyphAtlasPlane::Mask => color_with_opacity(mask_color, object_opacity),
                GlyphAtlasPlane::Color => [1.0, 1.0, 1.0, object_opacity],
            };
            let quad = GlyphQuadInstance {
                origin: [world_origin.x, world_origin.y],
                axis_x: [world_axis_x.x, world_axis_x.y],
                axis_y: [world_axis_y.x, world_axis_y.y],
                uv_min: atlas_image.uv_min,
                uv_max: atlas_image.uv_max,
                color,
            };
            let instance_index = match atlas_image.plane {
                GlyphAtlasPlane::Mask => {
                    let index = self.mask_quads.len();
                    self.mask_quads.push(quad);
                    self.stats.mask_quads += 1;
                    index
                }
                GlyphAtlasPlane::Color => {
                    let index = self.color_quads.len();
                    self.color_quads.push(quad);
                    self.stats.color_quads += 1;
                    index
                }
            };
            self.push_glyph_batch(
                object_index,
                text_handle,
                run_index,
                atlas_image.plane,
                atlas_image.page,
                instance_index,
            );
        }
        Ok(())
    }

    fn push_glyph_batch(
        &mut self,
        object_index: u32,
        text: TextResourceHandle,
        run_index: u32,
        plane: GlyphAtlasPlane,
        page: u32,
        instance_index: usize,
    ) {
        let start = u32::try_from(instance_index).expect("text quad count exceeds u32 draw limits");
        let end = start
            .checked_add(1)
            .expect("text quad count exceeds u32 draw limits");
        if let Some(PreparedTextItem::GlyphBatch {
            object_index: last_object,
            text: last_text,
            run_index: last_run,
            plane: last_plane,
            page: last_page,
            instance_range,
        }) = self.items.last_mut()
        {
            if *last_object == object_index
                && *last_text == text
                && *last_run == run_index
                && *last_plane == plane
                && *last_page == page
                && instance_range.end == start
            {
                instance_range.end = end;
                return;
            }
        }
        self.items.push(PreparedTextItem::GlyphBatch {
            object_index,
            text,
            run_index,
            plane,
            page,
            instance_range: start..end,
        });
    }
}

impl Default for RetainedTextQuadPreparer {
    fn default() -> Self {
        Self::with_default_atlas()
    }
}

fn raster_pixel_size(
    run: &GlyphRun,
    object_transform: Transform2D,
    metrics: TextDeviceMetrics,
) -> Result<f32, TextPrepareError> {
    if !run.font_size.is_finite() || run.font_size <= 0.0 {
        return Err(TextPrepareError::Raster(GlyphRasterError::InvalidPixelSize));
    }
    let resource_x = run.transform.transform_vector(Vec2::new(1.0, 0.0));
    let resource_y = run.transform.transform_vector(Vec2::new(0.0, 1.0));
    let world_x = transform_vector(object_transform, resource_x);
    let world_y = transform_vector(object_transform, resource_y);
    let scale = largest_device_scale(world_x, world_y, metrics.pixels_per_world);
    if !scale.is_finite() {
        return Err(TextPrepareError::InvalidTextTransform);
    }
    let requested = run.font_size * scale;
    if !requested.is_finite() {
        return Err(TextPrepareError::InvalidTextTransform);
    }
    Ok(raster_size_bucket(requested))
}

/// Preserve the legacy integer-ceil raster identity at ordinary display sizes and
/// switch to conservative geometric residency buckets only for very large glyphs.
///
/// Keeping the ordinary path exact protects raster parity. Above the threshold,
/// rounding upward still guarantees that the selected raster never undersamples the
/// active transform while bounding the number of identities accumulated during
/// extreme smooth zoom.
fn raster_size_bucket(requested: f32) -> f32 {
    let requested = requested.max(1.0);
    let legacy = requested.ceil();
    if legacy <= GLYPH_RASTER_SIZE_BUCKET_START {
        return legacy;
    }

    let mut bucket = GLYPH_RASTER_SIZE_BUCKET_START;
    while bucket < requested {
        let next = bucket * GLYPH_RASTER_SIZE_BUCKET_RATIO;
        if !next.is_finite() || next <= bucket {
            return legacy;
        }
        bucket = next;
    }
    bucket
}

/// Largest singular value of the local-to-device 2x2 transform.
///
/// This is exact for rotation/non-uniform scale and remains conservative for skew,
/// avoiding both the sqrt(2) over-rasterization of a Frobenius bound and the
/// undersampling possible when only the two transformed basis lengths are compared.
fn largest_device_scale(axis_x: Vec2, axis_y: Vec2, pixels_per_world: Vec2) -> f32 {
    let a = axis_x.x * pixels_per_world.x;
    let c = axis_x.y * pixels_per_world.y;
    let b = axis_y.x * pixels_per_world.x;
    let d = axis_y.y * pixels_per_world.y;
    let trace = a * a + b * b + c * c + d * d;
    let determinant = a * d - b * c;
    let discriminant = (trace * trace - 4.0 * determinant * determinant).max(0.0);
    (0.5 * (trace + discriminant.sqrt())).max(0.0).sqrt()
}

fn transform_vector(transform: Transform2D, vector: Vec2) -> Vec2 {
    vector
        .component_mul(transform.scale)
        .rotate(transform.rotation)
}

fn color_with_opacity(color: Color, opacity: f32) -> [f32; 4] {
    [color.red, color.green, color.blue, color.alpha * opacity]
}

fn raster_key(
    fonts: &FontResourceArena,
    run: &GlyphRun,
    glyph_id: u32,
    pixel_size: f32,
) -> Result<GlyphRasterKey, TextPrepareError> {
    let font = fonts
        .handle_for_face(&run.font)
        .ok_or(TextPrepareError::Raster(
            GlyphRasterError::MissingFontResource,
        ))?;
    let glyph_id = u16::try_from(glyph_id)
        .map_err(|_| TextPrepareError::Raster(GlyphRasterError::GlyphIdOutOfRange(glyph_id)))?;
    Ok(GlyphRasterKey {
        font,
        glyph_id,
        pixel_size_bits: pixel_size.to_bits(),
        variation_fingerprint: variation_fingerprint(run.variations.as_ref()),
    })
}

// Must stay byte-for-byte equivalent to `noon-text-raster` cache identity. It is
// intentionally tiny and duplicated here because the raster cache returns the image
// while the atlas independently requires the same stable key.
fn variation_fingerprint(settings: &[FontVariationSetting]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for setting in settings {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use noon_core::{ObjectContentRef, ObjectId, Style, TextResourceArena, Transform2D};
    use noon_runtime::{FrameChanges, RetainedFrameObjectState, RetainedFrameState};
    use noon_typst::{compile_typst_resource, TypstMode};

    use super::*;

    fn retained_frame(
        text: TextResourceHandle,
        present: bool,
        transform: Transform2D,
    ) -> RetainedFrameState {
        RetainedFrameState {
            time: 0.0,
            objects: vec![RetainedFrameObjectState {
                id: ObjectId::new(1),
                content: ObjectContentRef::Text(text),
                transform,
                style: Style::default(),
                appearance: 1.0,
            }],
            presences: vec![present],
            reveals: vec![1.0],
            morphs: vec![0.0],
            render_geometries: vec![None],
        }
    }

    fn two_text_frame(text: TextResourceHandle) -> RetainedFrameState {
        let transform = scene_transform();
        RetainedFrameState {
            time: 0.0,
            objects: vec![
                RetainedFrameObjectState {
                    id: ObjectId::new(1),
                    content: ObjectContentRef::Text(text),
                    transform,
                    style: Style::default(),
                    appearance: 1.0,
                },
                RetainedFrameObjectState {
                    id: ObjectId::new(2),
                    content: ObjectContentRef::Text(text),
                    transform,
                    style: Style::default(),
                    appearance: 1.0,
                },
            ],
            presences: vec![true, true],
            reveals: vec![1.0, 1.0],
            morphs: vec![0.0, 0.0],
            render_geometries: vec![None, None],
        }
    }

    fn scene_transform() -> Transform2D {
        Transform2D {
            scale: Vec2::new(0.05, 0.05),
            ..Transform2D::IDENTITY
        }
    }

    fn metrics() -> TextDeviceMetrics {
        TextDeviceMetrics::uniform(67.5).unwrap()
    }

    #[test]
    fn renderer_uses_finite_default_raster_cache_limits() {
        let preparer = RetainedTextQuadPreparer::new(128).unwrap();
        assert_eq!(
            preparer.raster_cache_limits(),
            DEFAULT_GLYPH_RASTER_CACHE_LIMITS
        );
        assert!(preparer.raster_cache_limits().max_entries < usize::MAX);
        assert!(preparer.raster_cache_limits().max_image_bytes < usize::MAX);
    }

    #[test]
    fn ordinary_raster_sizes_preserve_legacy_integer_ceil_identity() {
        for requested in [0.0, 1.0, 1.01, 10.0, 67.5, 100.0, 255.0, 255.1, 256.0] {
            assert_eq!(raster_size_bucket(requested), requested.max(1.0).ceil());
        }
    }

    #[test]
    fn raster_size_buckets_never_undersample_requested_size() {
        for requested in [0.0, 1.0, 1.01, 10.0, 100.0, 256.1, 1_000.0, 100_000.0] {
            let bucket = raster_size_bucket(requested);
            assert!(bucket >= requested.max(1.0));
            assert!(bucket.is_finite());
        }
    }

    #[test]
    fn nearby_high_zoom_requests_share_raster_residency() {
        assert_eq!(raster_size_bucket(300.0), raster_size_bucket(301.0));
        assert_eq!(raster_size_bucket(301.0), raster_size_bucket(320.0));
        assert_ne!(raster_size_bucket(320.0), raster_size_bucket(330.0));
    }

    #[test]
    fn high_zoom_range_uses_bounded_geometric_bucket_count() {
        let buckets = (257..=1_000)
            .map(|requested| raster_size_bucket(requested as f32).to_bits())
            .collect::<BTreeSet<_>>();
        assert!(buckets.len() < 20);
        assert!(buckets.len() > 5);
    }

    #[test]
    fn ordinary_typst_text_prepares_mask_quads_and_allocates_one_plane() {
        let artifact = compile_typst_resource("Hi", TypstMode::Markup).unwrap();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let frame = retained_frame(handle, true, scene_transform());
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedTextQuadPreparer::new(128).unwrap();

        let prepared = preparer
            .prepare(&device, &queue, &frame, &texts, &artifact.fonts, metrics())
            .unwrap();
        assert!(!prepared.mask_quads.is_empty());
        assert!(prepared.color_quads.is_empty());
        assert!(prepared
            .items
            .iter()
            .all(|item| matches!(item, PreparedTextItem::GlyphBatch { .. })));
        assert_eq!(prepared.stats.outline_runs, 0);
        assert!(preparer
            .atlas()
            .texture_view(GlyphAtlasPlane::Mask)
            .is_some());
        assert!(preparer
            .atlas()
            .texture_view(GlyphAtlasPlane::Color)
            .is_none());
    }

    #[test]
    fn math_preserves_backend_glyph_vector_painter_order() {
        let artifact = compile_typst_resource("frac(x, 2)", TypstMode::Math).unwrap();
        let expected = artifact
            .resource
            .render_items
            .iter()
            .map(|item| match item {
                TextRenderItem::GlyphRun(_) => "glyph",
                TextRenderItem::Vector(_) => "vector",
            })
            .collect::<Vec<_>>();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let frame = retained_frame(handle, true, scene_transform());
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedTextQuadPreparer::new(256).unwrap();

        let prepared = preparer
            .prepare(&device, &queue, &frame, &texts, &artifact.fonts, metrics())
            .unwrap();
        let actual = prepared
            .items
            .iter()
            .map(|item| match item {
                PreparedTextItem::GlyphBatch { .. } | PreparedTextItem::OutlineRun { .. } => {
                    "glyph"
                }
                PreparedTextItem::Vector { .. } => "vector",
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        assert!(prepared.stats.vector_items > 0);
        assert!(prepared.stats.mask_quads > 0);
    }

    #[test]
    fn stroked_glyph_runs_stay_on_the_outline_lane() {
        let artifact = compile_typst_resource(
            "#text(stroke: (paint: red, thickness: 1pt, dash: \"dashed\"))[A]",
            TypstMode::Markup,
        )
        .unwrap();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let frame = retained_frame(handle, true, scene_transform());
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedTextQuadPreparer::new(128).unwrap();

        let prepared = preparer
            .prepare(&device, &queue, &frame, &texts, &artifact.fonts, metrics())
            .unwrap();
        assert!(prepared.mask_quads.is_empty());
        assert!(prepared.color_quads.is_empty());
        assert!(prepared
            .items
            .iter()
            .any(|item| matches!(item, PreparedTextItem::OutlineRun { .. })));
        assert_eq!(prepared.stats.outline_runs, 1);
        assert_eq!(preparer.atlas_stats().entries, 0);
        assert_eq!(preparer.raster_stats().entries, 0);
    }

    #[test]
    fn partial_reveal_routes_fill_glyphs_to_outlines_without_rasterizing() {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let mut frame = retained_frame(handle, true, scene_transform());
        frame.reveals[0] = 0.4;
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedTextQuadPreparer::new(128).unwrap();

        let prepared = preparer
            .prepare(&device, &queue, &frame, &texts, &artifact.fonts, metrics())
            .unwrap();
        assert!(prepared.mask_quads.is_empty());
        assert!(matches!(
            prepared.items.first(),
            Some(PreparedTextItem::OutlineRun { reveal, .. }) if (*reveal - 0.4).abs() < f32::EPSILON
        ));
        assert_eq!(preparer.raster_stats().entries, 0);
    }

    #[test]
    fn absent_text_objects_do_not_touch_raster_or_atlas_caches() {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let frame = retained_frame(handle, false, scene_transform());
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedTextQuadPreparer::new(128).unwrap();

        let prepared = preparer
            .prepare(&device, &queue, &frame, &texts, &artifact.fonts, metrics())
            .unwrap();
        assert!(prepared.items.is_empty());
        assert!(prepared.mask_quads.is_empty());
        assert_eq!(prepared.stats.text_objects, 0);
        assert_eq!(preparer.raster_stats().entries, 0);
        assert_eq!(preparer.atlas_stats().entries, 0);
    }

    #[test]
    fn translation_changes_quads_without_changing_glyph_residency() {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let first_frame = retained_frame(handle, true, scene_transform());
        let mut translated = scene_transform();
        translated.translation = Vec2::new(2.0, -3.0);
        let second_frame = retained_frame(handle, true, translated);
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedTextQuadPreparer::new(128).unwrap();

        let first_quad = {
            let prepared = preparer
                .prepare(
                    &device,
                    &queue,
                    &first_frame,
                    &texts,
                    &artifact.fonts,
                    metrics(),
                )
                .unwrap();
            prepared.mask_quads[0]
        };
        let atlas_entries = preparer.atlas_stats().entries;
        let raster_entries = preparer.raster_stats().entries;
        let first_raster_hits = preparer.raster_stats().hits;
        let first_atlas_hits = preparer.atlas_stats().hits;

        let second_quad = {
            let prepared = preparer
                .prepare(
                    &device,
                    &queue,
                    &second_frame,
                    &texts,
                    &artifact.fonts,
                    metrics(),
                )
                .unwrap();
            prepared.mask_quads[0]
        };
        assert!((second_quad.origin[0] - first_quad.origin[0] - 2.0).abs() < 1e-5);
        assert!((second_quad.origin[1] - first_quad.origin[1] + 3.0).abs() < 1e-5);
        assert_eq!(second_quad.axis_x, first_quad.axis_x);
        assert_eq!(second_quad.axis_y, first_quad.axis_y);
        assert_eq!(preparer.atlas_stats().entries, atlas_entries);
        assert_eq!(preparer.raster_stats().entries, raster_entries);
        assert!(preparer.raster_stats().hits > first_raster_hits);
        assert!(preparer.atlas_stats().hits > first_atlas_hits);
    }

    #[test]
    fn unchanged_runtime_frame_reuses_prepared_text_without_cache_lookups() {
        let artifact = compile_typst_resource("Static", TypstMode::Markup).unwrap();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let frame = retained_frame(handle, true, scene_transform());
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedTextQuadPreparer::new(256).unwrap();

        {
            let prepared = preparer
                .prepare_with_changes(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::all(),
                    &texts,
                    &artifact.fonts,
                    metrics(),
                )
                .unwrap();
            assert!(!prepared.mask_quads.is_empty());
        }
        let raster = preparer.raster_stats();
        let atlas = preparer.atlas_stats();

        {
            let prepared = preparer
                .prepare_with_changes(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::default(),
                    &texts,
                    &artifact.fonts,
                    metrics(),
                )
                .unwrap();
            assert!(!prepared.mask_quads.is_empty());
        }

        assert_eq!(preparer.raster_stats(), raster);
        assert_eq!(preparer.atlas_stats(), atlas);
        assert_eq!(
            preparer.incremental_stats(),
            RetainedTextIncrementalStats {
                rebuild_attempts: 1,
                reused_frames: 1,
                ..RetainedTextIncrementalStats::default()
            }
        );
    }

    #[test]
    fn changed_text_translation_and_opacity_update_only_its_quads() {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let mut frame = two_text_frame(handle);
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedTextQuadPreparer::new(256).unwrap();

        let (before, batches) = {
            let prepared = preparer
                .prepare_with_changes(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::all(),
                    &texts,
                    &artifact.fonts,
                    metrics(),
                )
                .unwrap();
            let batches = prepared
                .items
                .iter()
                .filter_map(|item| match item {
                    PreparedTextItem::GlyphBatch {
                        object_index,
                        plane: GlyphAtlasPlane::Mask,
                        instance_range,
                        ..
                    } => Some((*object_index, instance_range.clone())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            (prepared.mask_quads.to_vec(), batches)
        };
        let raster = preparer.raster_stats();
        let atlas = preparer.atlas_stats();

        frame.objects[1].transform.translation = Vec2::new(2.0, -3.0);
        frame.objects[1].style.opacity = 0.5;
        frame.objects[1].appearance = 0.5;
        let after = {
            let prepared = preparer
                .prepare_with_changes(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::objects(vec![1]),
                    &texts,
                    &artifact.fonts,
                    metrics(),
                )
                .unwrap();
            prepared.mask_quads.to_vec()
        };

        assert_eq!(preparer.raster_stats(), raster);
        assert_eq!(preparer.atlas_stats(), atlas);
        for (object_index, range) in batches {
            let start = range.start as usize;
            let end = range.end as usize;
            if object_index == 0 {
                assert_eq!(&after[start..end], &before[start..end]);
            } else {
                for (after_quad, before_quad) in after[start..end].iter().zip(&before[start..end]) {
                    assert!((after_quad.origin[0] - before_quad.origin[0] - 2.0).abs() < 1e-5);
                    assert!((after_quad.origin[1] - before_quad.origin[1] + 3.0).abs() < 1e-5);
                    assert_eq!(after_quad.axis_x, before_quad.axis_x);
                    assert_eq!(after_quad.axis_y, before_quad.axis_y);
                    assert_eq!(after_quad.uv_min, before_quad.uv_min);
                    assert_eq!(after_quad.uv_max, before_quad.uv_max);
                    assert!((after_quad.color[3] - before_quad.color[3] * 0.25).abs() < 1e-6);
                }
            }
        }
        assert_eq!(preparer.incremental_stats().rebuild_attempts, 1);
        assert_eq!(preparer.incremental_stats().object_update_frames, 1);
        assert_eq!(preparer.incremental_stats().objects_updated, 1);
        assert_eq!(preparer.incremental_stats().fallback_rebuilds, 0);
    }

    #[test]
    fn text_scale_change_falls_back_to_full_rebuild() {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let mut frame = retained_frame(handle, true, scene_transform());
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedTextQuadPreparer::new(256).unwrap();

        preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::all(),
                &texts,
                &artifact.fonts,
                metrics(),
            )
            .unwrap();
        frame.objects[0].transform.scale = Vec2::new(0.1, 0.1);
        preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::objects(vec![0]),
                &texts,
                &artifact.fonts,
                metrics(),
            )
            .unwrap();

        assert_eq!(preparer.incremental_stats().rebuild_attempts, 2);
        assert_eq!(preparer.incremental_stats().object_update_frames, 0);
        assert_eq!(preparer.incremental_stats().objects_updated, 0);
        assert_eq!(preparer.incremental_stats().fallback_rebuilds, 1);
    }

    #[test]
    fn text_object_identity_change_falls_back_to_full_rebuild() {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let mut frame = retained_frame(handle, true, scene_transform());
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedTextQuadPreparer::new(256).unwrap();

        preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::all(),
                &texts,
                &artifact.fonts,
                metrics(),
            )
            .unwrap();
        frame.objects[0].id = ObjectId::new(99);
        preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::objects(vec![0]),
                &texts,
                &artifact.fonts,
                metrics(),
            )
            .unwrap();

        assert_eq!(preparer.incremental_stats().rebuild_attempts, 2);
        assert_eq!(preparer.incremental_stats().object_update_frames, 0);
        assert_eq!(preparer.incremental_stats().fallback_rebuilds, 1);
    }

    #[test]
    fn failed_full_rebuild_cannot_be_reused_as_a_valid_generation() {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let frame = retained_frame(handle, true, scene_transform());
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedTextQuadPreparer::new(256).unwrap();

        preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::all(),
                &texts,
                &artifact.fonts,
                metrics(),
            )
            .unwrap();
        let missing = TextResourceArena::new();
        assert!(matches!(
            preparer.prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::all(),
                &missing,
                &artifact.fonts,
                metrics(),
            ),
            Err(TextPrepareError::MissingTextResource(found)) if found == handle
        ));

        let prepared = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::default(),
                &texts,
                &artifact.fonts,
                metrics(),
            )
            .unwrap();
        assert!(!prepared.mask_quads.is_empty());
        assert_eq!(preparer.incremental_stats().rebuild_attempts, 3);
        assert_eq!(preparer.incremental_stats().reused_frames, 0);
    }

    #[test]
    fn device_metric_change_invalidates_unchanged_frame_reuse() {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(artifact.resource).unwrap();
        let frame = retained_frame(handle, true, scene_transform());
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedTextQuadPreparer::new(256).unwrap();

        preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::all(),
                &texts,
                &artifact.fonts,
                metrics(),
            )
            .unwrap();
        preparer
            .prepare_with_changes(
                &device,
                &queue,
                &frame,
                &FrameChanges::default(),
                &texts,
                &artifact.fonts,
                TextDeviceMetrics::uniform(90.0).unwrap(),
            )
            .unwrap();

        assert_eq!(preparer.incremental_stats().rebuild_attempts, 2);
        assert_eq!(preparer.incremental_stats().reused_frames, 0);
    }

    #[test]
    fn device_scale_uses_exact_largest_singular_value() {
        let metrics = Vec2::new(100.0, 100.0);
        assert!(
            (largest_device_scale(Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0), metrics) - 100.0).abs()
                < 1e-4
        );
        assert!(
            (largest_device_scale(Vec2::new(2.0, 0.0), Vec2::new(0.0, 0.5), metrics) - 200.0).abs()
                < 1e-4
        );
    }

    #[test]
    fn invalid_device_metrics_fail_before_resource_lookup() {
        assert_eq!(
            TextDeviceMetrics::new(Vec2::new(0.0, 1.0)).unwrap_err(),
            TextPrepareError::InvalidDeviceMetrics
        );
    }
}
