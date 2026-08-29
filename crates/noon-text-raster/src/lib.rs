#![forbid(unsafe_code)]

//! Backend-neutral CPU glyph raster preparation for Noon.
//!
//! Shaping/layout backends retain authoritative glyph ids, positions, fonts, and
//! variable-font coordinates in `noon-core`. This crate only turns those retained
//! glyph ids into reusable alpha/color images. GPU texture allocation and draw
//! submission remain renderer concerns.

use std::{collections::HashMap, fmt, sync::Arc};

use noon_core::{
    FontResource, FontResourceArena, FontResourceHandle, FontVariationSetting, GlyphRun,
};
use swash::{
    scale::{image::Content, Render, ScaleContext, Source, StrikeWith},
    zeno::Format,
    CacheKey, FontRef, GlyphId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GlyphRasterFormat {
    Alpha8,
    Rgba8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GlyphRasterPlacement {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphRasterImage {
    pub format: GlyphRasterFormat,
    pub placement: GlyphRasterPlacement,
    pub data: Arc<[u8]>,
}

/// A shaped glyph can legitimately have no visual pixels (for example whitespace).
/// Keeping that as a cached result avoids repeatedly asking the font rasterizer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlyphRaster {
    Empty,
    Image(GlyphRasterImage),
}

/// Exact cache identity for one position-independent glyph image.
///
/// Fractional scene position is intentionally excluded. Noon rasterizes unhinted
/// glyphs at an explicit device-pixel size and applies subpixel translation when
/// placing atlas quads, so ordinary animation does not explode the glyph cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphRasterKey {
    pub font: FontResourceHandle,
    pub glyph_id: GlyphId,
    pub pixel_size_bits: u32,
    pub variation_fingerprint: u64,
}

impl GlyphRasterKey {
    pub fn pixel_size(self) -> f32 {
        f32::from_bits(self.pixel_size_bits)
    }
}

#[derive(Clone, Debug)]
pub struct PreparedGlyphRaster {
    pub glyph_index: u32,
    pub raster: Arc<GlyphRaster>,
}

/// Residency limits for position-independent CPU glyph images.
///
/// The entry cap bounds map/metadata growth, including zero-byte whitespace glyphs.
/// The image-byte cap bounds retained pixel payloads. An image larger than the byte
/// budget is still returned to the caller but is not admitted to the cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphRasterCacheLimits {
    pub max_entries: usize,
    pub max_image_bytes: usize,
}

impl GlyphRasterCacheLimits {
    pub const UNBOUNDED: Self = Self {
        max_entries: usize::MAX,
        max_image_bytes: usize::MAX,
    };

    pub const fn new(max_entries: usize, max_image_bytes: usize) -> Self {
        Self {
            max_entries,
            max_image_bytes,
        }
    }
}

impl Default for GlyphRasterCacheLimits {
    fn default() -> Self {
        Self::UNBOUNDED
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphRasterStats {
    pub entries: usize,
    pub image_bytes: usize,
    pub font_faces: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub rejected_admissions: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlyphRasterError {
    MissingFontResource,
    InvalidFontData(FontResourceHandle),
    GlyphIdOutOfRange(u32),
    InvalidPixelSize,
    InvalidVariation,
    OutlineRequired,
    UnexpectedSubpixelMask,
}

impl fmt::Display for GlyphRasterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFontResource => {
                write!(formatter, "glyph run has no retained font resource")
            }
            Self::InvalidFontData(handle) => write!(
                formatter,
                "retained font resource {}:{} is not valid OpenType data",
                handle.id.get(),
                handle.version
            ),
            Self::GlyphIdOutOfRange(glyph_id) => {
                write!(
                    formatter,
                    "glyph id {glyph_id} exceeds the OpenType glyph-id range"
                )
            }
            Self::InvalidPixelSize => {
                write!(formatter, "glyph pixel size must be finite and positive")
            }
            Self::InvalidVariation => write!(formatter, "glyph variation values must be finite"),
            Self::OutlineRequired => write!(
                formatter,
                "stroked glyph runs require retained outline rendering"
            ),
            Self::UnexpectedSubpixelMask => write!(
                formatter,
                "Swash returned a subpixel mask for an alpha-only atlas request"
            ),
        }
    }
}

