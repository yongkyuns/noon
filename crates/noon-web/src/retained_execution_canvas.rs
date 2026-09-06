#[cfg(target_arch = "wasm32")]
mod wasm {
    use noon_core::Vec2;
    use noon_render_wgpu::{
        Camera2D, GpuRenderer, PathMeshPreload, RetainedFramePreparer, RetainedTextGpuState,
        UploadWrite,
    };
    use noon_runtime::FrameChanges;
    use noon_text_render_wgpu::TextDeviceMetrics;
    use wasm_bindgen::prelude::*;
    use web_sys::OffscreenCanvas;

    use crate::{
        finish_renderer_observation,
        gpu_diagnostics::{install_wgpu_error_handler, GpuDiagnosticMailbox},
        resolve_renderer_observation_target, InstalledRetainedExecutionMirror,
        RendererObservationOutcome, RendererObservationRequest, RetainedTransportApplyOutcome,
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
        preloaded_geometry_count: usize,
        preload_bytes_uploaded: usize,
        gpu_generation: u32,
        gpu_diagnostics: GpuDiagnosticMailbox,
        pending_renderer_observation: Option<RendererObservationRequest>,
        last_renderer_observation: Option<RendererObservationOutcome>,
        presentation_sequence: u64,
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
            let mut preparer = RetainedFramePreparer::new();
            preparer.set_scene_path_mesh_cache_budget(
                mirror
                    .resources()
                    .render_geometries()
                    .len()
                    .max(mirror.resources().render_geometry_preparation_count()),
                mirror.resources().geometries().len(),
            );

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
                    apply_limit_buckets: false,
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
            install_wgpu_error_handler(&device, gpu_generation, backend, gpu_diagnostics.clone());

            let width = canvas.width().max(1);
            let height = canvas.height().max(1);
            let config = surface
                .get_default_config(&adapter, width, height)
                .ok_or_else(|| js_message("GPU adapter cannot present retained execution"))?;
            surface.configure(&device, &config);

            let mut renderer = GpuRenderer::new(&device, config.format);
            renderer.set_viewport(&device, &queue, width, height);
            let text_gpu = renderer.create_retained_text_state(&device, &queue);

            // Prepare derived resident GPU geometry before the worker publishes
            // readiness. This does not evaluate the timeline or acquire a frame.
            let resources = mirror.resources().render_geometries();
            let requests = mirror
                .resources()
                .render_geometry_preparations()
                .iter()
                .map(|preparation| PathMeshPreload {
                    geometry: resources[preparation.resource as usize].as_ref(),
                    style: preparation.style,
                    transform: preparation.transform,
                })
                .collect::<Vec<_>>();
            let preload = preparer
                .preload_path_meshes(&device, &queue, &mut renderer, &requests)
                .map_err(js_error)?;
            let preloaded_geometry_count = preload.geometry.geometry_cache_misses;
            let preload_bytes_uploaded = preload.upload.bytes_uploaded;
            // Writes and subsequent initial-frame rendering use the same queue,
            // so no playback draw can overtake its resident resource upload.
            queue.submit([]);

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
                preparer,
                renderer,
                text_gpu,
                camera_center: camera.center,
                camera_height: camera.height,
                last_draw_calls: 0,
                last_instances_drawn: 0,
                last_bytes_uploaded: 0,
                last_geometry_cache_misses: 0,
                last_outline_cache_misses: 0,
                preloaded_geometry_count,
                preload_bytes_uploaded,
                gpu_generation,
                gpu_diagnostics,
                pending_renderer_observation: None,
                last_renderer_observation: None,
                presentation_sequence: 0,
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

        /// Arm one bounded callback-publication observation. The request is
        /// resolved only against the exact retained delta session/sequence.
        #[wasm_bindgen(js_name = setRendererObservationRequestJson)]
        pub fn set_renderer_observation_request_json(&mut self, json: &str) -> Result<(), JsValue> {
            if self.pending_renderer_observation.is_some()
                || self.last_renderer_observation.is_some()
            {
                return Err(js_message(
                    "take the current renderer observation before requesting another",
                ));
            }
            let request = serde_json::from_str(json).map_err(js_error)?;
            self.pending_renderer_observation = Some(request);
            self.last_renderer_observation = None;
            Ok(())
        }

        #[wasm_bindgen(js_name = takeRendererObservationJson)]
        pub fn take_renderer_observation_json(&mut self) -> Result<Option<String>, JsValue> {
            self.last_renderer_observation
                .take()
                .map(|observation| serde_json::to_string(&observation).map_err(js_error))
                .transpose()
        }

        pub fn render(&mut self) -> Result<bool, JsValue> {
            if !self.drawable || !self.pending_frame {
                return Ok(false);
            }

            let (surface_texture, reconfigure_after_present, surface_status) =
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false, "success"),
                    wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                        (texture, true, "suboptimal")
                    }
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
                    wgpu::CurrentSurfaceTexture::Validation => return Ok(false),
                };

            let renderer_backend = renderer_backend_label(self.backend);
            let observation_target =
                self.pending_renderer_observation
                    .as_ref()
                    .cloned()
                    .map(|request| {
                        resolve_renderer_observation_target(request, self.mirror.transport_mirror())
                    });
            let resolved_observation_target = observation_target
                .as_ref()
                .and_then(|target| target.as_ref().ok());
            let resources = self.mirror.resources();
            let camera = self.renderer.camera();
            let metrics = TextDeviceMetrics::new(Vec2::new(
                self.config.width as f32 / camera.world_size.x,
                self.config.height as f32 / camera.world_size.y,
            ))
            .map_err(js_error)?;
            let plans = self.mirror.family_plans();
            let prepared = if plans.is_empty() {
                let frame = self.mirror.frame().ok_or_else(|| {
                    js_message("retained execution renderer has no frame snapshot")
                })?;
                self.preparer
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
                    .map_err(js_error)?
            } else {
                let family_frame = self
                    .mirror
                    .planned_family_frame()
                    .map_err(js_error)?
                    .ok_or_else(|| {
                        js_message(
                            "retained family execution has plans without an evaluated family frame",
                        )
                    })?;
                self.preparer
                    .prepare_family_plan_set_with_changes(
                        &self.device,
                        &self.queue,
                        &family_frame,
                        plans,
                        &self.pending_changes,
                        resources.texts(),
                        resources.fonts(),
                        resources.geometries(),
                        metrics,
                    )
                    .map_err(js_error)?
            };
            self.last_geometry_cache_misses = prepared.geometry_stats().geometry_cache_misses;
            self.last_outline_cache_misses = prepared.stats.outline_cache_misses;
            let prepared_observation = resolved_observation_target.as_ref().map(|target| {
                prepared.observe_object(target.mirrored.frame_index, target.mirrored.object)
            });
            let observed_prepared = prepared_observation
                .as_ref()
                .and_then(|observation| observation.as_ref().ok());
            let mut upload_writes = observed_prepared.map(|_| Vec::<UploadWrite>::new());
            let upload = if let Some(observed) = observed_prepared {
                self.renderer.upload_retained_with_trace(
                    &self.device,
                    &self.queue,
                    &prepared,
                    &mut self.text_gpu,
                    observed,
                    upload_writes
                        .as_mut()
                        .expect("prepared observation owns its upload trace"),
                )
            } else {
                self.renderer.upload_retained(
                    &self.device,
                    &self.queue,
                    &prepared,
                    &mut self.text_gpu,
                )
            };
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
            self.queue.present(surface_texture);
            self.presentation_sequence = self.presentation_sequence.saturating_add(1);
            self.last_draw_calls = draw
                .geometry
                .draw_calls
                .saturating_add(draw.text.draw_calls);
            self.last_instances_drawn = draw
                .geometry
                .instances_drawn
                .saturating_add(draw.text.instances_drawn);
            if let Some(observation_target) = observation_target {
                self.pending_renderer_observation = None;
                self.last_renderer_observation = Some(match observation_target {
                    Ok(target) => finish_renderer_observation(
                        target,
                        prepared_observation.expect("resolved target was prepared"),
                        upload_writes.as_deref().unwrap_or_default(),
                        upload,
                        draw,
                        self.presentation_sequence,
                        renderer_backend,
                        surface_status,
                    ),
                    Err(outcome) => outcome,
                });
            }
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

        #[wasm_bindgen(js_name = preloadedGeometryCount)]
        pub fn preloaded_geometry_count(&self) -> usize {
            self.preloaded_geometry_count
        }

        #[wasm_bindgen(js_name = preloadBytesUploaded)]
        pub fn preload_bytes_uploaded(&self) -> usize {
            self.preload_bytes_uploaded
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

    const fn renderer_backend_label(backend: wgpu::Backend) -> &'static str {
        match backend {
            wgpu::Backend::BrowserWebGpu => "WebGPU",
            wgpu::Backend::Gl => "WebGL2",
            _ => "Other",
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
