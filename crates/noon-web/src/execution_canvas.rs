#[cfg(any(target_arch = "wasm32", test))]
const MANIM_DEFAULT_CAMERA_HEIGHT: f32 = 8.0;

#[cfg(target_arch = "wasm32")]
const MANIM_DEFAULT_CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::mem;

    use noon_core::{GeometryRef, ObjectId, Transform2D, Vec2};
    use noon_render_wgpu::{
        Camera2D, DrawStats, FramePreparer, GpuRenderer, LineInstance, PackedTransform,
        PreparedFrame, RenderPrimitive, UploadStats,
    };
    use noon_runtime::{FrameChanges, FrameObjectState};
    use serde::Serialize;
    use wasm_bindgen::prelude::*;
    use web_sys::OffscreenCanvas;

    use crate::{
        gpu_diagnostics::{install_wgpu_error_handler, GpuDiagnosticMailbox},
        ExecutionFrameMirror, TransportApplyOutcome, TransportSlotId,
    };

    use super::{MANIM_DEFAULT_CAMERA_HEIGHT, MANIM_DEFAULT_CLEAR_COLOR};

    #[derive(Debug)]
    struct WebDisplaySource;

    impl wgpu::rwh::HasDisplayHandle for WebDisplaySource {
        fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
            Ok(wgpu::rwh::DisplayHandle::web())
        }
    }

    #[derive(Clone, Copy, Debug, Serialize)]
    struct DiagnosticRange {
        start: usize,
        end: usize,
    }

    impl From<&std::ops::Range<usize>> for DiagnosticRange {
        fn from(value: &std::ops::Range<usize>) -> Self {
            Self {
                start: value.start,
                end: value.end,
            }
        }
    }

    #[derive(Clone, Debug, Serialize)]
    struct DiagnosticExecution {
        session: Option<u32>,
        sequence: u64,
        layout_generation: u64,
        time: f64,
    }

    #[derive(Clone, Debug, Serialize)]
    struct DiagnosticObjectState {
        object: u64,
        frame_index: usize,
        slot: Option<TransportSlotId>,
        transform: Transform2D,
        world_endpoints: Option<[Vec2; 2]>,
        dirty_classification: &'static str,
    }

    #[derive(Clone, Debug, Serialize)]
    struct DiagnosticPreparedState {
        state: DiagnosticObjectState,
        instance_kind: Option<&'static str>,
        instance_range: Option<DiagnosticRange>,
        full_rebuilds: usize,
        instances_repacked: usize,
    }

    #[derive(Clone, Debug, Serialize)]
    struct DiagnosticGpuState {
        state: DiagnosticObjectState,
        instance_kind: Option<&'static str>,
        instance_range: Option<DiagnosticRange>,
        instance_generation: u64,
        dirty_ranges: Vec<DiagnosticRange>,
        bytes_uploaded: usize,
        total_bytes_uploaded: usize,
        buffer_reallocations: usize,
    }

    #[derive(Clone, Debug, Serialize)]
    struct DiagnosticDrawBatch {
        primitive: &'static str,
        instance_range: DiagnosticRange,
    }

    #[derive(Clone, Debug, Serialize)]
    struct DiagnosticDrawState {
        state: DiagnosticObjectState,
        submission_membership: bool,
        batches: Vec<DiagnosticDrawBatch>,
        draw_calls: usize,
        instances_drawn: usize,
    }

    #[derive(Clone, Debug, Serialize)]
    struct DiagnosticPresentationState {
        surface_status: &'static str,
        submitted: bool,
        presented: bool,
    }

    #[derive(Clone, Debug, Serialize)]
    struct HostUpdaterDiagnostic {
        schema_version: u32,
        backend: &'static str,
        execution: DiagnosticExecution,
        committed: DiagnosticObjectState,
        prepared: DiagnosticPreparedState,
        gpu: DiagnosticGpuState,
        draw: DiagnosticDrawState,
        presentation: DiagnosticPresentationState,
    }

    struct InitializedGpu {
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        backend: wgpu::Backend,
        config: wgpu::SurfaceConfiguration,
    }

    #[wasm_bindgen(js_name = ExecutionCanvasRenderer)]
    pub struct WasmExecutionCanvasRenderer {
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        backend: wgpu::Backend,
        canvas: OffscreenCanvas,
        config: wgpu::SurfaceConfiguration,
        drawable: bool,
        mirror: ExecutionFrameMirror,
        pending_changes: FrameChanges,
        preparer: FramePreparer,
        renderer: GpuRenderer,
        camera_center: Vec2,
        camera_height: f32,
        clear_color: wgpu::Color,
        last_draw_calls: usize,
        last_instances_drawn: usize,
        last_bytes_uploaded: usize,
        last_geometry_cache_misses: usize,
        gpu_generation: u32,
        gpu_diagnostics: GpuDiagnosticMailbox,
        host_updater_diagnostic_object: Option<ObjectId>,
        last_host_updater_diagnostic: Option<HostUpdaterDiagnostic>,
        gpu_instance_generation: u64,
    }

    #[wasm_bindgen(js_class = ExecutionCanvasRenderer)]
    impl WasmExecutionCanvasRenderer {
        #[wasm_bindgen(js_name = create)]
        pub async fn create(
            canvas: OffscreenCanvas,
            initial_delta_json: &str,
        ) -> Result<WasmExecutionCanvasRenderer, JsValue> {
            let mut mirror = ExecutionFrameMirror::default();
            let (outcome, pending_changes) =
                mirror.apply_json(initial_delta_json).map_err(js_error)?;
            if outcome != TransportApplyOutcome::Applied || !pending_changes.is_all() {
                return Err(js_message(
                    "execution renderer must start from an applied transport snapshot",
                ));
            }
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
            } = initialize_gpu(&canvas, width, height).await?;
            let gpu_generation = 1;
            let gpu_diagnostics = GpuDiagnosticMailbox::default();
            install_wgpu_error_handler(&device, gpu_generation, backend, gpu_diagnostics.clone());
            let renderer = GpuRenderer::new(&device, config.format);

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
                pending_changes,
                preparer: FramePreparer::new(),
                renderer,
                camera_center: camera.center,
                camera_height: camera.height,
                clear_color: MANIM_DEFAULT_CLEAR_COLOR,
                last_draw_calls: 0,
                last_instances_drawn: 0,
                last_bytes_uploaded: 0,
                last_geometry_cache_misses: 0,
                gpu_generation,
                gpu_diagnostics,
                host_updater_diagnostic_object: None,
                last_host_updater_diagnostic: None,
                gpu_instance_generation: 0,
            };
            result.update_camera()?;
            Ok(result)
        }

        #[wasm_bindgen(js_name = applyDeltaJson)]
        pub fn apply_delta_json(&mut self, json: &str) -> Result<bool, JsValue> {
            if !self.pending_changes.is_empty() {
                return Err(js_message(
                    "render worker must present the applied execution delta before accepting another",
                ));
            }
            let (outcome, changes) = self.mirror.apply_json(json).map_err(js_error)?;
            match outcome {
                TransportApplyOutcome::Applied => {
                    let camera = self.mirror.camera();
                    if camera.center != self.camera_center || camera.height != self.camera_height {
                        self.camera_center = camera.center;
                        self.camera_height = camera.height;
                        self.update_camera()?;
                    }
                    self.pending_changes = changes;
                    Ok(true)
                }
                TransportApplyOutcome::DroppedStale => Ok(false),
            }
        }

        /// Enables the bounded RotationUpdater diagnostic seam used by the
        /// deterministic host-updater regression harness. It only records the
        /// selected object while a normal incremental render is already running.
        #[wasm_bindgen(js_name = setHostUpdaterDiagnosticObject)]
        pub fn set_host_updater_diagnostic_object(&mut self, object: u64) {
            self.host_updater_diagnostic_object = Some(ObjectId::new(object));
            self.last_host_updater_diagnostic = None;
        }

        #[wasm_bindgen(js_name = takeHostUpdaterDiagnosticJson)]
        pub fn take_host_updater_diagnostic_json(&mut self) -> Result<Option<String>, JsValue> {
            self.last_host_updater_diagnostic
                .take()
                .map(|diagnostic| serde_json::to_string(&diagnostic).map_err(js_error))
                .transpose()
        }

        pub fn render(&mut self) -> Result<bool, JsValue> {
            if !self.drawable || self.pending_changes.is_empty() {
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
                    // The device error callback owns validation diagnostics. Keep
                    // the pending frame intact so a recoverable validation can be
                    // reported once and presentation retried unchanged.
                    wgpu::CurrentSurfaceTexture::Validation => return Ok(false),
                };

            let changes = mem::take(&mut self.pending_changes);
            let frame = self
                .mirror
                .frame()
                .ok_or_else(|| js_message("execution renderer has no frame snapshot"))?;
            let prepared = self.preparer.prepare_incremental(frame, &changes);
            self.last_geometry_cache_misses = prepared.stats.geometry_cache_misses;
            let upload = self.renderer.upload(&self.device, &self.queue, &prepared);
            self.last_bytes_uploaded = upload.bytes_uploaded;
            self.gpu_instance_generation = self.gpu_instance_generation.saturating_add(1);

            let view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Noon execution render worker frame"),
                });
            let draw = self
                .renderer
                .encode(&mut encoder, &view, &prepared, self.clear_color);
            self.queue.submit(Some(encoder.finish()));
            surface_texture.present();
            self.last_draw_calls = draw.draw_calls;
            self.last_instances_drawn = draw.instances_drawn;
            if let Some(object) = self.host_updater_diagnostic_object {
                self.last_host_updater_diagnostic = build_host_updater_diagnostic(
                    self.backend,
                    &self.mirror,
                    &changes,
                    &prepared,
                    upload,
                    draw,
                    self.gpu_instance_generation,
                    surface_status,
                    true,
                    true,
                    object,
                );
            }
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
            }
            Ok(())
        }

        /// Manual camera control remains as a low-level host API. Semantic scene
        /// deltas automatically overwrite it with their shared Rust camera state.
        #[wasm_bindgen(js_name = setCamera)]
        pub fn set_camera(
            &mut self,
            center_x: f32,
            center_y: f32,
            world_height: f32,
        ) -> Result<(), JsValue> {
            let center = Vec2::new(center_x, center_y);
            Camera2D::new(center, Vec2::new(world_height, world_height)).map_err(js_error)?;
            self.camera_center = center;
            self.camera_height = world_height;
            self.update_camera()
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
            self.mirror.live_object_count()
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
    }

    fn build_host_updater_diagnostic(
        backend: wgpu::Backend,
        mirror: &ExecutionFrameMirror,
        changes: &FrameChanges,
        prepared: &PreparedFrame<'_>,
        upload: UploadStats,
        draw: DrawStats,
        instance_generation: u64,
        surface_status: &'static str,
        submitted: bool,
        presented: bool,
        target: ObjectId,
    ) -> Option<HostUpdaterDiagnostic> {
        let frame = mirror.frame()?;
        let frame_index = frame.objects.iter().position(|object| object.id == target)?;
        let committed_object = &frame.objects[frame_index];
        let execution = DiagnosticExecution {
            session: mirror.session(),
            sequence: mirror.next_sequence().saturating_sub(1),
            layout_generation: mirror.layout_generation(),
            time: frame.time,
        };
        let committed = diagnostic_object_state(
            frame,
            mirror,
            changes,
            frame_index,
            target,
            committed_object.transform,
            world_endpoints(committed_object),
        );

        let (instance_kind, instance_range, prepared_transform, prepared_endpoints) =
            prepared_line_state(prepared, target);
        let prepared_state = DiagnosticPreparedState {
            state: diagnostic_object_state(
                frame,
                mirror,
                changes,
                frame_index,
                target,
                prepared_transform.unwrap_or(committed_object.transform),
                prepared_endpoints,
            ),
            instance_kind,
            instance_range,
            full_rebuilds: prepared.stats.full_rebuilds,
            instances_repacked: prepared.stats.instances_repacked,
        };

        let dirty_ranges = prepared
            .line_dirty_ranges
            .iter()
            .map(DiagnosticRange::from)
            .collect::<Vec<_>>();
        let line_bytes_uploaded = if dirty_ranges.is_empty() {
            0
        } else {
            dirty_ranges
                .iter()
                .map(|range| range.end.saturating_sub(range.start))
                .sum::<usize>()
                .saturating_mul(std::mem::size_of::<LineInstance>())
        };
        let gpu = DiagnosticGpuState {
            state: diagnostic_object_state(
                frame,
                mirror,
                changes,
                frame_index,
                target,
                prepared_transform.unwrap_or(committed_object.transform),
                prepared_endpoints,
            ),
            instance_kind,
            instance_range,
            instance_generation,
            dirty_ranges,
            bytes_uploaded: line_bytes_uploaded,
            total_bytes_uploaded: upload.bytes_uploaded,
            buffer_reallocations: upload.buffer_reallocations,
        };

        let mut batches = Vec::new();
        for batch in prepared.render_batches {
            let RenderPrimitive::Line = batch.primitive else {
                continue;
            };
            let contains_target = batch
                .instance_range
                .clone()
                .filter_map(|index| prepared.line_ids.get(index as usize))
                .any(|object| *object == target);
            if contains_target {
                batches.push(DiagnosticDrawBatch {
                    primitive: "line",
                    instance_range: DiagnosticRange {
                        start: batch.instance_range.start as usize,
                        end: batch.instance_range.end as usize,
                    },
                });
            }
        }
        let draw_state = DiagnosticDrawState {
            state: diagnostic_object_state(
                frame,
                mirror,
                changes,
                frame_index,
                target,
                prepared_transform.unwrap_or(committed_object.transform),
                prepared_endpoints,
            ),
            submission_membership: !batches.is_empty(),
            batches,
            draw_calls: draw.draw_calls,
            instances_drawn: draw.instances_drawn,
        };

        Some(HostUpdaterDiagnostic {
            schema_version: 1,
            backend: backend_label(backend),
            execution,
            committed,
            prepared: prepared_state,
            gpu,
            draw: draw_state,
            presentation: DiagnosticPresentationState {
                surface_status,
                submitted,
                presented,
            },
        })
    }

    fn diagnostic_object_state(
        frame: &noon_runtime::FrameState,
        mirror: &ExecutionFrameMirror,
        changes: &FrameChanges,
        frame_index: usize,
        target: ObjectId,
        transform: Transform2D,
        world_endpoints: Option<[Vec2; 2]>,
    ) -> DiagnosticObjectState {
        DiagnosticObjectState {
            object: target.get(),
            frame_index,
            slot: mirror.slot_for_frame_index(frame_index),
            transform,
            world_endpoints,
            dirty_classification: dirty_classification(changes, frame_index, frame),
        }
    }

    fn dirty_classification(
        changes: &FrameChanges,
        frame_index: usize,
        frame: &noon_runtime::FrameState,
    ) -> &'static str {
        if changes.is_all() {
            return "all";
        }
        if changes.removed_indices().binary_search(&frame_index).is_ok()
            || !frame.is_present(frame_index)
        {
            return "removed";
        }
        if changes.added_indices().binary_search(&frame_index).is_ok() {
            return "added";
        }
        if changes.object_indices().binary_search(&frame_index).is_ok() {
            return "updated";
        }
        "unchanged"
    }

    fn prepared_line_state(
        prepared: &PreparedFrame<'_>,
        target: ObjectId,
    ) -> (
        Option<&'static str>,
        Option<DiagnosticRange>,
        Option<Transform2D>,
        Option<[Vec2; 2]>,
    ) {
        let Some(index) = prepared.line_ids.iter().position(|object| *object == target) else {
            return (None, None, None, None);
        };
        let Some(instance) = prepared.lines.get(index).copied() else {
            return (Some("line"), None, None, None);
        };
        let transform = transform_from_packed(instance.transform);
        let endpoints = [
            transform.transform_point(Vec2::new(instance.start[0], instance.start[1])),
            transform.transform_point(Vec2::new(instance.end[0], instance.end[1])),
        ];
        (
            Some("line"),
            Some(DiagnosticRange {
                start: index,
                end: index.saturating_add(1),
            }),
            Some(transform),
            Some(endpoints),
        )
    }

    fn transform_from_packed(value: PackedTransform) -> Transform2D {
        Transform2D {
            translation: Vec2::new(value.translation[0], value.translation[1]),
            rotation: value.rotation,
            scale: Vec2::new(value.scale[0], value.scale[1]),
        }
    }

    fn world_endpoints(object: &FrameObjectState) -> Option<[Vec2; 2]> {
        let GeometryRef::Line { start, end } = &object.geometry else {
            return None;
        };
        Some([
            object.transform.transform_point(*start),
            object.transform.transform_point(*end),
        ])
    }

    const fn backend_label(backend: wgpu::Backend) -> &'static str {
        match backend {
            wgpu::Backend::BrowserWebGpu => "WebGPU",
            wgpu::Backend::Gl => "WebGL2",
            _ => "Other",
        }
    }

    impl WasmExecutionCanvasRenderer {
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

    async fn initialize_gpu(
        canvas: &OffscreenCanvas,
        width: u32,
        height: u32,
    ) -> Result<InitializedGpu, JsValue> {
        let webgpu_error = if wgpu::util::is_browser_webgpu_supported().await {
            match initialize_webgpu(canvas, width, height).await {
                Ok(initialized) => return Ok(initialized),
                Err(error) => Some(error),
            }
        } else {
            None
        };

        initialize_webgl(canvas, width, height)
            .await
            .map_err(|webgl_error| {
                if let Some(webgpu_error) = webgpu_error {
                    js_message(&format!(
                        "WebGPU initialization failed: {webgpu_error}; WebGL2 fallback failed: {webgl_error}"
                    ))
                } else {
                    js_message(&format!("WebGL2 initialization failed: {webgl_error}"))
                }
            })
    }

    async fn initialize_webgpu(
        canvas: &OffscreenCanvas,
        width: u32,
        height: u32,
    ) -> Result<InitializedGpu, String> {
        let instance = create_instance(wgpu::Backends::BROWSER_WEBGPU);
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .map_err(|error| error.to_string())?;
        let backend = adapter.get_info().backend;
        let (device, queue) = request_device(&adapter, backend).await?;

        // Do not claim a canvas context until WebGPU has a usable device. Once a
        // canvas is bound to WebGPU, browsers cannot reliably create WebGL2 from
        // that same canvas for fallback.
        let surface = create_surface(&instance, canvas).map_err(js_value_message)?;
        let config = default_surface_config(&surface, &adapter, width, height)?;
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

    async fn initialize_webgl(
        canvas: &OffscreenCanvas,
        width: u32,
        height: u32,
    ) -> Result<InitializedGpu, String> {
        let instance = create_instance(wgpu::Backends::GL);
        let surface = create_surface(&instance, canvas).map_err(js_value_message)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .map_err(|error| error.to_string())?;
        let backend = adapter.get_info().backend;
        let (device, queue) = request_device(&adapter, backend).await?;
        let config = default_surface_config(&surface, &adapter, width, height)?;
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

    fn create_instance(backends: wgpu::Backends) -> wgpu::Instance {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = backends;
        descriptor.display = Some(Box::new(WebDisplaySource));
        wgpu::Instance::new(descriptor)
    }

    async fn request_device(
        adapter: &wgpu::Adapter,
        backend: wgpu::Backend,
    ) -> Result<(wgpu::Device, wgpu::Queue), String> {
        let required_limits = if backend == wgpu::Backend::Gl {
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
        } else {
            wgpu::Limits::default()
        };
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Noon execution render worker GPU device"),
                required_features: wgpu::Features::empty(),
                required_limits,
                ..Default::default()
            })
            .await
            .map_err(|error| error.to_string())
    }

    fn default_surface_config(
        surface: &wgpu::Surface<'_>,
        adapter: &wgpu::Adapter,
        width: u32,
        height: u32,
    ) -> Result<wgpu::SurfaceConfiguration, String> {
        surface
            .get_default_config(adapter, width, height)
            .ok_or_else(|| "GPU adapter cannot present to this OffscreenCanvas".to_owned())
    }

    fn create_surface(
        instance: &wgpu::Instance,
        canvas: &OffscreenCanvas,
    ) -> Result<wgpu::Surface<'static>, JsValue> {
        instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
            .map_err(js_error)
    }

    fn js_value_message(value: JsValue) -> String {
        value.as_string().unwrap_or_else(|| format!("{value:?}"))
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

#[cfg(test)]
mod tests {
    use super::MANIM_DEFAULT_CAMERA_HEIGHT;

    #[test]
    fn default_camera_matches_manim_frame_height() {
        assert_eq!(MANIM_DEFAULT_CAMERA_HEIGHT, 8.0);
    }
}
