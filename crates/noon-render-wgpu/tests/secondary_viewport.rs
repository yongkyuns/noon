use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    Camera2DState, Color, FontResourceArena, GeometryRef, GeometryResourceArena, Inset2DViewState,
    ObjectId, Style, TextResourceArena, Transform2D, Vec2, VectorPath,
};
use noon_render_wgpu::{
    AnalyticOverlay, Camera2D, FrameComposition, FramePreparer, GpuRenderer, OverlayGpuState,
    PreparedRetainedGpuFrame, RetainedFramePreparer, RetainedTextGpuState, SecondaryViewport,
    SecondaryViewportError,
};
use noon_runtime::SceneInstance;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 64;

#[test]
fn secondary_viewport_is_a_bounded_composition_descriptor() {
    let camera = Camera2D::new(Vec2::new(2.0, -1.0), Vec2::new(4.0, 3.0)).unwrap();
    let viewport = SecondaryViewport::new(camera, [640, 360, 320, 180], [1280, 720]).unwrap();
    assert_eq!(viewport.camera, camera);
    assert_eq!(viewport.destination, [640, 360, 320, 180]);
}

#[test]
fn secondary_viewport_rejects_empty_overflowing_and_out_of_bounds_destinations() {
    let camera = Camera2D::DEFAULT;
    assert_eq!(
        SecondaryViewport::new(camera, [0, 0, 0, 10], [100, 100]).unwrap_err(),
        SecondaryViewportError::EmptyDestination
    );
    assert_eq!(
        SecondaryViewport::new(camera, [90, 0, 11, 10], [100, 100]).unwrap_err(),
        SecondaryViewportError::DestinationOutOfBounds
    );
    assert_eq!(
        SecondaryViewport::new(camera, [u32::MAX, 0, 2, 1], [u32::MAX, 1]).unwrap_err(),
        SecondaryViewportError::DestinationOutOfBounds
    );
}

fn scene_with_geometry(geometry: GeometryRef, style: Style) -> SceneInstance {
    SceneInstance::new(
        CompiledScene::compile_objects(
            vec![CompiledObject::new(
                ObjectId::new(0),
                geometry,
                Transform2D::IDENTITY,
                style,
            )],
            &[],
        )
        .unwrap(),
    )
}

fn rgba(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * WIDTH + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().unwrap()
}

fn submit_and_read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    target: &wgpu::Texture,
    readback: &wgpu::Buffer,
) -> Vec<u8> {
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(WIDTH * 4),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().unwrap();
    let pixels = readback.slice(..).get_mapped_range().unwrap().to_vec();
    readback.unmap();
    pixels
}

