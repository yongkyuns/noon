#[cfg(test)]
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

    use noon::{
        ExecutionSession, LiveContinuation, LiveProgram, LiveProgramStatus, RendererPublication,
        RustHostCallbackTable,
    };
    use noon_core::{
        Camera2DState, NativeEventOccurrence, NativeEventSource, NativeInputValue,
        NativeStateSource, ReactiveValue, Rect, SemanticNodeId, Vec2,
    };
    use noon_render_wgpu::{
        Camera2D, FramePreparer, GpuRenderer, RetainedFramePreparer, RetainedTextGpuState,
    };
    use noon_runtime::{FrameChanges, FrameState};
    use noon_text_render_wgpu::TextDeviceMetrics;
    use serde::Serialize;
    use wasm_bindgen::prelude::*;
    use web_sys::OffscreenCanvas;

    use crate::{
        gpu_diagnostics::{install_wgpu_error_handler, GpuDiagnosticMailbox},
        gpu_timestamps::GpuTimestampProfiler,
        BrowserExecutionCadence, BrowserExecutionWakeClock, BrowserExecutionWakePlan,
        BrowserHostWake, ExecutionFrameMirror, TransportApplyOutcome,
    };

    use super::MANIM_DEFAULT_CLEAR_COLOR;

    #[derive(Debug)]
    struct WebDisplaySource;

    impl wgpu::rwh::HasDisplayHandle for WebDisplaySource {
        fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
            Ok(wgpu::rwh::DisplayHandle::web())
        }
    }

    #[derive(Clone, Copy, Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct DirectWakeDirectiveJson {
        present_now: bool,
        cadence: &'static str,
        delay_ms: Option<f64>,
    }

    struct InitializedGpu {
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        backend: wgpu::Backend,
        config: wgpu::SurfaceConfiguration,
        timestamp_query_supported: bool,
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct DirectDriveOutcome {
        resumed: bool,
        completed_callback_phase: bool,
    }

    /// Narrow direct-host view of a shared live continuation program.
    ///
    /// A continuation keeps its scene and session encapsulated; the canvas may
    /// only drive its current segment, consume a renderer publication, admit that
    /// exact publication after presentation, and forward typed platform input.
    trait DirectLiveProgram {
        fn session(&self) -> &ExecutionSession;
        fn wake_plan(&self) -> BrowserExecutionWakePlan;
        fn query_viewport(&mut self, bounds: Rect) -> noon::ExecutionViewportQuery;
        /// Returns whether this operation resumed a subsequent continuation stage.
        fn drive_to(&mut self, requested_time: f64) -> Result<DirectDriveOutcome, JsValue>;
        fn take_renderer_publication(&mut self) -> RendererPublication<'_>;
        fn admit_rendered_publication(
            &mut self,
            publication: noon_core::PublicationContext,
        ) -> Result<bool, JsValue>;
        fn set_native_state_input(
            &mut self,
            source: NativeStateSource,
            value: NativeInputValue,
        ) -> Result<(), JsValue>;
        fn emit_native_event(&mut self, occurrence: NativeEventOccurrence) -> Result<(), JsValue>;
    }

    struct DirectLiveProgramAdapter<C> {
        program: LiveProgram<C>,
        callbacks: RustHostCallbackTable,
    }

    impl<C> DirectLiveProgramAdapter<C>
    where
        C: LiveContinuation + 'static,
        C::Error: std::fmt::Display,
    {
        fn new_with_callbacks(
            mut program: LiveProgram<C>,
            callbacks: RustHostCallbackTable,
        ) -> Result<Self, JsValue> {
            let status = program.resume().map_err(js_error)?;
            if !matches!(
                status,
                LiveProgramStatus::Awaiting(_) | LiveProgramStatus::Finished
            ) {
                return Err(js_message(
                    "direct live continuation did not begin at an await or finished state",
                ));
            }
            Ok(Self { program, callbacks })
        }

        fn resume_if_ready(&mut self) -> Result<bool, JsValue> {
            if matches!(self.program.status(), LiveProgramStatus::ReadyToResume) {
                self.program.resume().map_err(js_error)?;
                return Ok(true);
            }
            Ok(false)
        }
    }

    impl<C> DirectLiveProgram for DirectLiveProgramAdapter<C>
    where
        C: LiveContinuation + 'static,
        C::Error: std::fmt::Display,
    {
        fn session(&self) -> &ExecutionSession {
            self.program.session()
        }

        fn wake_plan(&self) -> BrowserExecutionWakePlan {
            BrowserExecutionWakePlan::from_runtime(self.program.wake_state())
        }

        fn query_viewport(&mut self, bounds: Rect) -> noon::ExecutionViewportQuery {
            self.program.query_viewport(bounds)
        }

        fn drive_to(&mut self, requested_time: f64) -> Result<DirectDriveOutcome, JsValue> {
            self.program
                .drive_to(&mut self.callbacks, requested_time)
                .map_err(js_error)?;
            Ok(DirectDriveOutcome {
                resumed: self.resume_if_ready()?,
                completed_callback_phase: self.callbacks.last_advance_completed_callback_phase(),
            })
        }

        fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
            self.program.take_renderer_publication()
        }

        fn admit_rendered_publication(
            &mut self,
            publication: noon_core::PublicationContext,
        ) -> Result<bool, JsValue> {
            if !matches!(
                self.program.status(),
                LiveProgramStatus::PublicationPending(_)
            ) {
                return Ok(false);
            }
            self.program
                .admit_publication(publication)
                .map_err(js_error)?;
            self.resume_if_ready()
        }

        fn set_native_state_input(
            &mut self,
            source: NativeStateSource,
            value: NativeInputValue,
        ) -> Result<(), JsValue> {
            self.program
                .set_native_state_input(source, value)
                .map(|_| ())
                .map_err(js_error)
        }

        fn emit_native_event(&mut self, occurrence: NativeEventOccurrence) -> Result<(), JsValue> {
            self.program
                .emit_native_event(occurrence)
                .map(|_| ())
                .map_err(js_error)
        }
    }

    enum DirectSourceAuthority {
        Session {
            session: ExecutionSession,
            callbacks: RustHostCallbackTable,
        },
        Program(Box<dyn DirectLiveProgram>),
    }

    struct DirectExecutionSource {
        authority: DirectSourceAuthority,
        next_native_event_sequence: u64,
    }

    impl DirectExecutionSource {
        fn from_session(session: ExecutionSession, callbacks: RustHostCallbackTable) -> Self {
            Self {
                authority: DirectSourceAuthority::Session { session, callbacks },
                next_native_event_sequence: 0,
            }
        }

        fn from_live_program_with_callbacks<C>(
            program: LiveProgram<C>,
            callbacks: RustHostCallbackTable,
        ) -> Result<Self, JsValue>
        where
            C: LiveContinuation + 'static,
            C::Error: std::fmt::Display,
        {
            Ok(Self {
                authority: DirectSourceAuthority::Program(Box::new(
                    DirectLiveProgramAdapter::new_with_callbacks(program, callbacks)?,
                )),
                next_native_event_sequence: 0,
            })
        }

        fn session(&self) -> &ExecutionSession {
            match &self.authority {
                DirectSourceAuthority::Session { session, .. } => session,
                DirectSourceAuthority::Program(program) => program.session(),
            }
        }

        fn session_mut(&mut self) -> Option<&mut ExecutionSession> {
            match &mut self.authority {
                DirectSourceAuthority::Session { session, .. } => Some(session),
                DirectSourceAuthority::Program(_) => None,
            }
        }

        fn wake_plan(&self) -> BrowserExecutionWakePlan {
            match &self.authority {
                DirectSourceAuthority::Session { session, .. } => {
                    BrowserExecutionWakePlan::from_session(session)
                }
                DirectSourceAuthority::Program(program) => program.wake_plan(),
            }
        }

        fn query_viewport(&mut self, bounds: Rect) -> noon::ExecutionViewportQuery {
            match &mut self.authority {
                DirectSourceAuthority::Session { session, .. } => session.query_viewport(bounds),
                DirectSourceAuthority::Program(program) => program.query_viewport(bounds),
            }
        }

        fn drive_to(&mut self, requested_time: f64) -> Result<DirectDriveOutcome, JsValue> {
            match &mut self.authority {
                DirectSourceAuthority::Session { session, callbacks } => {
                    callbacks
                        .advance_to(session, requested_time)
                        .map_err(js_error)?;
                    Ok(DirectDriveOutcome {
                        resumed: false,
                        completed_callback_phase: callbacks.last_advance_completed_callback_phase(),
                    })
                }
                DirectSourceAuthority::Program(program) => program.drive_to(requested_time),
            }
        }

        fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
            match &mut self.authority {
                DirectSourceAuthority::Session { session, .. } => {
                    session.take_renderer_publication()
                }
                DirectSourceAuthority::Program(program) => program.take_renderer_publication(),
            }
        }

        fn admit_rendered_publication(
            &mut self,
            publication: noon_core::PublicationContext,
        ) -> Result<bool, JsValue> {
            match &mut self.authority {
                DirectSourceAuthority::Session { .. } => Ok(false),
                DirectSourceAuthority::Program(program) => {
                    program.admit_rendered_publication(publication)
                }
            }
        }

        fn set_native_state_input(
            &mut self,
            source: NativeStateSource,
            value: NativeInputValue,
        ) -> Result<(), JsValue> {
            match &mut self.authority {
                DirectSourceAuthority::Session { session, .. } => session
                    .set_native_state_input(source, value)
                    .map(|_| ())
                    .map_err(js_error),
                DirectSourceAuthority::Program(program) => {
                    program.set_native_state_input(source, value)
                }
            }
        }

        fn emit_native_event(&mut self, source: NativeEventSource) -> Result<(), JsValue> {
            let sequence = self.next_native_event_sequence;
            let next = sequence
                .checked_add(1)
                .ok_or_else(|| js_message("native input event sequence exhausted"))?;
            let occurrence = NativeEventOccurrence::new(sequence, source);
            match &mut self.authority {
                DirectSourceAuthority::Session { session, .. } => session
                    .emit_native_event(occurrence)
                    .map(|_| ())
                    .map_err(js_error)?,
                DirectSourceAuthority::Program(program) => program.emit_native_event(occurrence)?,
            }
            self.next_native_event_sequence = next;
            Ok(())
        }
    }

    enum CanvasExecutionSource {
        Transport(ExecutionFrameMirror),
        Direct(DirectExecutionSource),
    }

    impl CanvasExecutionSource {
        fn frame(&self) -> Option<&FrameState> {
            match self {
                Self::Transport(mirror) => mirror.frame(),
                Self::Direct(direct) => Some(direct.session().frame()),
            }
        }

        fn live_object_count(&self) -> usize {
            match self {
                Self::Transport(mirror) => mirror.live_object_count(),
                Self::Direct(direct) => direct
                    .session()
                    .frame()
                    .presences
                    .iter()
                    .filter(|present| **present)
                    .count(),
            }
        }

        fn transport_mut(&mut self) -> Option<&mut ExecutionFrameMirror> {
            match self {
                Self::Transport(mirror) => Some(mirror),
                Self::Direct(_) => None,
            }
        }

        fn direct(&self) -> Option<&ExecutionSession> {
            match self {
                Self::Transport(_) => None,
                Self::Direct(direct) => Some(direct.session()),
            }
        }

        fn direct_mut(&mut self) -> Option<&mut ExecutionSession> {
            match self {
                Self::Transport(_) => None,
                Self::Direct(direct) => direct.session_mut(),
            }
        }

        fn direct_source_mut(&mut self) -> Option<&mut DirectExecutionSource> {
            match self {
                Self::Transport(_) => None,
                Self::Direct(direct) => Some(direct),
            }
        }
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
        source: CanvasExecutionSource,
        pending_changes: FrameChanges,
        direct_wake_clock: BrowserExecutionWakeClock,
        preparer: FramePreparer,
        direct_preparer: RetainedFramePreparer,
        renderer: GpuRenderer,
        direct_text_gpu: RetainedTextGpuState,
        timestamp_query_supported: bool,
        timestamp_profiler: Option<GpuTimestampProfiler>,
        camera_center: Vec2,
        camera_height: f32,
        clear_color: wgpu::Color,
        last_draw_calls: usize,
        last_text_draw_calls: usize,
        last_instances_drawn: usize,
        last_bytes_uploaded: usize,
        last_geometry_cache_misses: usize,
        gpu_generation: u32,
        gpu_diagnostics: GpuDiagnosticMailbox,
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
            Self::create_with_source(
                canvas,
                CanvasExecutionSource::Transport(mirror),
                pending_changes,
                camera.center,
                camera.height,
            )
            .await
        }

        #[wasm_bindgen(js_name = applyDeltaJson)]
        pub fn apply_delta_json(&mut self, json: &str) -> Result<bool, JsValue> {
            if !self.pending_changes.is_empty() {
                return Err(js_message(
                    "render worker must present the applied execution delta before accepting another",
                ));
            }
            let (outcome, changes, camera) = {
                let mirror = self.source.transport_mut().ok_or_else(|| {
                    js_message("direct Rust/WASM execution source does not accept transport deltas")
                })?;
                let (outcome, changes) = mirror.apply_json(json).map_err(js_error)?;
                (outcome, changes, mirror.camera())
            };
            match outcome {
                TransportApplyOutcome::Applied => {
                    self.sync_camera(camera)?;
                    self.pending_changes = changes;
                    Ok(true)
                }
                TransportApplyOutcome::DroppedStale => Ok(false),
            }
        }

        pub fn render(&mut self) -> Result<bool, JsValue> {
            let changes_pending = match &self.source {
                CanvasExecutionSource::Transport(_) => !self.pending_changes.is_empty(),
                CanvasExecutionSource::Direct(direct) => {
                    direct.session().wake_state().frame_pending()
                }
            };
            if !self.drawable || !changes_pending {
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
                    // the pending frame intact so a recoverable validation can be
                    // reported once and presentation retried unchanged.
                    wgpu::CurrentSurfaceTexture::Validation => return Ok(false),
                };

            if self.source.direct().is_some() {
                return self.render_direct(surface_texture, reconfigure_after_present);
            }

            let changes = mem::take(&mut self.pending_changes);
            let frame = self
                .source
                .frame()
                .ok_or_else(|| js_message("execution renderer has no frame snapshot"))?;
            let prepared = self.preparer.prepare_incremental(frame, &changes);
            self.last_geometry_cache_misses = prepared.stats.geometry_cache_misses;
            let upload = self.renderer.upload(&self.device, &self.queue, &prepared);
            self.last_bytes_uploaded = upload.bytes_uploaded;

            let view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Noon execution render worker frame"),
                });
            let timestamp_slot = self
                .timestamp_profiler
                .as_mut()
                .and_then(GpuTimestampProfiler::reserve_slot);
            let draw = if let Some(slot) = timestamp_slot {
                self.renderer.encode_profiled(
                    &mut encoder,
                    &view,
                    &prepared,
                    self.clear_color,
                    self.timestamp_profiler
                        .as_ref()
                        .expect("timestamp profiler reserved its own slot")
                        .query_set(slot),
                )
            } else {
                self.renderer
                    .encode(&mut encoder, &view, &prepared, self.clear_color)
            };
            if let Some(slot) = timestamp_slot {
                self.timestamp_profiler
                    .as_ref()
                    .expect("timestamp profiler reserved its own slot")
                    .resolve(&mut encoder, slot);
            }
            self.queue.submit(Some(encoder.finish()));
            if let Some(slot) = timestamp_slot {
                self.timestamp_profiler
                    .as_ref()
                    .expect("timestamp profiler reserved its own slot")
                    .map_after_submit(slot);
            }
            self.queue.present(surface_texture);
            self.last_draw_calls = draw.draw_calls;
            self.last_instances_drawn = draw.instances_drawn;
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

        /// Manual camera control remains as a low-level host API. Authoritative
        /// transport or direct-session camera updates overwrite it when published.
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

        /// Drain asynchronously completed WebGPU render-pass timestamps. The
        /// host never waits for a mapped buffer while presenting a frame.
        #[wasm_bindgen(js_name = takeGpuTimestampJson)]
        pub fn take_gpu_timestamp_json(&self) -> Result<String, JsValue> {
            let metrics = self
                .timestamp_profiler
                .as_ref()
                .map(GpuTimestampProfiler::take_metrics)
                .unwrap_or_default();
            serde_json::to_string(&metrics).map_err(js_error)
        }

        /// Timestamp diagnostics are opt-in so ordinary browser presentation
        /// never allocates or maps profiling buffers.
        #[wasm_bindgen(js_name = enableGpuTimestampProfiling)]
        pub fn enable_gpu_timestamp_profiling(&mut self, enabled: bool) -> bool {
            if !enabled {
                self.timestamp_profiler = None;
                return false;
            }
            if !self.timestamp_query_supported {
                return false;
            }
            if self.timestamp_profiler.is_none() {
                self.timestamp_profiler =
                    Some(GpuTimestampProfiler::new(&self.device, &self.queue));
            }
            true
        }

        /// Start a new measurement window only once preceding asynchronous
        /// readbacks have drained, keeping its counters frame-local.
        #[wasm_bindgen(js_name = resetGpuTimestampMetrics)]
        pub fn reset_gpu_timestamp_metrics(&self) -> bool {
            self.timestamp_profiler
                .as_ref()
                .is_some_and(GpuTimestampProfiler::reset)
        }

        pub fn time(&self) -> f64 {
            self.source.frame().map_or(0.0, |frame| frame.time)
        }

        /// Return one direct-session browser scheduling directive. JavaScript owns
        /// only the concrete RAF/timer handles; cadence, deadlines, and presentation
        /// dirtiness remain derived from the authoritative ExecutionSession.
        #[wasm_bindgen(js_name = directWakeDirectiveJson)]
        pub fn direct_wake_directive_json(&mut self, wall_time_ms: f64) -> Result<String, JsValue> {
            let (plan, scene_time) = self.direct_wake_observation()?;
            let directive = self
                .direct_wake_clock
                .directive(plan, wall_time_ms, scene_time)
                .ok_or_else(|| js_message("direct execution wake clock received invalid time"))?;
            let (cadence, delay_ms) = match directive.wake() {
                BrowserHostWake::AnimationFrame => ("animation-frame", None),
                BrowserHostWake::TimerAfterMilliseconds(delay_ms) => ("timer", Some(delay_ms)),
                BrowserHostWake::Idle => ("idle", None),
            };
            serde_json::to_string(&DirectWakeDirectiveJson {
                present_now: directive.present_now(),
                cadence,
                delay_ms,
            })
            .map_err(js_error)
        }

        /// Advance direct authored time from one browser monotonic callback timestamp.
        ///
        /// A RAF advances continuously. A deterministic timer advances only after its
        /// runtime-authored deadline is due. Idle observations never advance scene time.
        #[wasm_bindgen(js_name = advanceDirectRealtime)]
        pub fn advance_direct_realtime(&mut self, wall_time_ms: f64) -> Result<bool, JsValue> {
            let (plan, scene_time) = self.direct_wake_observation()?;
            let directive = self
                .direct_wake_clock
                .directive(plan, wall_time_ms, scene_time)
                .ok_or_else(|| js_message("direct execution wake clock received invalid time"))?;

            let target_time = match (directive.wake(), plan.cadence()) {
                (BrowserHostWake::AnimationFrame, BrowserExecutionCadence::AnimationFrame) => {
                    Some(self.direct_scene_time_at(wall_time_ms)?)
                }
                (
                    BrowserHostWake::TimerAfterMilliseconds(delay_ms),
                    BrowserExecutionCadence::TimerAtSceneTime(scene_deadline),
                ) if delay_ms <= 0.0 => {
                    Some(self.direct_scene_time_at(wall_time_ms)?.max(scene_deadline))
                }
                (BrowserHostWake::TimerAfterMilliseconds(_), _) | (BrowserHostWake::Idle, _) => {
                    None
                }
                _ => {
                    return Err(js_message(
                        "direct execution wake directive diverged from session cadence",
                    ));
                }
            };

            let Some(target_time) = target_time else {
                return Ok(false);
            };
            let (pending, camera, outcome) = {
                let direct = self.source.direct_source_mut().ok_or_else(|| {
                    js_message("direct realtime APIs require a direct ExecutionSession source")
                })?;
                let outcome = direct.drive_to(target_time)?;
                let camera = direct.session().camera().map_err(js_error)?;
                (
                    direct.session().wake_state().frame_pending(),
                    camera,
                    outcome,
                )
            };
            if outcome.resumed || outcome.completed_callback_phase {
                // The program has started a distinct canonical segment. Re-anchor
                // after that transition or a required callback phase so neither
                // continuation setup nor opaque host latency is charged to the
                // next authored interval.
                self.direct_wake_clock = BrowserExecutionWakeClock::default();
            }
            self.sync_camera(camera)?;

            let (next_plan, next_scene_time) = self.direct_wake_observation()?;
            if !outcome.completed_callback_phase {
                self.direct_wake_clock
                    .directive(next_plan, wall_time_ms, next_scene_time)
                    .ok_or_else(|| {
                        js_message("direct execution wake clock received invalid time")
                    })?;
            }
            Ok(pending)
        }

        /// Deliver a DOM-normalized pointer position through the current typed
        /// camera into the canonical session.
        #[wasm_bindgen(js_name = nativePointerPosition)]
        pub fn native_pointer_position(
            &mut self,
            normalized_x: f32,
            normalized_y: f32,
        ) -> Result<bool, JsValue> {
            let position = self.normalized_pointer_world_position(normalized_x, normalized_y)?;
            self.set_native_state_input(
                NativeStateSource::PointerPosition,
                NativeInputValue::Vec2(position),
            )
        }

        /// Deliver one pointer button sample followed by its ordered edge event.
        #[wasm_bindgen(js_name = nativePointerButton)]
        pub fn native_pointer_button(
            &mut self,
            button: u8,
            pressed: bool,
        ) -> Result<bool, JsValue> {
            self.apply_direct_native_input(move |direct| {
                direct.set_native_state_input(
                    NativeStateSource::PointerButton { button },
                    NativeInputValue::Bool(pressed),
                )?;
                direct.emit_native_event(if pressed {
                    NativeEventSource::PointerDown { button }
                } else {
                    NativeEventSource::PointerUp { button }
                })
            })
        }

        /// Deliver one keyboard state sample followed by its ordered edge event.
        #[wasm_bindgen(js_name = nativeKey)]
        pub fn native_key(&mut self, code: String, pressed: bool) -> Result<bool, JsValue> {
            validate_native_name("key code", &code)?;
            self.apply_direct_native_input(move |direct| {
                direct.set_native_state_input(
                    NativeStateSource::Key { code: code.clone() },
                    NativeInputValue::Bool(pressed),
                )?;
                direct.emit_native_event(if pressed {
                    NativeEventSource::KeyPress { code }
                } else {
                    NativeEventSource::KeyRelease { code }
                })
            })
        }

        /// Deliver one CSS-pixel wheel sample followed by its ordered event.
        #[wasm_bindgen(js_name = nativeWheel)]
        pub fn native_wheel(&mut self, x: f32, y: f32) -> Result<bool, JsValue> {
            self.apply_direct_native_input(move |direct| {
                direct.set_native_state_input(
                    NativeStateSource::WheelDelta,
                    NativeInputValue::Vec2(Vec2::new(x, y)),
                )?;
                direct.emit_native_event(NativeEventSource::Wheel)
            })
        }

        /// Deliver one named scalar control sample.
        #[wasm_bindgen(js_name = nativeControl)]
        pub fn native_control(&mut self, name: String, value: f32) -> Result<bool, JsValue> {
            validate_native_name("control name", &name)?;
            self.set_native_state_input(
                NativeStateSource::Control { name },
                NativeInputValue::Scalar(value),
            )
        }

        /// Deliver one ordered named-control commit event.
        #[wasm_bindgen(js_name = nativeControlCommit)]
        pub fn native_control_commit(&mut self, name: String) -> Result<bool, JsValue> {
            validate_native_name("control name", &name)?;
            self.emit_native_event(NativeEventSource::ControlCommit { name })
        }

        #[wasm_bindgen(js_name = objectCount)]
        pub fn object_count(&self) -> usize {
            self.source.live_object_count()
        }

        #[wasm_bindgen(js_name = lastDrawCalls)]
        pub fn last_draw_calls(&self) -> usize {
            self.last_draw_calls
        }

        /// Draw calls issued for text from the direct mixed renderer's last frame.
        #[wasm_bindgen(js_name = lastTextDrawCalls)]
        pub fn last_text_draw_calls(&self) -> usize {
            self.last_text_draw_calls
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

    impl WasmExecutionCanvasRenderer {
        /// Build the browser canvas host directly from the typed in-process execution session.
        ///
        /// This constructor is intentionally Rust-only: JavaScript may supply the canvas during
        /// WASM bootstrap, but no scene/execution document or transport mirror is introduced.
        pub async fn create_from_execution_session(
            canvas: OffscreenCanvas,
            session: ExecutionSession,
        ) -> Result<Self, JsValue> {
            Self::create_from_execution_session_with_callbacks(
                canvas,
                session,
                RustHostCallbackTable::new(),
            )
            .await
        }

        /// Build the browser canvas host from one typed session and its Rust callables.
        ///
        /// The callable table stays in the same WASM execution context. JavaScript
        /// receives neither callback requests nor effective-property overlays.
        pub async fn create_from_execution_session_with_callbacks(
            canvas: OffscreenCanvas,
            mut session: ExecutionSession,
            mut callbacks: RustHostCallbackTable,
        ) -> Result<Self, JsValue> {
            let initial_time = session.frame().time;
            callbacks
                .advance_to(&mut session, initial_time)
                .map_err(js_error)?;
            let camera = session.camera().map_err(js_error)?;
            Self::create_with_source(
                canvas,
                CanvasExecutionSource::Direct(DirectExecutionSource::from_session(
                    session, callbacks,
                )),
                FrameChanges::default(),
                camera.center,
                camera.height,
            )
            .await
        }

        /// Build the browser canvas host from one shared Rust continuation program.
        ///
        /// The program owns its consumed semantic scene, matching execution session,
        /// continuation state, completion, and resume gates. JavaScript receives only
        /// the normal canvas renderer and its derived RAF/timer directives.
        pub async fn create_from_live_program<C>(
            canvas: OffscreenCanvas,
            program: LiveProgram<C>,
        ) -> Result<Self, JsValue>
        where
            C: LiveContinuation + 'static,
            C::Error: std::fmt::Display,
        {
            Self::create_from_live_program_with_callbacks(
                canvas,
                program,
                RustHostCallbackTable::new(),
            )
            .await
        }

        /// Build the browser canvas host from one shared Rust continuation and
        /// its target-native callable table.
        ///
        /// Callback phases are selected and committed by the program's execution
        /// session in the same WASM context. JavaScript receives only the canvas
        /// renderer and its derived wake directives.
        pub async fn create_from_live_program_with_callbacks<C>(
            canvas: OffscreenCanvas,
            program: LiveProgram<C>,
            callbacks: RustHostCallbackTable,
        ) -> Result<Self, JsValue>
        where
            C: LiveContinuation + 'static,
            C::Error: std::fmt::Display,
        {
            let source =
                DirectExecutionSource::from_live_program_with_callbacks(program, callbacks)?;
            let camera = source.session().camera().map_err(js_error)?;
            Self::create_with_source(
                canvas,
                CanvasExecutionSource::Direct(source),
                FrameChanges::default(),
                camera.center,
                camera.height,
            )
            .await
        }

        /// Evaluate a direct Rust/WASM execution session and publish only its runtime changes.
        pub fn evaluate(&mut self, time: f64) -> Result<bool, JsValue> {
            self.ensure_direct_source_idle()?;
            let (pending, camera) = {
                let session = self.source.direct_mut().ok_or_else(|| {
                    js_message("typed execution APIs require a direct session source")
                })?;
                if session.has_required_callbacks() {
                    return Err(js_message(
                        "opaque host callback sessions require monotonic realtime advancement",
                    ));
                }
                session.evaluate(time).map_err(js_error)?;
                let camera = session.camera().map_err(js_error)?;
                (session.wake_state().frame_pending(), camera)
            };
            self.sync_camera(camera)?;
            self.direct_wake_clock = BrowserExecutionWakeClock::default();
            Ok(pending)
        }

        /// Seek a direct Rust/WASM execution session and publish its renderer-facing changes.
        pub fn seek(&mut self, time: f64) -> Result<bool, JsValue> {
            self.ensure_direct_source_idle()?;
            let (pending, camera) = {
                let session = self.source.direct_mut().ok_or_else(|| {
                    js_message("typed execution APIs require a direct session source")
                })?;
                if session.has_required_callbacks() {
                    return Err(js_message(
                        "opaque host callback sessions do not support seek or replay",
                    ));
                }
                session.seek(time).map_err(js_error)?;
                let camera = session.camera().map_err(js_error)?;
                (session.wake_state().frame_pending(), camera)
            };
            self.sync_camera(camera)?;
            self.direct_wake_clock = BrowserExecutionWakeClock::default();
            Ok(pending)
        }

        /// Apply one semantic native-reactive input without exposing the execution VM signal ID.
        pub fn set_reactive_input(
            &mut self,
            signal: SemanticNodeId,
            value: impl Into<ReactiveValue>,
        ) -> Result<bool, JsValue> {
            self.ensure_direct_source_idle()?;
            let (pending, camera) = {
                let session = self.source.direct_mut().ok_or_else(|| {
                    js_message("typed execution APIs require a direct session source")
                })?;
                session
                    .set_reactive_input(signal, value)
                    .map_err(js_error)?;
                let camera = session.camera().map_err(js_error)?;
                (session.wake_state().frame_pending(), camera)
            };
            self.sync_camera(camera)?;
            Ok(pending)
        }

        /// Deliver one typed native state sample to this direct Rust/WASM session.
        ///
        /// Unlike timeline advancement, native input may accumulate while a
        /// renderer publication is pending. `ExecutionSession` unions those
        /// local changes and the next `render` consumes them once, matching the
        /// native host's pointer-state plus button-event delivery without a
        /// browser-owned queue or runtime mirror.
        pub fn set_native_state_input(
            &mut self,
            source: NativeStateSource,
            value: NativeInputValue,
        ) -> Result<bool, JsValue> {
            self.apply_direct_native_input(move |direct| {
                direct.set_native_state_input(source, value)
            })
        }

        /// Deliver one typed, ordered native event to this direct Rust/WASM session.
        ///
        /// The canvas host allocates only an occurrence serial. The semantic
        /// source routing and event-counter update remain session-owned, and a
        /// rejected occurrence does not consume the serial.
        pub fn emit_native_event(&mut self, source: NativeEventSource) -> Result<bool, JsValue> {
            self.apply_direct_native_input(move |direct| direct.emit_native_event(source))
        }

        fn apply_direct_native_input(
            &mut self,
            apply: impl FnOnce(&mut DirectExecutionSource) -> Result<(), JsValue>,
        ) -> Result<bool, JsValue> {
            let (pending, camera) = {
                let direct = self.source.direct_source_mut().ok_or_else(|| {
                    js_message("typed native input requires a direct ExecutionSession source")
                })?;
                apply(direct)?;
                let camera = direct.session().camera().map_err(js_error)?;
                (direct.session().wake_state().frame_pending(), camera)
            };
            self.sync_camera(camera)?;
            Ok(pending)
        }

        fn normalized_pointer_world_position(
            &self,
            normalized_x: f32,
            normalized_y: f32,
        ) -> Result<Vec2, JsValue> {
            if !normalized_x.is_finite() || !normalized_y.is_finite() {
                return Err(js_message("normalized pointer coordinates must be finite"));
            }
            let x = normalized_x.clamp(0.0, 1.0);
            let y = normalized_y.clamp(0.0, 1.0);
            let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
            let world_width = self.camera_height * aspect;
            Ok(Vec2::new(
                self.camera_center.x + (x - 0.5) * world_width,
                self.camera_center.y + (0.5 - y) * self.camera_height,
            ))
        }

        fn ensure_direct_source_idle(&self) -> Result<(), JsValue> {
            let session = self.source.direct().ok_or_else(|| {
                js_message("typed execution APIs require a direct ExecutionSession source")
            })?;
            if session.wake_state().frame_pending() {
                return Err(js_message(
                    "direct execution host must present pending runtime changes before advancing again",
                ));
            }
            Ok(())
        }

        fn direct_wake_observation(&self) -> Result<(BrowserExecutionWakePlan, f64), JsValue> {
            let direct = match &self.source {
                CanvasExecutionSource::Transport(_) => {
                    return Err(js_message(
                        "direct wake APIs require a direct ExecutionSession source",
                    ));
                }
                CanvasExecutionSource::Direct(direct) => direct,
            };
            Ok((direct.wake_plan(), direct.session().frame().time))
        }

        fn direct_scene_time_at(&self, wall_time_ms: f64) -> Result<f64, JsValue> {
            self.direct_wake_clock
                .scene_time_at(wall_time_ms)
                .ok_or_else(|| js_message("direct execution wake clock has no active time anchor"))
        }

        async fn create_with_source(
            canvas: OffscreenCanvas,
            source: CanvasExecutionSource,
            pending_changes: FrameChanges,
            camera_center: Vec2,
            camera_height: f32,
        ) -> Result<Self, JsValue> {
            let width = canvas.width().max(1);
            let height = canvas.height().max(1);
            let InitializedGpu {
                instance,
                surface,
                device,
                queue,
                backend,
                config,
                timestamp_query_supported,
            } = initialize_gpu(&canvas, width, height).await?;
            let gpu_generation = 1;
            let gpu_diagnostics = GpuDiagnosticMailbox::default();
            install_wgpu_error_handler(&device, gpu_generation, backend, gpu_diagnostics.clone());
            let renderer = GpuRenderer::new(&device, config.format);
            let direct_text_gpu = renderer.create_retained_text_state(&device, &queue);

            let mut result = Self {
                instance,
                surface,
                device,
                queue,
                backend,
                canvas,
                config,
                drawable: true,
                source,
                pending_changes,
                direct_wake_clock: BrowserExecutionWakeClock::default(),
                preparer: FramePreparer::new(),
                direct_preparer: RetainedFramePreparer::new(),
                renderer,
                direct_text_gpu,
                timestamp_query_supported,
                timestamp_profiler: None,
                camera_center,
                camera_height,
                clear_color: MANIM_DEFAULT_CLEAR_COLOR,
                last_draw_calls: 0,
                last_text_draw_calls: 0,
                last_instances_drawn: 0,
                last_bytes_uploaded: 0,
                last_geometry_cache_misses: 0,
                gpu_generation,
                gpu_diagnostics,
            };
            result.update_camera()?;
            Ok(result)
        }

        fn render_direct(
            &mut self,
            surface_texture: wgpu::SurfaceTexture,
            reconfigure_after_present: bool,
        ) -> Result<bool, JsValue> {
            let metrics = {
                let camera = self.renderer.camera();
                TextDeviceMetrics::new(Vec2::new(
                    self.config.width as f32 / camera.world_size.x,
                    self.config.height as f32 / camera.world_size.y,
                ))
                .map_err(js_error)?
            };
            let camera = self.renderer.camera();
            let half_extent = camera.world_size * 0.5;
            let publication_context;
            let draw = {
                let direct = self.source.direct_source_mut().ok_or_else(|| {
                    js_message(
                        "direct retained rendering requires a direct ExecutionSession source",
                    )
                })?;
                let visibility = direct.query_viewport(Rect::new(
                    camera.center - half_extent,
                    camera.center + half_extent,
                ));
                let publication = direct.take_renderer_publication();
                publication_context = publication.context();
                let prepared = self
                    .direct_preparer
                    .prepare_publication_visible(
                        &self.device,
                        &self.queue,
                        &publication,
                        visibility.object_indices(),
                        metrics,
                    )
                    .map_err(js_error)?;
                let upload = self.renderer.upload_retained(
                    &self.device,
                    &self.queue,
                    &prepared,
                    &mut self.direct_text_gpu,
                );
                self.last_geometry_cache_misses = prepared.geometry_stats().geometry_cache_misses;
                self.last_bytes_uploaded = upload
                    .geometry
                    .bytes_uploaded
                    .saturating_add(upload.text.bytes_uploaded);

                let view = surface_texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Noon direct execution render frame"),
                        });
                let draw = self
                    .renderer
                    .encode_retained(
                        &mut encoder,
                        &view,
                        &prepared,
                        &self.direct_text_gpu,
                        self.clear_color,
                    )
                    .map_err(js_error)?;
                self.queue.submit(Some(encoder.finish()));
                draw
            };
            self.queue.present(surface_texture);
            self.last_draw_calls = draw
                .geometry
                .draw_calls
                .saturating_add(draw.text.draw_calls);
            self.last_text_draw_calls = draw.text.draw_calls;
            self.last_instances_drawn = draw
                .geometry
                .instances_drawn
                .saturating_add(draw.text.instances_drawn);
            if reconfigure_after_present {
                self.surface.configure(&self.device, &self.config);
            }
            let resumed = self
                .source
                .direct_source_mut()
                .expect("direct renderer source was checked before rendering")
                .admit_rendered_publication(publication_context)?;
            if resumed {
                // Endpoint admission is the only place presentation may release a
                // continuation. Its next wake must start from that host boundary.
                self.direct_wake_clock = BrowserExecutionWakeClock::default();
            }
            Ok(true)
        }

        fn sync_camera(&mut self, camera: Camera2DState) -> Result<(), JsValue> {
            if camera.center == self.camera_center && camera.height == self.camera_height {
                return Ok(());
            }
            self.camera_center = camera.center;
            self.camera_height = camera.height;
            self.update_camera()
        }

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
                apply_limit_buckets: false,
            })
            .await
            .map_err(|error| error.to_string())?;
        let backend = adapter.get_info().backend;
        let (device, queue, timestamp_query_supported) = request_device(&adapter, backend).await?;

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
            timestamp_query_supported,
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
                apply_limit_buckets: false,
            })
            .await
            .map_err(|error| error.to_string())?;
        let backend = adapter.get_info().backend;
        let (device, queue, timestamp_query_supported) = request_device(&adapter, backend).await?;
        let config = default_surface_config(&surface, &adapter, width, height)?;
        surface.configure(&device, &config);
        Ok(InitializedGpu {
            instance,
            surface,
            device,
            queue,
            backend,
            config,
            timestamp_query_supported,
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
    ) -> Result<(wgpu::Device, wgpu::Queue, bool), String> {
        let required_limits = if backend == wgpu::Backend::Gl {
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
        } else {
            wgpu::Limits::default()
        };
        let timestamp_query_supported = backend == wgpu::Backend::BrowserWebGpu
            && adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let required_features = if timestamp_query_supported {
            wgpu::Features::TIMESTAMP_QUERY
        } else {
            wgpu::Features::empty()
        };
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Noon execution render worker GPU device"),
                required_features,
                required_limits,
                ..Default::default()
            })
            .await
            .map(|(device, queue)| (device, queue, timestamp_query_supported))
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

    fn validate_native_name(kind: &str, value: &str) -> Result<(), JsValue> {
        if value.trim().is_empty() {
            return Err(js_message(&format!("{kind} must be a non-empty string")));
        }
        Ok(())
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
