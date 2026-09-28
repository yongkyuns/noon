use super::*;
use crate::NativeViewportConfig;
use noon::{
    AnimationOptions, ContinuationStep, IndicateOptions, LiveContinuation, LiveSession, Scene,
};
use noon_core::Vec2;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton};

const SIZE: PhysicalSize<u32> = PhysicalSize::new(800, 400);

fn app(selection: bool, inspection: bool) -> NativeApp {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.3).unwrap();
    circle.set_fill(0.0, 0.5, 1.0, 1.0).unwrap();
    circle.set_translation(2.0, 0.0).unwrap();
    scene.add(&circle).unwrap();
    let mut session = scene.execution_session().unwrap();
    if selection {
        session.enable_pointer_fill_selection(4.0).unwrap();
    }
    NativeApp::new(
        session,
        NativeViewportConfig {
            inspection_zoom: inspection,
            ..Default::default()
        },
    )
}

// Models successful presentation only. The separate X11 smoke below exercises
// real acquire/encode/submit/present with normalized wheel delivery.
fn present(app: &mut NativeApp) {
    let frame = app.capture_pointer_presentation(SIZE, 1.0).unwrap();
    app.execution.take_renderer_publication();
    app.execution.admit_presented_frame(&frame).unwrap();
    app.pointer.presented = Some(frame);
    app.pointer.refresh_pending = false;
    app.last_selection_presentation = app.session().pointer_selection_presentation();
}

fn move_to(app: &mut NativeApp, x: f64, y: f64) {
    app.dispatch_pointer_position(PhysicalPosition::new(x, y), SIZE, 1.0)
        .unwrap();
}
fn edge(app: &mut NativeApp, state: ElementState) {
    app.dispatch_pointer_button(MouseButton::Left, state, SIZE, 1.0)
        .unwrap();
}
fn wheel(app: &mut NativeApp, y: f64) -> PointerDispatch {
    app.dispatch_inspection_scroll(
        MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, y)),
        SIZE,
        1.0,
    )
    .unwrap()
}
fn half_height() -> f64 {
    500.0 * std::f64::consts::LN_2
}

#[test]
fn line_and_pixel_units_normalize_direction_and_device_scale() {
    assert_eq!(
        vertical_pixels(MouseScrollDelta::LineDelta(7.0, 2.0), 2.0).unwrap(),
        -80.0
    );
    assert_eq!(
        vertical_pixels(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(7.0, 160.0)),
            2.0
        )
        .unwrap(),
        -80.0
    );
    assert_eq!(
        vertical_pixels(MouseScrollDelta::LineDelta(1.0, -2.0), 1.0).unwrap(),
        80.0
    );
    assert!(vertical_pixels(MouseScrollDelta::LineDelta(0.0, f32::NAN), 1.0).is_err());
    assert!(vertical_pixels(
        MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, f64::INFINITY)),
        1.0
    )
    .is_err());
    assert!(vertical_pixels(MouseScrollDelta::LineDelta(0.0, 1.0), 0.0).is_err());
}

#[test]
fn disabled_inspection_is_inert_and_horizontal_scroll_is_not_consumed() {
    let mut app = app(false, false);
    present(&mut app);
    move_to(&mut app, 400.0, 200.0);
    assert_eq!(wheel(&mut app, 50.0), PointerDispatch::Unsubscribed);
    assert_eq!(app.session().inspection_view_revision(), 0);
    assert!(!app.publication_pending());
    app.config.inspection_zoom = true;
    assert_eq!(wheel(&mut app, 0.0), PointerDispatch::Unsubscribed);
    assert_eq!(app.session().inspection_view_revision(), 0);
}

