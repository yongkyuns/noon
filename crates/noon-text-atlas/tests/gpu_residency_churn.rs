#![cfg(feature = "ci-noop")]

use std::sync::Arc;

use noon_core::{FontResourceHandle, FontResourceId};
use noon_text_atlas::{GlyphAtlasEntry, GlyphAtlasPlane, GpuGlyphAtlas};
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

fn mask_raster(value: u8) -> GlyphRaster {
    const WIDTH: u32 = 4;
    const HEIGHT: u32 = 2;
    GlyphRaster::Image(GlyphRasterImage {
        format: GlyphRasterFormat::Alpha8,
        placement: GlyphRasterPlacement {
            left: 0,
            top: HEIGHT as i32,
            width: WIDTH,
            height: HEIGHT,
        },
        data: Arc::from(vec![value; (WIDTH * HEIGHT) as usize]),
    })
}

#[test]
fn two_page_atlas_churn_plateaus_without_texture_reallocation() {
    const GENERATIONS: u16 = 1_000;

    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut atlas = GpuGlyphAtlas::with_page_limit(8, 2).unwrap();
    let raster = mask_raster(180);

    for glyph_id in 1..=4 {
        let GlyphAtlasEntry::Image(_) = atlas
            .insert(&device, &queue, glyph_key(glyph_id), &raster)
            .unwrap()
        else {
            panic!("visible mask glyph must occupy the atlas");
        };
    }

    assert_eq!(atlas.page_count(GlyphAtlasPlane::Mask), 2);
    assert_eq!(atlas.page_count(GlyphAtlasPlane::Color), 0);
    assert_eq!(atlas.stats().texture_allocations, 2);
    assert_eq!(atlas.stats().entries, 4);

    for generation in 0..GENERATIONS {
        atlas.begin_generation();
        let first = 5 + generation * 2;
        for glyph_id in first..=first + 1 {
            let GlyphAtlasEntry::Image(_) = atlas
                .insert(&device, &queue, glyph_key(glyph_id), &raster)
                .unwrap()
            else {
                panic!("visible mask glyph must occupy the atlas");
            };
        }

        let stats = atlas.stats();
        assert_eq!(atlas.page_count(GlyphAtlasPlane::Mask), 2);
        assert_eq!(atlas.page_count(GlyphAtlasPlane::Color), 0);
        assert_eq!(stats.texture_allocations, 2);
        assert_eq!(stats.entries, 4);
        assert_eq!(stats.mask_entries, 4);
        assert_eq!(stats.color_entries, 0);
    }

    let stats = atlas.stats();
    assert_eq!(stats.page_evictions, u64::from(GENERATIONS));
    assert_eq!(stats.page_reuses, u64::from(GENERATIONS));
    assert_eq!(stats.image_entries_evicted, u64::from(GENERATIONS) * 2);
}