impl std::error::Error for GlyphRasterError {}

#[derive(Clone, Copy)]
struct SwashFace {
    offset: u32,
    key: CacheKey,
}

#[derive(Clone)]
struct CachedGlyphRaster {
    raster: Arc<GlyphRaster>,
    image_bytes: usize,
    last_used: u64,
}

/// Position-independent CPU glyph raster cache used before GPU atlas allocation.
///
/// The cache deliberately does not shape text: `GlyphRun` already contains the
/// backend-authoritative glyph ids and positions. Swash is used only to turn those
/// exact glyph ids, retained font bytes, and retained variation settings into masks
/// or color images. This keeps Typst/native/LaTeX layout semantics out of renderers.
pub struct GlyphRasterCache {
    scale_context: ScaleContext,
    faces: HashMap<FontResourceHandle, SwashFace>,
    entries: HashMap<GlyphRasterKey, CachedGlyphRaster>,
    limits: GlyphRasterCacheLimits,
    image_bytes: usize,
    access_clock: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
    rejected_admissions: u64,
}

impl Default for GlyphRasterCache {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphRasterCache {
    /// Construct an unbounded cache for backward compatibility.
    ///
    /// Long-lived renderer owners should prefer [`Self::with_limits`] so glyph-size
    /// churn and large text workloads cannot grow retained CPU image memory forever.
    pub fn new() -> Self {
        Self::with_limits(GlyphRasterCacheLimits::UNBOUNDED)
    }

    pub fn with_limits(limits: GlyphRasterCacheLimits) -> Self {
        Self {
            scale_context: ScaleContext::new(),
            faces: HashMap::new(),
            entries: HashMap::new(),
            limits,
            image_bytes: 0,
            access_clock: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            rejected_admissions: 0,
        }
    }

    pub fn limits(&self) -> GlyphRasterCacheLimits {
        self.limits
    }

    /// Replace residency limits and immediately evict least-recently-used entries
    /// until the cache satisfies the new budget.
    pub fn set_limits(&mut self, limits: GlyphRasterCacheLimits) {
        self.limits = limits;
        self.enforce_limits();
    }

