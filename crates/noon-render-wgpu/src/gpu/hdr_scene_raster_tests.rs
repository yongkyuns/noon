//! Numerical proof for the actual planar float draw path, before scene authoring
//! integration. Inputs here are explicitly already-linear packed raster values;
//! this test does not claim that authored Color values are emitted radiance.
use super::hdr_bloom::tests::{decode, read, reference};
use super::*;
use crate::FramePreparer;
use noon_core::{Color, GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D, VectorPath};
use noon_runtime::{FrameObjectState, FrameState};

fn row(
    id: u64,
    geometry: GeometryRef,
    color: [f32; 4],
    x: f32,
    opacity: f32,
    appearance: f32,
) -> FrameObjectState {
    FrameObjectState {
        fill_paint: None,
        draw_enabled: true,
        effects: None,
        z_index: 0.0,
        id: ObjectId::new(id),
        content: ObjectContentRef::Geometry(geometry),
        text_bounds: None,
        spatial: None,
        transform: Transform2D {
            translation: Vec2::new(x, 0.0),
            ..Transform2D::IDENTITY
        },
        style: Style {
            fill: Some(Color {
                red: color[0],
                green: color[1],
                blue: color[2],
                alpha: color[3],
            }),
            stroke: None,
            stroke_width: 0.0,
            opacity,
            ..Style::default()
        },
        appearance,
    }
}
fn frame(path: bool, reverse: bool, paint: bool) -> FrameState {
    let overlay = if path {
        GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(-2.0, -4.0))
                .line_to(Vec2::new(2.0, -4.0))
                .line_to(Vec2::new(2.0, 4.0))
                .line_to(Vec2::new(-2.0, 4.0))
                .close(),
        )
    } else {
        GeometryRef::rectangle(4.0, 8.0)
    };
    let mut objects = vec![
        row(
            1,
            GeometryRef::rectangle(8.0, 8.0),
            [16.0, 4.0, 1.0, 0.5],
            0.0,
            1.0,
            1.0,
        ),
        row(2, overlay, [0.25, 8.0, 2.0, 0.5], -2.0, 0.5, 0.5),
    ];
    if paint {
        let color = Color::rgba(0.25, 0.5, 0.75, 0.5);
        let gradient = noon_core::LinearGradient::new(
            [0.0, 0.5],
            [1.0, 0.5],
            [
                noon_core::GradientStop::new(0.0, color).unwrap(),
                noon_core::GradientStop::new(1.0, color).unwrap(),
            ],
        )
        .unwrap();
        objects[1].fill_paint = Some(std::sync::Arc::new(
            noon_core::SemanticPaint::LinearGradient(std::sync::Arc::new(gradient)),
        ));
        objects[1].style.fill = Some(Color::WHITE);
    }
    if reverse {
        objects.reverse();
    }
    for (index, object) in objects.iter_mut().enumerate() {
        object.z_index = index as f64;
    }
    FrameState {
        time: 0.0,
        objects,
        presences: vec![true; 2],
        reveals: vec![1.0; 2],
        morphs: vec![0.0; 2],
        render_geometries: vec![None; 2],
        render_transforms: vec![None; 2],
        family_animations: vec![],
        family_animation_plan_indices: vec![],
    }
}
// The two full-height, pixel-aligned rectangles have exact 0/1 coverage.
// Independently associate the specified source alphas/opacities and apply
// source-over in painter order, including the supplied linear clear color.
fn expected(reverse: bool, clear: [f64; 4], paint: bool) -> Vec<[f64; 4]> {
    let overlay = if paint {
        [0.25, 0.5, 0.75, 0.125]
    } else {
        [0.25, 8.0, 2.0, 0.125]
    };
    let mut layers = [[16.0, 4.0, 1.0, 0.5], overlay];
    if reverse {
        layers.reverse();
    }
    (0..16 * 16)
        .map(|at| {
            let mut value = clear;
            for (index, layer) in layers.iter().enumerate() {
                let overlay = index == usize::from(!reverse);
                if overlay && at % 16 >= 8 {
                    continue;
                }
                for c in 0..3 {
                    value[c] = layer[c] * layer[3] + value[c] * (1.0 - layer[3]);
                }
                value[3] = layer[3] + value[3] * (1.0 - layer[3]);
            }
            value
        })
        .collect()
}
fn target(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("linear scene proof"),
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
fn new_renderer(device: &wgpu::Device, queue: &wgpu::Queue, legacy: bool) -> GpuRenderer {
    let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba16Float);
    renderer.set_viewport(device, queue, 16, 16);
    renderer.set_camera(
        queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 8.0)).unwrap(),
    );
    if legacy {
        let source = format!(
            "{}\n{}\n{}",
            include_str!("../analytic.wgsl"),
            include_str!("../cairo_color.wgsl"),
            include_str!("../polygon_coverage.wgsl")
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("actual early-clamp negative"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("negative layout"),
            bind_group_layouts: &[Some(&renderer.camera_bind_group_layout)],
            immediate_size: 0,
        });
        for samples in [1, PATH_SAMPLE_COUNT] {
            let pipeline = create_pipeline(
                device,
                &layout,
                &shader,
                wgpu::TextureFormat::Rgba16Float,
                samples,
                AnalyticPipelineDescriptor {
                    vertex_entry: "vs_rectangle",
                    fragment_entry: "fs_rectangle",
                    label: "previous compatibility clamp on float target",
                    instance_layout: analytic_instance_layout(),
                },
            );
            if samples == 1 {
                renderer.rectangle_pipeline_single_sample = pipeline;
            } else {
                renderer.rectangle_pipeline = pipeline;
            }
        }
    }
    renderer
}
#[test]
#[ignore = "requires a real native float/blending adapter; absence fails qualification"]
fn planar_float_draws_preserve_emission_alpha_order_and_feed_real_bloom() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("linear scene adapter: {:?}", adapter.get_info());
    let capabilities = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
    assert!(capabilities
        .flags
        .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE));
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut renderer = new_renderer(&device, &queue, false);
    let mut preparer = FramePreparer::new();
    let target = target(&device);
    let view = target.create_view(&Default::default());
    let mut bloom = HdrBloomFilter::new(&adapter, &device);
    let mut scope = HdrBloomScope::default();
    let parameters = HdrBloomParameters {
        sigma_pixels: 0.65,
        threshold: 1.0,
        intensity: 1.5,
        scratch_budget_bytes: 1024 * 1024,
    };
    let artifacts = std::env::var_os("NOON_HDR_SCENE_ARTIFACT_DIR").map(std::path::PathBuf::from);
    if let Some(path) = &artifacts {
        std::fs::create_dir_all(path).unwrap();
    }
    let mut profiles = 0;
    let mut max_linear_error = 0.0f64;
    let mut max_display_error = 0;
    for paint in [false, true] {
        for path in [false, true] {
            for reverse in [false, true] {
                for clear in [[0.0; 4], [0.25, 0.125, 0.0625, 1.0]] {
                    let frame = frame(path, reverse, paint);
                    let prepared = preparer.prepare(&frame).unwrap();
                    renderer.upload(&device, &queue, &prepared);
                    let mut encoder = device.create_command_encoder(&Default::default());
                    renderer.encode(
                        &mut encoder,
                        &view,
                        &prepared,
                        wgpu::Color {
                            r: clear[0],
                            g: clear[1],
                            b: clear[2],
                            a: clear[3],
                        },
                    );
                    queue.submit([encoder.finish()]);
                    let actual = read(&device, &queue, &target);
                    let actual_floats = decode(&actual);
                    let scene_expected = expected(reverse, clear, paint);
                    for (at, (a, e)) in actual_floats
                        .iter()
                        .zip(scene_expected.iter().flatten())
                        .enumerate()
                    {
                        let error = (a - e).abs();
                        max_linear_error = max_linear_error.max(error);
                        assert!(
                        a.is_finite() && error <= 0.006 + 0.004 * e.abs(),
                        "profile {profiles} path={path} reverse={reverse} channel {at}: {a} != {e}"
                    );
                    }
                    assert!(actual_floats.iter().any(|v| *v > 1.0));
                    bloom
                        .prepare(
                            &device,
                            &queue,
                            &mut scope,
                            HdrBloomCapture {
                                source: &target,
                                revision: profiles + 1,
                            },
                            parameters,
                        )
                        .unwrap();
                    let mut encoder = device.create_command_encoder(&Default::default());
                    bloom.encode(&mut encoder, &mut scope);
                    queue.submit([encoder.finish()]);
                    let (_, display_expected) = reference(&scene_expected, [16, 16], parameters);
                    let display = read(&device, &queue, scope.display_output().unwrap());
                    let error = display
                        .iter()
                        .zip(&display_expected)
                        .map(|(a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap();
                    max_display_error = max_display_error.max(error);
                    assert!(error <= 2, "profile {profiles} bloom display error {error}");
                    let clean = preparer
                        .prepare_incremental(&frame, &noon_runtime::FrameChanges::default())
                        .unwrap();
                    assert_eq!(renderer.upload(&device, &queue, &clean).bytes_uploaded, 0);
                    if let Some(directory) = &artifacts {
                        std::fs::write(directory.join(format!("case-{profiles}.json")), format!("{{\"paint\":{paint},\"path\":{path},\"reverse\":{reverse},\"clear\":{clear:?}}}")).unwrap();
                    }
                    if let Some(path) = &artifacts {
                        std::fs::write(path.join(format!("case-{profiles}.rgba16f")), actual)
                            .unwrap();
                        std::fs::write(
                            path.join(format!("case-{profiles}.expected-rgba64f")),
                            scene_expected
                                .iter()
                                .flatten()
                                .flat_map(|v| v.to_le_bytes())
                                .collect::<Vec<_>>(),
                        )
                        .unwrap();
                        std::fs::write(path.join(format!("case-{profiles}.rgba")), display)
                            .unwrap();
                        std::fs::write(
                            path.join(format!("case-{profiles}.expected-rgba")),
                            display_expected,
                        )
                        .unwrap();
                    }
                    profiles += 1;
                }
            }
        }
    }
    // A real rendering pipeline compiled with the previous compatibility source
    // helper loses emitted energy before even reaching the float attachment.
    let mut negative = new_renderer(&device, &queue, true);
    let frame = frame(false, false, false);
    let prepared = preparer.prepare(&frame).unwrap();
    negative.upload(&device, &queue, &prepared);
    let mut encoder = device.create_command_encoder(&Default::default());
    negative.encode(&mut encoder, &view, &prepared, wgpu::Color::TRANSPARENT);
    queue.submit([encoder.finish()]);
    let actual = read(&device, &queue, &target);
    let negative_error = decode(&actual)
        .iter()
        .zip(expected(false, [0.0; 4], false).iter().flatten())
        .map(|(a, e)| (a - e).abs())
        .fold(0.0f64, f64::max);
    assert!(
        negative_error > 1.0,
        "old shader unexpectedly retained HDR radiance: {negative_error}"
    );
    if let Some(path) = &artifacts {
        std::fs::write(path.join("negative-compatibility-clamp.rgba16f"), actual).unwrap();
    }
    if let Some(directory) = &artifacts {
        std::fs::write(directory.join("manifest.json"), format!("{{\"profiles\":{profiles},\"width\":16,\"height\":16,\"linear_channels\":{},\"display_channels\":{},\"max_linear_error\":{max_linear_error},\"max_display_byte_error\":{max_display_error},\"display_bound\":2,\"old_shader_negative_error\":{negative_error}}}", profiles * 16 * 16 * 4, profiles * 16 * 16 * 4)).unwrap();
    }
    eprintln!("linear scene: profiles={profiles} linear_channels={} display_channels={} max_linear_error={max_linear_error} max_display_error={max_display_error} actual_old_shader_error={negative_error}", profiles * 16 * 16 * 4, profiles * 16 * 16 * 4);
}