#[test]
fn zoom_only_viewport_needs_no_semantic_pointer_binding_or_timeline() {
    let mut app = app(false, true);
    present(&mut app);
    move_to(&mut app, 400.0, 200.0);
    let before = app.session().publication_context();
    let allocation = app.session().frame().objects.as_ptr();
    assert_eq!(wheel(&mut app, half_height()), PointerDispatch::Admitted);
    assert_eq!(app.execution.camera().unwrap().height, 4.0);
    assert_eq!(app.session().camera().unwrap().height, 8.0);
    assert_eq!(app.session().publication_context(), before);
    assert_eq!(app.session().frame().objects.as_ptr(), allocation);
    assert_eq!(app.session().frame().time, 0.0);
    assert!(app.session().wake_state().is_quiescent());
    assert!(app.publication_pending());
    assert_eq!(app.next_input_sequence, 0);
    assert!(app.session().native_pointer_input_token().is_err());
    present(&mut app);
    assert!(!app.publication_pending());
}

#[test]
fn native_camera_projection_keeps_cursor_anchor_through_both_directions() {
    let mut app = app(false, true);
    present(&mut app);
    move_to(&mut app, 550.0, 100.0);
    let anchor = app
        .pointer
        .presented
        .as_ref()
        .unwrap()
        .position(Vec2::new(550.0, 100.0))
        .unwrap();
    for y in [half_height(), -half_height()] {
        assert_eq!(wheel(&mut app, y), PointerDispatch::Admitted);
        present(&mut app);
        let snapshot = app.pointer.presented.as_ref().unwrap();
        assert_eq!(snapshot.view().camera(), app.execution.camera().unwrap());
        assert_eq!(snapshot.position(Vec2::new(550.0, 100.0)).unwrap(), anchor);
    }
    assert_eq!(
        app.execution.camera().unwrap(),
        app.session().camera().unwrap()
    );
    assert_eq!(app.session().inspection_view_revision(), 2);
}

#[test]
fn picking_moves_with_the_rendered_view_not_the_authored_camera() {
    let mut app = app(true, true);
    present(&mut app);
    move_to(&mut app, 400.0, 200.0);
    wheel(&mut app, half_height());
    present(&mut app);
    move_to(&mut app, 600.0, 200.0);
    edge(&mut app, ElementState::Pressed);
    edge(&mut app, ElementState::Released);
    assert!(app.session().selected_pointer_target().is_some());
    present(&mut app);
    move_to(&mut app, 500.0, 200.0);
    edge(&mut app, ElementState::Pressed);
    edge(&mut app, ElementState::Released);
    assert!(app.session().selected_pointer_target().is_none());
    assert_eq!(app.session().frame().time, 0.0);
}

#[test]
fn press_scroll_release_cannot_click_but_a_fresh_click_recovers() {
    let mut app = app(true, true);
    present(&mut app);
    move_to(&mut app, 500.0, 200.0);
    edge(&mut app, ElementState::Pressed);
    wheel(&mut app, half_height());
    assert!(!app.pointer.configured);
    present(&mut app);
    edge(&mut app, ElementState::Released);
    assert!(app.session().selected_pointer_target().is_none());
    edge(&mut app, ElementState::Pressed);
    edge(&mut app, ElementState::Released);
    assert!(app.session().selected_pointer_target().is_some());
}

#[test]
fn wheel_burst_does_not_retag_or_accumulate_unpresented_occurrences() {
    let mut app = app(false, true);
    present(&mut app);
    move_to(&mut app, 400.0, 200.0);
    wheel(&mut app, half_height());
    for _ in 0..100 {
        assert_eq!(
            wheel(&mut app, half_height()),
            PointerDispatch::AwaitingPresentation
        );
    }
    assert_eq!(app.session().inspection_view_revision(), 1);
    assert_eq!(app.execution.camera().unwrap().height, 4.0);
    present(&mut app);
    assert_eq!(wheel(&mut app, half_height()), PointerDispatch::Admitted);
    assert_eq!(app.execution.camera().unwrap().height, 2.0);
    assert_eq!(app.next_input_sequence, 0);
}

