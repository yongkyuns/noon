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
    pub last_scene_path_batches: usize,
}
impl Raster {
    // This support module is compiled separately by native and Vulkan test binaries.
    #[allow(dead_code)]
    pub async fn new() -> Self {
        Self::with_dimensions(SIZE, SIZE).await
    }

    #[allow(dead_code)]
    pub async fn with_dimensions(width: u32, height: u32) -> Self {
        Self::with_adapter_dimensions(width, height, wgpu::Backends::VULKAN, true).await
    }

    // Explicit real native adapter qualification, also usable on Metal hosts.
    #[allow(dead_code)]
    pub async fn with_primary_dimensions(width: u32, height: u32) -> Self {
        Self::with_adapter_dimensions(width, height, wgpu::Backends::PRIMARY, false).await
    }

    async fn with_adapter_dimensions(
        width: u32,
        height: u32,
        backends: wgpu::Backends,
        force_fallback_adapter: bool,
    ) -> Self {
        let layout = Rgba8ReadbackLayout::new(
            width,
            height,
            wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
            64 * 1024 * 1024,
        )
        .unwrap();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                force_fallback_adapter,
                ..Default::default()
            })
            .await
            .expect("native raster qualification requires the requested real adapter");
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
            last_scene_path_batches: 0,
        }
    }
    pub fn capture(&mut self, publication: &RendererPublication<'_>) -> Vec<u8> {
        self.capture_frame(publication, false, None, None, false)
    }

    #[allow(dead_code)]
    pub fn capture_effects(&mut self, publication: &RendererPublication<'_>) -> Vec<u8> {
        self.capture_frame(publication, false, None, None, true)
    }

    // Shared support is compiled independently by non-interactive integration tests.
    #[allow(dead_code)]
    pub fn capture_interactive(
        &mut self,
        publication: &RendererPublication<'_>,
        overlay: Option<noon_render_wgpu::AnalyticOverlay>,
    ) -> Vec<u8> {
        self.capture_frame(publication, true, overlay, None, false)
    }

    #[allow(dead_code)]
    pub fn capture_visible(
        &mut self,
        publication: &RendererPublication<'_>,
        visible_indices: &[usize],
    ) -> Vec<u8> {
        self.capture_frame(publication, false, None, Some(visible_indices), false)
    }

    fn capture_frame(
        &mut self,
        publication: &RendererPublication<'_>,
        interactive: bool,
        overlay: Option<noon_render_wgpu::AnalyticOverlay>,
        visible_indices: Option<&[usize]>,
        effects: bool,
    ) -> Vec<u8> {
        self.last_overlay_upload_bytes = if interactive {
            self.overlay
                .update(&self.device, &self.queue, overlay)
                .bytes_uploaded
        } else {
            0
        };
        if effects {
            pollster::block_on(
                self.renderer
                    .prepare_effect_programs(&self.device, publication),
            )
            .unwrap();
        }
        let metrics = TextDeviceMetrics::uniform(self.layout.height() as f32 / WORLD_SIZE).unwrap();
        let effect_rows = if effects {
            publication
                .painter_order()
                .iter()
                .map(|&index| index as usize)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let prepared = if effects {
            let visible = self
                .renderer
                .effect_source_visibility(publication, &effect_rows)
                .unwrap();
            self.preparer
                .prepare_planned_publication_visible(&self.device, publication, visible, metrics)
                .unwrap()
        } else {
            match visible_indices {
                Some(indices) => self.preparer.prepare_publication_visible(
                    &self.device,
                    publication,
                    indices,
                    metrics,
                ),
                None => self
                    .preparer
                    .prepare_publication(&self.device, publication, metrics),
            }
            .unwrap()
        };
        self.last_scene_repacked = prepared.geometry_stats().instances_repacked;
        self.last_scene_path_batches = prepared
            .geometry_render_chunks()
            .flat_map(|chunk| chunk.render_batches)
            .filter(|batch| {
                matches!(
                    batch.primitive,
                    noon_render_wgpu::RenderPrimitive::Path { .. }
                        | noon_render_wgpu::RenderPrimitive::MegaPath { .. }
                )
            })
            .count();
        self.last_scene_upload_bytes = self
            .renderer
            .upload_retained(&self.device, &self.queue, &prepared, &mut self.text)
            .bytes_uploaded();
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if effects {
            self.renderer
                .prepare_retained_effects(
                    &self.device,
                    &self.queue,
                    &mut encoder,
                    &prepared,
                    &self.text,
                    publication,
                    wgpu::Color::BLACK,
                    64 * 1024 * 1024,
                )
                .unwrap();
        }
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
