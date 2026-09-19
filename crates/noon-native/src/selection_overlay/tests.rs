use super::*;
use crate::NativeViewportConfig;
use noon::Scene;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton};

const SIZE: PhysicalSize<u32> = PhysicalSize::new(320, 180);

fn app() -> NativeApp {
    let mut scene = Scene::new();
    let mut object = scene.circle(1.0).unwrap();
    object.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    object.set_stroke_width(0.0).unwrap();
    scene.add(&object).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.enable_pointer_fill_selection(4.0).unwrap();
    NativeApp::new(
        session,
        NativeViewportConfig {
            title: "Noon paused click selection".to_owned(),
            width: SIZE.width,
            height: SIZE.height,
        },
    )
}

fn click(app: &mut NativeApp, x: f64) {
    app.dispatch_pointer_position(PhysicalPosition::new(x, 90.0), SIZE, 1.0)
        .unwrap();
    app.dispatch_pointer_button(MouseButton::Left, ElementState::Pressed, SIZE, 1.0)
        .unwrap();
    app.dispatch_pointer_button(MouseButton::Left, ElementState::Released, SIZE, 1.0)
        .unwrap();
}

#[test]
fn paused_click_and_clear_request_redraw_without_scene_dirtiness_or_timeline_work() {
    let mut app = app();
    app.static_session_mut().take_renderer_publication();
    let before = app.session().frame().clone();
    let context = app.session().publication_context();
    assert!(!app.publication_pending());
    click(&mut app, 160.0);
    assert!(app.session().selected_pointer_target().is_some());
    assert!(app.publication_pending());
    assert!(app.session().wake_state().is_quiescent());
    assert_eq!(app.session().frame(), &before);
    assert_eq!(app.session().publication_context(), context);
    // Model only a successful presentation acknowledgement, not input state.
    app.last_selection_highlight = app.session().pointer_selection_highlight();
    assert!(!app.publication_pending());
    click(&mut app, 160.0);
    assert!(
        !app.publication_pending(),
        "same selected image stays settled"
    );
    click(&mut app, 10.0);
    assert!(app.session().selected_pointer_target().is_none());
    assert!(
        app.publication_pending(),
        "clear must erase the old overlay"
    );
    let (_, publication) =
        NativeApp::take_renderer_publication_after_acquire(app.execution.as_mut(), false, Some(()))
            .unwrap();
    assert!(publication.changes().is_empty());
    assert_eq!(publication.frame().time, 0.0);
}

#[test]
fn failed_acquire_keeps_overlay_pending_and_out_and_back_does_not_select() {
    let mut app = app();
    app.static_session_mut().take_renderer_publication();
    click(&mut app, 160.0);
    assert!(NativeApp::take_renderer_publication_after_acquire(
        app.execution.as_mut(),
        false,
        None::<()>
    )
    .is_none());
    assert!(app.selection_overlay_pending());
    app.last_selection_highlight = app.session().pointer_selection_highlight();
    app.dispatch_pointer_button(MouseButton::Left, ElementState::Pressed, SIZE, 1.0)
        .unwrap();
    for x in [180.0, 160.0] {
        app.dispatch_pointer_position(PhysicalPosition::new(x, 90.0), SIZE, 1.0)
            .unwrap();
    }
    app.dispatch_pointer_button(MouseButton::Left, ElementState::Released, SIZE, 1.0)
        .unwrap();
    assert!(!app.selection_overlay_pending());
    assert!(app.session().selected_pointer_target().is_some());
    app.static_session_mut()
        .disable_pointer_fill_selection()
        .unwrap();
    assert!(app.selection_overlay_pending());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires an X11 display and a working native wgpu adapter"]
fn native_surface_smoke_presents_paused_click_selection_and_clear() {
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
            assert_eq!(self.app.presented_frame_time, Some(0.0));
            assert!(self.app.session().wake_state().is_quiescent());
            match self.presents {
                0 => {
                    assert!(self.app.last_selection_highlight.is_none());
                    click(&mut self.app, 160.0);
                    assert!(self.app.selection_overlay_pending());
                }
                1 => {
                    assert!(self.app.last_selection_highlight.is_some());
                    assert!(!self.app.publication_pending());
                    assert_eq!(self.app.last_geometry_draw_calls, 2);
                    click(&mut self.app, 10.0);
                    assert!(self.app.selection_overlay_pending());
                }
                2 => {
                    assert!(self.app.last_selection_highlight.is_none());
                    assert!(!self.app.publication_pending());
                    assert_eq!(self.app.last_geometry_draw_calls, 1);
                    event_loop.exit();
                }
                _ => panic!("unexpected repeated presentation"),
            }
            self.presents += 1;
        }
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut harness = Harness {
        app: app(),
        presents: 0,
    };
    event_loop.run_app(&mut harness).unwrap();
    assert!(harness.app.error.is_none(), "{:?}", harness.app.error);
    assert_eq!(harness.presents, 3);
    assert_eq!(harness.app.session().frame().time, 0.0);
}