#[test]
fn invalid_scroll_is_atomic_and_missing_cursor_is_not_fabricated() {
    let mut app = app(true, true);
    present(&mut app);
    assert_eq!(wheel(&mut app, half_height()), PointerDispatch::Cancelled);
    move_to(&mut app, 500.0, 200.0);
    edge(&mut app, ElementState::Pressed);
    let frame = app.pointer.presented.clone();
    let revision = app.session().inspection_view_revision();
    assert!(app
        .dispatch_inspection_scroll(MouseScrollDelta::LineDelta(0.0, f32::NAN), SIZE, 1.0)
        .is_err());
    assert_eq!(app.pointer.presented, frame);
    assert_eq!(app.session().inspection_view_revision(), revision);
    edge(&mut app, ElementState::Released);
    assert!(app.session().selected_pointer_target().is_some());
}

#[test]
fn failed_acquire_and_surface_recovery_keep_the_adjustment_and_pending_redraw() {
    let mut app = app(false, true);
    present(&mut app);
    move_to(&mut app, 400.0, 200.0);
    wheel(&mut app, half_height());
    let camera = app.execution.camera().unwrap();
    assert!(NativeApp::take_renderer_publication_after_acquire(
        app.execution.as_mut(),
        false,
        None::<()>
    )
    .is_none());
    assert!(app.publication_pending());
    app.rebind_pointer_view().unwrap();
    assert_eq!(app.execution.camera().unwrap(), camera);
    assert!(app.pointer.presented.is_none());
    assert_eq!(wheel(&mut app, half_height()), PointerDispatch::Cancelled);
    present(&mut app);
    assert_eq!(app.execution.camera().unwrap(), camera);
    assert!(!app.publication_pending());
}

#[test]
fn view_aba_cannot_acknowledge_an_old_presented_frame() {
    let mut app = app(false, true);
    present(&mut app);
    let old = app.pointer.presented.clone().unwrap();
    move_to(&mut app, 400.0, 200.0);
    wheel(&mut app, half_height());
    present(&mut app);
    wheel(&mut app, -half_height());
    assert_eq!(app.execution.camera().unwrap(), old.view().camera());
    assert_eq!(app.session().publication_context(), old.publication());
    assert!(app.execution.admit_presented_frame(&old).is_err());
    assert!(app.publication_pending());
    present(&mut app);
    assert!(!app.publication_pending());
}

struct IndicateOnce {
    object: noon::Mobject,
    started: bool,
}
impl LiveContinuation for IndicateOnce {
    type Error = noon::LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        if self.started {
            return Ok(ContinuationStep::Finished);
        }
        self.started = true;
        Ok(ContinuationStep::Await(
            live.declare_and_activate_indicate(
                &self.object,
                IndicateOptions::default(),
                AnimationOptions::new().run_time(1.0),
            )?,
        ))
    }
}

#[test]
fn live_indicate_and_endpoint_presentation_survive_independent_native_zoom() {
    use crate::execution_source::LiveProgramExecutionSource;
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.5).unwrap();
    circle.set_fill(0.0, 0.5, 1.0, 1.0).unwrap();
    scene.add(&circle).unwrap();
    let program = scene
        .into_live_program(IndicateOnce {
            object: circle,
            started: false,
        })
        .unwrap();
    let source =
        LiveProgramExecutionSource::new(program, noon::RustHostCallbackTable::new()).unwrap();
    let mut app = NativeApp::from_source(
        Box::new(source),
        NativeViewportConfig {
            inspection_zoom: true,
            ..Default::default()
        },
    );
    let original = app.session().frame().objects[0].clone();
    app.execution.advance_to(0.5).unwrap();
    present(&mut app);
    assert!(app.session().frame().objects[0].transform.scale.x > original.transform.scale.x);
    let midpoint = app.session().frame().clone();
    move_to(&mut app, 400.0, 200.0);
    wheel(&mut app, half_height());
    assert_eq!(app.session().frame(), &midpoint);
    present(&mut app);
    app.execution.advance_to(1.0).unwrap();
    // Capture, but deliberately do not acknowledge the animation endpoint.
    let endpoint = app.capture_pointer_presentation(SIZE, 1.0).unwrap();
    app.execution.take_renderer_publication();
    app.pointer.presented = Some(endpoint.clone());
    app.pointer.refresh_pending = false;
    assert!(!app.execution.resume_ready().unwrap());
    wheel(&mut app, -half_height());
    assert!(app.execution.admit_presented_frame(&endpoint).is_err());
    assert!(!app.execution.resume_ready().unwrap());
    present(&mut app);
    assert!(app.execution.resume_ready().unwrap());
    assert_eq!(
        app.session().frame().objects[0].transform,
        original.transform
    );
    assert_eq!(app.session().frame().objects[0].style, original.style);
    assert_eq!(app.session().frame().time, 1.0);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires an X11 display and a working native wgpu adapter"]
