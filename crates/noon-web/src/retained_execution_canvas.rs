#[cfg(any(target_arch = "wasm32", test))]
pub(crate) struct FamilyAdmission<T, E> {
    pub(crate) outcome: crate::RetainedTransportApplyOutcome,
    pub(crate) changes: noon_runtime::FrameChanges,
    pub(crate) resident_preparation: Option<Result<T, E>>,
}

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn apply_family_then_resident_preparation<T, E>(
    mirror: &mut crate::InstalledRetainedExecutionMirror,
    delta: crate::RetainedFamilyExecutionDeltaEnvelope,
    prepare_resident: impl FnOnce(&crate::InstalledRetainedExecutionMirror) -> Result<T, E>,
) -> Result<FamilyAdmission<T, E>, crate::InstalledExecutionError> {
    let applied = mirror.apply_family(delta)?;
    let resident_preparation = (applied.0 == crate::RetainedTransportApplyOutcome::Applied)
        .then(|| prepare_resident(mirror));
    Ok(FamilyAdmission {
        outcome: applied.0,
        changes: applied.1,
        resident_preparation,
    })
}

#[cfg(any(target_arch = "wasm32", test))]
const RENDER_SUBSTAGE_SAMPLE_CAPACITY: usize = 32;

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderSubstageSample {
    session: Option<u32>,
    sequence: Option<u64>,
    surface_acquire_cpu_wall_ms: f64,
    prepare_cpu_wall_ms: f64,
    upload_cpu_wall_ms: f64,
    encode_cpu_wall_ms: f64,
    submit_present_cpu_wall_ms: f64,
}

#[cfg(any(target_arch = "wasm32", test))]
struct RenderSubstageSamples(Option<std::collections::VecDeque<RenderSubstageSample>>);

#[cfg(any(target_arch = "wasm32", test))]
impl RenderSubstageSamples {
    fn new() -> Self {
        Self(None)
    }

    fn push(&mut self, sample: RenderSubstageSample) {
        let samples = self.0.get_or_insert_with(|| {
            std::collections::VecDeque::with_capacity(RENDER_SUBSTAGE_SAMPLE_CAPACITY)
        });
        if samples.len() == RENDER_SUBSTAGE_SAMPLE_CAPACITY {
            samples.pop_front();
        }
        samples.push_back(sample);
    }

    #[cfg(target_arch = "wasm32")]
    fn clear(&mut self) {
        if let Some(samples) = &mut self.0 {
            samples.clear();
        }
    }

    fn take_json(&mut self) -> Result<String, serde_json::Error> {
        let Some(samples) = &mut self.0 else {
            return Ok("[]".to_owned());
        };
        let json = serde_json::to_string(samples)?;
        samples.clear();
        Ok(json)
    }

    #[cfg(test)]
    fn is_allocated(&self) -> bool {
        self.0.is_some()
    }
}

#[cfg(test)]
mod render_substage_tests {
    use super::{RenderSubstageSample, RenderSubstageSamples, RENDER_SUBSTAGE_SAMPLE_CAPACITY};

