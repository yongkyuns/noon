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
    /// Top-left texel containing visible glyph pixels, excluding the transparent gutter.
    pub origin: [u32; 2],
    pub size: [u32; 2],
    /// Normalized UV rectangle for the visible glyph pixels.
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlyphAtlasError {
    InvalidExtent,
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

struct AtlasPlaneState {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    packer: ShelfPacker,
}

impl AtlasPlaneState {
    fn new(
        device: &wgpu::Device,
        plane: GlyphAtlasPlane,
        extent: u32,
    ) -> Result<Self, GlyphAtlasError> {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(match plane {
                GlyphAtlasPlane::Mask => "Noon glyph mask atlas",
                GlyphAtlasPlane::Color => "Noon glyph color atlas",
            }),
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
        Ok(Self {
            texture,
            view,
            packer: ShelfPacker::new(extent)?,
        })
    }
}

/// Lazily allocated two-plane glyph atlas.
///
/// Alpha-mask and color glyphs use independent textures because they have different
/// formats and shader semantics. Neither texture is allocated until the first image
/// for that plane is uploaded. Empty glyphs are cached without consuming GPU space.
pub struct GpuGlyphAtlas {
    extent: u32,
    mask: Option<AtlasPlaneState>,
    color: Option<AtlasPlaneState>,
    entries: HashMap<GlyphRasterKey, GlyphAtlasEntry>,
    stats: GlyphAtlasStats,
}

impl GpuGlyphAtlas {
    pub fn new(extent: u32) -> Result<Self, GlyphAtlasError> {
        ShelfPacker::new(extent)?;
        Ok(Self {
            extent,
            mask: None,
            color: None,
            entries: HashMap::new(),
            stats: GlyphAtlasStats::default(),
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

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn texture_view(&self, plane: GlyphAtlasPlane) -> Option<&wgpu::TextureView> {
        match plane {
            GlyphAtlasPlane::Mask => self.mask.as_ref().map(|state| &state.view),
            GlyphAtlasPlane::Color => self.color.as_ref().map(|state| &state.view),
        }
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

        // Reject impossible images before lazily allocating the GPU plane.
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

        let extent = self.extent;
        let state = self.ensure_plane(device, plane)?;
        let allocation =
            state
                .packer
                .allocate(plane, image.placement.width, image.placement.height)?;
        let upload = padded_upload(
            &image.data,
            image.placement.width,
            image.placement.height,
            bytes_per_pixel,
        )?;
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

    fn ensure_plane(
        &mut self,
        device: &wgpu::Device,
        plane: GlyphAtlasPlane,
    ) -> Result<&mut AtlasPlaneState, GlyphAtlasError> {
        match plane {
            GlyphAtlasPlane::Mask => {
                if self.mask.is_none() {
                    self.mask = Some(AtlasPlaneState::new(device, plane, self.extent)?);
                    self.stats.texture_allocations =
                        self.stats.texture_allocations.saturating_add(1);
                }
                Ok(self.mask.as_mut().expect("mask atlas was initialized"))
            }
            GlyphAtlasPlane::Color => {
                if self.color.is_none() {
                    self.color = Some(AtlasPlaneState::new(device, plane, self.extent)?);
                    self.stats.texture_allocations =
                        self.stats.texture_allocations.saturating_add(1);
                }
                Ok(self.color.as_mut().expect("color atlas was initialized"))
            }
        }
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
