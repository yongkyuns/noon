use noon_core::{
    NativeEventSource, NativeStateSource, ReactiveValue, SignalId, TimedSemanticScene,
};
use noon_ir::{decode_timed_semantic_scene, encode_timed_semantic_scene, TimedSemanticIrError};
use noon_runtime::{
    FrameChanges, FrameState, NativeInputRouter, NativeInputStats, TimedSceneInstance,
    TimedSceneRuntimeError,
};

#[derive(Debug)]
pub enum TimedPlayerError {
    Ir(TimedSemanticIrError),
    Runtime(TimedSceneRuntimeError),
}

impl std::fmt::Display for TimedPlayerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ir(error) => error.fmt(formatter),
            Self::Runtime(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TimedPlayerError {}

impl From<TimedSemanticIrError> for TimedPlayerError {
    fn from(value: TimedSemanticIrError) -> Self {
        Self::Ir(value)
    }
}

impl From<TimedSceneRuntimeError> for TimedPlayerError {
    fn from(value: TimedSceneRuntimeError) -> Self {
        Self::Runtime(value)
    }
}

#[derive(Clone, Debug)]
pub struct TimedScenePlayer {
    scene: TimedSemanticScene,
    instance: TimedSceneInstance,
    native_inputs: NativeInputRouter,
}

impl TimedScenePlayer {
    pub fn from_scene_json(json: &str) -> Result<Self, TimedPlayerError> {
        let scene = decode_timed_semantic_scene(json)?;
        let instance = TimedSceneInstance::from_timed(&scene)?;
        let native_inputs =
            NativeInputRouter::from_scene(&scene).map_err(TimedSceneRuntimeError::from)?;
        Ok(Self {
            scene,
            instance,
            native_inputs,
        })
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, TimedPlayerError> {
        Ok(self.instance.seek(time)?)
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, TimedPlayerError> {
        Ok(self.instance.advance_to(time)?)
    }

    pub fn set_reactive_input(
        &mut self,
        signal: SignalId,
        value: impl Into<ReactiveValue>,
    ) -> Result<&FrameState, TimedPlayerError> {
        Ok(self.instance.set_reactive_input(signal, value)?)
    }

    pub fn has_native_state_source(&self, source: &NativeStateSource) -> bool {
        self.native_inputs.has_state_source(source)
    }

    pub fn has_native_event_source(&self, source: &NativeEventSource) -> bool {
        self.native_inputs.has_event_source(source)
    }

    pub fn dispatch_native_state(
        &mut self,
        source: &NativeStateSource,
        value: impl Into<ReactiveValue>,
    ) -> Result<bool, TimedPlayerError> {
        let Self {
            instance,
            native_inputs,
            ..
        } = self;
        Ok(native_inputs.dispatch_state(instance, source, value)?)
    }

    pub fn emit_native_event(
        &mut self,
        source: &NativeEventSource,
    ) -> Result<bool, TimedPlayerError> {
        let Self {
            instance,
            native_inputs,
            ..
        } = self;
        Ok(native_inputs.emit_event(instance, source)?)
    }

    pub const fn native_input_stats(&self) -> NativeInputStats {
        self.native_inputs.stats()
    }

    pub fn reset_native_input_stats(&mut self) {
        self.native_inputs.reset_stats();
    }

    pub fn frame(&self) -> &FrameState {
        self.instance.frame()
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        self.instance.take_frame_changes()
    }

    pub fn object_count(&self) -> usize {
        self.instance.frame().objects.len()
    }

    pub fn scene_json(&self) -> Result<String, TimedPlayerError> {
        Ok(encode_timed_semantic_scene(&self.scene)?)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use noon_core::{NativeEventSource, NativeStateSource, SignalId, Vec2};
    use noon_render_wgpu::{Camera2D, FramePreparer, GpuRenderer};
    use wasm_bindgen::prelude::*;
    use web_sys::HtmlCanvasElement;

    use super::TimedScenePlayer;
    use crate::PlaybackClock;

    #[derive(Debug)]
    struct WebDisplaySource;

    impl wgpu::rwh::HasDisplayHandle for WebDisplaySource {
        fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
            Ok(wgpu::rwh::DisplayHandle::web())
        }
    }

    #[wasm_bindgen(js_name = ReactiveScenePlayer)]
    pub struct WasmReactiveScenePlayer {
        inner: TimedScenePlayer,
    }

    #[wasm_bindgen(js_class = ReactiveScenePlayer)]
    impl WasmReactiveScenePlayer {
        #[wasm_bindgen(constructor)]
        pub fn new(scene_json: &str) -> Result<WasmReactiveScenePlayer, JsValue> {
            Ok(Self {
                inner: TimedScenePlayer::from_scene_json(scene_json).map_err(js_error)?,
            })
        }

        pub fn seek(&mut self, time: f64) -> Result<(), JsValue> {
            self.inner.seek(time).map_err(js_error)?;
            Ok(())
        }

        #[wasm_bindgen(js_name = setReactiveInput)]
        pub fn set_reactive_input(&mut self, signal: u32, value: f32) -> Result<(), JsValue> {
            self.inner
                .set_reactive_input(SignalId::new(u64::from(signal)), value)
                .map_err(js_error)?;
            Ok(())
        }

        pub fn time(&self) -> f64 {
            self.inner.frame().time
        }

        #[wasm_bindgen(js_name = objectCount)]
        pub fn object_count(&self) -> usize {
            self.inner.object_count()
        }

        #[wasm_bindgen(js_name = sceneJson)]
        pub fn scene_json(&self) -> Result<String, JsValue> {
            self.inner.scene_json().map_err(js_error)
        }
    }

    /// GPU canvas host for semantic scenes with native reactive inputs and signal tracks.
    ///
    /// Native browser input dispatch mutates only declared reactive input signals. Rendering
    /// remains on the normal animation/presentation loop, so event handlers do not encode or
    /// submit GPU frames and remain compatible with the future engine/render-worker split.
    #[wasm_bindgen(js_name = ReactiveCanvasPlayer)]
    pub struct WasmReactiveCanvasPlayer {
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        backend: wgpu::Backend,
        canvas: HtmlCanvasElement,
        config: wgpu::SurfaceConfiguration,
        drawable: bool,
        player: TimedScenePlayer,
        clock: PlaybackClock,
        preparer: FramePreparer,
        renderer: GpuRenderer,
        camera_center: Vec2,
        camera_height: f32,
        clear_color: wgpu::Color,
    }

    #[wasm_bindgen(js_class = ReactiveCanvasPlayer)]
    impl WasmReactiveCanvasPlayer {
        #[wasm_bindgen(js_name = create)]
        pub async fn create(
            canvas: HtmlCanvasElement,
            scene_json: &str,
            loop_duration_seconds: f64,
        ) -> Result<WasmReactiveCanvasPlayer, JsValue> {
            let player = TimedScenePlayer::from_scene_json(scene_json).map_err(js_error)?;
            let clock = PlaybackClock::looping(loop_duration_seconds).map_err(js_error)?;

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
                    label: Some("Noon reactive browser GPU device"),
                    required_features: wgpu::Features::empty(),
                    required_limits,
                    ..Default::default()
                })
                .await
                .map_err(js_error)?;

            let width = canvas.width().max(1);
            let height = canvas.height().max(1);
            let config = surface
                .get_default_config(&adapter, width, height)
                .ok_or_else(|| js_message("GPU adapter cannot present to this canvas"))?;
            surface.configure(&device, &config);
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
                player,
                clock,
                preparer: FramePreparer::new(),
                renderer,
                camera_center: Vec2::ZERO,
                camera_height: noon_core::DEFAULT_FRAME_HEIGHT,
                clear_color: wgpu::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            };
            result.update_camera()?;
            result.dispatch_viewport_size(width as f32, height as f32)?;
            Ok(result)
        }

        pub fn resize(&mut self, width: u32, height: u32) -> Result<(), JsValue> {
            self.canvas.set_width(width);
            self.canvas.set_height(height);
            self.drawable = width > 0 && height > 0;
            self.dispatch_viewport_size(width as f32, height as f32)?;
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

        #[wasm_bindgen(js_name = renderFrame)]
        pub fn render_frame(&mut self, timestamp_ms: f64) -> Result<bool, JsValue> {
            let scene_time = self.clock.scene_time(timestamp_ms).map_err(js_error)?;
            self.player.advance_to(scene_time).map_err(js_error)?;
            self.render_current_frame()
        }

        pub fn seek(&mut self, time: f64) -> Result<bool, JsValue> {
            self.player.seek(time).map_err(js_error)?;
            self.render_current_frame()
        }

        #[wasm_bindgen(js_name = setReactiveInput)]
        pub fn set_reactive_input(&mut self, signal: u32, value: f32) -> Result<(), JsValue> {
            self.player
                .set_reactive_input(SignalId::new(u64::from(signal)), value)
                .map_err(js_error)?;
            Ok(())
        }

        #[wasm_bindgen(js_name = dispatchPointerPosition)]
        pub fn dispatch_pointer_position(
            &mut self,
            normalized_x: f32,
            normalized_y: f32,
        ) -> Result<bool, JsValue> {
            if !normalized_x.is_finite() || !normalized_y.is_finite() {
                return Err(js_message("pointer coordinates must be finite"));
            }
            let source = NativeStateSource::PointerPosition;
            if !self.player.has_native_state_source(&source) {
                return Ok(false);
            }
            let world = self.pointer_world_position(normalized_x, normalized_y);
            self.player
                .dispatch_native_state(&source, world)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = dispatchPointerButton)]
        pub fn dispatch_pointer_button(
            &mut self,
            button: u8,
            pressed: bool,
        ) -> Result<bool, JsValue> {
            let state_source = NativeStateSource::PointerButton { button };
            let event_source = if pressed {
                NativeEventSource::PointerDown { button }
            } else {
                NativeEventSource::PointerUp { button }
            };
            let mut handled = false;
            if self.player.has_native_state_source(&state_source) {
                self.player
                    .dispatch_native_state(&state_source, pressed)
                    .map_err(js_error)?;
                handled = true;
            }
            if self.player.has_native_event_source(&event_source) {
                self.player
                    .emit_native_event(&event_source)
                    .map_err(js_error)?;
                handled = true;
            }
            Ok(handled)
        }

        #[wasm_bindgen(js_name = dispatchKey)]
        pub fn dispatch_key(&mut self, code: String, pressed: bool) -> Result<bool, JsValue> {
            validate_name("key code", &code)?;
            let state_source = NativeStateSource::Key { code: code.clone() };
            let event_source = if pressed {
                NativeEventSource::KeyPress { code }
            } else {
                NativeEventSource::KeyRelease { code }
            };
            let mut handled = false;
            if self.player.has_native_state_source(&state_source) {
                self.player
                    .dispatch_native_state(&state_source, pressed)
                    .map_err(js_error)?;
                handled = true;
            }
            if self.player.has_native_event_source(&event_source) {
                self.player
                    .emit_native_event(&event_source)
                    .map_err(js_error)?;
                handled = true;
            }
            Ok(handled)
        }

        #[wasm_bindgen(js_name = dispatchWheel)]
        pub fn dispatch_wheel(&mut self, delta_x: f32, delta_y: f32) -> Result<bool, JsValue> {
            let delta = finite_vec2("wheel delta", delta_x, delta_y)?;
            let state_source = NativeStateSource::WheelDelta;
            let event_source = NativeEventSource::Wheel;
            let mut handled = false;
            if self.player.has_native_state_source(&state_source) {
                self.player
                    .dispatch_native_state(&state_source, delta)
                    .map_err(js_error)?;
                handled = true;
            }
            if self.player.has_native_event_source(&event_source) {
                self.player
                    .emit_native_event(&event_source)
                    .map_err(js_error)?;
                handled = true;
            }
            Ok(handled)
        }

        #[wasm_bindgen(js_name = dispatchGesture)]
        pub fn dispatch_gesture(
            &mut self,
            name: String,
            delta_x: f32,
            delta_y: f32,
        ) -> Result<bool, JsValue> {
            validate_name("gesture name", &name)?;
            let delta = finite_vec2("gesture delta", delta_x, delta_y)?;
            let state_source = NativeStateSource::GestureDelta { name: name.clone() };
            let event_source = NativeEventSource::Gesture { name };
            let mut handled = false;
            if self.player.has_native_state_source(&state_source) {
                self.player
                    .dispatch_native_state(&state_source, delta)
                    .map_err(js_error)?;
                handled = true;
            }
            if self.player.has_native_event_source(&event_source) {
                self.player
                    .emit_native_event(&event_source)
                    .map_err(js_error)?;
                handled = true;
            }
            Ok(handled)
        }

        #[wasm_bindgen(js_name = dispatchControl)]
        pub fn dispatch_control(&mut self, name: String, value: f32) -> Result<bool, JsValue> {
            validate_name("control name", &name)?;
            if !value.is_finite() {
                return Err(js_message("control value must be finite"));
            }
            let source = NativeStateSource::Control { name };
            if !self.player.has_native_state_source(&source) {
                return Ok(false);
            }
            self.player
                .dispatch_native_state(&source, value)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = dispatchControlCommit)]
        pub fn dispatch_control_commit(&mut self, name: String) -> Result<bool, JsValue> {
            validate_name("control name", &name)?;
            let source = NativeEventSource::ControlCommit { name };
            if !self.player.has_native_event_source(&source) {
                return Ok(false);
            }
            self.player.emit_native_event(&source).map_err(js_error)
        }

        #[wasm_bindgen(js_name = nativeInputStatsJson)]
        pub fn native_input_stats_json(&self) -> String {
            let stats = self.player.native_input_stats();
            format!(
                "{{\"state_samples_received\":{},\"state_samples_coalesced\":{},\"state_dispatches_dropped\":{},\"events_received\":{},\"event_dispatches_dropped\":{},\"reactive_updates\":{},\"derived_signals_evaluated\":{},\"bindings_invalidated\":{}}}",
                stats.state_samples_received,
                stats.state_samples_coalesced,
                stats.state_dispatches_dropped,
                stats.events_received,
                stats.event_dispatches_dropped,
                stats.reactive_updates,
                stats.derived_signals_evaluated,
                stats.bindings_invalidated,
            )
        }

        #[wasm_bindgen(js_name = resetNativeInputStats)]
        pub fn reset_native_input_stats(&mut self) {
            self.player.reset_native_input_stats();
        }

        #[wasm_bindgen(js_name = resetClock)]
        pub fn reset_clock(&mut self) {
            self.clock.reset();
        }

        pub fn time(&self) -> f64 {
            self.player.frame().time
        }

        #[wasm_bindgen(js_name = objectCount)]
        pub fn object_count(&self) -> usize {
            self.player.object_count()
        }

        #[wasm_bindgen(js_name = rendererBackend)]
        pub fn renderer_backend(&self) -> String {
            match self.backend {
                wgpu::Backend::BrowserWebGpu => "WebGPU".to_owned(),
                wgpu::Backend::Gl => "WebGL2".to_owned(),
                other => format!("{other:?}"),
            }
        }
    }

    impl WasmReactiveCanvasPlayer {
        fn dispatch_viewport_size(&mut self, width: f32, height: f32) -> Result<(), JsValue> {
            let source = NativeStateSource::ViewportSize;
            if self.player.has_native_state_source(&source) {
                self.player
                    .dispatch_native_state(&source, finite_vec2("viewport size", width, height)?)
                    .map_err(js_error)?;
            }
            Ok(())
        }

        fn pointer_world_position(&self, normalized_x: f32, normalized_y: f32) -> Vec2 {
            let x = normalized_x.clamp(0.0, 1.0);
            let y = normalized_y.clamp(0.0, 1.0);
            let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
            let world_width = self.camera_height * aspect;
            Vec2::new(
                self.camera_center.x + (x - 0.5) * world_width,
                self.camera_center.y + (0.5 - y) * self.camera_height,
            )
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

        fn render_current_frame(&mut self) -> Result<bool, JsValue> {
            if !self.drawable {
                return Ok(false);
            }
            let changes = self.player.take_frame_changes();
            let prepared = self
                .preparer
                .prepare_incremental(self.player.frame(), &changes);
            self.renderer.upload(&self.device, &self.queue, &prepared);

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
                    wgpu::CurrentSurfaceTexture::Validation => {
                        return Err(js_message(
                            "GPU backend rejected the reactive canvas surface texture",
                        ));
                    }
                };
            let view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Noon reactive browser frame"),
                });
            self.renderer
                .encode(&mut encoder, &view, &prepared, self.clear_color);
            self.queue.submit(std::iter::once(encoder.finish()));
            surface_texture.present();
            if reconfigure_after_present {
                self.surface.configure(&self.device, &self.config);
            }
            Ok(true)
        }
    }

    fn finite_vec2(name: &str, x: f32, y: f32) -> Result<Vec2, JsValue> {
        if x.is_finite() && y.is_finite() {
            Ok(Vec2::new(x, y))
        } else {
            Err(js_message(&format!("{name} must be finite")))
        }
    }

    fn validate_name(name: &str, value: &str) -> Result<(), JsValue> {
        if value.trim().is_empty() {
            Err(js_message(&format!("{name} must not be empty")))
        } else {
            Ok(())
        }
    }

    fn create_surface(
        instance: &wgpu::Instance,
        canvas: &HtmlCanvasElement,
    ) -> Result<wgpu::Surface<'static>, JsValue> {
        instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(js_error)
    }

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    fn js_message(message: &str) -> JsValue {
        JsValue::from_str(message)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, NativeEventSource, NativeInputDefinition, NativeStateSource, Property,
        RateFunction, SignalTimelineDefinition, TimedSemanticScene, TrackTiming, Vec2,
    };
    use noon_ir::encode_timed_semantic_scene;

    use super::*;

    #[test]
    fn timed_player_loads_signal_tracks_and_accepts_untracked_live_inputs() {
        let mut semantic = noon_core::SemanticScene::new();
        let object = semantic.add(GeometryRef::circle(0.5));
        let animated = semantic.add_input(0.0_f32);
        semantic.bind(animated, object, Property::Rotation);
        let live = semantic.add_input(1.0_f32);
        semantic.bind(live, object, Property::Opacity);
        let mut timeline = SignalTimelineDefinition::new();
        timeline
            .add_scalar_track(
                semantic.reactive(),
                animated,
                0.0,
                2.0,
                TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            )
            .unwrap();
        let scene = TimedSemanticScene::from_parts(semantic, timeline).unwrap();
        let json = encode_timed_semantic_scene(&scene).unwrap();

        let mut player = TimedScenePlayer::from_scene_json(&json).unwrap();
        player.seek(1.0).unwrap();
        assert_eq!(player.frame().objects[0].transform.rotation, 1.0);
        player.set_reactive_input(live, 0.4_f32).unwrap();
        assert_eq!(player.frame().objects[0].style.opacity, 0.4);
        assert!(player.set_reactive_input(animated, 0.5_f32).is_err());
    }

    #[test]
    fn timed_player_dispatches_native_inputs_by_semantic_source() {
        let mut semantic = noon_core::SemanticScene::new();
        let object = semantic.add(GeometryRef::circle(0.5));
        let pointer = semantic.add_input(Vec2::ZERO);
        semantic.bind(pointer, object, Property::Position);
        let clicks = semantic.add_input(0.0_f32);
        semantic.bind(clicks, object, Property::Rotation);
        let mut inputs = NativeInputDefinition::new();
        inputs
            .bind_state(NativeStateSource::PointerPosition, pointer)
            .bind_event(NativeEventSource::PointerDown { button: 0 }, clicks);
        let scene = TimedSemanticScene::from_parts_with_native_inputs(
            semantic,
            SignalTimelineDefinition::new(),
            inputs,
        )
        .unwrap();
        let json = encode_timed_semantic_scene(&scene).unwrap();
        let mut player = TimedScenePlayer::from_scene_json(&json).unwrap();

        player
            .dispatch_native_state(&NativeStateSource::PointerPosition, Vec2::new(1.5, -0.5))
            .unwrap();
        player
            .emit_native_event(&NativeEventSource::PointerDown { button: 0 })
            .unwrap();
        assert_eq!(
            player.frame().objects[0].transform.translation,
            Vec2::new(1.5, -0.5)
        );
        assert_eq!(player.frame().objects[0].transform.rotation, 1.0);
        assert_eq!(player.native_input_stats().state_samples_received, 1);
        assert_eq!(player.native_input_stats().events_received, 1);
    }
}
