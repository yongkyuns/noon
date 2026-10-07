//! Native device/target/submission/readback ownership, never scene authority.

use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use noon::integration::{
    ExportFrameSummary, PixelChannelOrder, PixelRowOrder, RendererPublication, Rgba8ReadbackLayout,
};
use noon::ExecutionSession;
use noon_core::{Camera2DState, Inset2DViewState, Rect, Vec2};
use noon_render_wgpu::{
    text::TextDeviceMetrics, Camera2D, GpuRenderer, InteractiveRetainedFrame, OverlayGpuState,
    RetainedFramePreparer, RetainedTextGpuState,
};

use crate::{CaptureError, CaptureOptions, CaptureSummary, CaptureWork};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub(super) struct CaptureView {
    camera: Camera2DState,
    insets: Vec<Inset2DViewState>,
    pub bounds: Vec<Rect>,
}

impl CaptureView {
    pub fn new(session: &ExecutionSession, options: &CaptureOptions) -> Result<Self, CaptureError> {
        // Never use inspection_camera(): viewer navigation is not authored output.
        let camera = session.camera_2d()
            .map_err(|e| CaptureError::View(e.to_string()))?.unwrap_or_default();
        let insets = session.inset_2d_views().map_err(|e| CaptureError::View(e.to_string()))?;
        let aspect = options.width as f32 / options.height as f32;
        let primary = camera.viewport_bounds(aspect)
            .ok_or_else(|| CaptureError::View("invalid camera viewport".to_owned()))?;
        let mut bounds = Vec::with_capacity(1 + insets.len());
        bounds.push(primary);
        for inset in &insets {
            bounds.push(inset.camera_bounds()
                .ok_or_else(|| CaptureError::View("invalid inset camera".to_owned()))?);
        }
        Ok(Self { camera, insets, bounds })
    }
}

/// Keep only the first asynchronous GPU fault. The callback never panics or
/// retains an unbounded error list. A poisoned latch is also terminal to a run.
#[derive(Clone, Default)]
struct GpuFault(Arc<Mutex<Option<String>>>);

impl GpuFault {
    fn record(&self, message: String) {
        if let Ok(mut first) = self.0.lock() {
            if first.is_none() {
                *first = Some(message);
            }
        }
    }

    fn check(&self) -> Result<(), CaptureError> {
        let first = self.0.lock().map_err(|_| CaptureError::gpu("GPU fault latch poisoned"))?;
        match first.as_ref() {
            Some(message) => Err(CaptureError::Gpu(message.clone())),
            None => Ok(()),
        }
    }
}

pub(super) struct NativeCapture {
    _instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: GpuRenderer,
    preparer: RetainedFramePreparer,
    text: RetainedTextGpuState,
    overlay: OverlayGpuState,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    layout: Rgba8ReadbackLayout,
    pixels: Vec<u8>,
    clear: wgpu::Color,
    wait_timeout: Duration,
    fault: GpuFault,
    adapter: wgpu::AdapterInfo,
    renders: u64,
    readbacks: u64,
}

