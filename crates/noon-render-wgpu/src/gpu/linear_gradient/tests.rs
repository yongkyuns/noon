use std::{sync::Arc, time::Duration};

use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{
    Color, GeometryRef, GradientStop, LinearGradient, ObjectId, Style, Transform2D, Vec2,
    VectorPath,
};
use noon_runtime::{FrameChanges, SceneInstance};

use super::*;
use crate::{Camera2D, FramePreparer};

fn gradient(first: Color) -> Arc<LinearGradient> {
    Arc::new(
        LinearGradient::new(
            [0.0, 0.5],
            [1.0, 0.5],
            [
                GradientStop::new(0.0, first).unwrap(),
                GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)).unwrap(),
            ],
        )
        .unwrap(),
    )
}

fn runtime(geometry: GeometryRef) -> SceneInstance {
    SceneInstance::new(
        CompiledScene::compile_objects(
            vec![CompiledObject::new(
                ObjectId::new(7),
                geometry,
                Transform2D::IDENTITY,
                Style {
                    fill: Some(Color::WHITE),
                    stroke: None,
                    ..Style::default()
                },
            )],
            &[],
        )
        .unwrap(),
    )
}

fn set_gradient(runtime: &mut SceneInstance, paint: Option<Arc<LinearGradient>>) {
    let style = runtime.frame().objects[0].style;
    runtime
        .apply_execution_patch(&ExecutionPatch::SetStyle {
            object: ObjectId::new(7),
            style,
            fill_gradient: paint,
            fill_image: None,
        })
        .unwrap();
}

#[test]
fn one_gradient_in_100k_circles_has_one_sparse_draw_route() {
    let objects = (0..100_000)
        .map(|index| {
            CompiledObject::new(
                ObjectId::new(index),
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            )
        })
        .collect();
    let mut runtime = SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
    let mut preparer = FramePreparer::new();
    preparer.prepare(runtime.frame());
    runtime.take_frame_changes();
    runtime
        .apply_execution_patch(&ExecutionPatch::SetStyle {
            object: ObjectId::new(50_000),
            style: Style::default(),
            fill_gradient: Some(gradient(Color::rgb(1.0, 0.0, 0.0))),
            fill_image: None,
        })
        .unwrap();
    let changes = runtime.take_frame_changes();
    let prepared = preparer.prepare_incremental(runtime.frame(), &changes);
    let routes = prepared
        .gradient_indices(RenderPrimitive::Circle, 0..100_000)
        .unwrap()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(routes, [50_000]);
    assert_eq!(prepared.stats.instances_repacked, 1);
    assert_eq!(prepared.stats.dirty_instance_count, 0);
    assert_eq!(prepared.stats.render_order_positions_visited, 0);
    assert_eq!(prepared.gradient_changes, &[ObjectId::new(50_000)]);
}

#[test]
fn path_gradient_keeps_meshes_and_rebuilds_only_its_painter_chunk() {
    let objects = (0..1024)
        .map(|index| {
            let r = 1.0 + index as f32 / 2048.0;
            let path = VectorPath::new()
                .move_to(Vec2::new(-r, -1.0))
                .line_to(Vec2::new(r, -1.0))
                .line_to(Vec2::new(r, 1.0))
                .line_to(Vec2::new(-r, 1.0))
                .close();
            CompiledObject::new(
                ObjectId::new(index),
                GeometryRef::path(path),
                Transform2D::IDENTITY,
                Style {
                    fill: Some(Color::WHITE),
                    stroke: None,
                    ..Style::default()
                },
            )
        })
        .collect();
    let mut runtime = SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
    let mut preparer = FramePreparer::new();
    preparer.prepare(runtime.frame());
    runtime.take_frame_changes();
    for paint in [
        Some(gradient(Color::rgb(1.0, 0.0, 0.0))),
        Some(gradient(Color::rgb(0.0, 1.0, 0.0))),
        None,
    ] {
        runtime
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: ObjectId::new(500),
                style: runtime.frame().objects[500].style,
                fill_gradient: paint,
                fill_image: None,
            })
            .unwrap();
        let changes = runtime.take_frame_changes();
        let prepared = preparer.prepare_incremental(runtime.frame(), &changes);
        assert_eq!(prepared.gradient_changes, &[ObjectId::new(500)]);
        assert_eq!(prepared.stats.full_rebuilds, 0);
        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert_eq!(prepared.stats.path_vertices_repacked, 0);
        assert_eq!(prepared.stats.path_indices_repacked, 0);
        assert!(prepared.stats.render_order_positions_visited <= 256);
    }
}