    #[test]
    fn render_substage_samples_remain_bounded_and_drain_without_losing_capacity() {
        let mut samples = RenderSubstageSamples::new();
        assert!(
            !samples.is_allocated(),
            "default diagnostics do not allocate a ring"
        );
        for sequence in 0..(RENDER_SUBSTAGE_SAMPLE_CAPACITY as u64 + 1) {
            samples.push(RenderSubstageSample {
                session: Some(7),
                sequence: Some(sequence),
                surface_acquire_cpu_wall_ms: 1.0,
                prepare_cpu_wall_ms: 2.0,
                upload_cpu_wall_ms: 3.0,
                encode_cpu_wall_ms: 4.0,
                submit_present_cpu_wall_ms: 5.0,
            });
        }
        let drained: Vec<RenderSubstageSample> =
            serde_json::from_str(&samples.take_json().unwrap()).unwrap();
        assert_eq!(drained.len(), RENDER_SUBSTAGE_SAMPLE_CAPACITY);
        assert_eq!(drained.first().and_then(|sample| sample.sequence), Some(1));
        assert_eq!(drained.last().and_then(|sample| sample.sequence), Some(32));
        assert!(
            samples.is_allocated(),
            "draining preserves the opt-in ring allocation"
        );
        assert_eq!(samples.take_json().unwrap(), "[]");
        samples.push(RenderSubstageSample {
            session: Some(7),
            sequence: Some(33),
            surface_acquire_cpu_wall_ms: 0.0,
            prepare_cpu_wall_ms: 0.0,
            upload_cpu_wall_ms: 0.0,
            encode_cpu_wall_ms: 0.0,
            submit_present_cpu_wall_ms: 0.0,
        });
        assert_eq!(samples.take_json().unwrap().matches("sequence").count(), 1);
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use noon_core::Vec2;
    use noon_render_wgpu::text::TextDeviceMetrics;
    use noon_render_wgpu::{
        AnalyticOverlay, Camera2D, GpuRenderer, InteractiveRetainedFrame, OverlayGpuState,
        PathMeshPreload, RetainedFramePreparer, RetainedTextGpuState, UploadWrite,
    };
    use noon_runtime::FrameChanges;
    use wasm_bindgen::{prelude::*, JsCast};
    use web_sys::OffscreenCanvas;

    use super::{apply_family_then_resident_preparation, FamilyAdmission};

    #[derive(Default)]
    struct RenderSubstageTimings {
        surface_acquire_cpu_wall_ms: f64,
        prepare_cpu_wall_ms: f64,
        upload_cpu_wall_ms: f64,
        encode_cpu_wall_ms: f64,
        submit_present_cpu_wall_ms: f64,
    }

    fn performance_now_ms() -> f64 {
        let global = js_sys::global();
        let Ok(performance) = js_sys::Reflect::get(&global, &"performance".into()) else {
            return js_sys::Date::now();
        };
        let Ok(now) = js_sys::Reflect::get(&performance, &"now".into()) else {
            return js_sys::Date::now();
        };
        now.dyn_ref::<js_sys::Function>()
            .and_then(|now| now.call0(&performance).ok())
            .and_then(|value| value.as_f64())
            .unwrap_or_else(js_sys::Date::now)
    }

    use super::{RenderSubstageSample, RenderSubstageSamples};
    use crate::{
        finish_renderer_observation,
        gpu_diagnostics::{
            install_wgpu_error_handler, GpuCompletionSample, GpuCompletionSamples,
            GpuDiagnosticMailbox,
        },
        resolve_renderer_observation_target,
        webgl_context_lifecycle::{
            ensure_webgl_context_available, webgl_context_is_lost, WebGlContextLifecycle,
        },
        InstalledRetainedExecutionMirror, RendererObservationOutcome, RendererObservationRequest,
        RetainedFamilyExecutionDeltaEnvelope, RetainedTransportApplyOutcome,
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

    struct InitializedGpu {
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        backend: wgpu::Backend,
        config: wgpu::SurfaceConfiguration,
    }

    struct RetainedGpuState {
        preparer: RetainedFramePreparer,
        renderer: GpuRenderer,
        text_gpu: RetainedTextGpuState,
        preloaded_geometry_count: usize,
        preload_bytes_uploaded: usize,
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
        // Renderer-side capability bit, derived only from admitted worker rows.
        // Ordinary no-glow sessions retain the original zero-extra-work path.
        glow_transport_seen: bool,
        preparer: RetainedFramePreparer,
        renderer: GpuRenderer,
        text_gpu: RetainedTextGpuState,
        selection_overlay: Option<AnalyticOverlay>,
        pointer_view: Option<crate::PointerPresentationView>,
        selection_overlay_gpu: OverlayGpuState,
        camera_center: Vec2,
        camera_height: f32,
        last_draw_calls: usize,
        last_instances_drawn: usize,
        last_bytes_uploaded: usize,
        last_geometry_cache_misses: usize,
        last_outline_cache_misses: u64,
        preloaded_geometry_count: usize,
        preload_bytes_uploaded: usize,
        resident_path_maintenance_retry_deferred: bool,
        gpu_generation: u32,
        gpu_diagnostics: GpuDiagnosticMailbox,
        gpu_validation_scope: Option<wgpu::ErrorScopeGuard>,
        webgl_context_lifecycle: WebGlContextLifecycle,
        pending_renderer_observation: Option<RendererObservationRequest>,
        last_renderer_observation: Option<RendererObservationOutcome>,
        presentation_sequence: u64,
        render_substage_profiling: bool,
        render_substage_samples: RenderSubstageSamples,
        gpu_completion_samples: GpuCompletionSamples,
    }

    #[wasm_bindgen(js_class = RetainedExecutionCanvasRenderer)]
    impl WasmRetainedExecutionCanvasRenderer {
        /// Recreate WebGL-owned GPU state after a restored context while keeping
        /// the installed retained mirror authoritative. The next render rebuilds
        /// dynamic GPU state from that mirror even when no new delta arrives.
        #[wasm_bindgen(js_name = recoverWebGlContext)]
        pub async fn recover_webgl_context(&mut self) -> Result<bool, JsValue> {
            if self.backend != wgpu::Backend::Gl || !self.webgl_context_lifecycle.recovery_pending()
            {
                return Ok(false);
            }
            if self.webgl_context_lifecycle.is_lost() || webgl_context_is_lost(&self.canvas)? {
                self.webgl_context_lifecycle.mark_lost();
                return Ok(false);
            }

            let next_generation = self
                .gpu_generation
                .checked_add(1)
                .ok_or_else(|| js_message("GPU recovery generation exhausted"))?;
            let width = self.config.width.max(1);
            let height = self.config.height.max(1);
            let initialized = match initialize_gpu(&self.canvas, width, height, true).await {
                Ok(initialized) => initialized,
                Err(_error) if webgl_context_is_lost(&self.canvas)? => {
                    self.webgl_context_lifecycle.mark_lost();
                    return Ok(false);
                }
                Err(error) => return Err(error),
            };
            if self.webgl_context_lifecycle.is_lost() || webgl_context_is_lost(&self.canvas)? {
                self.webgl_context_lifecycle.mark_lost();
                return Ok(false);
            }
            let InitializedGpu {
                instance,
                surface,
                device,
                queue,
                backend,
                config,
            } = initialized;
            install_wgpu_error_handler(
                &device,
                next_generation,
                backend,
                self.gpu_diagnostics.clone(),
            );
            let RetainedGpuState {
                preparer,
                renderer,
                text_gpu,
                preloaded_geometry_count,
                preload_bytes_uploaded,
            } = build_retained_gpu_state(&self.mirror, &device, &queue, config.format)?;

            self.instance = instance;
            self.surface = surface;
            self.device = device;
            self.queue = queue;
            self.backend = backend;
            self.config = config;
            self.preparer = preparer;
            self.renderer = renderer;
            self.text_gpu = text_gpu;
            self.selection_overlay_gpu = OverlayGpuState::default();
            self.gpu_validation_scope = None;
            self.preloaded_geometry_count = preloaded_geometry_count;
            self.preload_bytes_uploaded = preload_bytes_uploaded;
            self.resident_path_maintenance_retry_deferred = false;
            self.last_draw_calls = 0;
            self.last_instances_drawn = 0;
            self.last_bytes_uploaded = 0;
            self.last_geometry_cache_misses = 0;
            self.last_outline_cache_misses = 0;
            self.gpu_generation = next_generation;
            self.gpu_completion_samples.restart();
            self.pending_changes = FrameChanges::all();
            self.pending_frame = self.mirror.frame().is_some();
            self.webgl_context_lifecycle.finish_recovery();
            self.update_camera()?;
            Ok(true)
        }

        #[wasm_bindgen(js_name = create)]
        pub async fn create(
            canvas: OffscreenCanvas,
            resource_bundle_bytes: Vec<u8>,
        ) -> Result<WasmRetainedExecutionCanvasRenderer, JsValue> {
            let mirror =
                InstalledRetainedExecutionMirror::from_bundle_bytes(&resource_bundle_bytes)
                    .map_err(js_error)?;
            let camera = mirror.camera();

            let width = canvas.width().max(1);
            let height = canvas.height().max(1);
            let InitializedGpu {
                instance,
                surface,
                device,
                queue,
                backend,
                config,
            } = initialize_gpu(&canvas, width, height, false).await?;
            if backend == wgpu::Backend::Gl {
                ensure_webgl_context_available(&canvas)?;
            }
            let gpu_generation = 1;
            let gpu_diagnostics = GpuDiagnosticMailbox::default();
            install_wgpu_error_handler(&device, gpu_generation, backend, gpu_diagnostics.clone());
            let gpu_validation_scope = (backend == wgpu::Backend::BrowserWebGpu)
                .then(|| device.push_error_scope(wgpu::ErrorFilter::Validation));
            let webgl_context_lifecycle = WebGlContextLifecycle::install(&canvas, backend)?;
            if webgl_context_lifecycle.is_lost() {
                return Err(js_message(
                    "cannot create a renderer while the WebGL2 context is lost",
                ));
            }

            let RetainedGpuState {
                preparer,
                renderer,
                text_gpu,
                preloaded_geometry_count,
                preload_bytes_uploaded,
            } = build_retained_gpu_state(&mirror, &device, &queue, config.format)?;

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
                glow_transport_seen: false,
                preparer,
                renderer,
                text_gpu,
                selection_overlay: None,
                pointer_view: None,
                selection_overlay_gpu: OverlayGpuState::default(),
                camera_center: camera.center,
                camera_height: camera.height,
                last_draw_calls: 0,
                last_instances_drawn: 0,
                last_bytes_uploaded: 0,
                last_geometry_cache_misses: 0,
                last_outline_cache_misses: 0,
                preloaded_geometry_count,
                preload_bytes_uploaded,
                resident_path_maintenance_retry_deferred: false,
                gpu_generation,
                gpu_diagnostics,
                gpu_validation_scope,
                webgl_context_lifecycle,
                pending_renderer_observation: None,
                last_renderer_observation: None,
                presentation_sequence: 0,
                render_substage_profiling: false,
                render_substage_samples: RenderSubstageSamples::new(),
                gpu_completion_samples: GpuCompletionSamples::default(),
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

            // The mirror remains authoritative through context loss. Resource
            // uploads are deferred until recovery, but the exact delta is still
            // admitted so its frame can be rebuilt without a transport retry.
            let delta: RetainedFamilyExecutionDeltaEnvelope =
                serde_json::from_str(json).map_err(js_error)?;
            let gpu_available = !self.webgl_context_lifecycle.is_lost()
                && !self.webgl_context_lifecycle.recovery_pending();
            let stale = self.mirror.transport_mirror().session() == Some(delta.retained.session)
                && self
                    .mirror
                    .transport_mirror()
                    .applied_sequence()
                    .is_some_and(|sequence| delta.retained.sequence <= sequence);
            let replaces_session = self
                .mirror
                .transport_mirror()
                .session()
                .is_some_and(|session| session != delta.retained.session);

            // Stale packets cannot change the current overlay. Validate a fresh
            // presentation before resource preparation or mirror admission.
            let overlay = if stale {
                None
            } else {
                delta
                    .selection_overlay
                    .as_ref()
                    .map(crate::SelectionOverlayPresentation::prepare)
                    .transpose()
                    .map_err(js_error)?
            };
            let resident_additions = if !stale {
                if let Some(view) = delta.pointer_view {
                    view.validate().map_err(js_error)?;
                    if !view.drawable() {
                        return Err(js_message("published pointer view is unavailable"));
                    }
                }
                delta.validate().map_err(js_error)?;
                if let Some(bundle) = delta.resource_additions.as_ref() {
                    if let Some(addition) = bundle.render_geometry_addition().map_err(js_error)? {
                        if addition.session != delta.retained.session {
                            return Err(js_message(
                                "retained render geometry addition session does not match delta",
                            ));
                        }
                        Some(
                            addition
                                .preparations
                                .iter()
                                .map(|preparation| {
                                    (
                                        preparation.resource,
                                        preparation.style,
                                        preparation.transform,
                                    )
                                })
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Mark only validated, accepted source glow publications. Once seen,
            // keep the path active long enough to retire removed GPU scopes.
            let received_glow = delta.retained.objects.iter().any(|row| row.glow.is_some());
            let pointer_view = delta.pointer_view;
            let mirror = &mut self.mirror;
            let preparer = &mut self.preparer;
            let renderer = &mut self.renderer;
            let device = &self.device;
            let queue = &self.queue;
            let preloaded_geometry_count = &mut self.preloaded_geometry_count;
            let preload_bytes_uploaded = &mut self.preload_bytes_uploaded;
            let FamilyAdmission {
                outcome,
                changes,
                resident_preparation,
            } = apply_family_then_resident_preparation(mirror, delta, |mirror| {
                let Some(preparations) = resident_additions else {
                    return Ok(());
                };
                let resources = mirror.resources();
                preparer.set_scene_path_mesh_cache_budget(
                    resources
                        .render_geometries()
                        .len()
                        .max(resources.render_geometry_preparation_count()),
                    resources.geometry_count(),
                );
                if !gpu_available {
                    return Ok(());
                }
                let geometries = resources.render_geometries();
                let requests =
                    preparations
                        .iter()
                        .map(|(resource, style, transform)| {
                            geometries.get(*resource as usize)?.geometry.as_deref().map(
                                |geometry| PathMeshPreload {
                                    geometry,
                                    style: *style,
                                    transform: *transform,
                                },
                            )
                        })
                        .collect::<Option<Vec<_>>>();
                let Some(requests) = requests else {
                    return Err("admitted geometry is missing".to_owned());
                };
                let preload = preparer
                    .append_preload_path_meshes(device, queue, renderer, &requests)
                    .map_err(|error| error.to_string())?;
                *preloaded_geometry_count =
                    preloaded_geometry_count.saturating_add(preload.geometry.geometry_cache_misses);
                *preload_bytes_uploaded =
                    preload_bytes_uploaded.saturating_add(preload.upload.bytes_uploaded);
                // Queue writes from the admitted resource suffix precede any
                // first-frame uploads/draw submission that follows.
                if preload.upload.bytes_uploaded != 0 {
                    queue.submit([]);
                }
                Ok(())
            })
            .map_err(js_error)?;
            if let Some(Err(error)) = resident_preparation {
                // Admission is authoritative. Preparation hints are opportunistic and
                // accepted resources remain usable through ordinary lazy path preparation.
                web_sys::console::warn_1(&js_sys::Error::new(&format!(
                    "resident path preload skipped: {error}"
                )));
            }
            match outcome {
                RetainedTransportApplyOutcome::Applied => {
                    // The admitted worker source is authoritative. Retain this
                    // preparation path for subsequent removal/cleanup frames.
                    self.glow_transport_seen |= received_glow;
                    if replaces_session {
                        self.renderer.reset_spatial_publication_context();
                        // A previous glow session can already have returned to
                        // the no-effect fast path. Its retained preparer's last
                        // publication epoch must still be retired before a new
                        // session may introduce another attachment.
                        if self.glow_transport_seen
                            || self.preparer.last_applied_publication().is_some()
                        {
                            self.preparer.reset_publication_context();
                            self.renderer.reset_retained_analytic_glow_publication();
                        }
                    }
                    self.selection_overlay = overlay;
                    let view_changed = self.pointer_view != pointer_view;
                    self.pointer_view = pointer_view;
                    let camera = self.mirror.camera();
                    if view_changed
                        || camera.center != self.camera_center
                        || camera.height != self.camera_height
                    {
                        self.camera_center = camera.center;
                        self.camera_height = camera.height;
                        self.update_camera()?;
                    }
                    self.pending_changes = changes;
                    self.pending_frame = true;
                    let defer_path_maintenance = self.resident_path_maintenance_retry_deferred;
                    self.resident_path_maintenance_retry_deferred = false;
                    if gpu_available
                        && !defer_path_maintenance
                        && self.preparer.resident_path_maintenance_due(
                            self.mirror.resources().render_geometry_preparation_count(),
                        )
                    {
                        let requests = {
                            let resources = self.mirror.resources();
                            let geometries = resources.render_geometries();
                            resources
                                .render_geometry_preparations()
                                .map(|preparation| {
                                    Some(PathMeshPreload {
                                        geometry: geometries
                                            .get(preparation.resource as usize)?
                                            .geometry
                                            .as_ref()?
                                            .as_ref(),
                                        style: preparation.style,
                                        transform: preparation.transform,
                                    })
                                })
                                .collect::<Option<Vec<_>>>()
                        };
                        let Some(requests) = requests else {
                            web_sys::console::warn_1(&js_sys::Error::new(
                                "resident path maintenance skipped: live geometry preparation is stale",
                            ));
                            self.resident_path_maintenance_retry_deferred = true;
                            return Ok(true);
                        };
                        match self.preparer.compact_path_meshes(
                            &self.device,
                            &self.queue,
                            &mut self.renderer,
                            &requests,
                        ) {
                            Ok(compacted) => {
                                if compacted.upload.bytes_uploaded != 0 {
                                    self.queue.submit([]);
                                }
                                self.preloaded_geometry_count = self
                                    .preloaded_geometry_count
                                    .saturating_add(compacted.geometry.geometry_cache_misses);
                                self.preload_bytes_uploaded = self
                                    .preload_bytes_uploaded
                                    .saturating_add(compacted.upload.bytes_uploaded);
                                self.pending_changes = FrameChanges::all();
                            }
                            Err(error) => {
                                // The delta has already been accepted. Maintenance is
                                // opportunistic, so retain the old buffers and continue
                                // to render the accepted mirror state normally.
                                web_sys::console::warn_1(&js_sys::Error::new(&format!(
                                    "resident path maintenance skipped: {error}"
                                )));
                                self.resident_path_maintenance_retry_deferred = true;
                            }
                        }
                    }
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

        /// Enable bounded CPU wall-time diagnostics for the existing render stages.
        /// GPU completion and physical scanout are not measured.
        #[wasm_bindgen(js_name = setRenderSubstageProfiling)]
        pub fn set_render_substage_profiling(&mut self, enabled: bool) {
            if enabled && !self.render_substage_profiling {
                self.render_substage_samples.clear();
            }
            self.render_substage_profiling = enabled;
        }

        /// Take and clear the bounded samples collected since the previous query.
        #[wasm_bindgen(js_name = takeRenderSubstageSamplesJson)]
        pub fn take_render_substage_samples_json(&mut self) -> Result<String, JsValue> {
            self.render_substage_samples.take_json().map_err(js_error)
        }

        /// Observe bounded asynchronous queue-completion callbacks. Elapsed time
        /// includes callback dispatch delay; it is not GPU duration or scanout.
        #[wasm_bindgen(js_name = setGpuCompletionProfiling)]
        pub fn set_gpu_completion_profiling(&mut self, enabled: bool) {
            self.gpu_completion_samples.set_enabled(enabled);
        }

        #[wasm_bindgen(js_name = takeGpuCompletionSamplesJson)]
        pub fn take_gpu_completion_samples_json(&self) -> Result<String, JsValue> {
            // WebGL callbacks otherwise wait for a later submit when playback
            // has settled. Only an explicit diagnostic query polls, never waits.
            if self.gpu_completion_samples.is_enabled() && self.backend == wgpu::Backend::Gl {
                self.device.poll(wgpu::PollType::Poll).map_err(js_error)?;
                if self
                    .gpu_diagnostics
                    .device_loss_pending(self.gpu_generation)
                {
                    return Err(js_message(
                        "GPU completion observations invalidated by device loss",
                    ));
                }
            }
            self.gpu_completion_samples.take_json().map_err(js_error)
        }

        pub fn render(&mut self) -> Result<bool, JsValue> {
            if self.webgl_context_lifecycle.is_lost()
                || self.webgl_context_lifecycle.recovery_pending()
            {
                return Ok(false);
            }
            if !self.drawable || !self.pending_frame {
                return Ok(false);
            }

            let profiling = self.render_substage_profiling;
            let acquire_started = profiling.then(performance_now_ms);
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
                        if self.backend == wgpu::Backend::Gl {
                            self.webgl_context_lifecycle.mark_recovery_pending();
                        } else {
                            self.surface = create_surface(&self.instance, &self.canvas)?;
                            self.surface.configure(&self.device, &self.config);
                        }
                        return Ok(false);
                    }
                    wgpu::CurrentSurfaceTexture::Validation => return Ok(false),
                };
            let mut substage_timings = profiling.then(RenderSubstageTimings::default);
            if let (Some(started), Some(timings)) = (acquire_started, substage_timings.as_mut()) {
                timings.surface_acquire_cpu_wall_ms = performance_now_ms() - started;
            }

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
            let prepare_started = profiling.then(performance_now_ms);
            let resources = self.mirror.resources();
            // Worker transport has already resolved and validated this frame and
            // its resource handles. The borrowed renderer publication uses those
            // same objects; no JS effects state or separate renderer is introduced.
            let glow_publication = if self.glow_transport_seen {
                Some(
                    self.mirror
                        .renderer_publication(&self.pending_changes)
                        .map_err(js_error)?,
                )
            } else {
                None
            };
            let installed_frame = self
                .mirror
                .frame()
                .ok_or_else(|| js_message("retained execution renderer has no frame snapshot"))?;
            let spatial_upload = self
                .renderer
                .prepare_spatial_with_resources(
                    &self.device,
                    &self.queue,
                    self.mirror.publication_context(),
                    installed_frame,
                    &self.pending_changes,
                    resources.geometries(),
                    resources.texts(),
                    resources.fonts(),
                    self.mirror.painter_order(),
                )
                .map_err(js_error)?;
            let camera = self.renderer.camera();
            let metrics = TextDeviceMetrics::new(Vec2::new(
                self.config.width as f32 / camera.world_size.x,
                self.config.height as f32 / camera.world_size.y,
            ))
            .and_then(|metrics| {
                metrics.with_world_origin_pixels(Vec2::new(
                    self.config.width as f32 * 0.5 - camera.center.x * metrics.pixels_per_world.x,
                    self.config.height as f32 * 0.5 + camera.center.y * metrics.pixels_per_world.y,
                ))
            })
            .map_err(js_error)?;
            let inset_views = self.mirror.inset_2d_views().to_vec();
            self.preparer
                .set_inset_views_active(!inset_views.is_empty());
            self.renderer
                .set_inset_2d_views(&self.device, &self.queue, &mut self.text_gpu, &inset_views)
                .map_err(js_error)?;
            let plans = self.mirror.family_plans();
            let family_frame = self.mirror.planned_family_frame().map_err(js_error)?;
            if let Some(publication) = glow_publication.as_ref() {
                // This worker's normal retained preparation already contains all
                // installed rows; there is no viewport-candidate culling to
                // augment. Updating the shared sparse source index is still
                // required for live changes/removal before painter capture.
                self.renderer
                    .glow_source_visibility(publication, &[])
                    .map_err(js_error)?;
                if !self.mirror.active_family_animation_indices().is_empty() {
                    return Err(js_message(
                        "retained worker glow does not yet support simultaneous family animation",
                    ));
                }
            }
            if self.pending_changes.is_all() {
                self.preparer.set_painter_order(self.mirror.painter_order());
            } else if let Some(range) = self.pending_changes.painter_order_range() {
                self.preparer
                    .set_painter_order_range(self.mirror.painter_order(), range);
            }
            let transient = self
                .preparer
                .prepare_transient_presentation_rows(
                    self.mirror.frame().ok_or_else(|| {
                        js_message("retained execution renderer has no frame snapshot")
                    })?,
                    self.mirror.painter_order(),
                    self.mirror.transient_presentations(),
                )
                .map_err(js_error)?;
            let family_active = !self.mirror.active_family_animation_indices().is_empty();
            let prepared = if !family_active {
                self.preparer.release_planned_family_realization();
                let frame = self.mirror.frame().ok_or_else(|| {
                    js_message("retained execution renderer has no frame snapshot")
                })?;
                if let Some(publication) = glow_publication.as_ref() {
                    self.preparer
                        .prepare_publication(&self.device, publication, metrics)
                        .map_err(js_error)?
                } else {
                    self.preparer
                        .prepare_with_image_resources(
                            &self.device,
                            frame,
                            &self.pending_changes,
                            resources.texts(),
                            resources.fonts(),
                            resources.geometries(),
                            resources.images(),
                            metrics,
                        )
                        .map_err(js_error)?
                }
            } else {
                let family_frame = family_frame.ok_or_else(|| {
                    js_message(
                        "retained family execution has plans without an evaluated family frame",
                    )
                })?;
                self.preparer
                    .prepare_active_family_plan_set_with_changes(
                        &self.device,
                        &family_frame,
                        plans,
                        self.mirror.active_family_animation_indices(),
                        &self.pending_changes,
                        resources.texts(),
                        resources.fonts(),
                        resources.geometries(),
                        resources.images(),
                        metrics,
                    )
                    .map_err(js_error)?
            };
            if let (Some(started), Some(timings)) = (prepare_started, substage_timings.as_mut()) {
                timings.prepare_cpu_wall_ms = performance_now_ms() - started;
            }
            self.last_geometry_cache_misses = prepared.geometry_stats().geometry_cache_misses;
            self.last_outline_cache_misses = prepared.stats.outline_cache_misses;
            let prepared_observation = resolved_observation_target.as_ref().map(|target| {
                prepared.observe_object(target.mirrored.frame_index, target.mirrored.object)
            });
            let observed_prepared = prepared_observation
                .as_ref()
                .and_then(|observation| observation.as_ref().ok());
            let mut upload_writes = observed_prepared.map(|_| Vec::<UploadWrite>::new());
            let upload_started = profiling.then(performance_now_ms);
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
            let transient_upload =
                self.renderer
                    .upload_transient_presentations(&self.device, &self.queue, &transient);
            let overlay_upload = self.selection_overlay_gpu.update(
                &self.device,
                &self.queue,
                self.selection_overlay,
            );
            self.last_bytes_uploaded = upload
                .bytes_uploaded()
                .saturating_add(spatial_upload.bytes_uploaded())
                .saturating_add(transient_upload.bytes_uploaded)
                .saturating_add(overlay_upload.bytes_uploaded);
            if let (Some(started), Some(timings)) = (upload_started, substage_timings.as_mut()) {
                timings.upload_cpu_wall_ms = performance_now_ms() - started;
            }

            let encode_started = profiling.then(performance_now_ms);
            let view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Noon retained execution render worker frame"),
                });
            if let Some(publication) = glow_publication.as_ref() {
                self.renderer
                    .prepare_retained_analytic_glows(
                        &self.device,
                        &self.queue,
                        &mut encoder,
                        &prepared,
                        publication,
                        noon_render_wgpu::DEFAULT_ANALYTIC_GLOW_TEXTURE_BUDGET,
                    )
                    .map_err(js_error)?;
            }
            let draw = self
                .renderer
                .encode_retained_with_transient_presentations_and_overlay(
                    &mut encoder,
                    &view,
                    InteractiveRetainedFrame {
                        prepared: &prepared,
                        text: &self.text_gpu,
                        transient: Some(&transient),
                        overlay: &self.selection_overlay_gpu,
                    },
                    CLEAR_COLOR,
                    None,
                )
                .map_err(js_error)?;
            let command_buffer = encoder.finish();
            if let (Some(started), Some(timings)) = (encode_started, substage_timings.as_mut()) {
                timings.encode_cpu_wall_ms = performance_now_ms() - started;
            }
            let submit_started = profiling.then(performance_now_ms);
            let completion_ticket = self
                .gpu_completion_samples
                .is_enabled()
                .then(|| {
                    self.gpu_completion_samples.reserve(GpuCompletionSample {
                        session: self.mirror.transport_mirror().session(),
                        sequence: self.mirror.transport_mirror().applied_sequence(),
                        presentation_sequence: self.presentation_sequence.saturating_add(1),
                        gpu_generation: self.gpu_generation,
                        submission_started_ms: performance_now_ms(),
                        completion_observed_ms: 0.0,
                    })
                })
                .flatten();
            self.queue.submit(Some(command_buffer));
            if let Some(ticket) = completion_ticket {
                let diagnostics = self.gpu_diagnostics.clone();
                let generation = self.gpu_generation;
                self.queue.on_submitted_work_done(move || {
                    ticket.complete(
                        performance_now_ms(),
                        diagnostics.device_loss_pending(generation),
                    );
                });
            }
            self.queue.present(surface_texture);
            // Returning to no-effect content restores the original worker fast
            // path once the last old retained GPU scope has been retired.
            if self.glow_transport_seen
                && !self.renderer.has_retained_analytic_glow_sources()
                && self.renderer.analytic_glow_texture_bytes() == 0
            {
                self.glow_transport_seen = false;
            }
            self.presentation_sequence = self.presentation_sequence.saturating_add(1);
            if let (Some(started), Some(timings)) = (submit_started, substage_timings.as_mut()) {
                timings.submit_present_cpu_wall_ms = performance_now_ms() - started;
            }
            if let Some(timings) = substage_timings {
                self.render_substage_samples.push(RenderSubstageSample {
                    session: self.mirror.transport_mirror().session(),
                    sequence: self.mirror.transport_mirror().applied_sequence(),
                    surface_acquire_cpu_wall_ms: timings.surface_acquire_cpu_wall_ms,
                    prepare_cpu_wall_ms: timings.prepare_cpu_wall_ms,
                    upload_cpu_wall_ms: timings.upload_cpu_wall_ms,
                    encode_cpu_wall_ms: timings.encode_cpu_wall_ms,
                    submit_present_cpu_wall_ms: timings.submit_present_cpu_wall_ms,
                });
            }
            self.last_draw_calls = draw.draw_calls();
            self.last_instances_drawn = draw.instances_drawn();
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
            // Assigning equal OffscreenCanvas dimensions resets its backing
            // store. Renderer transitions always reconcile the current CSS
            // size after bootstrap, so this must be a no-op when the backing
            // dimensions are already current.
            if self.canvas.width() == width && self.canvas.height() == height {
                return Ok(());
            }
            self.canvas.set_width(width);
            self.canvas.set_height(height);
            self.drawable = width > 0 && height > 0;
            if !self.drawable {
                return Ok(());
            }
            if self.config.width != width || self.config.height != height {
                self.config.width = width;
                self.config.height = height;
                if self.webgl_context_lifecycle.is_lost()
                    || self.webgl_context_lifecycle.recovery_pending()
                {
                    self.pending_frame = self.mirror.frame().is_some();
                    return Ok(());
                }
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

        /// Return identity reported by the adapter backing this renderer's device.
        /// This is intentionally an explicit diagnostics call; it is not read per frame.
        #[wasm_bindgen(js_name = rendererAdapterInfo)]
        pub fn renderer_adapter_info(&self) -> Result<String, JsValue> {
            let info = self.device.adapter_info();
            serde_json::to_string(&serde_json::json!({
                "backend": format!("{:?}", info.backend),
                "vendor": info.vendor,
                "device": info.device,
                "name": info.name,
                "deviceType": format!("{:?}", info.device_type),
                "driver": info.driver,
                "driverInfo": info.driver_info,
            }))
            .map_err(js_error)
        }

        #[wasm_bindgen(js_name = gpuGeneration)]
        pub fn gpu_generation(&self) -> u32 {
            self.gpu_generation
        }

        #[wasm_bindgen(js_name = flushGpuDiagnostics)]
        pub fn flush_gpu_diagnostics(&mut self) -> js_sys::Promise {
            let Some(scope) = self.gpu_validation_scope.take() else {
                return js_sys::Promise::resolve(&JsValue::from_bool(false));
            };
            let pending = scope.pop();
            self.gpu_validation_scope =
                Some(self.device.push_error_scope(wgpu::ErrorFilter::Validation));
            let diagnostics = self.gpu_diagnostics.clone();
            let generation = self.gpu_generation;
            let backend = self.backend;
            wasm_bindgen_futures::future_to_promise(async move {
                let Some(error) = pending.await else {
                    return Ok(JsValue::from_bool(false));
                };
                diagnostics.record_wgpu(generation, backend, error);
                Ok(JsValue::from_bool(true))
            })
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
                self.mirror
                    .painter_order()
                    .iter()
                    .filter(|&&index| {
                        frame
                            .presences
                            .get(index as usize)
                            .copied()
                            .unwrap_or(false)
                    })
                    .count()
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
            if !self.drawable
                || self.webgl_context_lifecycle.is_lost()
                || self.webgl_context_lifecycle.recovery_pending()
            {
                return Ok(());
            }
            let aspect = self.pointer_view.map_or(
                self.config.width as f32 / self.config.height as f32,
                |view| view.width / view.height,
            );
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

    async fn initialize_gpu(
        canvas: &OffscreenCanvas,
        width: u32,
        height: u32,
        force_webgl: bool,
    ) -> Result<InitializedGpu, JsValue> {
        let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        instance_descriptor.backends = if force_webgl {
            wgpu::Backends::GL
        } else {
            wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
        };
        instance_descriptor.display = Some(Box::new(WebDisplaySource));
        let instance = if force_webgl {
            wgpu::Instance::new(instance_descriptor)
        } else {
            wgpu::util::new_instance_with_webgpu_detection(instance_descriptor).await
        };
        let surface = create_surface(&instance, canvas)?;
        // `create_surface` has selected/claimed the browser context at this
        // point, so this check does not alter WebGPU selection. It must happen
        // before adapter enumeration reaches glow's GL_VERSION probe.
        ensure_webgl_context_available(canvas)?;
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
        if force_webgl && backend != wgpu::Backend::Gl {
            return Err(js_message(
                "WebGL context recovery unexpectedly selected a different GPU backend",
            ));
        }
        if backend == wgpu::Backend::Gl {
            ensure_webgl_context_available(canvas)?;
        }
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
        if backend == wgpu::Backend::Gl {
            ensure_webgl_context_available(canvas)?;
        }
        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| js_message("GPU adapter cannot present retained execution"))?;
        surface.configure(&device, &config);
        Ok(InitializedGpu {
            instance,
            surface,
            device,
            queue,
            backend,
            config,
        })
    }

    fn build_retained_gpu_state(
        mirror: &InstalledRetainedExecutionMirror,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Result<RetainedGpuState, JsValue> {
        let mut preparer = RetainedFramePreparer::new();
        preparer.set_scene_path_mesh_cache_budget(
            mirror
                .resources()
                .render_geometries()
                .len()
                .max(mirror.resources().render_geometry_preparation_count()),
            mirror.resources().geometry_count(),
        );
        let mut renderer = GpuRenderer::new(device, queue, format);
        let text_gpu = renderer.create_retained_text_state(device, queue);
        let resources = mirror.resources().render_geometries();
        let requests = mirror
            .resources()
            .render_geometry_preparations()
            .map(|preparation| PathMeshPreload {
                geometry: resources[preparation.resource as usize]
                    .geometry
                    .as_ref()
                    .expect("prepared render geometry is live")
                    .as_ref(),
                style: preparation.style,
                transform: preparation.transform,
            })
            .collect::<Vec<_>>();
        let preload = preparer
            .preload_path_meshes(device, queue, &mut renderer, &requests)
            .map_err(js_error)?;
        if preload.upload.bytes_uploaded != 0 {
            queue.submit([]);
        }
        Ok(RetainedGpuState {
            preparer,
            renderer,
            text_gpu,
            preloaded_geometry_count: preload.geometry.geometry_cache_misses,
            preload_bytes_uploaded: preload.upload.bytes_uploaded,
        })
    }

    impl Drop for WasmRetainedExecutionCanvasRenderer {
        fn drop(&mut self) {
            // Pop the retained validation scope before invalidating its device.
            self.gpu_validation_scope.take();
            // wgpu's WebGPU backend deliberately leaves GPUDevice alive on Rust
            // drop. A renderer owns its browser device, so retire it explicitly.
            if self.backend == wgpu::Backend::BrowserWebGpu {
                self.device.destroy();
            }
        }
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
