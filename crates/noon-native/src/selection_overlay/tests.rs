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
    // This unit test models a successful presentation; the window-loop test
    // below relies only on the production queue/present boundary.
    app.pointer.presented = Some(app.capture_pointer_presentation(SIZE, 1.0).unwrap());
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
    app.last_selection_presentation = app.session().pointer_selection_presentation();
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
fn failed_acquire_keeps_overlay_pending_and_out_and_back_does_not_clear() {
    let mut app = app();
    app.static_session_mut().take_renderer_publication();
    // This unit test models a successful presentation; the window-loop test
    // below relies only on the production queue/present boundary.
    app.pointer.presented = Some(app.capture_pointer_presentation(SIZE, 1.0).unwrap());
    click(&mut app, 160.0);
    assert!(NativeApp::take_renderer_publication_after_acquire(
        app.execution.as_mut(),
        false,
        None::<()>
    )
    .is_none());
    assert!(app.selection_overlay_pending());
    app.last_selection_presentation = app.session().pointer_selection_presentation();
    // An incorrectly recognized background click would erase the selection.
    app.dispatch_pointer_position(PhysicalPosition::new(10.0, 90.0), SIZE, 1.0)
        .unwrap();
    app.dispatch_pointer_button(MouseButton::Left, ElementState::Pressed, SIZE, 1.0)
        .unwrap();
    for x in [30.0, 10.0] {
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
            // A created/configured surface is not a presentation receipt.
            assert!(self.app.pointer.presented.is_none());
            assert_eq!(
                self.app
                    .dispatch_pointer_position(PhysicalPosition::new(160.0, 90.0), SIZE, 1.0)
                    .unwrap(),
                crate::pointer_input::PointerDispatch::AwaitingPresentation
            );
            assert!(matches!(
                self.app.session().native_pointer_input_token(),
                Err(noon::ExecutionSessionInputError::PointerNotConfigured)
            ));
            assert_eq!(self.app.next_input_sequence, 0);
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
            let receipt = self
                .app
                .pointer
                .presented
                .clone()
                .expect("successful present installs a receipt");
            assert_eq!(
                receipt.publication(),
                self.app.session().publication_context()
            );
            assert!(!self.app.pointer.refresh_pending);
            match self.presents {
                0 => {
                    assert!(self.app.last_selection_presentation.is_none());
                    click(&mut self.app, 160.0);
                    assert!(self.app.selection_overlay_pending());
                }
                1 => {
                    assert!(self.app.last_selection_presentation.is_some());
                    assert!(!self.app.publication_pending());
                    assert_eq!(self.app.last_geometry_draw_calls, 2);
                    // Execution makes an unpresented round trip back to the
                    // same time/image. Its new publication must not revive the
                    // historical displayed receipt or retag an incoming click.
                    self.app.static_session_mut().evaluate(0.25).unwrap();
                    self.app.static_session_mut().seek(0.0).unwrap();
                    assert_ne!(
                        self.app.session().publication_context(),
                        receipt.publication()
                    );
                    assert!(matches!(
                        self.app
                            .dispatch_pointer_position(
                                PhysicalPosition::new(160.0, 90.0),
                                SIZE,
                                1.0
                            )
                            .unwrap(),
                        crate::pointer_input::PointerDispatch::RejectedFrame(_)
                    ));
                    assert_eq!(self.app.pointer.presented, Some(receipt));
                    assert!(self.app.session().selected_pointer_target().is_none());
                    assert!(self.app.publication_pending());
                }
                2 => {
                    assert!(self.app.last_selection_presentation.is_none());
                    assert!(!self.app.publication_pending());
                    assert_eq!(self.app.last_geometry_draw_calls, 1);
                    click(&mut self.app, 160.0);
                    assert!(self.app.selection_overlay_pending());
                }
                3 => {
                    assert!(self.app.last_selection_presentation.is_some());
                    // A same-size surface/view lifecycle reset also invalidates
                    // the receipt even though authored state and image agree.
                    self.app.rebind_pointer_view().unwrap();
                    assert!(self.app.pointer.presented.is_none());
                    assert_eq!(
                        self.app
                            .dispatch_pointer_position(PhysicalPosition::new(10.0, 90.0), SIZE, 1.0)
                            .unwrap(),
                        crate::pointer_input::PointerDispatch::AwaitingPresentation
                    );
                    assert!(self.app.session().selected_pointer_target().is_some());
                    assert!(self.app.publication_pending());
                    assert!(!self.app.force_full_redraw);
                }
                4 => {
                    assert!(self.app.last_selection_presentation.is_some());
                    assert!(!self.app.publication_pending());
                    click(&mut self.app, 10.0);
                    assert!(self.app.selection_overlay_pending());
                }
                5 => {
                    assert!(self.app.last_selection_presentation.is_none());
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
    assert_eq!(harness.presents, 6);
    assert_eq!(harness.app.session().frame().time, 0.0);
}

#[test]
fn rejected_motion_cancels_click_even_after_fresh_display_and_return_to_press_point() {
    let mut app = app();
    app.static_session_mut().take_renderer_publication();
    app.pointer.presented = Some(app.capture_pointer_presentation(SIZE, 1.0).unwrap());
    app.dispatch_pointer_position(PhysicalPosition::new(160.0, 90.0), SIZE, 1.0)
        .unwrap();
    app.dispatch_pointer_button(MouseButton::Left, ElementState::Pressed, SIZE, 1.0)
        .unwrap();
    app.static_session_mut().evaluate(0.25).unwrap();
    // The movement threshold must not accidentally supply the cancellation:
    // this excursion is rejected before it reaches the session gesture state.
    assert!(matches!(
        app.dispatch_pointer_position(PhysicalPosition::new(260.0, 90.0), SIZE, 1.0)
            .unwrap(),
        crate::pointer_input::PointerDispatch::RejectedFrame(_)
    ));
    app.pointer.presented = Some(app.capture_pointer_presentation(SIZE, 1.0).unwrap());
    app.pointer.refresh_pending = false;
    app.dispatch_pointer_position(PhysicalPosition::new(160.0, 90.0), SIZE, 1.0)
        .unwrap();
    app.dispatch_pointer_button(MouseButton::Left, ElementState::Released, SIZE, 1.0)
        .unwrap();
    assert!(
        app.session().selected_pointer_target().is_none(),
        "rejected motion must retire the prior press"
    );
    click(&mut app, 160.0);
    assert!(
        app.session().selected_pointer_target().is_some(),
        "fresh click must recover"
    );
    assert_eq!(app.session().frame().time, 0.25);
}

#[test]
fn same_time_selection_clear_keeps_an_unchanged_effective_frame_receipt_compatible() {
    let mut app = app();
    app.static_session_mut().take_renderer_publication();
    // Model the displayed scene and selection without an actual window here.
    app.pointer.presented = Some(app.capture_pointer_presentation(SIZE, 1.0).unwrap());
    click(&mut app, 160.0);
    app.last_selection_presentation = app.session().pointer_selection_presentation();
    let displayed = app.pointer.presented.clone();
    let before = app.session().publication_context();
    assert!(app.session().selected_pointer_target().is_some());
    app.static_session_mut().seek(0.0).unwrap();
    assert_eq!(app.session().publication_context(), before);
    assert!(app.session().selected_pointer_target().is_none());
    assert!(
        app.publication_pending(),
        "selection clear still needs its own redraw"
    );
    // A session overlay is not a new effective scene or a picking target.
    assert_eq!(
        app.dispatch_pointer_position(PhysicalPosition::new(160.0, 90.0), SIZE, 1.0)
            .unwrap(),
        crate::pointer_input::PointerDispatch::Admitted
    );
    assert_eq!(app.pointer.presented, displayed);
    assert_eq!(app.session().frame().time, 0.0);
}