#[test]
fn inset_capture_reuse_matches_fresh_rasters_at_filtered_edges_and_path_samples() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!("skipping inset capture pixel comparison: no GPU adapter is available");
            return;
        };
        eprintln!(
            "inset capture pixel comparison adapter: {:?}",
            adapter.get_info()
        );
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("inset capture pixel comparison target"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("inset capture pixel comparison readback"),
            size: u64::from(WIDTH * HEIGHT * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let texts = TextResourceArena::new();
        let fonts = FontResourceArena::new();
        let geometries = GeometryResourceArena::new();

        let render = |renderer: &mut GpuRenderer,
                      text: &mut RetainedTextGpuState,
                      prepared: &PreparedRetainedGpuFrame<'_>,
                      inset: Inset2DViewState| {
            renderer
                .set_inset_2d_views(&device, &queue, text, &[inset])
                .unwrap();
            renderer.upload_retained(&device, &queue, prepared, text);
            let mut encoder = device.create_command_encoder(&Default::default());
            let draws = renderer
                .encode_retained(
                    &mut encoder,
                    &view,
                    prepared,
                    text,
                    wgpu::Color::BLACK,
                    None,
                )
                .unwrap();
            let pixels = submit_and_read(&device, &queue, encoder, &target, &readback);
            (pixels, draws)
        };

        for with_path in [false, true] {
            let mut reused = GpuRenderer::new(&device, &queue, format);
            reused.set_viewport(&device, &queue, WIDTH, HEIGHT);
            reused.set_camera(
                &queue,
                Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 4.0)).unwrap(),
            );
            let mut reused_text = reused.create_retained_text_state(&device, &queue);
            let mut reused_preparer = RetainedFramePreparer::new();
            reused_preparer.set_inset_views_active(true);
            for scale in [2.0, 1.5, 2.9, 1.2, 0.3, 0.3] {
                let mut objects = vec![
                    CompiledObject::new(
                        ObjectId::new(0),
                        GeometryRef::rectangle(3.0, 3.0),
                        Transform2D::IDENTITY,
                        Style {
                            fill: Some(Color::rgba(0.05, 0.8, 0.1, 1.0)),
                            stroke: None,
                            ..Style::default()
                        },
                    ),
                    CompiledObject::new(
                        ObjectId::new(1),
                        GeometryRef::circle(0.7),
                        Transform2D::IDENTITY,
                        Style {
                            fill: Some(Color::WHITE),
                            stroke: None,
                            ..Style::default()
                        },
                    ),
                ];
                let mut panel_backing = Transform2D::IDENTITY;
                panel_backing.translation = Vec2::new(2.1, 0.2);
                objects.push(CompiledObject::new(
                    ObjectId::new(2),
                    GeometryRef::rectangle(4.0, 4.0),
                    panel_backing,
                    Style {
                        fill: Some(Color::rgba(0.1, 0.1, 0.8, 1.0)),
                        stroke: None,
                        ..Style::default()
                    },
                ));
                if with_path {
                    objects.push(CompiledObject::new(
                        ObjectId::new(3),
                        GeometryRef::path(
                            VectorPath::new()
                                .move_to(Vec2::new(-0.9, -0.7))
                                .line_to(Vec2::new(0.9, 0.65)),
                        ),
                        Transform2D::IDENTITY,
                        Style {
                            fill: None,
                            stroke: Some(Color::WHITE),
                            stroke_width: 0.11,
                            ..Style::default()
                        },
                    ));
                }
                let display_id = ObjectId::new(4);
                let mut display_transform = Transform2D::IDENTITY;
                display_transform.translation = Vec2::new(2.1, 0.2);
                display_transform.scale = Vec2::new(scale, scale);
                objects.push(CompiledObject::new(
                    display_id,
                    GeometryRef::rectangle(1.0, 1.0),
                    display_transform,
                    Style {
                        fill: None,
                        stroke: Some(Color::WHITE),
                        stroke_width: 0.002,
                        ..Style::default()
                    },
                ));
                let scene =
                    SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
                let inset = Inset2DViewState {
                    camera_frame: ObjectId::new(4),
                    display: display_id,
                    camera: Camera2DState {
                        center: Vec2::ZERO,
                        height: 2.0,
                    },
                    display_center: Vec2::new(2.1, 0.2),
                    display_size: Vec2::new(scale, scale),
                    display_stroke_width: 0.002,
                    capture_own_display: false,
                };
                let prepared = reused_preparer
                    .prepare(
                        &device,
                        scene.frame(),
                        &texts,
                        &fonts,
                        &geometries,
                        noon_render_wgpu::text::TextDeviceMetrics::uniform(64.0).unwrap(),
                    )
                    .unwrap();
                let (reused_pixels, reused_draws) =
                    render(&mut reused, &mut reused_text, &prepared, inset);
                assert_eq!(reused_draws.images, 1, "the inset capture must draw");

                let mut fresh = GpuRenderer::new(&device, &queue, format);
                fresh.set_viewport(&device, &queue, WIDTH, HEIGHT);
                fresh.set_camera(
                    &queue,
                    Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 4.0)).unwrap(),
                );
                let mut fresh_text = fresh.create_retained_text_state(&device, &queue);
                let mut fresh_preparer = RetainedFramePreparer::new();
                fresh_preparer.set_inset_views_active(true);
                let fresh_prepared = fresh_preparer
                    .prepare(
                        &device,
                        scene.frame(),
                        &texts,
                        &fonts,
                        &geometries,
                        noon_render_wgpu::text::TextDeviceMetrics::uniform(64.0).unwrap(),
                    )
                    .unwrap();
                let (fresh_pixels, fresh_draws) =
                    render(&mut fresh, &mut fresh_text, &fresh_prepared, inset);
                assert_eq!(fresh_draws.images, 1, "the fresh inset capture must draw");
                assert_eq!(
                    reused_pixels, fresh_pixels,
                    "reused capture at scale {scale}, path={with_path} differs from fresh exact capture"
                );

                if scale < 1.0 {
                    continue; // Tiny captures still require exact pixels, without a resolved interior.
                }

                let left = (0.5 + (2.1 - scale * 0.5) / 8.0) * WIDTH as f32;
                let top = (0.5 - (0.2 + scale * 0.5) / 4.0) * HEIGHT as f32;
                let edge = rgba(&reused_pixels, left.ceil() as u32, top.ceil() as u32);
                assert!(
                    edge[1] > edge[0].saturating_mul(2) && edge[1] > 150,
                    "filtered edge should clamp to the colored source background: scale={scale}, path={with_path}, {edge:?}"
                );
                let center = rgba(
                    &reused_pixels,
                    ((0.5 + 2.1 / 8.0) * WIDTH as f32) as u32,
                    ((0.5 - 0.2 / 4.0) * HEIGHT as f32) as u32,
                );
                assert!(
                    center[0] > 200 && center[1] > 200 && center[2] > 200,
                    "captured source circle should remain visible in the inset: {center:?}"
                );
            }
        }
    });
}

