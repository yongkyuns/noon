#![forbid(unsafe_code)]

//! GPU glyph atlas allocation and upload for Noon.
//!
//! This crate deliberately stops at texture ownership, deterministic placement,
//! and uploads. It does not decide scene painter order, create text pipelines, or
//! interpret layout semantics. Those remain renderer concerns built on top of the
//! retained `TextResource` and `noon-text-raster` contracts.

use std::collections::HashMap;

use noon_text_raster::{GlyphRaster, GlyphRasterFormat, GlyphRasterKey};

pub const DEFAULT_GLYPH_ATLAS_EXTENT: u32 = 2048;
pub const GLYPH_ATLAS_GUTTER: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GlyphAtlasPlane {
    Mask,
    Color,
}

impl GlyphAtlasPlane {
    pub const fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            Self::Mask => wgpu::TextureFormat::R8Unorm,
            // Swash color glyphs are conventional 8-bit RGBA color data. Keep the
            // texture sRGB-aware so sampling later produces linear scene color.
            Self::Color => wgpu::TextureFormat::Rgba8UnormSrgb,
        }
    }

    const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Mask => 1,
            Self::Color => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphAtlasImage {
    pub plane: GlyphAtlasPlane,
    /// Texture page containing this image.
    pub page: u32,
    /// Top-left texel containing visible glyph pixels, excluding the transparent gutter.
    pub origin: [u32; 2],
    pub size: [u32; 2],
    /// Normalized UV rectangle for the visible glyph pixels within `page`.
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GlyphAtlasEntry {
    Empty,
    Image(GlyphAtlasImage),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphAtlasStats {
    pub entries: usize,
    pub mask_entries: usize,
    pub color_entries: usize,
    pub empty_entries: usize,
    pub texture_allocations: usize,
    pub bytes_uploaded: usize,
    pub hits: u64,
    pub misses: u64,
    pub page_evictions: u64,
    pub page_reuses: u64,
    pub image_entries_evicted: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlyphAtlasError {
    InvalidExtent,
    InvalidPageCount,
    DimensionOverflow,
    ImageTooLarge {
        width: u32,
        height: u32,
        extent: u32,
    },
    Full {
        plane: GlyphAtlasPlane,
        extent: u32,
    },
    InvalidImageData {
        expected: usize,
        actual: usize,
    },
}

impl std::fmt::Display for GlyphAtlasError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidExtent => write!(formatter, "glyph atlas extent is too small"),
            Self::InvalidPageCount => write!(formatter, "glyph atlas page count must be positive"),
            Self::DimensionOverflow => write!(formatter, "glyph atlas dimensions overflow"),
            Self::ImageTooLarge {
                width,
                height,
                extent,
            } => write!(
                formatter,
                "glyph image {width}x{height} does not fit atlas extent {extent}"
            ),
            Self::Full { plane, extent } => {
                write!(
                    formatter,
                    "{plane:?} glyph atlas of extent {extent} is full"
                )
            }
            Self::InvalidImageData { expected, actual } => write!(
                formatter,
                "glyph raster contains {actual} bytes, expected {expected}"
            ),
        }
    }
}

impl std::error::Error for GlyphAtlasError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AtlasAllocation {
    outer_origin: [u32; 2],
    outer_size: [u32; 2],
    inner_origin: [u32; 2],
}

/// Deterministic page-aware shelf allocation returned before GPU texture ownership
/// is involved. This is intentionally separate from residency/eviction policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphAtlasPageAllocation {
    pub page: u32,
    pub outer_origin: [u32; 2],
    pub outer_size: [u32; 2],
    pub inner_origin: [u32; 2],
}

#[derive(Clone, Debug)]
struct ShelfPacker {
    extent: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
}

impl ShelfPacker {
    fn new(extent: u32) -> Result<Self, GlyphAtlasError> {
        let minimum = GLYPH_ATLAS_GUTTER
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or(GlyphAtlasError::DimensionOverflow)?;
        if extent < minimum {
            return Err(GlyphAtlasError::InvalidExtent);
        }
        Ok(Self {
            extent,
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
        })
    }

