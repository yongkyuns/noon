use noon::integration::{PixelChannelOrder, PixelRowOrder, Rgba8ReadbackLayout};
use noon_core::Vec2;
use noon_render_wgpu::{
    text::TextDeviceMetrics, Camera2D, GpuRenderer, RetainedFramePreparer, RetainedTextGpuState,
};
use noon_runtime::RendererPublication;

pub const SIZE: u32 = 256;
pub const WORLD_SIZE: f32 = 8.0;
pub struct Raster {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
    target: wgpu::Texture,
    readback: wgpu::Buffer,
    layout: Rgba8ReadbackLayout,
    overlay: noon_render_wgpu::OverlayGpuState,
    pub last_scene_upload_bytes: usize,
    pub last_overlay_upload_bytes: usize,
    pub last_scene_repacked: usize,
}
impl Raster {
    pub async fn new() -> Self {
        Self::with_dimensions(SIZE, SIZE).await
    }

    pub async fn with_dimensions(width: u32, height: u32) -> Self {
        let layout = Rgba8ReadbackLayout::new(
            width,
            height,
            wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
            64 * 1024 * 1024,
        )
        .unwrap();
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
        eprintln!("native raster adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, width, height);
        // Preserve the oracle's 8-unit world height, expanding horizontal extent
        // at non-square resolutions so text and geometry share uniform density.
        let world_size = Vec2::new(WORLD_SIZE * width as f32 / height as f32, WORLD_SIZE);
        renderer.set_camera(&queue, Camera2D::new(Vec2::ZERO, world_size).unwrap());
        assert_eq!(renderer.camera().world_size, world_size);
        let text = renderer.create_retained_text_state(&device, &queue);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("native raster qualification target"),
            size: wgpu::Extent3d {
                width,
                height,
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
            label: Some("native raster qualification readback"),
            size: layout.buffer_len() as u64,
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
            layout,
            overlay: noon_render_wgpu::OverlayGpuState::default(),
            last_scene_upload_bytes: 0,
            last_overlay_upload_bytes: 0,
            last_scene_repacked: 0,
        }
    }
    pub fn capture(&mut self, publication: &RendererPublication<'_>) -> Vec<u8> {
        self.capture_frame(publication, false, None)
    }

    // Shared support is compiled independently by non-interactive integration tests.
    #[allow(dead_code)]
    pub fn capture_interactive(
        &mut self,
        publication: &RendererPublication<'_>,
        overlay: Option<noon_render_wgpu::AnalyticOverlay>,
    ) -> Vec<u8> {
        self.capture_frame(publication, true, overlay)
    }

    fn capture_frame(
        &mut self,
        publication: &RendererPublication<'_>,
        interactive: bool,
        overlay: Option<noon_render_wgpu::AnalyticOverlay>,
    ) -> Vec<u8> {
        self.last_overlay_upload_bytes = if interactive {
            self.overlay
                .update(&self.device, &self.queue, overlay)
                .bytes_uploaded
        } else {
            0
        };
        let prepared = self
            .preparer
            .prepare_publication(
                &self.device,
                publication,
                TextDeviceMetrics::uniform(self.layout.height() as f32 / WORLD_SIZE).unwrap(),
            )
            .unwrap();
        self.last_scene_repacked = prepared.geometry_stats().instances_repacked;
        self.last_scene_upload_bytes = self
            .renderer
            .upload_retained(&self.device, &self.queue, &prepared, &mut self.text)
            .bytes_uploaded();
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if interactive {
            self.renderer
                .encode_retained_with_transient_presentations_and_overlay(
                    &mut encoder,
                    &self.target.create_view(&Default::default()),
                    noon_render_wgpu::InteractiveRetainedFrame {
                        prepared: &prepared,
                        text: &self.text,
                        transient: None,
                        overlay: &self.overlay,
                    },
                    wgpu::Color::BLACK,
                    None,
                )
                .unwrap();
        } else {
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
        }
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.layout.padded_bytes_per_row()),
                    rows_per_image: Some(self.layout.height()),
                },
            },
            wgpu::Extent3d {
                width: self.layout.width(),
                height: self.layout.height(),
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
        let pixels = {
            let mapped = self.readback.slice(..).get_mapped_range().unwrap();
            self.layout
                .copy_rgba8(&mapped, PixelChannelOrder::Rgba, PixelRowOrder::TopToBottom)
                .unwrap()
        };
        self.readback.unmap();
        pixels
    }
}
