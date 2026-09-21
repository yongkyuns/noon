use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{Color, GeometryRef, ObjectId, Style, Transform2D, Vec2, VectorPath};
use noon_render_wgpu::{
    AnalyticOverlay, Camera2D, FrameComposition, FramePreparer, GpuRenderer, OverlayGpuState,
    SecondaryViewport, SecondaryViewportError,
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

        let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
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
