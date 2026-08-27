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
    Color, FontResourceArena, FontVariationSetting, GlyphRun, TextRenderItem, TextResourceArena,
    TextResourceHandle, Transform2D, Vec2,
};
use noon_runtime::RetainedFrameState;
use noon_text_atlas::{
    GlyphAtlasEntry, GlyphAtlasError, GlyphAtlasPlane, GlyphAtlasStats, GpuGlyphAtlas,
    DEFAULT_GLYPH_ATLAS_EXTENT,
};
use noon_text_raster::{
    GlyphRaster, GlyphRasterCache, GlyphRasterError, GlyphRasterKey, GlyphRasterStats,
};

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
/// Raster and atlas caches survive frame preparation so translation/opacity changes
/// only rebuild inexpensive quad records. The glyph raster identity intentionally
/// excludes position; changes to effective device scale select a new integer pixel
/// bucket instead.
pub struct RetainedTextQuadPreparer {
    raster_cache: GlyphRasterCache,
    atlas: GpuGlyphAtlas,
    mask_quads: Vec<GlyphQuadInstance>,
    color_quads: Vec<GlyphQuadInstance>,
    items: Vec<PreparedTextItem>,
    stats: RetainedTextPrepareStats,
}

impl RetainedTextQuadPreparer {
    pub fn new(atlas_extent: u32) -> Result<Self, GlyphAtlasError> {
        Ok(Self {
            raster_cache: GlyphRasterCache::new(),
            atlas: GpuGlyphAtlas::new(atlas_extent)?,
            mask_quads: Vec::new(),
            color_quads: Vec::new(),
            items: Vec::new(),
            stats: RetainedTextPrepareStats::default(),
        })
    }

    pub fn with_default_atlas() -> Self {
        Self::new(DEFAULT_GLYPH_ATLAS_EXTENT).expect("default glyph atlas extent is valid")
    }

    pub fn raster_stats(&self) -> GlyphRasterStats {
        self.raster_cache.stats()
    }

    pub const fn atlas_stats(&self) -> GlyphAtlasStats {
        self.atlas.stats()
    }

    pub fn atlas(&self) -> &GpuGlyphAtlas {
        &self.atlas
    }

    pub fn prepare<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedFrameState,
        texts: &TextResourceArena,
        fonts: &FontResourceArena,
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedTextFrame<'a>, TextPrepareError> {
        metrics.validate()?;
        self.mask_quads.clear();
        self.color_quads.clear();
        self.items.clear();
        self.stats = RetainedTextPrepareStats::default();

        for (object_index, object) in frame.objects.iter().enumerate() {
            if !frame.is_present(object_index) {
                continue;
            }
            let Some(text_handle) = object.text() else {
                continue;
            };
            let resource = texts
                .get(text_handle)
                .ok_or(TextPrepareError::MissingTextResource(text_handle))?;
            let object_index = u32::try_from(object_index)
                .expect("retained frame object count exceeds u32 painter-order limits");
            self.stats.text_objects += 1;
            let reveal = frame.reveal(object_index as usize);
            let morph = frame.morph(object_index as usize);

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
        }

        Ok(PreparedRetainedTextFrame {
            time: frame.time,
            mask_quads: &self.mask_quads,
            color_quads: &self.color_quads,
            items: &self.items,
            stats: self.stats,
        })
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
            instance_range,
        }) = self.items.last_mut()
        {
            if *last_object == object_index
                && *last_text == text
                && *last_run == run_index
                && *last_plane == plane
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
    Ok(requested.ceil().max(1.0))
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
    use noon_core::{ObjectContentRef, ObjectId, Style, TextResourceArena, Transform2D};
    use noon_runtime::{RetainedFrameObjectState, RetainedFrameState};
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
