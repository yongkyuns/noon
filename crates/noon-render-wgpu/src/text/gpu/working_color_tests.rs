//! Float glyph-color proof. The atlas owns the sRGB texture format; the shader
//! must decode only authored tint, associate color taps, and preserve mask alpha.
use super::*;
use crate::gpu::{test_decode_hdr as decode, test_read_hdr as read};
use crate::text::atlas::{GlyphAtlasEntry, GlyphAtlasPlane, GpuGlyphAtlas};
use noon_core::{FontResourceHandle, FontResourceId, TextResourceHandle, TextResourceId};
use noon_text::raster::{
    GlyphRaster, GlyphRasterFormat, GlyphRasterImage, GlyphRasterKey, GlyphRasterPlacement,
};
use std::sync::Arc;
const SIDE: u32 = 16;
const COLOR: [u8; 16] = [
    128, 32, 224, 128, 240, 120, 16, 64, 16, 240, 128, 0, 80, 160, 224, 255,
];
const MASK: [u8; 4] = [255, 128, 64, 0];
fn srgb(x: f64) -> f64 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}
fn expected(color: bool, opacity: f64, clear: [f64; 4]) -> Vec<[f64; 4]> {
    (0..SIDE * SIDE)
        .map(|at| {
            let x = (f64::from(at % SIDE) + 0.5) / 8.0 - 0.5;
            let y = (f64::from(at / SIDE) + 0.5) / 8.0 - 0.5;
            let mut value = [0.0; 4];
            for yy in 0..2 {
                for xx in 0..2 {
                    let w = (1.0 - (x - f64::from(xx)).abs()).max(0.0)
                        * (1.0 - (y - f64::from(yy)).abs()).max(0.0);
                    let i = (yy * 2 + xx) as usize;
                    let source = if color {
                        let alpha = f64::from(COLOR[4 * i + 3]) / 255.0;
                        [
                            srgb(f64::from(COLOR[4 * i]) / 255.0) * alpha,
                            srgb(f64::from(COLOR[4 * i + 1]) / 255.0) * alpha,
                            srgb(f64::from(COLOR[4 * i + 2]) / 255.0) * alpha,
                            alpha,
                        ]
                    } else {
                        let alpha = f64::from(MASK[i]) / 255.0;
                        [
                            srgb(0.5) * alpha,
                            srgb(0.04) * alpha,
                            srgb(0.75) * alpha,
                            alpha,
                        ]
                    };
                    for c in 0..4 {
                        value[c] += source[c] * w * opacity;
                    }
                }
            }
            std::array::from_fn(|c| value[c] + clear[c] * (1.0 - value[3]))
        })
        .collect()
}
fn shader_override(renderer: &mut TextGlyphGpuRenderer, device: &wgpu::Device, mode: u32) {
    let working = crate::color_profile::working_color_shader(if mode == 1 {
        wgpu::TextureFormat::Rgba8Unorm
    } else {
        wgpu::TextureFormat::Rgba16Float
    });
    let glyph = if mode == 2 {
        include_str!("../glyph.wgsl").replace(
            "return vec4(texel.rgb * texel.a, texel.a);",
            "return vec4(working_color_rgb(texel.rgb) * texel.a, texel.a);",
        )
    } else {
        include_str!("../glyph.wgsl").to_string()
    };
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual glyph color negative"),
        source: wgpu::ShaderSource::Wgsl(format!("{working}\n{glyph}").into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("glyph proof negative"),
        bind_group_layouts: &[Some(&renderer.camera_layout), Some(&renderer.atlas_layout)],
        immediate_size: 0,
    });
    renderer.mask_pipeline_single_sample = create_glyph_pipeline(
        device,
        &layout,
        &shader,
        wgpu::TextureFormat::Rgba16Float,
        "fs_mask",
        1,
        "actual encoded tint negative",
    );
    renderer.color_pipeline_single_sample = create_glyph_pipeline(
        device,
        &layout,
        &shader,
        wgpu::TextureFormat::Rgba16Float,
        "fs_color",
        1,
        "actual color association/decode negative",
    );
}
#[test]
#[ignore = "requires real native float blending; missing adapter fails qualification"]
fn glyph_working_color_matches_independent_tint_atlas_alpha_and_rejects_real_negatives() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("linear glyph proof"),
        size: wgpu::Extent3d {
            width: SIDE,
            height: SIDE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let artifact =
        std::env::var_os("NOON_GLYPH_WORKING_COLOR_ARTIFACT_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &artifact {
        std::fs::create_dir_all(dir).unwrap();
    }
    let mut profiles = 0;
    let mut max_error = 0.0f64;
    let mut negatives = [0.0f64; 3];
    for color in [false, true] {
        for opacity in [0.5, 1.0] {
            for clear in [[0.0; 4], [0.125, 0.0625, 0.03125, 1.0]] {
                let mut atlas = GpuGlyphAtlas::new(8).unwrap();
                let key = GlyphRasterKey {
                    font: FontResourceHandle {
                        arena: 0,
                        id: FontResourceId::new(1),
                        version: 0,
                    },
                    glyph_id: 1,
                    pixel_size_bits: 2.0f32.to_bits(),
                    variation_fingerprint: 0,
                    phase: noon_text::raster::GlyphRasterPhase::ZERO,
                    outline_profile: noon_core::GlyphOutlineProfile::Font,
                    outline_size_bits: 0,
                };
                let raster = GlyphRaster::Image(GlyphRasterImage {
                    format: if color {
                        GlyphRasterFormat::Rgba8
                    } else {
                        GlyphRasterFormat::Alpha8
                    },
                    placement: GlyphRasterPlacement {
                        left: 0,
                        top: 2,
                        width: 2,
                        height: 2,
                    },
                    data: if color {
                        Arc::from(COLOR)
                    } else {
                        Arc::from(MASK)
                    },
                });
                let GlyphAtlasEntry::Image(image) = atlas.insert(&device, key, &raster).unwrap()
                else {
                    panic!("nonempty fixture")
                };
                let quads = [GlyphQuadInstance {
                    origin: [-1.0, -1.0],
                    axis_x: [2.0, 0.0],
                    axis_y: [0.0, 2.0],
                    uv_min: image.uv_min,
                    uv_max: image.uv_max,
                    color: if color {
                        [1.0, 1.0, 1.0, opacity as f32]
                    } else {
                        [0.5, 0.04, 0.75, opacity as f32]
                    },
                }];
                let items = [PreparedTextItem::GlyphBatch {
                    object_index: 0,
                    text: TextResourceHandle {
                        arena: 0,
                        id: TextResourceId::new(1),
                        version: 0,
                    },
                    run_index: 0,
                    plane: if color {
                        GlyphAtlasPlane::Color
                    } else {
                        GlyphAtlasPlane::Mask
                    },
                    page: image.page,
                    instance_range: 0..1,
                    object_opacity: opacity as f32,
                }];
                let slots = [vec![0]];
                let prepared = PreparedRetainedTextFrame {
                    time: 0.0,
                    layout_rebuilt: true,
                    mask_quads: if color { &[] } else { &quads },
                    color_quads: if color { &quads } else { &[] },
                    items: &items,
                    object_glyph_slots: &slots,
                    stats: Default::default(),
                };
                let reference = expected(color, opacity, clear);
                for mode in 0..=2 {
                    if mode == 2 && !color {
                        continue;
                    }
                    let mut renderer = TextGlyphGpuRenderer::new(
                        &device,
                        &queue,
                        wgpu::TextureFormat::Rgba16Float,
                        TextCamera2D::DEFAULT,
                    );
                    if mode != 0 {
                        shader_override(&mut renderer, &device, mode);
                    }
                    renderer.upload(&device, &queue, &prepared, &atlas);
                    let mut encoder = device.create_command_encoder(&Default::default());
                    atlas.encode_pending_uploads(&mut encoder);
                    {
                        let attachment = [Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: clear[0],
                                    g: clear[1],
                                    b: clear[2],
                                    a: clear[3],
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })];
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("real glyph color"),
                            color_attachments: &attachment,
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });
                        renderer
                            .draw_ordered_glyphs(&mut pass, &prepared, 1)
                            .unwrap();
                    }
                    queue.submit([encoder.finish()]);
                    atlas.acknowledge_pending_uploads();
                    let pixels = read(&device, &queue, &target);
                    let actual = decode(&pixels);
                    assert!(actual.iter().all(|v| v.is_finite()));
                    let error = actual
                        .iter()
                        .zip(reference.iter().flatten())
                        .map(|(a, e)| (a - e).abs())
                        .fold(0.0, f64::max);
                    if mode == 0 {
                        assert!(
                            error <= 0.002,
                            "glyph profile {profiles} color={color}: {error}"
                        );
                        max_error = max_error.max(error);
                        assert_eq!(
                            renderer
                                .upload_ranges(&device, &queue, &prepared, &atlas, &[], &[])
                                .bytes_uploaded,
                            0
                        );
                    } else {
                        assert!(
                            error > 0.01,
                            "glyph negative mode={mode} color={color} only differed by {error}"
                        );
                        negatives[mode as usize] = negatives[mode as usize].max(error);
                    }
                    if let Some(dir) = &artifact {
                        std::fs::write(
                            dir.join(format!("glyph-{profiles}-mode-{mode}.rgba16f")),
                            pixels,
                        )
                        .unwrap();
                    }
                }
                if let Some(dir) = &artifact {
                    std::fs::write(
                        dir.join(format!("glyph-{profiles}.expected-rgba64f")),
                        reference
                            .iter()
                            .flatten()
                            .flat_map(|v| v.to_le_bytes())
                            .collect::<Vec<_>>(),
                    )
                    .unwrap();
                    std::fs::write(
                        dir.join(format!("glyph-{profiles}.json")),
                        format!("{{\"color\":{color},\"opacity\":{opacity},\"clear\":{clear:?}}}"),
                    )
                    .unwrap();
                }
                profiles += 1;
            }
        }
    }
    let manifest=format!("{{\"profiles\":{profiles},\"float_channels\":{},\"linear_bound\":0.002,\"max_linear_error\":{max_error},\"actual_encoded_tint_or_unassociated_filter_negative\":{},\"actual_double_atlas_decode_negative\":{}}}",profiles*1024,negatives[1],negatives[2]);
    if let Some(dir) = &artifact {
        std::fs::write(dir.join("manifest.json"), &manifest).unwrap();
    }
    eprintln!("glyph working color: {manifest}");
}