#[test]
#[ignore = "requires software Vulkan; executed by Native Host Smoke"]
fn composed_secondary_views_keep_camera_state_isolated_overlay_last_and_rejection_atomic() {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                force_fallback_adapter: true,
                ..Default::default()
            })
            .await
            .expect("secondary viewport qualification requires Vulkan");
        eprintln!("secondary viewport adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("secondary viewport qualification target"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("secondary viewport qualification readback"),
            size: u64::from(WIDTH * HEIGHT * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let scene = scene_with_geometry(
            GeometryRef::circle(0.35),
            Style {
                fill: Some(Color::WHITE),
                stroke: None,
                ..Style::default()
            },
        );
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(scene.frame());

        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(2.0, 2.0)).unwrap(),
        );
        renderer.upload(&device, &queue, &prepared);

        let mut overlay_transform = Transform2D::IDENTITY;
        overlay_transform.translation = Vec2::new(-0.5, 0.0);
        let overlay_geometry = GeometryRef::circle(0.08);
        let overlay = AnalyticOverlay::new(
            &overlay_geometry,
            overlay_transform,
            Color::rgba(1.0, 0.0, 0.0, 0.6),
        )
        .unwrap();
        let mut overlay_state = OverlayGpuState::default();
        overlay_state.update(&device, &queue, Some(overlay));

        let secondary = [
            SecondaryViewport::new(
                Camera2D::new(Vec2::ZERO, Vec2::new(2.0, 2.0)).unwrap(),
                [0, 0, WIDTH / 2, HEIGHT],
                [WIDTH, HEIGHT],
            )
            .unwrap(),
            SecondaryViewport::new(
                Camera2D::new(Vec2::new(10.0, 0.0), Vec2::new(2.0, 2.0)).unwrap(),
                [WIDTH / 2, 0, WIDTH / 2, HEIGHT],
                [WIDTH, HEIGHT],
            )
            .unwrap(),
        ];

        let mut encoder = device.create_command_encoder(&Default::default());
        renderer
            .encode_composed_frame(
                &device,
                &queue,
                &mut encoder,
                &view,
                FrameComposition {
                    prepared: &prepared,
                    presentations: None,
                    secondary_viewports: &secondary,
                    overlay: Some(&overlay_state),
                    clear_color: wgpu::Color::BLACK,
                    query_set: None,
                },
            )
            .unwrap();
        let pixels = submit_and_read(&device, &queue, encoder, &target, &readback);

        let left_secondary = rgba(&pixels, 40, HEIGHT / 2);
        assert!(
            left_secondary[0] > 240 && left_secondary[1] > 240 && left_secondary[2] > 240,
            "first secondary camera must retain its own visible circle: {left_secondary:?}"
        );
        let right_secondary = rgba(&pixels, 96, HEIGHT / 2);
        assert!(
            right_secondary[0] < 10 && right_secondary[1] < 10 && right_secondary[2] < 10,
            "second secondary camera must independently look away: {right_secondary:?}"
        );
        let overlay_pixel = rgba(&pixels, 32, HEIGHT / 2);
        assert!(
            overlay_pixel[0] > 220 && overlay_pixel[1] < 200 && overlay_pixel[2] < 200,
            "session overlay must be composed after every secondary view: {overlay_pixel:?}"
        );

        let path = VectorPath::new()
            .move_to(Vec2::new(-0.5, -0.5))
            .line_to(Vec2::new(0.5, 0.5));
        let path_scene = scene_with_geometry(
            GeometryRef::path(path),
            Style {
                fill: None,
                stroke: Some(Color::WHITE),
                stroke_width: 0.1,
                ..Style::default()
            },
        );
        let mut path_preparer = FramePreparer::new();
        let path_prepared = path_preparer.prepare(path_scene.frame());

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.25,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })];
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("secondary rejection atomicity baseline"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        assert_eq!(
            renderer
                .encode_composed_frame(
                    &device,
                    &queue,
                    &mut encoder,
                    &view,
                    FrameComposition {
                        prepared: &path_prepared,
                        presentations: None,
                        secondary_viewports: &secondary[..1],
                        overlay: None,
                        clear_color: wgpu::Color::BLACK,
                        query_set: None,
                    },
                )
                .unwrap_err(),
            SecondaryViewportError::MultisampledContentUnsupported
        );
        let rejected = submit_and_read(&device, &queue, encoder, &target, &readback);
        let untouched = rgba(&rejected, WIDTH / 2, HEIGHT / 2);
        assert!(
            untouched[0] < 10
                && untouched[1] < 10
                && (55..=75).contains(&untouched[2]),
            "rejected composition must not encode a partial primary or secondary pass: {untouched:?}"
        );
    });
}

