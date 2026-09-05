#![cfg(feature = "ci-noop")]

use std::sync::Arc;

use noon_core::{FontResourceHandle, FontResourceId};
use noon_text_atlas::{GlyphAtlasEntry, GlyphAtlasError, GlyphAtlasPlane, GpuGlyphAtlas};
use noon_text_raster::{
    GlyphRaster, GlyphRasterFormat, GlyphRasterImage, GlyphRasterKey, GlyphRasterPlacement,
};

fn glyph_key(glyph_id: u16) -> GlyphRasterKey {
    GlyphRasterKey {
        font: FontResourceHandle {
            arena: 0,
            id: FontResourceId::new(0),
            version: 0,
        },
        glyph_id,
        pixel_size_bits: 16.0f32.to_bits(),
        variation_fingerprint: 0,
    }
}

fn mask_raster(width: u32, height: u32, value: u8) -> GlyphRaster {
    GlyphRaster::Image(GlyphRasterImage {
        format: GlyphRasterFormat::Alpha8,
        placement: GlyphRasterPlacement {
            left: 0,
            top: height as i32,
            width,
            height,
        },
        data: Arc::from(vec![value; (width * height) as usize]),
    })
}

#[test]
fn noop_device_validates_lazy_mask_and_color_uploads() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut atlas = GpuGlyphAtlas::new(16).unwrap();

    assert!(atlas.texture_view(GlyphAtlasPlane::Mask).is_none());
    assert!(atlas.texture_view(GlyphAtlasPlane::Color).is_none());

    let mask_key = glyph_key(1);
    let mask = GlyphRaster::Image(GlyphRasterImage {
        format: GlyphRasterFormat::Alpha8,
        placement: GlyphRasterPlacement {
            left: -1,
            top: 2,
            width: 2,
            height: 2,
        },
        data: Arc::from([255, 128, 64, 0]),
    });
    let mask_entry = atlas.insert(&device, &queue, mask_key, &mask).unwrap();
    let GlyphAtlasEntry::Image(mask_image) = mask_entry else {
        panic!("visible mask glyph must occupy the atlas");
    };
    assert_eq!(mask_image.plane, GlyphAtlasPlane::Mask);
    assert_eq!(mask_image.page, 0);
    assert_eq!(mask_image.origin, [1, 1]);
    assert_eq!(mask_image.size, [2, 2]);
    assert!(atlas.texture_view(GlyphAtlasPlane::Mask).is_some());
    assert!(atlas.texture_view(GlyphAtlasPlane::Color).is_none());

    assert_eq!(
        atlas.insert(&device, &queue, mask_key, &mask).unwrap(),
        mask_entry
    );

    let color_key = glyph_key(2);
    let color = GlyphRaster::Image(GlyphRasterImage {
        format: GlyphRasterFormat::Rgba8,
        placement: GlyphRasterPlacement {
            left: 0,
            top: 1,
            width: 1,
            height: 1,
        },
        data: Arc::from([10, 20, 30, 255]),
    });
    let color_entry = atlas.insert(&device, &queue, color_key, &color).unwrap();
    let GlyphAtlasEntry::Image(color_image) = color_entry else {
        panic!("visible color glyph must occupy the atlas");
    };
    assert_eq!(color_image.plane, GlyphAtlasPlane::Color);
    assert_eq!(color_image.page, 0);
    assert_eq!(color_image.origin, [1, 1]);
    assert_eq!(color_image.size, [1, 1]);
    assert!(atlas.texture_view(GlyphAtlasPlane::Color).is_some());

    let stats = atlas.stats();
    assert_eq!(stats.entries, 2);
    assert_eq!(stats.mask_entries, 1);
    assert_eq!(stats.color_entries, 1);
    assert_eq!(stats.texture_allocations, 2);
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 2);
    assert_eq!(atlas.get(mask_key), Some(mask_entry));
    assert_eq!(atlas.get(color_key), Some(color_entry));
}

