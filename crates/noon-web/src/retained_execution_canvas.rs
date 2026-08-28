#[cfg(target_arch = "wasm32")]
mod wasm {
    use noon_core::Vec2;
    use noon_render_wgpu::{Camera2D, GpuRenderer, RetainedFramePreparer, RetainedTextGpuState};
    use noon_runtime::FrameChanges;
    use noon_text_render_wgpu::TextDeviceMetrics;
    use wasm_bindgen::prelude::*;
    use web_sys::OffscreenCanvas;

    use crate::{
        gpu_diagnostics::{install_wgpu_error_handler, GpuDiagnosticMailbox},
        InstalledRetainedExecutionMirror, RetainedTransportApplyOutcome,
    };

    const CLEAR_COLOR: wgpu::Color = wgpu::Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    #[derive(Debug)]
    struct WebDisplaySource;

    impl wgpu::rwh::HasDisplayHandle for WebDisplaySource {
        fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
            Ok(wgpu::rwh::DisplayHandle::web())
        }
    }

    /// Render-worker endpoint for `noon.execution.retained`.
    ///
    /// The immutable resource bundle is installed exactly once at construction.
    /// Subsequent calls consume only retained execution snapshots/deltas whose
    /// engine-side text handles are resolved by `InstalledRetainedExecutionMirror`
    /// into renderer-local arenas before the existing mixed retained GPU path runs.
    #[wasm_bindgen(js_name = RetainedExecutionCanvasRenderer)]
    pub struct WasmRetainedExecutionCanvasRenderer {
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        backend: wgpu::Backend,
        canvas: OffscreenCanvas,
        config: wgpu::SurfaceConfiguration,
        drawable: bool,
        mirror: InstalledRetainedExecutionMirror,
        pending_frame: bool,
        pending_changes: FrameChanges,
        preparer: RetainedFramePreparer,
        renderer: GpuRenderer,
        text_gpu: RetainedTextGpuState,
        camera_center: Vec2,
        camera_height: f32,
        last_draw_calls: usize,
        last_instances_drawn: usize,
        last_bytes_uploaded: usize,
        last_geometry_cache_misses: usize,
        last_outline_cache_misses: u64,
        gpu_generation: u32,
        gpu_diagnostics: GpuDiagnosticMailbox,
    }

    #[wasm_bindgen(js_class = RetainedExecutionCanvasRenderer)]
    impl WasmRetainedExecutionCanvasRenderer {
        #[wasm_bindgen(js_name = create)]
        pub async fn create(
            canvas: OffscreenCanvas,
            resource_bundle_bytes: Vec<u8>,
        ) -> Result<WasmRetainedExecutionCanvasRenderer, JsValue> {
            let mirror =
                InstalledRetainedExecutionMirror::from_bundle_bytes(&resource_bundle_bytes)
                    .map_err(js_error)?;
            let camera = mirror.camera();

            let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            instance_descriptor.backends = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
            instance_descriptor.display = Some(Box::new(WebDisplaySource));
            let instance =
                wgpu::util::new_instance_with_webgpu_detection(instance_descriptor).await;
            let surface = create_surface(&instance, &canvas)?;
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: Some(&surface),
                })
                .await
                .map_err(js_error)?;
            let backend = adapter.get_info().backend;
            let required_limits = if backend == wgpu::Backend::Gl {
                wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
            } else {
                wgpu::Limits::default()
            };
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("Noon retained execution render worker GPU device"),
                    required_features: wgpu::Features::empty(),
                    required_limits,
                    ..Default::default()
                })
                .await
                .map_err(js_error)?;
            let gpu_generation = 1;
            let gpu_diagnostics = GpuDiagnosticMailbox::default();
            install_wgpu_error_handler(
                &device,
                gpu_generation,
                backend,
                gpu_diagnostics.clone(),
            );

            let width = canvas.width().max(1);
            let height = canvas.height().max(1);
            let config = surface
                .get_default_config(&adapter, width, height)
                .ok_or_else(|| js_message("GPU adapter cannot present retained execution"))?;
            surface.configure(&device, &config);

            let mut renderer = GpuRenderer::new(&device, config.format);
            renderer.set_viewport(&device, &queue, width, height);
            let text_gpu = renderer.create_retained_text_state(&device, &queue);

            let mut result = Self {
                instance,
                surface,
                device,
                queue,
                backend,
                canvas,
                config,
                drawable: true,
                mirror,
                pending_frame: false,
                pending_changes: FrameChanges::default(),
                preparer: RetainedFramePreparer::new(),
                renderer,
                text_gpu,
                camera_center: camera.center,
                camera_height: camera.height,
                last_draw_calls: 0,
                last_instances_drawn: 0,
                last_bytes_uploaded: 0,
                last_geometry_cache_misses: 0,
                last_outline_cache_misses: 0,
                gpu_generation,
                gpu_diagnostics,
            };
            result.update_camera()?;
            Ok(result)
        }

        #[wasm_bindgen(js_name = applyDeltaJson)]
        pub fn apply_delta_json(&mut self, json: &str) -> Result<bool, JsValue> {
            if self.pending_frame {
                return Err(js_message(
                    "render worker must present the retained execution delta before accepting another",
                ));
            }
            let (outcome, changes) = self.mirror.apply_json(json).map_err(js_error)?;
            match outcome {
                RetainedTransportApplyOutcome::Applied => {
                    let camera = self.mirror.camera();
                    if camera.center != self.camera_center || camera.height != self.camera_height {
                        self.camera_center = camera.center;
                        self.camera_height = camera.height;
                        self.update_camera()?;
                    }
                    self.pending_changes = changes;
                    self.pending_frame = true;
                    Ok(true)
                }
                RetainedTransportApplyOutcome::DroppedStale => Ok(false),
            }
        }

        pub fn render(&mut self) -> Result<bool, JsValue> {
            if !self.drawable || !self.pending_frame {
                return Ok(false);
            }

            let (surface_texture, reconfigure_after_present) =
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
                    wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
                    wgpu::CurrentSurfaceTexture::Timeout
                    | wgpu::CurrentSurfaceTexture::Occluded => return Ok(false),
                    wgpu::CurrentSurfaceTexture::Outdated => {
                        self.surface.configure(&self.device, &self.config);
                        return Ok(false);
                    }
                    wgpu::CurrentSurfaceTexture::Lost => {
                        self.surface = create_surface(&self.instance, &self.canvas)?;
                        self.surface.configure(&self.device, &self.config);
                        return Ok(false);
                    }
                    // The device error callback owns validation diagnostics. Keep
                    // the retained frame pending so a recoverable validation can
                    // be reported once and presentation retried unchanged.
                    wgpu::CurrentSurfaceTexture::Validation => return Ok(false),
                };

            let frame = self
                .mirror
                .frame()
                .ok_or_else(|| js_message("retained execution renderer has no frame snapshot"))?;
            let resources = self.mirror.resources();
            let camera = self.renderer.camera();
            let metrics = TextDeviceMetrics::new(Vec2::new(
                self.config.width as f32 / camera.world_size.x,
                self.config.height as f32 / camera.world_size.y,
            ))
            .map_err(js_error)?;
            let prepared = self
                .preparer
                .prepare_with_changes(
                    &self.device,
                    &self.queue,
                    frame,
                    &self.pending_changes,
                    resources.texts(),
                    resources.fonts(),
                    resources.geometries(),
                    metrics,
                )
                .map_err(js_error)?;
            self.last_geometry_cache_misses = prepared.geometry_stats().geometry_cache_misses;
            self.last_outline_cache_misses = prepared.stats.outline_cache_misses;
            let upload = self.renderer.upload_retained(
                &self.device,
                &self.queue,
                &prepared,
                &mut self.text_gpu,
            );
            self.last_bytes_uploaded = upload
                .geometry
                .bytes_uploaded
                .saturating_add(upload.text.bytes_uploaded);

            let view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Noon retained execution render worker frame"),
                });
            let draw = self
                .renderer
                .encode_retained(&mut encoder, &view, &prepared, &self.text_gpu, CLEAR_COLOR)
                .map_err(js_error)?;
            self.queue.submit(Some(encoder.finish()));
            surface_texture.present();
            self.last_draw_calls = draw
                .geometry
                .draw_calls
                .saturating_add(draw.text.draw_calls);
            self.last_instances_drawn = draw
                .geometry
                .instances_drawn
                .saturating_add(draw.text.instances_drawn);
            self.pending_frame = false;
            self.pending_changes = FrameChanges::default();
            if reconfigure_after_present {
                self.surface.configure(&self.device, &self.config);
            }
            Ok(true)
        }

        pub fn resize(&mut self, width: u32, height: u32) -> Result<(), JsValue> {
            self.canvas.set_width(width);
            self.canvas.set_height(height);
            self.drawable = width > 0 && height > 0;
            if !self.drawable {
                return Ok(());
            }
            if self.config.width != width || self.config.height != height {
                self.config.width = width;
                self.config.height = height;
                self.surface.configure(&self.device, &self.config);
                self.update_camera()?;
                if self.mirror.frame().is_some() {
                    self.pending_frame = true;
                }
            }
            Ok(())
        }

        #[wasm_bindgen(js_name = rendererBackend)]
        pub fn renderer_backend(&self) -> String {
            match self.backend {
                wgpu::Backend::BrowserWebGpu => "WebGPU".to_owned(),
                wgpu::Backend::Gl => "WebGL2".to_owned(),
                other => format!("{other:?}"),
            }
        }

        #[wasm_bindgen(js_name = gpuGeneration)]
        pub fn gpu_generation(&self) -> u32 {
            self.gpu_generation
        }

        #[wasm_bindgen(js_name = takeGpuDiagnosticJson)]
        pub fn take_gpu_diagnostic_json(&self) -> Result<Option<String>, JsValue> {
            self.gpu_diagnostics
                .take_for_generation(self.gpu_generation)
                .map(|diagnostic| serde_json::to_string(&diagnostic).map_err(js_error))
                .transpose()
        }

        pub fn time(&self) -> f64 {
            self.mirror.frame().map_or(0.0, |frame| frame.time)
        }

        #[wasm_bindgen(js_name = objectCount)]
        pub fn object_count(&self) -> usize {
            self.mirror.frame().map_or(0, |frame| {
                frame.presences.iter().filter(|&&present| present).count()
            })
        }

        #[wasm_bindgen(js_name = lastDrawCalls)]
        pub fn last_draw_calls(&self) -> usize {
            self.last_draw_calls
        }

        #[wasm_bindgen(js_name = lastInstancesDrawn)]
        pub fn last_instances_drawn(&self) -> usize {
            self.last_instances_drawn
        }

        #[wasm_bindgen(js_name = lastBytesUploaded)]
        pub fn last_bytes_uploaded(&self) -> usize {
            self.last_bytes_uploaded
        }

        #[wasm_bindgen(js_name = lastGeometryCacheMisses)]
        pub fn last_geometry_cache_misses(&self) -> usize {
            self.last_geometry_cache_misses
        }

        #[wasm_bindgen(js_name = lastOutlineCacheMisses)]
        pub fn last_outline_cache_misses(&self) -> u64 {
            self.last_outline_cache_misses
        }
    }

    impl WasmRetainedExecutionCanvasRenderer {
        fn update_camera(&mut self) -> Result<(), JsValue> {
            if !self.drawable {
                return Ok(());
            }
            let aspect = self.config.width as f32 / self.config.height as f32;
            let camera = Camera2D::new(
                self.camera_center,
                Vec2::new(self.camera_height * aspect, self.camera_height),
            )
            .map_err(js_error)?;
            self.renderer.set_viewport(
                &self.device,
                &self.queue,
                self.config.width,
                self.config.height,
            );
            self.renderer.set_camera(&self.queue, camera);
            Ok(())
        }
    }

    fn create_surface(
        instance: &wgpu::Instance,
        canvas: &OffscreenCanvas,
    ) -> Result<wgpu::Surface<'static>, JsValue> {
        instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
            .map_err(js_error)
    }

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    fn js_message(message: &str) -> JsValue {
        JsValue::from_str(message)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