    fn allocate(
        &mut self,
        plane: GlyphAtlasPlane,
        width: u32,
        height: u32,
    ) -> Result<AtlasAllocation, GlyphAtlasError> {
        let gutter_twice = GLYPH_ATLAS_GUTTER
            .checked_mul(2)
            .ok_or(GlyphAtlasError::DimensionOverflow)?;
        let outer_width = width
            .checked_add(gutter_twice)
            .ok_or(GlyphAtlasError::DimensionOverflow)?;
        let outer_height = height
            .checked_add(gutter_twice)
            .ok_or(GlyphAtlasError::DimensionOverflow)?;
        if outer_width > self.extent || outer_height > self.extent {
            return Err(GlyphAtlasError::ImageTooLarge {
                width,
                height,
                extent: self.extent,
            });
        }

        let row_end = self
            .cursor_x
            .checked_add(outer_width)
            .ok_or(GlyphAtlasError::DimensionOverflow)?;
        if row_end > self.extent {
            self.cursor_x = 0;
            self.cursor_y = self
                .cursor_y
                .checked_add(self.row_height)
                .ok_or(GlyphAtlasError::DimensionOverflow)?;
            self.row_height = 0;
        }

        let bottom = self
            .cursor_y
            .checked_add(outer_height)
            .ok_or(GlyphAtlasError::DimensionOverflow)?;
        if bottom > self.extent {
            return Err(GlyphAtlasError::Full {
                plane,
                extent: self.extent,
            });
        }

        let allocation = AtlasAllocation {
            outer_origin: [self.cursor_x, self.cursor_y],
            outer_size: [outer_width, outer_height],
            inner_origin: [
                self.cursor_x + GLYPH_ATLAS_GUTTER,
                self.cursor_y + GLYPH_ATLAS_GUTTER,
            ],
        };
        self.cursor_x = self
            .cursor_x
            .checked_add(outer_width)
            .ok_or(GlyphAtlasError::DimensionOverflow)?;
        self.row_height = self.row_height.max(outer_height);
        Ok(allocation)
    }
}

/// Pure deterministic multi-page allocator used by the paged residency manager.
///
/// Existing pages are tried in stable order. Each trial uses a clone and commits
/// the shelf cursor only on success, so probing a page that cannot fit an image
/// cannot consume otherwise reusable space. A new page is appended only after all
/// existing pages reject the allocation as full.
#[derive(Clone, Debug)]
pub struct GlyphAtlasPageAllocator {
    extent: u32,
    max_pages: usize,
    pages: Vec<ShelfPacker>,
}

impl GlyphAtlasPageAllocator {
    pub fn new(extent: u32, max_pages: usize) -> Result<Self, GlyphAtlasError> {
        if max_pages == 0 {
            return Err(GlyphAtlasError::InvalidPageCount);
        }
        Ok(Self {
            extent,
            max_pages,
            pages: vec![ShelfPacker::new(extent)?],
        })
    }