    pub fn stats(&self) -> GlyphRasterStats {
        GlyphRasterStats {
            entries: self.entries.len(),
            image_bytes: self.image_bytes,
            font_faces: self.faces.len(),
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            rejected_admissions: self.rejected_admissions,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear_images(&mut self) {
        self.entries.clear();
        self.faces.clear();
        self.image_bytes = 0;
        self.access_clock = 0;
        self.hits = 0;
        self.misses = 0;
        self.evictions = 0;
        self.rejected_admissions = 0;
    }

    /// Rasterize all glyphs in one retained run at an explicit device-pixel size.
    ///
    /// Pixel-size selection belongs to the renderer/camera policy layer. Keeping it
    /// explicit here lets a later atlas implementation choose stable size buckets
    /// without coupling retained text resources to a particular display density.
    pub fn prepare_run(
        &mut self,
        fonts: &FontResourceArena,
        run: &GlyphRun,
        pixel_size: f32,
    ) -> Result<Vec<PreparedGlyphRaster>, GlyphRasterError> {
        run.glyphs
            .iter()
            .enumerate()
            .map(|(glyph_index, glyph)| {
                Ok(PreparedGlyphRaster {
                    glyph_index: glyph_index as u32,
                    raster: self.get_or_rasterize(fonts, run, glyph.glyph_id, pixel_size)?,
                })
            })
            .collect()
    }

    pub fn get_or_rasterize(
        &mut self,
        fonts: &FontResourceArena,
        run: &GlyphRun,
        glyph_id: u32,
        pixel_size: f32,
    ) -> Result<Arc<GlyphRaster>, GlyphRasterError> {
        if !pixel_size.is_finite() || pixel_size <= 0.0 {
            return Err(GlyphRasterError::InvalidPixelSize);
        }
        if run
            .variations
            .iter()
            .any(|setting| !setting.value.is_finite())
        {
            return Err(GlyphRasterError::InvalidVariation);
        }
        if run.stroke.is_some() {
            return Err(GlyphRasterError::OutlineRequired);
        }

        let glyph_id = GlyphId::try_from(glyph_id)
            .map_err(|_| GlyphRasterError::GlyphIdOutOfRange(glyph_id))?;
        let font_handle = fonts
            .handle_for_face(&run.font)
            .ok_or(GlyphRasterError::MissingFontResource)?;
        let key = GlyphRasterKey {
            font: font_handle,
            glyph_id,
            pixel_size_bits: pixel_size.to_bits(),
            variation_fingerprint: variation_fingerprint(run.variations.as_ref()),
        };
        let access = self.next_access();
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = access;
            self.hits = self.hits.saturating_add(1);
            return Ok(entry.raster.clone());
        }

        self.misses = self.misses.saturating_add(1);
        let resource = fonts
            .get(font_handle)
            .ok_or(GlyphRasterError::MissingFontResource)?;
        let face = self.swash_face(font_handle, resource)?;
        let raster = Arc::new(self.rasterize(resource, face, run, glyph_id, pixel_size)?);
        let image_bytes = raster_image_bytes(raster.as_ref());

        if self.limits.max_entries == 0 || image_bytes > self.limits.max_image_bytes {
            self.rejected_admissions = self.rejected_admissions.saturating_add(1);
            return Ok(raster);
        }

        self.image_bytes = self.image_bytes.saturating_add(image_bytes);
        self.faces.entry(font_handle).or_insert(face);
        self.entries.insert(
            key,
            CachedGlyphRaster {
                raster: raster.clone(),
                image_bytes,
                last_used: access,
            },
        );
        self.enforce_limits();
        Ok(raster)
    }

    fn next_access(&mut self) -> u64 {
        self.access_clock = self.access_clock.saturating_add(1);
        self.access_clock
    }

    fn enforce_limits(&mut self) {
        while self.entries.len() > self.limits.max_entries
            || self.image_bytes > self.limits.max_image_bytes
        {
            let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| *key)
            else {
                break;
            };
            let removed = self
                .entries
                .remove(&oldest_key)
                .expect("selected glyph raster cache entry must still exist");
            self.image_bytes = self.image_bytes.saturating_sub(removed.image_bytes);
            self.evictions = self.evictions.saturating_add(1);
            if !self.entries.keys().any(|key| key.font == oldest_key.font) {
                self.faces.remove(&oldest_key.font);
            }
        }
    }

    fn swash_face(
        &self,
        handle: FontResourceHandle,
        resource: &FontResource,
    ) -> Result<SwashFace, GlyphRasterError> {
        if let Some(face) = self.faces.get(&handle).copied() {
            return Ok(face);
        }
        let font = FontRef::from_index(resource.data.as_ref(), resource.key.face_index as usize)
            .ok_or(GlyphRasterError::InvalidFontData(handle))?;
        Ok(SwashFace {
            offset: font.offset,
            key: font.key,
        })
    }

