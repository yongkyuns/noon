#![cfg(feature = "ci-noop")]

use std::{mem::size_of, sync::Arc};

use noon_core::{FontResourceHandle, FontResourceId, TextResourceHandle, TextResourceId};
use noon_text_atlas::{GlyphAtlasEntry, GlyphAtlasPlane, GpuGlyphAtlas};
use noon_text_raster::{
    GlyphRaster, GlyphRasterFormat, GlyphRasterImage, GlyphRasterKey, GlyphRasterPlacement,
};
use noon_text_render_wgpu::{
    GlyphQuadInstance, PreparedRetainedTextFrame, PreparedTextItem, TextCamera2D,
    TextGlyphGpuRenderer,
};

fn raster_key(glyph_id: u16) -> GlyphRasterKey {
    GlyphRasterKey {
        font: FontResourceHandle {
            arena: 0,
            id: FontResourceId::new(0),
            version: 0,
        },
        glyph_id,
        pixel_size_bits: 32.0f32.to_bits(),
        variation_fingerprint: 0,
    }
}

fn text_handle() -> TextResourceHandle {
    TextResourceHandle {
        arena: 0,
        id: TextResourceId::new(0),
        version: 0,
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

fn quad(image: noon_text_atlas::GlyphAtlasImage) -> GlyphQuadInstance {
    GlyphQuadInstance {
        origin: [-0.5, -0.5],
        axis_x: [1.0, 0.0],
        axis_y: [0.0, 1.0],
        uv_min: image.uv_min,
        uv_max: image.uv_max,
        color: [1.0; 4],
    }
}

fn prepared_mask_frame<'a>(
    quads: &'a [GlyphQuadInstance],
    items: &'a [PreparedTextItem],
) -> PreparedRetainedTextFrame<'a> {
    PreparedRetainedTextFrame {
        time: 0.0,
        mask_quads: quads,
        color_quads: &[],
        items,
        stats: Default::default(),
    }
}

#[test]
fn empty_glyphs_do_not_accumulate_gpu_atlas_metadata() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut atlas = GpuGlyphAtlas::with_page_limit(8, 2).unwrap();

    for glyph_id in 1..=1_024_u16 {
        assert_eq!(
            atlas
                .insert(&device, &queue, raster_key(glyph_id), &GlyphRaster::Empty)
                .unwrap(),
            GlyphAtlasEntry::Empty
        );
    }
    assert_eq!(
        atlas
            .insert(&device, &queue, raster_key(2_000), &mask_raster(0, 3, 255))
            .unwrap(),
        GlyphAtlasEntry::Empty
    );

    assert!(atlas.is_empty());
    assert_eq!(atlas.len(), 0);
    assert_eq!(atlas.page_count(GlyphAtlasPlane::Mask), 0);
    assert_eq!(atlas.page_count(GlyphAtlasPlane::Color), 0);
    assert_eq!(atlas.stats().entries, 0);
    assert_eq!(atlas.stats().mask_entries, 0);
    assert_eq!(atlas.stats().color_entries, 0);
    assert_eq!(atlas.stats().empty_entries, 0);
    assert_eq!(atlas.stats().texture_allocations, 0);
    assert_eq!(atlas.stats().hits, 0);
    assert_eq!(atlas.stats().misses, 1_025);
}

#[test]
fn renderer_extends_bindings_when_persistent_atlas_grows() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut atlas = GpuGlyphAtlas::with_page_limit(8, 2).unwrap();

    let GlyphAtlasEntry::Image(page_zero) = atlas
        .insert(&device, &queue, raster_key(1), &mask_raster(4, 2, 101))
        .unwrap()
    else {
        panic!("visible glyph must allocate an atlas image");
    };
    assert_eq!(page_zero.page, 0);

    let first_quads = [quad(page_zero)];
    let first_items = [PreparedTextItem::GlyphBatch {
        object_index: 0,
        text: text_handle(),
        run_index: 0,
        plane: GlyphAtlasPlane::Mask,
        page: page_zero.page,
        instance_range: 0..1,
    }];
    let first = prepared_mask_frame(&first_quads, &first_items);
    let mut renderer = TextGlyphGpuRenderer::new(
        &device,
        &queue,
        wgpu::TextureFormat::Rgba8Unorm,
        TextCamera2D::DEFAULT,
    );
    let first_upload = renderer.upload(&device, &queue, &first, &atlas);
    assert_eq!(first_upload.bytes_uploaded, size_of::<GlyphQuadInstance>());

    let GlyphAtlasEntry::Image(page_zero_second) = atlas
        .insert(&device, &queue, raster_key(2), &mask_raster(4, 2, 102))
        .unwrap()
    else {
        panic!("visible glyph must allocate an atlas image");
    };
    assert_eq!(page_zero_second.page, 0);

    let GlyphAtlasEntry::Image(page_one) = atlas
        .insert(&device, &queue, raster_key(3), &mask_raster(1, 1, 255))
        .unwrap()
    else {
        panic!("visible glyph must allocate an atlas image");
    };
    assert_eq!(page_one.page, 1);
    assert_eq!(atlas.page_count(GlyphAtlasPlane::Mask), 2);

    let second_quads = [quad(page_one)];
    let second_items = [PreparedTextItem::GlyphBatch {
        object_index: 0,
        text: text_handle(),
        run_index: 0,
        plane: GlyphAtlasPlane::Mask,
        page: page_one.page,
        instance_range: 0..1,
    }];
    let second = prepared_mask_frame(&second_quads, &second_items);
    renderer.upload(&device, &queue, &second, &atlas);

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Noon growing glyph atlas integration target"),
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let attachments = [Some(wgpu::RenderPassColorAttachment {
        view: &view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        },
    })];
    let stats = {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Noon growing glyph atlas integration pass"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        renderer.draw_item(&mut pass, &second_items[0], 1).unwrap()
    };
    queue.submit(Some(encoder.finish()));

    assert_eq!(stats.draw_calls, 1);
    assert_eq!(stats.instances_drawn, 1);
}