    pub const fn extent(&self) -> u32 {
        self.extent
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub const fn max_pages(&self) -> usize {
        self.max_pages
    }

    pub fn allocate(
        &mut self,
        plane: GlyphAtlasPlane,
        width: u32,
        height: u32,
    ) -> Result<GlyphAtlasPageAllocation, GlyphAtlasError> {
        for (page_index, page) in self.pages.iter_mut().enumerate() {
            let mut candidate = page.clone();
            match candidate.allocate(plane, width, height) {
                Ok(allocation) => {
                    *page = candidate;
                    return page_allocation(page_index, allocation);
                }
                Err(GlyphAtlasError::Full { .. }) => {}
                Err(error) => return Err(error),
            }
        }

        if self.pages.len() >= self.max_pages {
            return Err(GlyphAtlasError::Full {
                plane,
                extent: self.extent,
            });
        }

        let mut page = ShelfPacker::new(self.extent)?;
        let allocation = page.allocate(plane, width, height)?;
        let page_index = self.pages.len();
        self.pages.push(page);
        page_allocation(page_index, allocation)
    }

    fn reset_page(&mut self, page_index: usize) -> Result<(), GlyphAtlasError> {
        let page = self
            .pages
            .get_mut(page_index)
            .ok_or(GlyphAtlasError::DimensionOverflow)?;
        *page = ShelfPacker::new(self.extent)?;
        Ok(())
    }
}

fn page_allocation(
    page_index: usize,
    allocation: AtlasAllocation,
) -> Result<GlyphAtlasPageAllocation, GlyphAtlasError> {
    let page = u32::try_from(page_index).map_err(|_| GlyphAtlasError::DimensionOverflow)?;
    Ok(GlyphAtlasPageAllocation {
        page,
        outer_origin: allocation.outer_origin,
        outer_size: allocation.outer_size,
        inner_origin: allocation.inner_origin,
    })
}

struct AtlasPlaneState {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    last_used_generation: u64,
    keys: Vec<GlyphRasterKey>,
}

impl AtlasPlaneState {
    fn new(
        device: &wgpu::Device,
        plane: GlyphAtlasPlane,
        extent: u32,
        page: u32,
        generation: u64,
    ) -> Self {
        let label = match plane {
            GlyphAtlasPlane::Mask => format!("Noon glyph mask atlas page {page}"),
            GlyphAtlasPlane::Color => format!("Noon glyph color atlas page {page}"),
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&label),
            size: wgpu::Extent3d {
                width: extent,
                height: extent,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: plane.texture_format(),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            last_used_generation: generation,
            keys: Vec::new(),
        }
    }
}

/// Lazily allocated two-plane glyph atlas with an explicit bounded page budget.
///
/// Alpha-mask and color glyphs use independent page vectors because they have
/// different formats and shader semantics. Textures remain lazy: a plane allocates
/// only the pages actually reached by deterministic shelf packing. Empty glyphs are
/// cached without consuming GPU space.
///
/// `GpuGlyphAtlas::new` preserves the historical one-page-per-plane policy. Callers
/// must opt into a larger bounded budget with `with_page_limit`. Page reuse is also
/// explicit: callers advance preparation generations with `begin_generation`, and
/// only pages untouched in the current generation are eligible for deterministic
/// oldest-page reuse.
pub struct GpuGlyphAtlas {
    extent: u32,
    mask_allocator: GlyphAtlasPageAllocator,
    color_allocator: GlyphAtlasPageAllocator,
    mask_pages: Vec<AtlasPlaneState>,
    color_pages: Vec<AtlasPlaneState>,
    entries: HashMap<GlyphRasterKey, GlyphAtlasEntry>,
    stats: GlyphAtlasStats,
    generation: u64,
}

impl GpuGlyphAtlas {
    pub fn new(extent: u32) -> Result<Self, GlyphAtlasError> {
        Self::with_page_limit(extent, 1)
    }

    pub fn with_page_limit(
        extent: u32,
        max_pages_per_plane: usize,
    ) -> Result<Self, GlyphAtlasError> {
        Ok(Self {
            extent,
            mask_allocator: GlyphAtlasPageAllocator::new(extent, max_pages_per_plane)?,
            color_allocator: GlyphAtlasPageAllocator::new(extent, max_pages_per_plane)?,
            mask_pages: Vec::new(),
            color_pages: Vec::new(),
            entries: HashMap::new(),
            stats: GlyphAtlasStats::default(),
            generation: 1,
        })
    }

    pub fn with_default_extent() -> Self {
        Self::new(DEFAULT_GLYPH_ATLAS_EXTENT).expect("default glyph atlas extent is valid")
    }

    pub const fn extent(&self) -> u32 {
        self.extent
    }