#[test]
fn gradient_only_edits_reuse_uniforms_and_clean_frames_upload_nothing() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut runtime = runtime(GeometryRef::rectangle(2.0, 2.0));
    let mut preparer = FramePreparer::new();
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.upload(&device, &queue, &preparer.prepare(runtime.frame()));
    assert!(renderer.fill_gradients.is_none());
    runtime.take_frame_changes();
    let original_style = runtime.frame().objects[0].style;
    for (color, allocations) in [
        (Color::rgb(1.0, 0.0, 0.0), 1),
        (Color::rgb(0.0, 1.0, 0.0), 0),
    ] {
        set_gradient(&mut runtime, Some(gradient(color)));
        assert_eq!(runtime.frame().objects[0].style, original_style);
        let changes = runtime.take_frame_changes();
        assert_eq!(changes.object_indices(), &[0]);
        let prepared = preparer.prepare_incremental(runtime.frame(), &changes);
        assert_eq!(prepared.gradient_changes, &[ObjectId::new(7)]);
        assert_eq!(prepared.stats.dirty_instance_count, 0);
        let stats = renderer.upload(&device, &queue, &prepared);
        assert_eq!(stats.buffer_reallocations, allocations);
        assert_eq!(stats.bytes_uploaded, size_of::<GradientUniform>());
        let prepared = preparer.prepare_incremental(runtime.frame(), &FrameChanges::default());
        assert!(prepared.gradient_changes.is_empty());
        assert_eq!(
            renderer.upload(&device, &queue, &prepared),
            UploadStats::default()
        );
    }
    set_gradient(&mut runtime, None);
    let changes = runtime.take_frame_changes();
    renderer.upload(
        &device,
        &queue,
        &preparer.prepare_incremental(runtime.frame(), &changes),
    );
    assert!(renderer
        .fill_gradients
        .as_ref()
        .unwrap()
        .residents
        .is_empty());
}

