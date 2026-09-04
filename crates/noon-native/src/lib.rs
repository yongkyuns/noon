//! Native window lifecycle for Noon's typed in-process execution path.
//!
//! This crate owns the OS event loop, window, wgpu surface/device/queue, resize,
//! realtime clock, native platform input collection, frame acquisition, submission,
//! and presentation. Semantic state, input declarations, lowering/runtime behavior,
//! and retained GPU rendering remain owned by their existing engine layers.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use noon::{
    EvaluationError, ExecutionSession, ExecutionSessionCameraError, ExecutionSessionInputError,
    FrameChanges, TimelineWakeState,
};
use noon_core::{
    Camera2DState, NativeEventOccurrence, NativeEventSource, NativeInputValue, NativeStateSource,
    Vec2,
};
use noon_render_wgpu::{Camera2D, FramePreparer, GpuRenderer};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

/// Minimal platform configuration for a native Noon viewport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeViewportConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl Default for NativeViewportConfig {
    fn default() -> Self {
        Self {
            title: "Noon".to_owned(),
            width: 960,
            height: 540,
        }
    }
}

#[derive(Debug)]
pub enum NativeHostError {
    Platform(String),
    Gpu(String),
    Runtime(EvaluationError),
    Camera(ExecutionSessionCameraError),
    Input(ExecutionSessionInputError),
}

