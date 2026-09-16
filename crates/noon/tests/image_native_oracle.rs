//! Explicit real-adapter differential test against the pinned Manim image oracle.
//! No software image renderer substitutes for Noon's ordinary retained GPU path.
use noon::{
    example_scenes::raster_image, LiveProgramStatus, RasterImageSampling, RustHostCallbackTable,
};
use noon_core::Vec2;
use noon_render_wgpu::{
    text::TextDeviceMetrics, Camera2D, GpuRenderer, RetainedFramePreparer, RetainedTextGpuState,
};
use noon_runtime::RendererPublication;
use std::path::Path;

const SIDE: u32 = 256;
struct RasterTarget {
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
    texture: wgpu::Texture,
    readback: wgpu::Buffer,
}
impl RasterTarget {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut renderer = GpuRenderer::new(device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(device, queue, SIDE, SIDE);
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 8.0)).unwrap(),
        );
        let text = renderer.create_retained_text_state(device, queue);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("image oracle target"),
            size: wgpu::Extent3d {
                width: SIDE,
                height: SIDE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image oracle readback"),
            size: u64::from(SIDE * SIDE * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            renderer,
            preparer: RetainedFramePreparer::new(),
            text,
            texture,
            readback,
        }
    }
    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        publication: &RendererPublication<'_>,
    ) -> Vec<u8> {
        let prepared = self
            .preparer
            .prepare_publication(
                device,
                queue,
                publication,
                TextDeviceMetrics::uniform(32.0).unwrap(),
            )
            .unwrap();
        self.renderer
            .upload_retained(device, queue, &prepared, &mut self.text);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.renderer
            .encode_retained(
                &mut encoder,
                &self.texture.create_view(&Default::default()),
                &prepared,
                &self.text,
                wgpu::Color::BLACK,
                None,
            )
            .unwrap();
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIDE * 4),
                    rows_per_image: Some(SIDE),
                },
            },
            wgpu::Extent3d {
                width: SIDE,
                height: SIDE,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        let pixels = self.readback.slice(..).get_mapped_range().unwrap().to_vec();
        self.readback.unmap();
        pixels
    }
}

fn compare(directory: &Path, output: &Path, name: &str, actual: &[u8]) {
    let expected = image::open(directory.join(name))
        .expect("pinned reference PNG")
        .to_rgba8();
    assert_eq!(expected.dimensions(), (SIDE, SIDE));
    image::save_buffer(
        output.join(name),
        actual,
        SIDE,
        SIDE,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let mut sum = 0u64;
    let mut beyond = 0usize;
    for (a, e) in actual
        .chunks_exact(4)
        .zip(expected.as_raw().chunks_exact(4))
    {
        let error = [
            a[0].abs_diff(e[0]),
            a[1].abs_diff(e[1]),
            a[2].abs_diff(e[2]),
        ];
        sum += error.iter().map(|&e| u64::from(e)).sum::<u64>();
        beyond += usize::from(error.into_iter().any(|e| e > 8));
    }
    let mean = sum as f64 / f64::from(SIDE * SIDE * 3);
    let fraction = beyond as f64 / f64::from(SIDE * SIDE);
    eprintln!("{name}: mean RGB error={mean:.6}, fraction beyond 8={fraction:.6}");
    // Same whole-frame acceptance as the browser differential, not a central crop.
    assert!(
        mean <= 1.0 && fraction <= 0.008,
        "{name}: mean={mean}, fraction={fraction}"
    );
}

#[test]
#[ignore = "requires Vulkan and a generated ManimCE 0.21.0 oracle; run explicitly"]
fn native_image_filters_and_lifecycle_match_pinned_manim() {
    let directory = std::env::var("NOON_IMAGE_MANIM_DIRECTORY")
        .expect("generate scripts/image-manim-reference.py first");
    let directory = Path::new(&directory);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["manim"], "0.21.0");
    let output = std::env::var("NOON_IMAGE_NATIVE_ORACLE_OUTPUT")
        .unwrap_or_else(|_| "browser-smoke-artifacts/images/native-oracle".into());
    let output = Path::new(&output);
    std::fs::create_dir_all(output).unwrap();
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
            .expect("real Vulkan qualification adapter");
        eprintln!("image oracle adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        for (name, sampling) in [
            ("nearest", RasterImageSampling::Nearest),
            ("bilinear", RasterImageSampling::Linear),
            ("bicubic", RasterImageSampling::Bicubic),
        ] {
            for opacity in [1.0, 0.5] {
                let mut session = raster_image::sampling_session(sampling, opacity).unwrap();
                let mut target = RasterTarget::new(&device, &queue);
                let pixels = target.render(&device, &queue, &session.take_renderer_publication());
                compare(
                    directory,
                    output,
                    &format!("{name}-{opacity:.1}.png"),
                    &pixels,
                );
            }
        }
        let mut program = raster_image::program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        let mut target = RasterTarget::new(&device, &queue);
        program.resume().unwrap();
        for time in [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5] {
            if let LiveProgramStatus::PublicationPending(context) =
                program.drive_to(&mut callbacks, time).unwrap()
            {
                target.render(&device, &queue, &program.take_renderer_publication());
                program.admit_publication(context).unwrap();
                program.resume().unwrap();
                program.drive_to(&mut callbacks, time).unwrap();
            }
            let pixels = target.render(&device, &queue, &program.take_renderer_publication());
            compare(
                directory,
                output,
                &format!("lifecycle-{time:.1}.png"),
                &pixels,
            );
        }
    });
}
