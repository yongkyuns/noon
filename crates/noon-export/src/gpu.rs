//! Native device/target/submission/readback ownership, never scene authority.

use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use noon::integration::{
    ExportFrameSummary, PixelChannelOrder, PixelRowOrder, RendererPublication, Rgba8ReadbackLayout,
};
use noon::ExecutionSession;
use noon_core::{Camera2DState, Inset2DViewState, Rect, Vec2};
use noon_render_wgpu::{
    text::TextDeviceMetrics, Camera2D, GpuRenderer, HdrBloomCapture, HdrBloomFilter, HdrBloomScope,
    InteractiveRetainedFrame, OverlayGpuState, RetainedFramePreparer, RetainedTextGpuState,
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
        let camera = session
            .camera_2d()
            .map_err(|e| CaptureError::View(e.to_string()))?
            .unwrap_or_default();
        let insets = session
            .inset_2d_views()
            .map_err(|e| CaptureError::View(e.to_string()))?;
        let aspect = options.width as f32 / options.height as f32;
        let primary = camera
            .viewport_bounds(aspect)
            .ok_or_else(|| CaptureError::View("invalid camera viewport".to_owned()))?;
        let mut bounds = Vec::with_capacity(1 + insets.len());
        bounds.push(primary);
        for inset in &insets {
            bounds.push(
                inset
                    .camera_bounds()
                    .ok_or_else(|| CaptureError::View("invalid inset camera".to_owned()))?,
            );
        }
        Ok(Self {
            camera,
            insets,
            bounds,
        })
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
        let first = self
            .0
            .lock()
            .map_err(|_| CaptureError::gpu("GPU fault latch poisoned"))?;
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
    prepared_effect_modules: Option<noon_render_wgpu::PreparedEffectPrograms>,
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
    working_profile: noon_core::ViewColorProfile,
    linear_supported: bool,
    linear_target: Option<wgpu::Texture>,
    view_bloom: HdrBloomFilter,
    view_bloom_scope: HdrBloomScope,
}

