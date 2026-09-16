//! Qualification of the ordinary retained image lane, not a second renderer.
use noon::{ExecutionSession, ImageMobjectOptions, RasterImageSampling, Scene};
use noon_core::{
    FontResourceArena, GeometryResourceArena, RasterImageResourceArena, TextResourceArena,
};
use noon_render_wgpu::text::TextDeviceMetrics;
use noon_render_wgpu::{
    GpuRenderer, RasterImagePrepareError, RetainedFramePreparer, RetainedPrepareError,
    RetainedTextGpuState, RetainedUploadStats,
};
use noon_runtime::FrameChanges;

const PIXELS: [u8; 16] = [
    255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 255, 255,
];
fn options() -> ImageMobjectOptions {
    let mut options = ImageMobjectOptions::rgba8(2, 2, PIXELS.as_slice()).unwrap();
    options.set_height(2.0).unwrap();
    options.set_sampling(RasterImageSampling::Nearest);
    options
}
fn upload(
    session: &mut ExecutionSession,
    preparer: &mut RetainedFramePreparer,
    renderer: &mut GpuRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    text: &mut RetainedTextGpuState,
) -> RetainedUploadStats {
    let publication = session.take_renderer_publication();
    let prepared = preparer
        .prepare_publication(
            device,
            queue,
            &publication,
            TextDeviceMetrics::uniform(32.0).unwrap(),
        )
        .unwrap();
    renderer.upload_retained(device, queue, &prepared, text)
}