    fn rasterize(
        &mut self,
        resource: &FontResource,
        face: SwashFace,
        run: &GlyphRun,
        glyph_id: GlyphId,
        pixel_size: f32,
    ) -> Result<GlyphRaster, GlyphRasterError> {
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
            .size(pixel_size)
            .hint(false)
            .variations(variations.iter())
            .build();
        let sources = [
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ];
        let mut render = Render::new(&sources);
        render.format(Format::Alpha);
        let Some(image) = render.render(&mut scaler, glyph_id) else {
            return Ok(GlyphRaster::Empty);
        };

        let format = match image.content {
            Content::Mask => GlyphRasterFormat::Alpha8,
            Content::Color => GlyphRasterFormat::Rgba8,
            Content::SubpixelMask => return Err(GlyphRasterError::UnexpectedSubpixelMask),
        };
        Ok(GlyphRaster::Image(GlyphRasterImage {
            format,
            placement: GlyphRasterPlacement {
                left: image.placement.left,
                top: image.placement.top,
                width: image.placement.width,
                height: image.placement.height,
            },
            data: image.data.into(),
        }))
    }
}

fn raster_image_bytes(raster: &GlyphRaster) -> usize {
    match raster {
        GlyphRaster::Empty => 0,
        GlyphRaster::Image(image) => image.data.len(),
    }
}

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
    use super::*;
    use noon_typst::{compile_typst_resource, TypstMode};

    #[test]
    fn typst_glyphs_rasterize_from_retained_font_bytes() {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let run = artifact.resource.runs.first().unwrap();
        let glyph = run.glyphs.first().unwrap();
        let mut cache = GlyphRasterCache::new();

        let raster = cache
            .get_or_rasterize(&artifact.fonts, run, glyph.glyph_id, 64.0)
            .unwrap();
        let GlyphRaster::Image(image) = raster.as_ref() else {
            panic!("visible glyph should produce pixels");
        };
        assert!(image.placement.width > 0);
        assert!(image.placement.height > 0);
        assert!(!image.data.is_empty());
        assert_eq!(cache.stats().entries, 1);
        assert_eq!(cache.stats().font_faces, 1);
    }