impl NativeCapture {
    pub(super) fn prepare_effect_modules(
        &mut self,
        programs: &[noon_core::WgslEffectProgram],
    ) -> Result<(), CaptureError> {
        let request = self
            .renderer
            .effect_module_preparation(&self.device, programs);
        let prepared = pollster::block_on(request.prepare()).map_err(CaptureError::gpu)?;
        self.renderer
            .cache_effect_programs(&self.device, &prepared)
            .map_err(CaptureError::gpu)?;
        self.prepared_effect_modules = Some(prepared);
        Ok(())
    }
    pub async fn new(
        options: &CaptureOptions,
        layout: Rgba8ReadbackLayout,
    ) -> Result<Self, CaptureError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: options.backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: options.force_fallback_adapter,
                compatible_surface: None,
                apply_limit_buckets: false,
            })
            .await
            .map_err(CaptureError::gpu)?;
        let adapter_info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Noon headless capture device"),
                ..Default::default()
            })
            .await
            .map_err(CaptureError::gpu)?;
        let limits = device.limits();
        if options.width > limits.max_texture_dimension_2d
            || options.height > limits.max_texture_dimension_2d
            || layout.buffer_len() as u64 > limits.max_buffer_size
        {
            return Err(CaptureError::Configuration(
                "capture exceeds the device's enabled limits",
            ));
        }
        let fault = GpuFault::default();
        let errors = fault.clone();
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            errors.record(format!("{error}"))
        }));
        let lost = fault.clone();
        device.set_device_lost_callback(move |reason, message| {
            lost.record(format!("device lost ({reason:?}): {message}"));
        });

        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(layout.packed_len())
            .map_err(CaptureError::gpu)?;
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Noon capture staging buffer"),
            size: layout.buffer_len() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let features = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
        let linear_supported = features.allowed_usages.contains(
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        ) && features
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
            && features.flags.sample_count_supported(4)
            && limits.max_color_attachment_bytes_per_sample >= 8;
        let view_bloom = HdrBloomFilter::new(&adapter, &device);
        fault.check()?;
        Ok(Self {
            _instance: instance,
            device,
            queue,
            renderer,
            prepared_effect_modules: None,
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
            working_profile: noon_core::ViewColorProfile::Compatibility,
            linear_supported,
            linear_target: None,
            view_bloom,
            view_bloom_scope: HdrBloomScope::default(),
        })
    }

    fn prepare_working_profile(
        &mut self,
        publication: &RendererPublication<'_>,
        view: &CaptureView,
    ) -> Result<(), CaptureError> {
        let profile = publication.color_profile();
        if profile == noon_core::ViewColorProfile::LinearHdr {
            if !self.linear_supported
                || !view.insets.is_empty()
                || publication.primary_view().is_none()
            {
                return Err(CaptureError::Configuration(
                    "HDR capture requires a supported primary 2D float view without insets",
                ));
            }
            let unsupported = |row: &noon::integration::FrameObjectState| row.spatial.is_some();
            let has_spatial = if self.working_profile != profile || publication.changes().is_all() {
                publication.frame().objects.iter().any(unsupported)
            } else {
                publication
                    .changes()
                    .object_indices()
                    .iter()
                    .any(|&i| unsupported(&publication.frame().objects[i]))
            };
            if has_spatial {
                return Err(CaptureError::Configuration(
                    "HDR spatial capture is not qualified",
                ));
            }
            let bytes =
                HdrBloomFilter::required_scratch_bytes([self.layout.width(), self.layout.height()])
                    .ok_or(CaptureError::Configuration("HDR output extent overflow"))?;
            if bytes > noon_render_wgpu::DEFAULT_EFFECT_TEXTURE_BUDGET {
                return Err(CaptureError::Configuration(
                    "HDR view exceeds the retained output budget",
                ));
            }
        }
        if profile == self.working_profile {
            return Ok(());
        }
        let format = if profile == noon_core::ViewColorProfile::LinearHdr {
            wgpu::TextureFormat::Rgba16Float
        } else {
            FORMAT
        };
        let mut renderer = GpuRenderer::new(&self.device, &self.queue, format);
        renderer.set_viewport(
            &self.device,
            &self.queue,
            self.layout.width(),
            self.layout.height(),
        );
        let text = renderer.create_retained_text_state(&self.device, &self.queue);
        let target = (profile == noon_core::ViewColorProfile::LinearHdr).then(|| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Noon linear authored capture"),
                size: wgpu::Extent3d {
                    width: self.layout.width(),
                    height: self.layout.height(),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        });
        self.fault.check()?;
        self.renderer = renderer;
        self.text = text;
        self.preparer = RetainedFramePreparer::new();
        self.linear_target = target;
        self.view_bloom_scope.clear();
        self.working_profile = profile;
        Ok(())
    }

    pub fn pixel_format(&self) -> crate::CapturePixelFormat {
        if self.working_profile == noon_core::ViewColorProfile::LinearHdr {
            crate::CapturePixelFormat::LinearHdrSdrRgba8Opaque
        } else {
            crate::CapturePixelFormat::RendererRgba8UnormOpaque
        }
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
        self.prepare_working_profile(publication, view)?;
        pollster::block_on(
            self.renderer
                .prepare_effect_programs(&self.device, publication),
        )
        .map_err(CaptureError::gpu)?;
        self.prepared_effect_modules = None;
        let clear = noon_render_wgpu::authored_clear_color(self.clear, self.working_profile);
        let camera = view.camera;
        let width = self.layout.width();
        let height = self.layout.height();
        let render_camera = Camera2D::new(
            camera.center,
            Vec2::new(camera.height * width as f32 / height as f32, camera.height),
        )
        .map_err(CaptureError::gpu)?;
        self.renderer.set_camera(&self.queue, render_camera);
        let density = height as f32 / camera.height;
        let metrics = TextDeviceMetrics::uniform(density)
            .and_then(|metrics| {
                metrics.with_world_origin_pixels(Vec2::new(
                    width as f32 * 0.5 - camera.center.x * density,
                    height as f32 * 0.5 + camera.center.y * density,
                ))
            })
            .map_err(CaptureError::gpu)?;
        self.renderer
            .prepare_spatial(&self.device, &self.queue, publication)
            .map_err(CaptureError::gpu)?;
        self.preparer
            .set_inset_views_active(!view.insets.is_empty());
        self.renderer
            .set_inset_2d_views(&self.device, &self.queue, &mut self.text, &view.insets)
            .map_err(CaptureError::gpu)?;
        let transient = self
            .preparer
            .prepare_transient_presentations_visible(publication, visible)
            .map_err(CaptureError::gpu)?;
        let glow_visible = self
            .renderer
            .effect_source_visibility(publication, visible)
            .map_err(CaptureError::gpu)?;
        let prepared = self
            .preparer
            .prepare_planned_publication_visible(&self.device, publication, glow_visible, metrics)
            .map_err(CaptureError::gpu)?;
        let geometry_instances_repacked = prepared.geometry_stats().instances_repacked;
        let mut retained_upload_bytes = self
            .renderer
            .upload_retained(&self.device, &self.queue, &prepared, &mut self.text)
            .bytes_uploaded();
        if !transient.slots.is_empty() {
            self.renderer
                .upload_derived(&self.device, &self.queue, &transient);
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Noon capture frame"),
            });
        let glow_work = self
            .renderer
            .prepare_retained_effects(
                &self.device,
                &self.queue,
                &mut encoder,
                &prepared,
                &self.text,
                publication,
                clear,
                noon_render_wgpu::DEFAULT_EFFECT_TEXTURE_BUDGET,
            )
            .map_err(CaptureError::gpu)?;
        retained_upload_bytes += glow_work.bytes_uploaded;
        let linear_view = self
            .linear_target
            .as_ref()
            .map(|target| target.create_view(&Default::default()));
        self.renderer
            .encode_retained_with_transient_presentations_and_overlay(
                &mut encoder,
                linear_view.as_ref().unwrap_or(&self.view),
                InteractiveRetainedFrame {
                    prepared: &prepared,
                    text: &self.text,
                    transient: Some(&transient),
                    overlay: &self.overlay,
                },
                clear,
                None,
            )
            .map_err(|error| {
                self.renderer.invalidate_effect_captures();
                self.view_bloom_scope.invalidate();
                CaptureError::gpu(error)
            })?;
        if let Some(source) = &self.linear_target {
            let work = self
                .view_bloom
                .prepare_view(
                    &self.device,
                    &self.queue,
                    &mut self.view_bloom_scope,
                    publication,
                    HdrBloomCapture {
                        source,
                        revision: self
                            .renders
                            .checked_add(1)
                            .ok_or(CaptureError::Configuration("capture revision exhausted"))?,
                    },
                    f64::from(camera.height),
                    noon_render_wgpu::DEFAULT_EFFECT_TEXTURE_BUDGET,
                )
                .map_err(CaptureError::gpu)?;
            retained_upload_bytes += work.bytes_uploaded;
            self.view_bloom
                .encode(&mut encoder, &mut self.view_bloom_scope);
            encoder.copy_texture_to_texture(
                self.view_bloom_scope
                    .display_output()
                    .expect("prepared view output")
                    .as_image_copy(),
                self.target.as_image_copy(),
                self.target.size(),
            );
        }
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
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        if let Err(error) = self.fault.check() {
            self.renderer.invalidate_effect_captures();
            self.view_bloom_scope.invalidate();
            return Err(error);
        }
        let submission = self.queue.submit([encoder.finish()]);
        if read_pixels {
            self.read_pixels(submission)?;
            self.readbacks = self
                .readbacks
                .checked_add(1)
                .ok_or(CaptureError::Configuration("readback counter exhausted"))?;
        } else {
            self.wait(submission)?;
        }
        self.renders = self
            .renders
            .checked_add(1)
            .ok_or(CaptureError::Configuration("render counter exhausted"))?;
        Ok(CaptureWork {
            geometry_instances_repacked,
            retained_upload_bytes,
        })
    }

    fn wait(&self, submission: wgpu::SubmissionIndex) -> Result<(), CaptureError> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(self.wait_timeout),
            })
            .map_err(CaptureError::gpu)?;
        self.fault.check()
    }

    fn read_pixels(&mut self, submission: wgpu::SubmissionIndex) -> Result<(), CaptureError> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                // A failed/cancelled capture may already have dropped the receiver.
                let _ = sender.send(result);
            });
        let result = (|| {
            self.wait(submission)?;
            // The native Wait invokes mapping callbacks before returning. Never
            // block a second, unbounded recv after the bounded GPU wait.
            receiver
                .try_recv()
                .map_err(CaptureError::gpu)?
                .map_err(CaptureError::gpu)?;
            let mapped = self
                .readback
                .slice(..)
                .get_mapped_range()
                .map_err(CaptureError::gpu)?;
            self.layout
                .copy_rgba8_into(
                    &mapped,
                    PixelChannelOrder::Rgba,
                    PixelRowOrder::TopToBottom,
                    &mut self.pixels,
                )
                .map_err(CaptureError::Pixels)
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