#[test]
#[ignore = "requires a real native raster adapter; absence is a failure"]
fn continuous_gradient_pixels_match_literal_encoded_ldr_reference() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    eprintln!("gradient raster adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let square = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0))
        .line_to(Vec2::new(-1.0, 1.0))
        .close();
    let concave = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0))
        .line_to(Vec2::new(0.0, 0.5))
        .line_to(Vec2::new(-1.0, 1.0))
        .close();
    for geometry in [
        GeometryRef::rectangle(2.0, 2.0),
        GeometryRef::circle(1.0),
        GeometryRef::path(square),
        GeometryRef::path(concave),
    ] {
        let mut runtime = runtime(geometry);
        let mut preparer = FramePreparer::new();
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, 64, 64);
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(4.0, 4.0)).unwrap(),
        );
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("linear gradient pixel qualification"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        renderer.upload(&device, &queue, &preparer.prepare(runtime.frame()));
        runtime.take_frame_changes();
        for profile in 0..8 {
            let red = Color::rgb(1.0, 0.0, 0.0);
            let blue = Color::rgb(0.0, 0.0, 1.0);
            let make = |start, end, stops: Vec<(f64, Color)>| {
                Some(Arc::new(
                    LinearGradient::new(
                        start,
                        end,
                        stops
                            .into_iter()
                            .map(|(p, c)| GradientStop::new(p, c).unwrap()),
                    )
                    .unwrap(),
                ))
            };
            let paint = match profile {
                0 => None,
                2 => Some(gradient(Color::rgb(0.0, 1.0, 0.0))),
                3 => Some(gradient(Color::rgba(1.0, 0.0, 0.0, 0.0))),
                4 => make(
                    [0.0, 0.5],
                    [1.0, 0.5],
                    vec![
                        (0.0, red),
                        (31.0 / 64.0, red),
                        (31.0 / 64.0, blue),
                        (1.0, blue),
                    ],
                ),
                5 => make([0.0, 0.0], [1.0, 1.0], vec![(0.0, red), (1.0, blue)]),
                _ => Some(gradient(red)),
            };
            runtime
                .apply_execution_patch(&ExecutionPatch::SetStyle {
                    object: ObjectId::new(7),
                    fill_gradient: paint,
                    fill_image: None,
                    style: Style {
                        fill: Some(Color::rgba(
                            1.0,
                            1.0,
                            1.0,
                            if profile == 6 { 0.5 } else { 1.0 },
                        )),
                        stroke: None,
                        opacity: if profile == 6 { 0.5 } else { 1.0 },
                        ..Style::default()
                    },
                })
                .unwrap();
            if profile == 7 {
                runtime
                    .apply_execution_patch(&ExecutionPatch::SetTransform {
                        object: ObjectId::new(7),
                        transform: Transform2D {
                            translation: Vec2::new(0.125, -0.2),
                            scale: Vec2::new(1.2, 0.75),
                            rotation: 0.4,
                        },
                    })
                    .unwrap();
            }
            let changes = runtime.take_frame_changes();
            let prepared = preparer.prepare_incremental(runtime.frame(), &changes);
            renderer.upload(&device, &queue, &prepared);
            let mut encoder = device.create_command_encoder(&Default::default());
            renderer.encode(
                &mut encoder,
                &target.create_view(&Default::default()),
                &prepared,
                wgpu::Color::BLACK,
            );
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 64 * 256,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &target,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(64),
                    },
                },
                target.size(),
            );
            queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(Duration::from_secs(30)),
                })
                .unwrap();
            rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
            let pixels = buffer.slice(..).get_mapped_range().unwrap();
            // Literal interior references are independent of all engine samplers,
            // vertex attributes and gradient uniforms. Coverage is qualified separately.
            let quantize = |value: f64| ((value * 65535.0 + 0.5).floor() / 256.0).floor() as u8;
            for y in [31, 32] {
                let xs: &[u32] = if profile == 7 {
                    &[28, 30, 31, 32, 33, 35]
                } else {
                    &[20, 24, 31, 32, 39, 43]
                };
                for &x in xs {
                    let px = (f64::from(x) + 0.5 - 32.0) / 16.0;
                    let py = (32.0 - f64::from(y) - 0.5) / 16.0;
                    let t = if profile == 7 {
                        let dx = px - 0.125;
                        let dy = py + 0.2;
                        ((0.4_f64.cos() * dx + 0.4_f64.sin() * dy) / 1.2 + 1.0) / 2.0
                    } else if profile == 5 {
                        (px + py + 2.0) / 4.0
                    } else {
                        (px + 1.0) / 2.0
                    };
                    let channels = match profile {
                        0 => [1.0, 1.0, 1.0],
                        2 => [0.0, 1.0 - t, t],
                        3 => [0.0, 0.0, t],
                        4 if t < 31.0 / 64.0 => [1.0, 0.0, 0.0],
                        4 => [0.0, 0.0, 1.0],
                        6 => [(1.0 - t) * 0.25, 0.0, t * 0.25],
                        _ => [1.0 - t, 0.0, t],
                    };
                    let expected = [
                        quantize(channels[0]),
                        quantize(channels[1]),
                        quantize(channels[2]),
                        255,
                    ];
                    let start = ((y * 64 + x) * 4) as usize;
                    let actual = &pixels[start..start + 4];
                    assert!(
                        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
                        "profile={profile} ({x},{y}): {actual:?} != {expected:?}"
                    );
                }
            }
            drop(pixels);
            buffer.unmap();
        }
    }
}
