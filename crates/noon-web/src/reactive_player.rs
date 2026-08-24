use noon_core::{ReactiveValue, SignalId, TimedSemanticScene};
use noon_ir::{decode_timed_semantic_scene, encode_timed_semantic_scene, TimedSemanticIrError};
use noon_runtime::{FrameChanges, FrameState, TimedSceneInstance, TimedSceneRuntimeError};

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
}

impl TimedScenePlayer {
    pub fn from_scene_json(json: &str) -> Result<Self, TimedPlayerError> {
        let scene = decode_timed_semantic_scene(json)?;
        let instance = TimedSceneInstance::from_timed(&scene)?;
        Ok(Self { scene, instance })
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

    pub fn frame(&self) -> &FrameState {
        self.instance.frame()
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        self.instance.take_frame_changes()
    }

    pub fn object_count(&self) -> usize {
        self.instance.frame().live_object_count()
    }

    pub fn scene_json(&self) -> Result<String, TimedPlayerError> {
        Ok(encode_timed_semantic_scene(&self.scene)?)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use noon_core::{SignalId, Vec2};
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
    /// This intentionally keeps the same rendering architecture as `NoonCanvasPlayer` while
    /// leaving legacy patch/reconcile and profiling APIs on that class until those operations
    /// become reactive-graph-aware.
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
                camera_height: 6.0,
                clear_color: wgpu::Color {
                    r: 0.035,
                    g: 0.047,
                    b: 0.075,
                    a: 1.0,
                },
            };
            result.update_camera()?;
            Ok(result)
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
        GeometryRef, Property, RateFunction, SignalTimelineDefinition, TimedSemanticScene,
        TrackTiming,
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
}