impl std::fmt::Display for NativeHostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Platform(message) => write!(formatter, "native platform error: {message}"),
            Self::Gpu(message) => write!(formatter, "native GPU error: {message}"),
            Self::Runtime(error) => error.fmt(formatter),
            Self::Camera(error) => error.fmt(formatter),
            Self::Input(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for NativeHostError {}

impl From<EvaluationError> for NativeHostError {
    fn from(value: EvaluationError) -> Self {
        Self::Runtime(value)
    }
}

impl From<ExecutionSessionCameraError> for NativeHostError {
    fn from(value: ExecutionSessionCameraError) -> Self {
        Self::Camera(value)
    }
}

impl From<ExecutionSessionInputError> for NativeHostError {
    fn from(value: ExecutionSessionInputError) -> Self {
        Self::Input(value)
    }
}

/// Run one typed execution session in a native OS window.
///
/// No serialization or language host participates in this path. The function owns
/// platform lifecycle until the viewport closes.
pub fn run(session: ExecutionSession) -> Result<(), NativeHostError> {
    run_with_config(session, NativeViewportConfig::default())
}

/// Run one typed execution session with explicit window configuration.
pub fn run_with_config(
    session: ExecutionSession,
    config: NativeViewportConfig,
) -> Result<(), NativeHostError> {
    if config.width == 0 || config.height == 0 {
        return Err(NativeHostError::Platform(
            "initial viewport dimensions must be positive".to_owned(),
        ));
    }

    let event_loop =
        EventLoop::new().map_err(|error| NativeHostError::Platform(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = NativeApp::new(session, config);
    event_loop
        .run_app(&mut app)
        .map_err(|error| NativeHostError::Platform(error.to_string()))?;
    if let Some(error) = app.error.take() {
        return Err(error);
    }
    Ok(())
}

struct NativeApp {
    session: ExecutionSession,
    config: NativeViewportConfig,
    window: Option<Arc<Window>>,
    gpu: Option<NativeGpu>,
    started: Option<Instant>,
    next_input_sequence: u64,
    force_full_redraw: bool,
    error: Option<NativeHostError>,
    #[cfg(test)]
    exit_after_present: bool,
    #[cfg(test)]
    presented_frame: bool,
}

impl NativeApp {
    fn new(session: ExecutionSession, config: NativeViewportConfig) -> Self {
        Self {
            session,
            config,
            window: None,
            gpu: None,
            started: None,
            next_input_sequence: 0,
            force_full_redraw: false,
            error: None,
            #[cfg(test)]
            exit_after_present: false,
            #[cfg(test)]
            presented_frame: false,
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: NativeHostError) {
        if self.error.is_none() {
            self.error = Some(error);
        }
        event_loop.exit();
    }

    fn dispatch_state(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<(), NativeHostError> {
        self.session.set_native_state_input(source, value)?;
        Ok(())
    }

    fn dispatch_event(&mut self, source: NativeEventSource) -> Result<(), NativeHostError> {
        let sequence = self.next_input_sequence;
        let next = sequence.checked_add(1).ok_or_else(|| {
            NativeHostError::Platform("native input event sequence exhausted".to_owned())
        })?;
        self.session
            .emit_native_event(NativeEventOccurrence::new(sequence, source))?;
        self.next_input_sequence = next;
        Ok(())
    }

    fn dispatch_viewport_size(
        &mut self,
        window: &Window,
        size: PhysicalSize<u32>,
    ) -> Result<(), NativeHostError> {
        let logical = size.to_logical::<f32>(window.scale_factor());
        self.dispatch_state(
            NativeStateSource::ViewportSize,
            NativeInputValue::Vec2(Vec2::new(logical.width, logical.height)),
        )
    }

    fn dispatch_keyboard(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
    ) -> Result<(), NativeHostError> {
        let PhysicalKey::Code(code) = physical_key else {
            return Ok(());
        };
        let code = native_key_code(code);
        let pressed = state == ElementState::Pressed;
        self.dispatch_state(
            NativeStateSource::Key { code: code.clone() },
            NativeInputValue::Bool(pressed),
        )?;
        self.dispatch_event(if pressed {
            NativeEventSource::KeyPress { code }
        } else {
            NativeEventSource::KeyRelease { code }
        })
    }

    fn dispatch_pointer_button(
        &mut self,
        button: MouseButton,
        state: ElementState,
    ) -> Result<(), NativeHostError> {
        let Some(button) = native_pointer_button(button) else {
            return Ok(());
        };
        let pressed = state == ElementState::Pressed;
        self.dispatch_state(
            NativeStateSource::PointerButton { button },
            NativeInputValue::Bool(pressed),
        )?;
        self.dispatch_event(if pressed {
            NativeEventSource::PointerDown { button }
        } else {
            NativeEventSource::PointerUp { button }
        })
    }

    fn redraw(&mut self) -> Result<(), NativeHostError> {
        let Some(window) = self.window.as_ref().cloned() else {
            return Ok(());
        };
        let Some(gpu) = self.gpu.as_mut() else {
            return Ok(());
        };
        if !gpu.drawable {
            return Ok(());
        }

        let started = self.started.get_or_insert_with(Instant::now);
        self.session.advance_to(started.elapsed().as_secs_f64())?;
        let camera = self.session.camera()?;
        gpu.set_camera(camera)?;
        let changes = self.session.take_frame_changes();
        let changes = if self.force_full_redraw {
            FrameChanges::all()
        } else {
            changes
        };
        if changes.is_empty() {
            return Ok(());
        }

        let Some((surface_texture, reconfigure_after_present)) = gpu.acquire(window.clone())?
        else {
            // The runtime invalidation has already been consumed. Preserve correctness
            // across a transient surface failure by rebuilding the next presented frame.
            self.force_full_redraw = true;
            return Ok(());
        };

        let frame = self.session.frame();
        let prepared = gpu.preparer.prepare_incremental(frame, &changes);
        gpu.renderer.upload(&gpu.device, &gpu.queue, &prepared);
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Noon native frame"),
            });
        gpu.renderer
            .encode(&mut encoder, &view, &prepared, CLEAR_COLOR);
        window.pre_present_notify();
        gpu.queue.submit(Some(encoder.finish()));
        surface_texture.present();
        #[cfg(test)]
        {
            self.presented_frame = true;
        }
        if reconfigure_after_present {
            gpu.surface.configure(&gpu.device, &gpu.config);
        }
        self.force_full_redraw = false;
        Ok(())
    }
}

impl ApplicationHandler for NativeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attributes = Window::default_attributes()
                .with_title(self.config.title.clone())
                .with_inner_size(PhysicalSize::new(self.config.width, self.config.height));
            let window = match event_loop.create_window(attributes) {
                Ok(window) => Arc::new(window),
                Err(error) => {
                    self.fail(event_loop, NativeHostError::Platform(error.to_string()));
                    return;
                }
            };
            if let Err(error) = self.dispatch_viewport_size(&window, window.inner_size()) {
                self.fail(event_loop, error);
                return;
            }
            self.window = Some(window);
        }

        if self.gpu.is_none() {
            let window = self
                .window
                .as_ref()
                .expect("resumed native host must own a window")
                .clone();
            let camera = match self.session.camera() {
                Ok(camera) => camera,
                Err(error) => {
                    self.fail(event_loop, error.into());
                    return;
                }
            };
            match pollster::block_on(NativeGpu::new(window, camera)) {
                Ok(gpu) => {
                    self.gpu = Some(gpu);
                    self.started = Some(Instant::now());
                    self.force_full_redraw = true;
                }
                Err(error) => self.fail(event_loop, error),
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref().cloned() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Err(error) = self.dispatch_viewport_size(&window, size) {
                    self.fail(event_loop, error);
                    return;
                }
                let camera = match self.session.camera() {
                    Ok(camera) => camera,
                    Err(error) => {
                        self.fail(event_loop, error.into());
                        return;
                    }
                };
                if let Some(gpu) = self.gpu.as_mut() {
                    if let Err(error) = gpu.resize(size, camera) {
                        self.fail(event_loop, error);
                        return;
                    }
                    self.force_full_redraw = true;
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Err(error) = self.dispatch_keyboard(event.physical_key, event.state) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Err(error) = self.dispatch_pointer_button(button, state) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw() {
                    self.fail(event_loop, error);
                }
                #[cfg(test)]
                if self.exit_after_present && self.presented_frame {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = self.window.as_ref() else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        if !self.gpu.as_ref().is_some_and(|gpu| gpu.drawable) {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        let wake = self.session.wake_state();
        if self.force_full_redraw || wake.frame_pending() {
            window.request_redraw();
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        match wake.timeline() {
            TimelineWakeState::Continuous => {
                window.request_redraw();
                event_loop.set_control_flow(ControlFlow::Poll);
            }
            TimelineWakeState::Deadline(scene_time) => {
                let Some(started) = self.started else {
                    window.request_redraw();
                    event_loop.set_control_flow(ControlFlow::Wait);
                    return;
                };
                let Some(deadline) = started.checked_add(Duration::from_secs_f64(scene_time))
                else {
                    event_loop.set_control_flow(ControlFlow::Wait);
                    return;
                };
                if deadline <= Instant::now() {
                    window.request_redraw();
                    event_loop.set_control_flow(ControlFlow::Wait);
                } else {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                }
            }
            TimelineWakeState::Quiescent => {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
    }
}

struct NativeGpu {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    drawable: bool,
    preparer: FramePreparer,
    renderer: GpuRenderer,
}

impl NativeGpu {
    async fn new(window: Arc<Window>, camera: Camera2DState) -> Result<Self, NativeHostError> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .map_err(|error| NativeHostError::Gpu(error.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .map_err(|error| NativeHostError::Gpu(error.to_string()))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Noon native GPU device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .map_err(|error| NativeHostError::Gpu(error.to_string()))?;
        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| {
                NativeHostError::Gpu("surface has no supported configuration".to_owned())
            })?;
        surface.configure(&device, &config);

        let mut renderer = GpuRenderer::new(&device, config.format);
        renderer.set_viewport(&device, &queue, width, height);
        renderer.set_camera(&queue, camera_for_viewport(camera, width, height)?);

        Ok(Self {
            instance,
            surface,
            device,
            queue,
            config,
            drawable: size.width > 0 && size.height > 0,
            preparer: FramePreparer::new(),
            renderer,
        })
    }

    fn resize(
        &mut self,
        size: PhysicalSize<u32>,
        camera: Camera2DState,
    ) -> Result<(), NativeHostError> {
        self.drawable = size.width > 0 && size.height > 0;
        if !self.drawable {
            return Ok(());
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.renderer
            .set_viewport(&self.device, &self.queue, size.width, size.height);
        self.renderer.set_camera(
            &self.queue,
            camera_for_viewport(camera, size.width, size.height)?,
        );
        Ok(())
    }

    fn set_camera(&mut self, camera: Camera2DState) -> Result<(), NativeHostError> {
        if !self.drawable {
            return Ok(());
        }
        self.renderer.set_camera(
            &self.queue,
            camera_for_viewport(camera, self.config.width, self.config.height)?,
        );
        Ok(())
    }

    fn acquire(
        &mut self,
        window: Arc<Window>,
    ) -> Result<Option<(wgpu::SurfaceTexture, bool)>, NativeHostError> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => Ok(Some((texture, false))),
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Ok(Some((texture, true))),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self
                    .instance
                    .create_surface(window)
                    .map_err(|error| NativeHostError::Gpu(error.to_string()))?;
                self.surface.configure(&self.device, &self.config);
                Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Validation => Err(NativeHostError::Gpu(
                "surface acquisition reported a validation failure".to_owned(),
            )),
        }
    }
}

fn native_key_code(code: KeyCode) -> String {
    // winit mostly follows the W3C `KeyboardEvent.code` vocabulary, but names
    // MetaLeft/MetaRight as SuperLeft/SuperRight. Normalize those exceptions so
    // browser and native hosts address the same authored source identity.
    match code {
        KeyCode::SuperLeft => "MetaLeft".to_owned(),
        KeyCode::SuperRight => "MetaRight".to_owned(),
        _ => format!("{code:?}"),
    }
}

fn native_pointer_button(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        MouseButton::Back => Some(3),
        MouseButton::Forward => Some(4),
        MouseButton::Other(button) => u8::try_from(button).ok(),
    }
}

fn camera_for_viewport(
    state: Camera2DState,
    width: u32,
    height: u32,
) -> Result<Camera2D, NativeHostError> {
    if width == 0 || height == 0 {
        return Err(NativeHostError::Gpu(
            "camera viewport dimensions must be positive".to_owned(),
        ));
    }
    let aspect = width as f32 / height as f32;
    Camera2D::new(state.center, Vec2::new(state.height * aspect, state.height))
        .map_err(|error| NativeHostError::Gpu(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_camera_tracks_native_viewport_aspect() {
        let state = Camera2DState {
            center: Vec2::new(2.0, -1.0),
            height: 6.0,
        };
        let camera = camera_for_viewport(state, 1600, 900).unwrap();
        assert_eq!(camera.center, state.center);
        assert!((camera.world_size.x - (6.0 * 16.0 / 9.0)).abs() < 1.0e-5);
        assert_eq!(camera.world_size.y, 6.0);
    }

    #[test]
    fn default_camera_tracks_native_viewport_aspect() {
        let camera = camera_for_viewport(Camera2DState::default(), 1600, 900).unwrap();
        assert_eq!(camera.center, Vec2::ZERO);
        assert!((camera.world_size.x - (8.0 * 16.0 / 9.0)).abs() < 1.0e-5);
        assert_eq!(camera.world_size.y, 8.0);
    }

    #[test]
    fn zero_sized_camera_viewport_is_rejected() {
        assert!(camera_for_viewport(Camera2DState::default(), 0, 720).is_err());
        assert!(camera_for_viewport(Camera2DState::default(), 1280, 0).is_err());
    }

    #[test]
    fn native_key_and_pointer_identity_match_cross_platform_input_vocabulary() {
        assert_eq!(native_key_code(KeyCode::Space), "Space");
        assert_eq!(native_key_code(KeyCode::KeyA), "KeyA");
        assert_eq!(native_key_code(KeyCode::SuperLeft), "MetaLeft");
        assert_eq!(native_key_code(KeyCode::SuperRight), "MetaRight");
        assert_eq!(native_pointer_button(MouseButton::Left), Some(0));
        assert_eq!(native_pointer_button(MouseButton::Middle), Some(1));
        assert_eq!(native_pointer_button(MouseButton::Right), Some(2));
        assert_eq!(native_pointer_button(MouseButton::Back), Some(3));
        assert_eq!(native_pointer_button(MouseButton::Forward), Some(4));
        assert_eq!(native_pointer_button(MouseButton::Other(42)), Some(42));
        assert_eq!(native_pointer_button(MouseButton::Other(300)), None);
    }

    #[test]
    fn native_app_dispatches_normalized_state_and_events_through_execution_session() {
        use noon_core::{
            SemanticObjectProperty, SemanticObjectState, SemanticStore, SemanticVec3,
            StoredGeometry,
        };

        let mut store = SemanticStore::new();
        let viewport = store
            .insert_semantic_input_signal(SemanticVec3::new(0.0, 0.0, 0.0))
            .unwrap();
        store
            .bind_semantic_native_state_input(viewport, NativeStateSource::ViewportSize)
            .unwrap();
        let key_event = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_native_event_input(
                key_event,
                NativeEventSource::KeyPress {
                    code: "Space".to_owned(),
                },
            )
            .unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(viewport, object, SemanticObjectProperty::Translation)
            .unwrap();
        store
            .bind_semantic_signal(key_event, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let session = ExecutionSession::from_semantic_store(&store).unwrap();
        let mut app = NativeApp::new(session, NativeViewportConfig::default());

        app.dispatch_state(
            NativeStateSource::ViewportSize,
            NativeInputValue::Vec2(Vec2::new(640.0, 360.0)),
        )
        .unwrap();
        assert_eq!(
            app.session.frame().objects[0].transform.translation,
            Vec2::new(640.0, 360.0)
        );

        app.dispatch_event(NativeEventSource::KeyPress {
            code: "Space".to_owned(),
        })
        .unwrap();
        app.dispatch_event(NativeEventSource::KeyPress {
            code: "Space".to_owned(),
        })
        .unwrap();
        assert_eq!(app.session.frame().objects[0].transform.rotation, 2.0);
        assert_eq!(app.next_input_sequence, 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an X11 display and a working native wgpu adapter"]
    fn native_surface_smoke_presents_typed_semantic_frame() {
        use noon_core::{
            SemanticObjectRole, SemanticObjectState, SemanticStore, SemanticVec3, StoredGeometry,
        };
        use winit::platform::x11::EventLoopBuilderExtX11;

        let mut store = SemanticStore::new();
        let circle =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.5,
            }));
        store.attach_to_scene(circle).unwrap();

        let mut camera_state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(12.0, 6.0),
        });
        camera_state.transform.translation = SemanticVec3::new(2.0, -1.0, 0.0);
        camera_state.set_role(SemanticObjectRole::Camera2D);
        let camera = store.insert_semantic_object(camera_state);
        store.attach_to_scene(camera).unwrap();

        let session = ExecutionSession::from_semantic_store(&store).unwrap();
        assert_eq!(
            session.camera().unwrap(),
            Camera2DState {
                center: Vec2::new(2.0, -1.0),
                height: 6.0,
            }
        );

        let mut event_loop_builder = EventLoop::builder();
        event_loop_builder.with_any_thread(true);
        let event_loop = event_loop_builder.build().unwrap();
        event_loop.set_control_flow(ControlFlow::Wait);

        let mut app = NativeApp::new(
            session,
            NativeViewportConfig {
                title: "Noon native smoke".to_owned(),
                width: 320,
                height: 180,
            },
        );
        app.exit_after_present = true;
        event_loop.run_app(&mut app).unwrap();

        if let Some(error) = app.error.take() {
            panic!("native surface smoke failed before presentation: {error}");
        }
        assert!(
            app.presented_frame,
            "native host exited without presenting a frame"
        );
    }
}