    #[test]
    fn repeated_requests_reuse_the_same_raster() {
        let artifact = compile_typst_resource("AA", TypstMode::Markup).unwrap();
        let run = artifact.resource.runs.first().unwrap();
        let glyph = run.glyphs.first().unwrap();
        let mut cache = GlyphRasterCache::new();

        let first = cache
            .get_or_rasterize(&artifact.fonts, run, glyph.glyph_id, 48.0)
            .unwrap();
        let second = cache
            .get_or_rasterize(&artifact.fonts, run, glyph.glyph_id, 48.0)
            .unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 1);
    }

    #[test]
    fn entry_budget_evicts_the_least_recently_used_raster() {
        let artifact = compile_typst_resource("ABC", TypstMode::Markup).unwrap();
        let run = artifact.resource.runs.first().unwrap();
        assert!(run.glyphs.len() >= 3);
        let a = run.glyphs[0].glyph_id;
        let b = run.glyphs[1].glyph_id;
        let c = run.glyphs[2].glyph_id;
        let mut cache = GlyphRasterCache::with_limits(GlyphRasterCacheLimits::new(2, usize::MAX));

        cache
            .get_or_rasterize(&artifact.fonts, run, a, 48.0)
            .unwrap();
        cache
            .get_or_rasterize(&artifact.fonts, run, b, 48.0)
            .unwrap();
        cache
            .get_or_rasterize(&artifact.fonts, run, a, 48.0)
            .unwrap();
        cache
            .get_or_rasterize(&artifact.fonts, run, c, 48.0)
            .unwrap();

        let stats = cache.stats();
        assert_eq!(stats.entries, 2);
        assert_eq!(stats.evictions, 1);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 3);
        assert!(stats.font_faces <= stats.entries);

        cache
            .get_or_rasterize(&artifact.fonts, run, b, 48.0)
            .unwrap();
        assert_eq!(cache.stats().misses, 4);
        assert_eq!(cache.stats().evictions, 2);
    }

    #[test]
    fn images_larger_than_the_byte_budget_are_not_admitted() {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let run = artifact.resource.runs.first().unwrap();
        let glyph = run.glyphs.first().unwrap();
        let mut cache = GlyphRasterCache::with_limits(GlyphRasterCacheLimits::new(16, 1));

        let raster = cache
            .get_or_rasterize(&artifact.fonts, run, glyph.glyph_id, 64.0)
            .unwrap();
        let GlyphRaster::Image(image) = raster.as_ref() else {
            panic!("visible glyph should produce pixels");
        };
        assert!(image.data.len() > 1);
        assert_eq!(cache.stats().entries, 0);
        assert_eq!(cache.stats().image_bytes, 0);
        assert_eq!(cache.stats().font_faces, 0);
        assert_eq!(cache.stats().rejected_admissions, 1);

        cache
            .get_or_rasterize(&artifact.fonts, run, glyph.glyph_id, 64.0)
            .unwrap();
        assert_eq!(cache.stats().misses, 2);
        assert_eq!(cache.stats().font_faces, 0);
        assert_eq!(cache.stats().rejected_admissions, 2);
    }

    #[test]
    fn clearing_images_releases_cached_font_faces() {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let run = artifact.resource.runs.first().unwrap();
        let glyph = run.glyphs.first().unwrap();
        let mut cache = GlyphRasterCache::new();

        cache
            .get_or_rasterize(&artifact.fonts, run, glyph.glyph_id, 48.0)
            .unwrap();
        assert_eq!(cache.stats().font_faces, 1);

        cache.clear_images();
        assert_eq!(cache.stats().entries, 0);
        assert_eq!(cache.stats().font_faces, 0);
    }

    #[test]
    fn tightening_limits_evicts_immediately() {
        let artifact = compile_typst_resource("AB", TypstMode::Markup).unwrap();
        let run = artifact.resource.runs.first().unwrap();
        let mut cache = GlyphRasterCache::new();
        for glyph in run.glyphs.iter().take(2) {
            cache
                .get_or_rasterize(&artifact.fonts, run, glyph.glyph_id, 48.0)
                .unwrap();
        }
        assert!(cache.stats().entries >= 2);

        cache.set_limits(GlyphRasterCacheLimits::new(1, usize::MAX));
        assert_eq!(cache.stats().entries, 1);
        assert!(cache.stats().font_faces <= cache.stats().entries);
        assert!(cache.stats().evictions >= 1);
    }

    #[test]
    fn stroked_typst_runs_require_outline_rendering() {
        let artifact = compile_typst_resource(
            "#text(stroke: (paint: red, thickness: 1pt, dash: \"dashed\"))[A]",
            TypstMode::Markup,
        )
        .unwrap();
        let run = artifact
            .resource
            .runs
            .iter()
            .find(|run| run.stroke.is_some())
            .expect("Typst should retain the text stroke");
        let glyph = run.glyphs.first().unwrap();
        let mut cache = GlyphRasterCache::new();

        assert_eq!(
            cache
                .get_or_rasterize(&artifact.fonts, run, glyph.glyph_id, 48.0)
                .unwrap_err(),
            GlyphRasterError::OutlineRequired
        );
        assert!(cache.is_empty());
        assert_eq!(cache.stats().misses, 0);
    }

    #[test]
    fn pixel_size_and_variations_are_part_of_cache_identity() {
        let no_variation = variation_fingerprint(&[]);
        let weight = variation_fingerprint(&[FontVariationSetting {
            tag: *b"wght",
            value: 520.5,
        }]);
        assert_ne!(no_variation, weight);
        assert_ne!(48.0_f32.to_bits(), 64.0_f32.to_bits());
    }

    #[test]
    fn invalid_pixel_size_is_rejected_before_font_lookup() {
        let artifact = compile_typst_resource("A", TypstMode::Markup).unwrap();
        let run = artifact.resource.runs.first().unwrap();
        let glyph = run.glyphs.first().unwrap();
        let mut cache = GlyphRasterCache::new();
        assert_eq!(
            cache
                .get_or_rasterize(&artifact.fonts, run, glyph.glyph_id, f32::NAN)
                .unwrap_err(),
            GlyphRasterError::InvalidPixelSize
        );
    }
}