#[test]
fn line_gpu_coverage_is_stable_at_pixel_phases_and_preserves_caps() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!("skipping butt-line coverage readback: no GPU adapter is available");
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("butt line coverage target"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("butt line coverage readback"),
            size: u64::from(WIDTH * HEIGHT * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(2.0, 1.0)).unwrap(),
        );
        let gray = Color::rgba(0.5, 0.5, 0.5, 1.0);
        let style = Style {
            fill: None,
            stroke: Some(gray),
            stroke_width: 0.01,
            stroke_cap: noon_core::StrokeCap::Butt,
            ..Style::default()
        };
        let mut preparer = FramePreparer::new();
        let mut render_line = |renderer: &mut GpuRenderer, start: Vec2, end: Vec2| {
            let scene = scene_with_geometry(GeometryRef::line(start, end), style);
            let prepared = preparer.prepare(scene.frame());
            renderer.upload(&device, &queue, &prepared);
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            renderer.encode(&mut encoder, &view, &prepared, wgpu::Color::BLACK);
            submit_and_read(&device, &queue, encoder, &target, &readback)
        };
        let cross_sum = |pixels: &[u8], vertical: bool, fixed: u32| -> u32 {
            (-3_i32..=3)
                .map(|offset| {
                    let varying = (fixed as i32 + offset) as u32;
                    if vertical {
                        rgba(pixels, varying, 32)[0] as u32
                    } else {
                        rgba(pixels, 64, varying)[0] as u32
                    }
                })
                .sum()
        };
        // With 64 pixels per world unit, 1/128 world units shifts the line by
        // exactly half a pixel. The integrated opaque gray coverage should stay
        // stable across both phases instead of disappearing or brightening.
        let horizontal_integer =
            render_line(&mut renderer, Vec2::new(-0.65, 0.0), Vec2::new(0.65, 0.0));
        let horizontal_half = render_line(
            &mut renderer,
            Vec2::new(-0.65, 1.0 / 128.0),
            Vec2::new(0.65, 1.0 / 128.0),
        );
        let vertical_integer =
            render_line(&mut renderer, Vec2::new(0.0, -0.35), Vec2::new(0.0, 0.35));
        let vertical_half = render_line(
            &mut renderer,
            Vec2::new(1.0 / 128.0, -0.35),
            Vec2::new(1.0 / 128.0, 0.35),
        );
        for (name, total) in [
            (
                "horizontal integer",
                cross_sum(&horizontal_integer, false, 32),
            ),
            ("horizontal half", cross_sum(&horizontal_half, false, 32)),
            ("vertical integer", cross_sum(&vertical_integer, true, 64)),
            ("vertical half", cross_sum(&vertical_half, true, 64)),
        ] {
            assert!(
                (60..=105).contains(&total),
                "{name} integrated gray coverage: {total}"
            );
        }

        let diagonal = render_line(
            &mut renderer,
            Vec2::new(-0.35, -0.35),
            Vec2::new(0.35, 0.35),
        );
        let diagonal_energy: u32 = (29..=35)
            .flat_map(|y| (61..=67).map(move |x| (x, y)))
            .map(|(x, y)| rgba(&diagonal, x, y)[0] as u32)
            .sum();
        assert!(
            diagonal_energy > 400,
            "diagonal line must cover pixels: {diagonal_energy}"
        );

        let capped = render_line(&mut renderer, Vec2::new(-0.25, 0.0), Vec2::new(0.25, 0.0));
        assert_eq!(
            rgba(&capped, 47, 31)[0],
            0,
            "butt cap must not extend before start"
        );
        assert_eq!(
            rgba(&capped, 47, 32)[0],
            0,
            "butt cap must not extend before start"
        );
        assert!(
            rgba(&capped, 48, 31)[0] > 0,
            "line must cover its first in-bounds pixel"
        );
        assert_eq!(
            rgba(&capped, 80, 31)[0],
            0,
            "butt cap must not extend past end"
        );
        assert_eq!(
            rgba(&capped, 80, 32)[0],
            0,
            "butt cap must not extend past end"
        );

        // Retain the same line while only the camera changes. Round and butt
        // bodies have the same pixel-box integral; rounded endpoints must not
        // make a subpixel-width body pulse in brightness while panning.
        for cap in [noon_core::StrokeCap::Butt, noon_core::StrokeCap::Round] {
            for vertical in [false, true] {
                for width_px in [0.25_f32, 0.64, 1.3518, 2.5] {
                    let (start, end) = if vertical {
                        (Vec2::new(0.0, -0.35), Vec2::new(0.0, 0.35))
                    } else {
                        (Vec2::new(-0.65, 0.0), Vec2::new(0.65, 0.0))
                    };
                    let scene = scene_with_geometry(
                        GeometryRef::line(start, end),
                        Style {
                            stroke_width: width_px / 64.0,
                            stroke_cap: cap,
                            ..style
                        },
                    );
                    let prepared = preparer.prepare(scene.frame());
                    renderer.upload(&device, &queue, &prepared);
                    for phase in [0.0_f32, 0.125, 0.25, 0.4375, 0.5, 0.75, 0.875] {
                        let center = if vertical {
                            Vec2::new(-phase / 64.0, 0.0)
                        } else {
                            Vec2::new(0.0, phase / 64.0)
                        };
                        renderer.set_camera(
                            &queue,
                            Camera2D::new(center, Vec2::new(2.0, 1.0)).unwrap(),
                        );
                        let mut encoder = device.create_command_encoder(&Default::default());
                        renderer.encode(&mut encoder, &view, &prepared, wgpu::Color::BLACK);
                        let pixels = submit_and_read(&device, &queue, encoder, &target, &readback);
                        let fixed = if vertical { WIDTH / 2 } else { HEIGHT / 2 };
                        let total = cross_sum(&pixels, vertical, fixed);
                        // cairo_source_color maps opaque 0.5 gray to 128/255.
                        let expected = 128.0 * width_px;
                        assert!(
                            (total as f32 - expected).abs() <= 2.0,
                            "{cap:?}/{vertical}: width={width_px}, phase={phase}, energy={total}"
                        );
                        let pixel_center = fixed as f32 + phase;
                        for offset in -3_i32..=3 {
                            let coordinate = (fixed as i32 + offset) as u32;
                            let lower = (pixel_center - width_px * 0.5).max(coordinate as f32);
                            let upper =
                                (pixel_center + width_px * 0.5).min(coordinate as f32 + 1.0);
                            let coverage = (upper - lower).clamp(0.0, 1.0);
                            let actual = if vertical {
                                rgba(&pixels, coordinate, HEIGHT / 2)[0]
                            } else {
                                rgba(&pixels, WIDTH / 2, coordinate)[0]
                            };
                            assert!(
                                (f32::from(actual) - 128.0 * coverage).abs() <= 1.0,
                                "{cap:?}/{vertical}: w={width_px}, phase={phase}, pixel={coordinate}"
                            );
                        }
                    }
                }
            }
        }

        // The body specialization must not turn a round end into a butt cap.
        let round_scene = scene_with_geometry(
            GeometryRef::line(Vec2::new(-0.25, 0.0), Vec2::new(0.25, 0.0)),
            Style {
                stroke_width: 4.0 / 64.0,
                stroke_cap: noon_core::StrokeCap::Round,
                ..style
            },
        );
        let prepared = preparer.prepare(round_scene.frame());
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(2.0, 1.0)).unwrap(),
        );
        renderer.upload(&device, &queue, &prepared);
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.encode(&mut encoder, &view, &prepared, wgpu::Color::BLACK);
        let rounded = submit_and_read(&device, &queue, encoder, &target, &readback);
        assert!(
            rgba(&rounded, 47, 31)[0] > 0,
            "round start extends beyond its endpoint"
        );
        assert!(
            rgba(&rounded, 80, 31)[0] > 0,
            "round end extends beyond its endpoint"
        );
        assert_eq!(
            rgba(&rounded, 44, 31)[0],
            0,
            "round start has bounded extent"
        );
        assert_eq!(rgba(&rounded, 83, 31)[0], 0, "round end has bounded extent");
    });
}