impl NativeCapture {
    pub async fn new(
        options: &CaptureOptions,
        layout: Rgba8ReadbackLayout,
    ) -> Result<Self, CaptureError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: options.backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: options.force_fallback_adapter,
            compatible_surface: None,
            apply_limit_buckets: false,
        }).await.map_err(CaptureError::gpu)?;
        let adapter_info = adapter.get_info();
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Noon headless capture device"),
            ..Default::default()
        }).await.map_err(CaptureError::gpu)?;
        let limits = device.limits();
        if options.width > limits.max_texture_dimension_2d
            || options.height > limits.max_texture_dimension_2d
            || layout.buffer_len() as u64 > limits.max_buffer_size
        {
            return Err(CaptureError::Configuration("capture exceeds the device's enabled limits"));
        }
        let fault = GpuFault::default();
        let errors = fault.clone();
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| errors.record(format!("{error}"))));
        let lost = fault.clone();
        device.set_device_lost_callback(move |reason, message| {
            lost.record(format!("device lost ({reason:?}): {message}"));
        });

        let mut pixels = Vec::new();
        pixels.try_reserve_exact(layout.packed_len()).map_err(CaptureError::gpu)?;
        pixels.resize(layout.packed_len(), 0);
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer.set_viewport(&device, &queue, options.width, options.height);
        let text = renderer.create_retained_text_state(&device, &queue);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon offscreen capture target"),
            size: wgpu::Extent3d {
                width: options.width,
                height: options.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Noon capture staging buffer"),
            size: layout.buffer_len() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        fault.check()?;
        Ok(Self {
            _instance: instance,
            device,
            queue,
            renderer,
            preparer: RetainedFramePreparer::new(),
            text,
            overlay: OverlayGpuState::default(),
            target,
            view,
            readback,
            layout,
            pixels,
            clear: wgpu::Color {
                r: options.background[0],
                g: options.background[1],
                b: options.background[2],
                a: 1.0,
            },
            wait_timeout: options.gpu_wait_timeout,
            fault,
            adapter: adapter_info,
            renders: 0,
            readbacks: 0,
        })
    }

    /// Same retained production composition as the native viewer, without its
    /// surface, display clock, selection overlay or inspection-camera state.
    /// The serial completion fence protects all resources before the next source
    /// mutation. Pipelining can replace that fence without changing semantics.
    pub fn render(
        &mut self,
        publication: &RendererPublication<'_>,
        view: &CaptureView,
        visible: &[usize],
        read_pixels: bool,
    ) -> Result<CaptureWork, CaptureError> {
        self.fault.check()?;
        let camera = view.camera;
        let width = self.layout.width();
        let height = self.layout.height();
        let render_camera = Camera2D::new(camera.center, Vec2::new(
            camera.height * width as f32 / height as f32, camera.height,
        )).map_err(CaptureError::gpu)?;
        self.renderer.set_camera(&self.queue, render_camera);
        let density = height as f32 / camera.height;
        let metrics = TextDeviceMetrics::uniform(density).and_then(|metrics| {
            metrics.with_world_origin_pixels(Vec2::new(
                width as f32 * 0.5 - camera.center.x * density,
                height as f32 * 0.5 + camera.center.y * density,
            ))
        }).map_err(CaptureError::gpu)?;
        self.renderer.prepare_spatial(&self.device, &self.queue, publication)
            .map_err(CaptureError::gpu)?;
        self.preparer.set_inset_views_active(!view.insets.is_empty());
        self.renderer.set_inset_2d_views(&self.device, &self.queue, &mut self.text, &view.insets)
            .map_err(CaptureError::gpu)?;
        let transient = self.preparer
            .prepare_transient_presentations_visible(publication, visible)
            .map_err(CaptureError::gpu)?;
        let prepared = self.preparer.prepare_planned_publication_visible(
            &self.device, publication, visible, metrics,
        ).map_err(CaptureError::gpu)?;
        let geometry_instances_repacked = prepared.geometry_stats().instances_repacked;
        let retained_upload_bytes = self.renderer
            .upload_retained(&self.device, &self.queue, &prepared, &mut self.text)
            .bytes_uploaded();
        if !transient.slots.is_empty() {
            self.renderer.upload_derived(&self.device, &self.queue, &transient);
        }
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Noon capture frame"),
        });
        self.renderer.encode_retained_with_transient_presentations_and_overlay(
            &mut encoder,
            &self.view,
            InteractiveRetainedFrame {
                prepared: &prepared,
                text: &self.text,
                transient: Some(&transient),
                overlay: &self.overlay,
            },
            self.clear,
            None,
        ).map_err(CaptureError::gpu)?;
        if read_pixels {
            encoder.copy_texture_to_buffer(
                self.target.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &self.readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(self.layout.padded_bytes_per_row()),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
        }
        self.fault.check()?;
        let submission = self.queue.submit([encoder.finish()]);
        if read_pixels {
            self.read_pixels(submission)?;
            self.readbacks = self.readbacks.checked_add(1)
                .ok_or(CaptureError::Configuration("readback counter exhausted"))?;
        } else {
            self.wait(submission)?;
        }
        self.renders = self.renders.checked_add(1)
            .ok_or(CaptureError::Configuration("render counter exhausted"))?;
        Ok(CaptureWork { geometry_instances_repacked, retained_upload_bytes })
    }

    fn wait(&self, submission: wgpu::SubmissionIndex) -> Result<(), CaptureError> {
        self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(self.wait_timeout),
        }).map_err(CaptureError::gpu)?;
        self.fault.check()
    }

    fn read_pixels(&mut self, submission: wgpu::SubmissionIndex) -> Result<(), CaptureError> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.readback.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            // A failed/cancelled capture may already have dropped the receiver.
            let _ = sender.send(result);
        });
        let result = (|| {
            self.wait(submission)?;
            // The native Wait invokes mapping callbacks before returning. Never
            // block a second, unbounded recv after the bounded GPU wait.
            receiver.try_recv().map_err(CaptureError::gpu)?.map_err(CaptureError::gpu)?;
            let mapped = self.readback.slice(..).get_mapped_range().map_err(CaptureError::gpu)?;
            self.layout.copy_rgba8_into(
                &mapped,
                PixelChannelOrder::Rgba,
                PixelRowOrder::TopToBottom,
                &mut self.pixels,
            ).map_err(CaptureError::Pixels)
        })();
        // Also cancel a pending map on error. The mapped view above is already
        // dropped on both branches, before the buffer can be reused or dropped.
        self.readback.unmap();
        result
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn summary(&self, sampling: ExportFrameSummary) -> CaptureSummary {
        CaptureSummary {
            sampling,
            adapter: self.adapter.clone(),
            rendered_publications: self.renders,
            readbacks: self.readbacks,
            staging_buffer_bytes: self.layout.buffer_len(),
            pixel_buffer_capacity: self.pixels.capacity(),
        }
    }
}

#[cfg(test)]
mod tests;
