//! Actual native retained rendering against the pinned Manim image oracle.
#![cfg(feature = "image-decode")]

use noon::{
    example_scenes::raster_image, LiveProgramStatus, RasterImageSampling, RustHostCallbackTable,
};
use noon_core::Vec2;
use noon_render_wgpu::{
    text::TextDeviceMetrics, Camera2D, GpuRenderer, RetainedFramePreparer, RetainedTextGpuState,
};
use noon_runtime::RendererPublication;
use std::path::{Path, PathBuf};

const SIZE: u32 = 256;
const WORLD_SIZE: f32 = 8.0;
struct Raster {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
    target: wgpu::Texture,
    readback: wgpu::Buffer,
}
impl Raster {
    async fn new() -> Self {
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
            .expect("native raster qualification requires Vulkan");
        eprintln!("native image adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, SIZE, SIZE);
        // The oracle explicitly uses an 8x8 world. Never compare it with the
        // low-level renderer's unrelated default 2x2 camera.
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(WORLD_SIZE, WORLD_SIZE)).unwrap(),
        );
        assert_eq!(
            renderer.camera().world_size,
            Vec2::new(WORLD_SIZE, WORLD_SIZE)
        );
        let text = renderer.create_retained_text_state(&device, &queue);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("native image oracle target"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
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
            label: Some("native image qualification readback"),
            size: u64::from(SIZE * SIZE * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            device,
            queue,
            renderer,
            preparer: RetainedFramePreparer::new(),
            text,
            target,
            readback,
        }
    }
    fn capture(&mut self, publication: &RendererPublication<'_>) -> Vec<u8> {
        let prepared = self
            .preparer
            .prepare_publication(
                &self.device,
                &self.queue,
                publication,
                TextDeviceMetrics::uniform(SIZE as f32 / WORLD_SIZE).unwrap(),
            )
            .unwrap();
        self.renderer
            .upload_retained(&self.device, &self.queue, &prepared, &mut self.text);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.renderer
            .encode_retained(
                &mut encoder,
                &self.target.create_view(&Default::default()),
                &prepared,
                &self.text,
                wgpu::Color::BLACK,
                None,
            )
            .unwrap();
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIZE * 4),
                    rows_per_image: Some(SIZE),
                },
            },
            wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap()
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        receiver.recv().unwrap().unwrap();
        let pixels = self.readback.slice(..).get_mapped_range().unwrap().to_vec();
        self.readback.unmap();
        pixels
    }
}
fn compare(pixels: &[u8], name: &str, oracle: &Path, output: &Path) -> bool {
    image::save_buffer(
        output.join(name),
        pixels,
        SIZE,
        SIZE,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let expected = image::open(oracle.join(name)).unwrap().to_rgba8();
    assert_eq!(expected.dimensions(), (SIZE, SIZE));
    let mut sum = 0u64;
    let mut outliers = 0u64;
    let mut maximum = 0;
    assert_eq!(pixels.len(), expected.as_raw().len());
    for (actual, expected) in pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_raw().as_chunks::<4>().0)
    {
        let mut largest = 0;
        for channel in 0..3 {
            let error = actual[channel].abs_diff(expected[channel]);
            sum += u64::from(error);
            largest = largest.max(error);
        }
        maximum = maximum.max(largest);
        outliers += u64::from(largest > 8);
    }
    let mean = sum as f64 / f64::from(SIZE * SIZE * 3);
    let fraction = outliers as f64 / f64::from(SIZE * SIZE);
    eprintln!("{name}: mean={mean:.5}, outliers={fraction:.5}, max={maximum}");
    mean <= 1.0 && fraction <= 0.008
}

#[test]
#[ignore = "requires Vulkan and artifacts from scripts/image-manim-reference.py"]
fn native_images_match_manim_sampling_opacity_and_lifecycle() {
    let oracle = PathBuf::from(
        std::env::var("NOON_IMAGE_MANIM_DIRECTORY").expect("pinned Manim oracle directory"),
    );
    let output = PathBuf::from(
        std::env::var("NOON_IMAGE_ARTIFACTS").expect("qualification artifact directory"),
    )
    .join("native");
    std::fs::create_dir_all(&output).unwrap();
    let mut raster = pollster::block_on(Raster::new());
    let mut failures = Vec::new();
    for (name, sampling) in [
        ("nearest", RasterImageSampling::Nearest),
        ("bilinear", RasterImageSampling::Linear),
        ("bicubic", RasterImageSampling::Bicubic),
    ] {
        for opacity in [1.0, 0.5] {
            let mut session = raster_image::sampling_session(sampling, opacity).unwrap();
            let pixels = raster.capture(&session.take_renderer_publication());
            let name = format!("{name}-{opacity:.1}.png");
            if !compare(&pixels, &name, &oracle, &output) {
                failures.push(name);
            }
        }
    }
    let mut program = raster_image::program().unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    for time in [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5] {
        for attempt in 0..16 {
            assert!(attempt < 15, "image continuation did not settle at {time}");
            match program.status() {
                LiveProgramStatus::ReadyToResume => {
                    program.resume().unwrap();
                }
                LiveProgramStatus::PublicationPending(expected) => {
                    let publication = program.take_renderer_publication();
                    assert_eq!(publication.context(), expected);
                    raster.capture(&publication);
                    program.admit_publication(expected).unwrap();
                }
                LiveProgramStatus::Awaiting(_) => {
                    if program.session().frame().time == time {
                        break;
                    }
                    program.drive_to(&mut callbacks, time).unwrap();
                }
                LiveProgramStatus::Finished => break,
                LiveProgramStatus::Terminal => panic!("terminal image program"),
            }
        }
        assert_eq!(program.session().frame().time, time);
        let pixels = raster.capture(&program.take_renderer_publication());
        let name = format!("lifecycle-{time:.1}.png");
        if !compare(&pixels, &name, &oracle, &output) {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "Manim image raster mismatch: {failures:?}"
    );
}