#[test]
#[ignore = "requires software Vulkan; executed by Native Host Smoke"]
fn zero_contribution_camera_is_pixel_identical_and_visible_strokes_restore() {
    use noon_runtime::FrameChanges;
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                force_fallback_adapter: true,
                ..Default::default()
            })
            .await
            .expect("zero-contribution qualification requires software Vulkan");
        eprintln!("zero-contribution adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("zero-contribution camera target"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("zero-contribution readback"),
            size: u64::from(WIDTH * HEIGHT * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let scene = SceneInstance::new(
            CompiledScene::compile_objects(
                vec![
                    CompiledObject::new(
                        ObjectId::new(0),
                        GeometryRef::rectangle(2.0, 2.0),
                        Transform2D::IDENTITY,
                        Style {
                            opacity: 0.0,
                            fill: Some(Color::WHITE),
                            stroke: None,
                            ..Style::default()
                        },
                    ),
                    CompiledObject::new(
                        ObjectId::new(1),
                        GeometryRef::circle(0.3),
                        Transform2D::IDENTITY,
                        Style {
                            fill: Some(Color::rgba(1.0, 0.0, 0.0, 1.0)),
                            stroke: None,
                            ..Style::default()
                        },
                    ),
                ],
                &[],
            )
            .unwrap(),
        );
        let mut frame = scene.frame().clone();
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(2.0, 2.0)).unwrap(),
        );
        let mut preparer = FramePreparer::new();
        let mut render = |prepared: &noon_render_wgpu::PreparedFrame<'_>| {
            renderer.upload(&device, &queue, prepared);
            let mut encoder = device.create_command_encoder(&Default::default());
            let draws = renderer.encode(&mut encoder, &view, prepared, wgpu::Color::BLACK);
            let pixels = submit_and_read(&device, &queue, encoder, &target, &readback);
            (draws, pixels)
        };
        let (initial_draw, hidden_pixels) = render(&preparer.prepare(&frame));
        assert_eq!(initial_draw.instances_drawn, 1);
        assert_eq!(initial_draw.draw_calls, 1);
        assert!(
            rgba(&hidden_pixels, WIDTH / 2, HEIGHT / 2)[0] > 200,
            "independent visible row remains rendered"
        );
        let mut omitted = frame.clone();
        omitted.presences[0] = false;
        let mut reference_preparer = FramePreparer::new();
        let (_, omitted_pixels) = render(&reference_preparer.prepare(&omitted));
        assert_eq!(
            hidden_pixels, omitted_pixels,
            "invisible camera and omitted draw must be byte-identical"
        );
        // Independent reference uploads require a full candidate upload afterward.
        render(&preparer.prepare(&frame));
        for opacity in [0.25, 1.0, 0.0, 0.5] {
            frame.objects[0].style.opacity = opacity;
            let (draws, pixels) =
                render(&preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0])));
            assert_eq!(draws.instances_drawn, 1 + usize::from(opacity != 0.0));
            if opacity == 0.0 {
                assert_eq!(pixels, hidden_pixels);
            } else {
                assert_ne!(pixels, hidden_pixels);
            }
        }
        frame.objects[0].style.opacity = 1.0;
        frame.objects[0].style.fill = Some(Color::rgba(1.0, 1.0, 1.0, 0.0));
        frame.objects[0].style.stroke = Some(Color::WHITE);
        frame.objects[0].style.stroke_width = 0.1;
        let (draws, stroked) =
            render(&preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0])));
        assert_eq!(draws.instances_drawn, 2);
        assert_ne!(
            stroked, hidden_pixels,
            "visible stroke must not be culled with transparent fill"
        );
        assert!(frame.presences[0]);
        assert_eq!(frame.objects[0].id, ObjectId::new(0));
    });
}