#[test]
fn shared_textures_motion_opacity_sampling_and_retirement_are_sparse() {
    let mut scene = Scene::new();
    let a = scene.image(options()).unwrap();
    let b = scene.image(options()).unwrap();
    scene.add_many(&[(&a).into(), (&b).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut text = renderer.create_retained_text_state(&device, &queue);
    let mut preparer = RetainedFramePreparer::new();
    let first = upload(
        &mut session,
        &mut preparer,
        &mut renderer,
        &device,
        &queue,
        &mut text,
    );
    assert_eq!(first.images.textures_uploaded, 1);
    assert_eq!(first.images.pixel_bytes_uploaded, PIXELS.len());
    assert_eq!(first.images.instances_uploaded, 2);
    assert_eq!(first.bytes_uploaded(), PIXELS.len() + 2 * 48);
    assert_eq!(renderer.image_residency_stats().objects, 2);
    let baseline = preparer.incremental_stats();
    for tick in 1..=128 {
        scene
            .live(&mut session)
            .set_translation(&a, tick as f64 * 0.01, 0.0)
            .unwrap();
        let changed = upload(
            &mut session,
            &mut preparer,
            &mut renderer,
            &device,
            &queue,
            &mut text,
        );
        assert_eq!(changed.images.textures_uploaded, 0);
        assert_eq!(changed.images.pixel_bytes_uploaded, 0);
        assert_eq!(changed.images.instances_uploaded, 1);
        assert_eq!(changed.images.instance_bytes_uploaded, 48);
        assert_eq!(changed.bytes_uploaded(), 48);
    }
    let after = preparer.incremental_stats();
    assert_eq!(after.scratch_rebuilds, baseline.scratch_rebuilds);
    assert_eq!(after.text_snapshot_copies, baseline.text_snapshot_copies);
    assert_eq!(after.mixed_order_rebuilds, baseline.mixed_order_rebuilds);
    assert_eq!(after.scratch_reuses - baseline.scratch_reuses, 128);
    scene.live(&mut session).set_opacity(&a, 0.25).unwrap();
    assert_eq!(
        upload(
            &mut session,
            &mut preparer,
            &mut renderer,
            &device,
            &queue,
            &mut text
        )
        .images
        .pixel_bytes_uploaded,
        0
    );
    scene
        .live(&mut session)
        .set_image_sampling(&a, RasterImageSampling::Bicubic)
        .unwrap();
    assert_eq!(
        upload(
            &mut session,
            &mut preparer,
            &mut renderer,
            &device,
            &queue,
            &mut text
        )
        .images
        .pixel_bytes_uploaded,
        0
    );
    let idle = upload(
        &mut session,
        &mut preparer,
        &mut renderer,
        &device,
        &queue,
        &mut text,
    );
    assert_eq!(idle.images, Default::default());
    assert_eq!(idle.bytes_uploaded(), 0);
    scene.live(&mut session).remove(&a).unwrap();
    let removal = upload(
        &mut session,
        &mut preparer,
        &mut renderer,
        &device,
        &queue,
        &mut text,
    );
    assert_eq!(removal.images.textures_retired, 0);
    assert_eq!(renderer.image_residency_stats().textures, 1);
    scene.live(&mut session).remove(&b).unwrap();
    let removal = upload(
        &mut session,
        &mut preparer,
        &mut renderer,
        &device,
        &queue,
        &mut text,
    );
    assert_eq!(removal.images.textures_retired, 1);
    assert_eq!(renderer.image_residency_stats(), Default::default());
}

#[test]
fn failed_image_preparation_cannot_be_reused_by_an_empty_delta() {
    let mut scene = Scene::new();
    let object = scene.image(options()).unwrap();
    scene.add(&object).unwrap();
    let session = scene.execution_session().unwrap();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut preparer = RetainedFramePreparer::new();
    let texts = TextResourceArena::new();
    let fonts = FontResourceArena::new();
    let geometry = GeometryResourceArena::new();
    let missing = RasterImageResourceArena::new();
    let metrics = TextDeviceMetrics::uniform(32.0).unwrap();
    for changes in [FrameChanges::all(), FrameChanges::default()] {
        let result = preparer.prepare_with_image_resources(
            &device,
            &queue,
            session.frame(),
            &changes,
            &texts,
            &fonts,
            &geometry,
            &missing,
            metrics,
        );
        assert!(matches!(
            result,
            Err(RetainedPrepareError::Image(
                RasterImagePrepareError::MissingResource(_)
            ))
        ));
    }
    assert!(preparer
        .prepare_with_image_resources(
            &device,
            &queue,
            session.frame(),
            &FrameChanges::default(),
            &texts,
            &fonts,
            &geometry,
            session.raster_image_resources(),
            metrics
        )
        .is_ok());
}

#[test]
fn skipped_image_generation_reconciles_without_reuploading_pixels() {
    let mut scene = Scene::new();
    let a = scene.image(options()).unwrap();
    let b = scene.image(options()).unwrap();
    scene.add_many(&[(&a).into(), (&b).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut text = renderer.create_retained_text_state(&device, &queue);
    let mut preparer = RetainedFramePreparer::new();
    upload(
        &mut session,
        &mut preparer,
        &mut renderer,
        &device,
        &queue,
        &mut text,
    );
    scene
        .live(&mut session)
        .set_translation(&a, 0.2, 0.0)
        .unwrap();
    preparer
        .prepare_publication(
            &device,
            &queue,
            &session.take_renderer_publication(),
            TextDeviceMetrics::uniform(32.0).unwrap(),
        )
        .unwrap();
    scene
        .live(&mut session)
        .set_translation(&b, 0.5, 0.0)
        .unwrap();
    let caught_up = upload(
        &mut session,
        &mut preparer,
        &mut renderer,
        &device,
        &queue,
        &mut text,
    );
    assert_eq!(caught_up.images.instances_uploaded, 2);
    assert_eq!(caught_up.images.pixel_bytes_uploaded, 0);
}

#[test]
#[ignore = "requires a real Vulkan adapter; run explicitly in native raster qualification"]
fn native_image_pixels_preserve_orientation_alpha_and_painter_order() {
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
            .expect("Vulkan qualification adapter");
        eprintln!("image raster adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let mut scene = Scene::new();
        let mut background = scene.rectangle(2.0, 2.0).unwrap();
        background
            .set_fill(20.0 / 255.0, 40.0 / 255.0, 60.0 / 255.0, 1.0)
            .unwrap();
        background.disable_stroke().unwrap();
        scene.add(&background).unwrap();
        let object = scene.image(options()).unwrap();
        scene.add(&object).unwrap();
        let mut foreground = scene.rectangle(0.5, 0.5).unwrap();
        foreground.set_fill(1.0, 1.0, 0.0, 1.0).unwrap();
        foreground.disable_stroke().unwrap();
        scene.add(&foreground).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, 64, 64);
        let mut text = renderer.create_retained_text_state(&device, &queue);
        let mut preparer = RetainedFramePreparer::new();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("image qualification target"),
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
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image qualification readback"),
            size: 64 * 64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let prepared = preparer
            .prepare_publication(
                &device,
                &queue,
                &session.take_renderer_publication(),
                TextDeviceMetrics::uniform(32.0).unwrap(),
            )
            .unwrap();
        renderer.upload_retained(&device, &queue, &prepared, &mut text);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let draw = renderer
            .encode_retained(
                &mut encoder,
                &texture.create_view(&Default::default()),
                &prepared,
                &text,
                wgpu::Color::BLACK,
                None,
            )
            .unwrap();
        assert_eq!(draw.images, 1);
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(64),
                },
            },
            wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap()
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        let pixels = buffer.slice(..).get_mapped_range().unwrap().to_vec();
        let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x + 1) * 4];
        assert_eq!(
            pixel(32, 32),
            [255, 255, 0, 255],
            "foreground geometry must draw after image"
        );
        assert_eq!(pixel(16, 16), [255, 0, 0, 255]);
        assert_eq!(
            pixel(16, 48),
            [20, 40, 60, 255],
            "transparent blue must not contaminate background"
        );
        assert_eq!(pixel(48, 48), [255, 255, 255, 255]);
        for (actual, expected) in pixel(48, 16).iter().zip([10u8, 148, 30, 255]) {
            assert!(
                actual.abs_diff(expected) <= 1,
                "straight-alpha pixel was not blended once: {:?}",
                pixel(48, 16)
            );
        }
        if let Ok(path) = std::env::var("NOON_IMAGE_RASTER_OUTPUT") {
            image::save_buffer(path, &pixels, 64, 64, image::ColorType::Rgba8).unwrap();
        }
        buffer.unmap();
    });
}