    pub const fn stats(&self) -> GlyphAtlasStats {
        self.stats
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Advance the residency generation used to pin pages referenced by one
    /// preparation pass. Pages touched by `insert` during this generation cannot be
    /// recycled until a later generation begins.
    pub fn begin_generation(&mut self) -> u64 {
        if self.generation == u64::MAX {
            self.generation = 1;
            for state in self
                .mask_pages
                .iter_mut()
                .chain(self.color_pages.iter_mut())
            {
                state.last_used_generation = 0;
            }
        } else {
            self.generation += 1;
        }
        self.generation
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn max_pages_per_plane(&self) -> usize {
        self.mask_allocator.max_pages()
    }

    pub fn page_count(&self, plane: GlyphAtlasPlane) -> usize {
        self.page_states(plane).len()
    }

    /// Compatibility accessor for page zero of a plane.
    pub fn texture_view(&self, plane: GlyphAtlasPlane) -> Option<&wgpu::TextureView> {
        self.texture_view_for_page(plane, 0)
    }

    pub fn texture_view_for_page(
        &self,
        plane: GlyphAtlasPlane,
        page: u32,
    ) -> Option<&wgpu::TextureView> {
        let page = usize::try_from(page).ok()?;
        self.page_states(plane).get(page).map(|state| &state.view)
    }

    pub fn get(&self, key: GlyphRasterKey) -> Option<GlyphAtlasEntry> {
        self.entries.get(&key).copied()
    }

    /// Upload one cached CPU raster if it is not already resident.
    ///
    /// A one-texel transparent gutter is uploaded around every image so later
    /// linear sampling cannot bleed from adjacent glyphs. `Queue::write_texture`
    /// permits tightly packed rows, so no 256-byte staging-row padding is required.
    pub fn insert(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: GlyphRasterKey,
        raster: &GlyphRaster,
    ) -> Result<GlyphAtlasEntry, GlyphAtlasError> {
        if let Some(entry) = self.entries.get(&key).copied() {
            self.stats.hits = self.stats.hits.saturating_add(1);
            self.touch_entry(entry);
            return Ok(entry);
        }
        self.stats.misses = self.stats.misses.saturating_add(1);

        let GlyphRaster::Image(image) = raster else {
            self.entries.insert(key, GlyphAtlasEntry::Empty);
            self.stats.entries = self.entries.len();
            self.stats.empty_entries = self.stats.empty_entries.saturating_add(1);
            return Ok(GlyphAtlasEntry::Empty);
        };
        if image.placement.width == 0 || image.placement.height == 0 {
            self.entries.insert(key, GlyphAtlasEntry::Empty);
            self.stats.entries = self.entries.len();
            self.stats.empty_entries = self.stats.empty_entries.saturating_add(1);
            return Ok(GlyphAtlasEntry::Empty);
        }

        let plane = match image.format {
            GlyphRasterFormat::Alpha8 => GlyphAtlasPlane::Mask,
            GlyphRasterFormat::Rgba8 => GlyphAtlasPlane::Color,
        };
        let bytes_per_pixel = plane.bytes_per_pixel();
        let expected = image_len(
            image.placement.width,
            image.placement.height,
            bytes_per_pixel,
        )?;
        if image.data.len() != expected {
            return Err(GlyphAtlasError::InvalidImageData {
                expected,
                actual: image.data.len(),
            });
        }

        // Reject impossible images before mutating allocator state or allocating a GPU page.
        let gutter_twice = GLYPH_ATLAS_GUTTER
            .checked_mul(2)
            .ok_or(GlyphAtlasError::DimensionOverflow)?;
        if image
            .placement
            .width
            .checked_add(gutter_twice)
            .ok_or(GlyphAtlasError::DimensionOverflow)?
            > self.extent
            || image
                .placement
                .height
                .checked_add(gutter_twice)
                .ok_or(GlyphAtlasError::DimensionOverflow)?
                > self.extent
        {
            return Err(GlyphAtlasError::ImageTooLarge {
                width: image.placement.width,
                height: image.placement.height,
                extent: self.extent,
            });
        }

        let upload = padded_upload(
            &image.data,
            image.placement.width,
            image.placement.height,
            bytes_per_pixel,
        )?;
        let allocation = self.allocate(plane, image.placement.width, image.placement.height)?;
        let extent = self.extent;
        let state = self.ensure_page(device, plane, allocation.page)?;
        let bytes_per_row = allocation.outer_size[0]
            .checked_mul(bytes_per_pixel as u32)
            .ok_or(GlyphAtlasError::DimensionOverflow)?;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &state.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: allocation.outer_origin[0],
                    y: allocation.outer_origin[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &upload,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(allocation.outer_size[1]),
            },
            wgpu::Extent3d {
                width: allocation.outer_size[0],
                height: allocation.outer_size[1],
                depth_or_array_layers: 1,
            },
        );

        let origin = allocation.inner_origin;
        let size = [image.placement.width, image.placement.height];
        let atlas_image = GlyphAtlasImage {
            plane,
            page: allocation.page,
            origin,
            size,
            uv_min: [
                origin[0] as f32 / extent as f32,
                origin[1] as f32 / extent as f32,
            ],
            uv_max: [
                (origin[0] + size[0]) as f32 / extent as f32,
                (origin[1] + size[1]) as f32 / extent as f32,
            ],
        };
        self.record_page_key(plane, allocation.page, key)?;
        let entry = GlyphAtlasEntry::Image(atlas_image);
        self.entries.insert(key, entry);
        self.stats.entries = self.entries.len();
        self.stats.bytes_uploaded = self.stats.bytes_uploaded.saturating_add(upload.len());
        match plane {
            GlyphAtlasPlane::Mask => {
                self.stats.mask_entries = self.stats.mask_entries.saturating_add(1)
            }
            GlyphAtlasPlane::Color => {
                self.stats.color_entries = self.stats.color_entries.saturating_add(1)
            }
        }
        Ok(entry)
    }

    fn allocate(
        &mut self,
        plane: GlyphAtlasPlane,
        width: u32,
        height: u32,
    ) -> Result<GlyphAtlasPageAllocation, GlyphAtlasError> {
        match self.allocate_without_reuse(plane, width, height) {
            Err(error @ GlyphAtlasError::Full { .. }) => {
                let Some(victim) = self.eviction_victim(plane) else {
                    return Err(error);
                };
                self.recycle_page(plane, victim)?;
                self.allocate_without_reuse(plane, width, height)
            }
            result => result,
        }
    }

    fn allocate_without_reuse(
        &mut self,
        plane: GlyphAtlasPlane,
        width: u32,
        height: u32,
    ) -> Result<GlyphAtlasPageAllocation, GlyphAtlasError> {
        match plane {
            GlyphAtlasPlane::Mask => self.mask_allocator.allocate(plane, width, height),
            GlyphAtlasPlane::Color => self.color_allocator.allocate(plane, width, height),
        }
    }

    fn eviction_victim(&self, plane: GlyphAtlasPlane) -> Option<usize> {
        self.page_states(plane)
            .iter()
            .enumerate()
            .filter(|(_, state)| state.last_used_generation != self.generation)
            .min_by_key(|(page_index, state)| (state.last_used_generation, *page_index))
            .map(|(page_index, _)| page_index)
    }

    fn recycle_page(
        &mut self,
        plane: GlyphAtlasPlane,
        page_index: usize,
    ) -> Result<(), GlyphAtlasError> {
        match plane {
            GlyphAtlasPlane::Mask => self.mask_allocator.reset_page(page_index)?,
            GlyphAtlasPlane::Color => self.color_allocator.reset_page(page_index)?,
        }

        let generation = self.generation;
        let keys = {
            let state = self
                .page_states_mut(plane)
                .get_mut(page_index)
                .ok_or(GlyphAtlasError::DimensionOverflow)?;
            state.last_used_generation = generation;
            std::mem::take(&mut state.keys)
        };
        let removed = keys
            .iter()
            .filter(|key| self.entries.remove(key).is_some())
            .count();
        debug_assert_eq!(removed, keys.len());
        self.stats.entries = self.entries.len();
        match plane {
            GlyphAtlasPlane::Mask => {
                self.stats.mask_entries = self.stats.mask_entries.saturating_sub(removed)
            }
            GlyphAtlasPlane::Color => {
                self.stats.color_entries = self.stats.color_entries.saturating_sub(removed)
            }
        }
        self.stats.page_evictions = self.stats.page_evictions.saturating_add(1);
        self.stats.page_reuses = self.stats.page_reuses.saturating_add(1);
        self.stats.image_entries_evicted = self
            .stats
            .image_entries_evicted
            .saturating_add(removed as u64);
        Ok(())
    }

    fn touch_entry(&mut self, entry: GlyphAtlasEntry) {
        let GlyphAtlasEntry::Image(image) = entry else {
            return;
        };
        let Ok(page_index) = usize::try_from(image.page) else {
            debug_assert!(false, "cached glyph atlas page must fit usize");
            return;
        };
        let generation = self.generation;
        if let Some(state) = self.page_states_mut(image.plane).get_mut(page_index) {
            state.last_used_generation = generation;
        } else {
            debug_assert!(false, "cached glyph atlas page must remain resident");
        }
    }

    fn record_page_key(
        &mut self,
        plane: GlyphAtlasPlane,
        page: u32,
        key: GlyphRasterKey,
    ) -> Result<(), GlyphAtlasError> {
        let page_index = usize::try_from(page).map_err(|_| GlyphAtlasError::DimensionOverflow)?;
        let generation = self.generation;
        let state = self
            .page_states_mut(plane)
            .get_mut(page_index)
            .ok_or(GlyphAtlasError::DimensionOverflow)?;
        state.last_used_generation = generation;
        state.keys.push(key);
        Ok(())
    }

    fn page_states(&self, plane: GlyphAtlasPlane) -> &[AtlasPlaneState] {
        match plane {
            GlyphAtlasPlane::Mask => &self.mask_pages,
            GlyphAtlasPlane::Color => &self.color_pages,
        }
    }

    fn page_states_mut(&mut self, plane: GlyphAtlasPlane) -> &mut [AtlasPlaneState] {
        match plane {
            GlyphAtlasPlane::Mask => &mut self.mask_pages,
            GlyphAtlasPlane::Color => &mut self.color_pages,
        }
    }

    fn ensure_page(
        &mut self,
        device: &wgpu::Device,
        plane: GlyphAtlasPlane,
        page: u32,
    ) -> Result<&mut AtlasPlaneState, GlyphAtlasError> {
        let page_index = usize::try_from(page).map_err(|_| GlyphAtlasError::DimensionOverflow)?;
        let required = page_index
            .checked_add(1)
            .ok_or(GlyphAtlasError::DimensionOverflow)?;
        let missing = required.saturating_sub(self.page_count(plane));
        for _ in 0..missing {
            let next_page = u32::try_from(self.page_count(plane))
                .map_err(|_| GlyphAtlasError::DimensionOverflow)?;
            let state = AtlasPlaneState::new(
                device,
                plane,
                self.extent,
                next_page,
                self.generation,
            );
            match plane {
                GlyphAtlasPlane::Mask => self.mask_pages.push(state),
                GlyphAtlasPlane::Color => self.color_pages.push(state),
            }
            self.stats.texture_allocations = self.stats.texture_allocations.saturating_add(1);
        }
        match plane {
            GlyphAtlasPlane::Mask => self.mask_pages.get_mut(page_index),
            GlyphAtlasPlane::Color => self.color_pages.get_mut(page_index),
        }
        .ok_or(GlyphAtlasError::DimensionOverflow)
    }
}

impl Default for GpuGlyphAtlas {
    fn default() -> Self {
        Self::with_default_extent()
    }
}

fn image_len(width: u32, height: u32, bytes_per_pixel: usize) -> Result<usize, GlyphAtlasError> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
        .ok_or(GlyphAtlasError::DimensionOverflow)
}

fn padded_upload(
    data: &[u8],
    width: u32,
    height: u32,
    bytes_per_pixel: usize,
) -> Result<Vec<u8>, GlyphAtlasError> {
    let expected = image_len(width, height, bytes_per_pixel)?;
    if data.len() != expected {
        return Err(GlyphAtlasError::InvalidImageData {
            expected,
            actual: data.len(),
        });
    }
    let gutter_twice = GLYPH_ATLAS_GUTTER
        .checked_mul(2)
        .ok_or(GlyphAtlasError::DimensionOverflow)?;
    let padded_width = width
        .checked_add(gutter_twice)
        .ok_or(GlyphAtlasError::DimensionOverflow)?;
    let padded_height = height
        .checked_add(gutter_twice)
        .ok_or(GlyphAtlasError::DimensionOverflow)?;
    let padded_len = image_len(padded_width, padded_height, bytes_per_pixel)?;
    let mut padded = vec![0; padded_len];
    let source_row = (width as usize)
        .checked_mul(bytes_per_pixel)
        .ok_or(GlyphAtlasError::DimensionOverflow)?;
    let target_row = (padded_width as usize)
        .checked_mul(bytes_per_pixel)
        .ok_or(GlyphAtlasError::DimensionOverflow)?;
    let left_offset = (GLYPH_ATLAS_GUTTER as usize)
        .checked_mul(bytes_per_pixel)
        .ok_or(GlyphAtlasError::DimensionOverflow)?;
    for row in 0..height as usize {
        let source_start = row * source_row;
        let target_start = (row + GLYPH_ATLAS_GUTTER as usize) * target_row + left_offset;
        padded[target_start..target_start + source_row]
            .copy_from_slice(&data[source_start..source_start + source_row]);
    }
    Ok(padded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelf_packing_is_deterministic_and_preserves_gutters() {
        let mut packer = ShelfPacker::new(12).unwrap();
        let first = packer.allocate(GlyphAtlasPlane::Mask, 3, 2).unwrap();
        let second = packer.allocate(GlyphAtlasPlane::Mask, 4, 3).unwrap();
        let third = packer.allocate(GlyphAtlasPlane::Mask, 3, 2).unwrap();

        assert_eq!(first.outer_origin, [0, 0]);
        assert_eq!(first.inner_origin, [1, 1]);
        assert_eq!(first.outer_size, [5, 4]);
        assert_eq!(second.outer_origin, [5, 0]);
        assert_eq!(second.outer_size, [6, 5]);
        assert_eq!(third.outer_origin, [0, 5]);
        assert_eq!(third.inner_origin, [1, 6]);
    }

    #[test]
    fn paged_allocator_rolls_over_in_stable_page_order() {
        let mut allocator = GlyphAtlasPageAllocator::new(8, 2).unwrap();
        let first = allocator.allocate(GlyphAtlasPlane::Mask, 4, 2).unwrap();
        let second = allocator.allocate(GlyphAtlasPlane::Mask, 4, 2).unwrap();
        let third = allocator.allocate(GlyphAtlasPlane::Mask, 1, 1).unwrap();

        assert_eq!(first.page, 0);
        assert_eq!(second.page, 0);
        assert_eq!(third.page, 1);
        assert_eq!(third.outer_origin, [0, 0]);
        assert_eq!(third.inner_origin, [1, 1]);
        assert_eq!(allocator.page_count(), 2);
    }

    #[test]
    fn paged_allocator_respects_page_budget() {
        let mut allocator = GlyphAtlasPageAllocator::new(8, 1).unwrap();
        allocator.allocate(GlyphAtlasPlane::Color, 4, 2).unwrap();
        allocator.allocate(GlyphAtlasPlane::Color, 4, 2).unwrap();
        assert_eq!(
            allocator
                .allocate(GlyphAtlasPlane::Color, 1, 1)
                .unwrap_err(),
            GlyphAtlasError::Full {
                plane: GlyphAtlasPlane::Color,
                extent: 8,
            }
        );
        assert_eq!(allocator.page_count(), 1);
    }

    #[test]
    fn oversized_paged_allocation_does_not_append_a_page() {
        let mut allocator = GlyphAtlasPageAllocator::new(8, 3).unwrap();
        assert_eq!(
            allocator.allocate(GlyphAtlasPlane::Mask, 7, 1).unwrap_err(),
            GlyphAtlasError::ImageTooLarge {
                width: 7,
                height: 1,
                extent: 8,
            }
        );
        assert_eq!(allocator.page_count(), 1);
        let next = allocator.allocate(GlyphAtlasPlane::Mask, 2, 2).unwrap();
        assert_eq!(next.page, 0);
        assert_eq!(next.outer_origin, [0, 0]);
    }

    #[test]
    fn zero_page_budget_is_rejected() {
        assert_eq!(
            GlyphAtlasPageAllocator::new(8, 0).unwrap_err(),
            GlyphAtlasError::InvalidPageCount
        );
        assert!(matches!(
            GpuGlyphAtlas::with_page_limit(8, 0),
            Err(GlyphAtlasError::InvalidPageCount)
        ));
    }

    #[test]
    fn default_gpu_atlas_page_budget_stays_one() {
        let atlas = GpuGlyphAtlas::new(8).unwrap();
        assert_eq!(atlas.max_pages_per_plane(), 1);
        assert_eq!(atlas.page_count(GlyphAtlasPlane::Mask), 0);
        assert_eq!(atlas.page_count(GlyphAtlasPlane::Color), 0);
        assert_eq!(atlas.generation(), 1);
    }

    #[test]
    fn generation_wrap_releases_old_page_pins() {
        let mut atlas = GpuGlyphAtlas::new(8).unwrap();
        atlas.generation = u64::MAX;
        assert_eq!(atlas.begin_generation(), 1);
    }

    #[test]
    fn padded_mask_upload_has_transparent_border() {
        let upload = padded_upload(&[1, 2, 3, 4], 2, 2, 1).unwrap();
        assert_eq!(
            upload,
            vec![
                0, 0, 0, 0, // top gutter
                0, 1, 2, 0, // row 0
                0, 3, 4, 0, // row 1
                0, 0, 0, 0, // bottom gutter
            ]
        );
    }

    #[test]
    fn padded_rgba_upload_preserves_pixel_bytes() {
        let pixel = [10, 20, 30, 40];
        let upload = padded_upload(&pixel, 1, 1, 4).unwrap();
        assert_eq!(upload.len(), 3 * 3 * 4);
        assert_eq!(&upload[16..20], &pixel);
        assert!(upload[..16].iter().all(|byte| *byte == 0));
        assert!(upload[20..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn oversized_images_are_rejected_without_advancing_packer() {
        let mut packer = ShelfPacker::new(8).unwrap();
        assert_eq!(
            packer.allocate(GlyphAtlasPlane::Color, 7, 1).unwrap_err(),
            GlyphAtlasError::ImageTooLarge {
                width: 7,
                height: 1,
                extent: 8,
            }
        );
        let next = packer.allocate(GlyphAtlasPlane::Color, 2, 2).unwrap();
        assert_eq!(next.outer_origin, [0, 0]);
    }

    #[test]
    fn atlas_full_is_reported_per_plane() {
        let mut packer = ShelfPacker::new(8).unwrap();
        packer.allocate(GlyphAtlasPlane::Mask, 4, 2).unwrap();
        packer.allocate(GlyphAtlasPlane::Mask, 4, 2).unwrap();
        assert_eq!(
            packer.allocate(GlyphAtlasPlane::Mask, 1, 1).unwrap_err(),
            GlyphAtlasError::Full {
                plane: GlyphAtlasPlane::Mask,
                extent: 8,
            }
        );
    }
}