fn native_surface_smoke_presents_inspection_zoom_and_recovery() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::x11::EventLoopBuilderExtX11;
    use winit::window::WindowId;

    struct Harness {
        app: NativeApp,
        presents: usize,
    }
    impl ApplicationHandler for Harness {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            self.app.resumed(event_loop);
        }
        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            self.app.about_to_wait(event_loop);
        }
        fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
            let redraw = matches!(event, WindowEvent::RedrawRequested);
            self.app.window_event(event_loop, id, event);
            if self.app.error.is_some() || !redraw {
                return;
            }
            let Some(frame) = self.app.pointer.presented.clone() else {
                return;
            };
            assert_eq!(self.app.presented_frame_time, Some(0.0));
            assert_eq!(frame.view().camera(), self.app.execution.camera().unwrap());
            assert_eq!(
                frame.inspection_revision(),
                self.app.session().inspection_view_revision()
            );
            assert!(self.app.session().wake_state().is_quiescent());
            assert!(!self.app.publication_pending());
            assert!(self.app.last_geometry_draw_calls > 0);
            let window = self.app.window.as_ref().unwrap();
            let size = window.inner_size();
            let scale = window.scale_factor();
            match self.presents {
                0 | 2 => {
                    let expected = if self.presents == 0 { 8.0 } else { 4.0 };
                    assert_eq!(frame.view().camera().height, expected);
                    self.app
                        .dispatch_pointer_position(
                            PhysicalPosition::new(
                                f64::from(size.width) * 0.5,
                                f64::from(size.height) * 0.5,
                            ),
                            size,
                            scale,
                        )
                        .unwrap();
                    let y = if self.presents == 0 {
                        half_height()
                    } else {
                        -half_height()
                    };
                    assert_eq!(
                        self.app
                            .dispatch_inspection_scroll(
                                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, y * scale)),
                                size,
                                scale
                            )
                            .unwrap(),
                        PointerDispatch::Admitted
                    );
                    assert!(self.app.pointer.presented.is_none());
                }
                1 => {
                    assert_eq!(frame.view().camera().height, 4.0);
                    self.app.rebind_pointer_view().unwrap();
                    assert!(self.app.pointer.presented.is_none());
                    assert!(self.app.publication_pending());
                }
                3 => {
                    assert_eq!(frame.view().camera().height, 8.0);
                    event_loop.exit();
                }
                _ => panic!("unexpected presentation"),
            }
            self.presents += 1;
        }
    }
    let mut builder = EventLoop::builder();
    builder.with_x11().with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut app = app(false, true);
    app.config.width = SIZE.width;
    app.config.height = SIZE.height;
    let mut harness = Harness { app, presents: 0 };
    event_loop.run_app(&mut harness).unwrap();
    assert!(harness.app.error.is_none(), "{:?}", harness.app.error);
    assert_eq!(harness.presents, 4);
}