#[test]
fn noop_device_allocates_live_pages_within_explicit_budget() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut atlas = GpuGlyphAtlas::with_page_limit(8, 2).unwrap();

    for glyph_id in 1..=2 {
        let GlyphAtlasEntry::Image(image) = atlas
            .insert(
                &device,
                &queue,
                glyph_key(glyph_id),
                &mask_raster(4, 2, 100 + glyph_id as u8),
            )
            .unwrap()
        else {
            panic!("visible mask glyph must occupy the atlas");
        };
        assert_eq!(image.page, 0);
    }

    let GlyphAtlasEntry::Image(page_one) = atlas
        .insert(&device, &queue, glyph_key(3), &mask_raster(1, 1, 255))
        .unwrap()
    else {
        panic!("visible mask glyph must occupy the atlas");
    };
    assert_eq!(page_one.page, 1);
    assert_eq!(page_one.origin, [1, 1]);
    assert_eq!(atlas.page_count(GlyphAtlasPlane::Mask), 2);
    assert_eq!(atlas.page_count(GlyphAtlasPlane::Color), 0);
    assert!(atlas.texture_view(GlyphAtlasPlane::Mask).is_some());
    assert!(atlas
        .texture_view_for_page(GlyphAtlasPlane::Mask, 1)
        .is_some());
    assert!(atlas
        .texture_view_for_page(GlyphAtlasPlane::Mask, 2)
        .is_none());
    assert_eq!(atlas.stats().texture_allocations, 2);
}

#[test]
fn noop_device_keeps_current_generation_pages_pinned() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut atlas = GpuGlyphAtlas::with_page_limit(8, 1).unwrap();
    let raster = mask_raster(4, 2, 200);

    for glyph_id in 1..=2 {
        atlas
            .insert(&device, &queue, glyph_key(glyph_id), &raster)
            .unwrap();
    }
    assert_eq!(atlas.stats().texture_allocations, 1);

    let error = atlas
        .insert(&device, &queue, glyph_key(3), &mask_raster(1, 1, 255))
        .unwrap_err();
    assert_eq!(
        error,
        GlyphAtlasError::Full {
            plane: GlyphAtlasPlane::Mask,
            extent: 8,
        }
    );
    assert_eq!(atlas.stats().page_evictions, 0);
    assert_eq!(atlas.stats().page_reuses, 0);
    assert_eq!(atlas.stats().texture_allocations, 1);
    assert!(atlas.get(glyph_key(1)).is_some());
    assert!(atlas.get(glyph_key(2)).is_some());
}

#[test]
fn noop_device_reuses_oldest_unpinned_page_without_reallocating_texture() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut atlas = GpuGlyphAtlas::with_page_limit(8, 2).unwrap();
    let full_page_raster = mask_raster(4, 2, 180);

    for glyph_id in 1..=4 {
        let GlyphAtlasEntry::Image(image) = atlas
            .insert(&device, &queue, glyph_key(glyph_id), &full_page_raster)
            .unwrap()
        else {
            panic!("visible mask glyph must occupy the atlas");
        };
        assert_eq!(image.page, u32::from(glyph_id > 2));
    }
    assert_eq!(atlas.page_count(GlyphAtlasPlane::Mask), 2);
    assert_eq!(atlas.stats().texture_allocations, 2);

    assert_eq!(atlas.begin_generation(), 2);
    atlas
        .insert(&device, &queue, glyph_key(4), &full_page_raster)
        .unwrap();

    let GlyphAtlasEntry::Image(reused) = atlas
        .insert(&device, &queue, glyph_key(5), &mask_raster(1, 1, 255))
        .unwrap()
    else {
        panic!("visible mask glyph must occupy the atlas");
    };
    assert_eq!(reused.page, 0);
    assert_eq!(reused.origin, [1, 1]);

    assert_eq!(atlas.get(glyph_key(1)), None);
    assert_eq!(atlas.get(glyph_key(2)), None);
    assert!(atlas.get(glyph_key(3)).is_some());
    assert!(atlas.get(glyph_key(4)).is_some());
    assert!(atlas.get(glyph_key(5)).is_some());

    let stats = atlas.stats();
    assert_eq!(stats.entries, 3);
    assert_eq!(stats.mask_entries, 3);
    assert_eq!(stats.texture_allocations, 2);
    assert_eq!(stats.page_evictions, 1);
    assert_eq!(stats.page_reuses, 1);
    assert_eq!(stats.image_entries_evicted, 2);
}
