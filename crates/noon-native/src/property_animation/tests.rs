use super::*;
use crate::{execution_source::LiveProgramExecutionSource, NativeViewportConfig};
use noon::integration::{NativeInputModifiers, NativePointerId, NativePointerInputKind};
use noon::{ContinuationStep, LiveContinuation, LiveSession, Scene};
use noon_core::{SemanticPointerClickAction, Vec2};
use std::{cell::Cell, rc::Rc, time::Duration};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton};

const SIZE: PhysicalSize<u32> = PhysicalSize::new(800, 400);

struct Finish(Rc<Cell<usize>>);
impl LiveContinuation for Finish {
    type Error = noon::LiveSessionError;
    fn resume(&mut self, _live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        self.0.set(self.0.get() + 1);
        Ok(ContinuationStep::Finished)
    }
}

fn fixture() -> (NativeApp, Rc<Cell<usize>>) {
    fixture_with_action(SemanticPointerClickAction::default(), 1.0)
}

fn fixture_with_action(
    action: SemanticPointerClickAction,
    scale: f64,
) -> (NativeApp, Rc<Cell<usize>>) {
    let mut scene = Scene::new();
    for x in [0.0, 2.0] {
        let mut object = scene.circle(0.5).unwrap();
        object.set_translation(x, 0.0).unwrap();
        object.set_scale(scale, scale).unwrap();
        object.set_fill(0.0, 0.5, 1.0, 1.0).unwrap();
        object.set_pointer_click_action(Some(action)).unwrap();
        scene.add(&object).unwrap();
    }
    let resumes = Rc::new(Cell::new(0));
    let mut program = scene
        .into_live_program(Finish(Rc::clone(&resumes)))
        .unwrap();
    program.set_pointer_fill_clicks(Some(5.0)).unwrap();
    let source =
        LiveProgramExecutionSource::new(program, noon::RustHostCallbackTable::new()).unwrap();
    (
        NativeApp::from_source(
            Box::new(source),
            NativeViewportConfig {
                inspection_zoom: true,
                ..Default::default()
            },
        ),
        resumes,
    )
}

fn present(app: &mut NativeApp) {
    let frame = app.capture_pointer_presentation(SIZE, 1.0).unwrap();
    app.execution.take_renderer_publication();
    app.execution.admit_presented_frame(&frame).unwrap();
    app.pointer.presented = Some(frame);
    app.pointer.refresh_pending = false;
}

fn collector_click(app: &mut NativeApp, size: PhysicalSize<u32>, scale: f64) {
    app.dispatch_pointer_position(
        PhysicalPosition::new(f64::from(size.width) * 0.5, f64::from(size.height) * 0.5),
        size,
        scale,
    )
    .unwrap();
    for state in [ElementState::Pressed, ElementState::Released] {
        app.dispatch_pointer_button(MouseButton::Left, state, size, scale)
            .unwrap();
    }
}

// Deterministic platform timestamps, using the real shared displayed-view token
// and ordinary native source dispatcher. No effect token or host interpolation.
fn occurrence_at(
    app: &mut NativeApp,
    x: f32,
    down: bool,
    now: Instant,
) -> Result<(), NativeHostError> {
    if app.execution.native_pointer_input_token().is_err() {
        app.execution.configure_native_pointer_input(
            NativePointerId {
                source: 1,
                pointer: 0,
            },
            0,
        )?;
    }
    let frame = app.pointer.presented.as_ref().unwrap();
    let token = frame.input_token(app.session(), frame.view()).unwrap();
    let position = frame.position(Vec2::new(400.0 + x * 50.0, 200.0)).unwrap();
    let kind = if down {
        NativePointerInputKind::Press {
            position,
            button: 0,
        }
    } else {
        NativePointerInputKind::Release {
            position,
            button: 0,
        }
    };
    let sequence = app.next_input_sequence;
    let input = NativePointerInput::new(
        sequence,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        kind,
    );
    app.submit_pointer_occurrence_at(&token, input, sequence + 1, now)
}

fn click_at(app: &mut NativeApp, x: f32, now: Instant) {
    occurrence_at(app, x, true, now).unwrap();
    occurrence_at(app, x, false, now).unwrap();
}

#[test]
fn native_collector_starts_and_restores_after_source_finishes_without_authored_time() {
    let (mut app, resumes) = fixture();
    present(&mut app);
    let before = app.session().frame().clone();
    let revision = app.session().publication_context().scene_revision();
    collector_click(&mut app, SIZE, 1.0);
    assert!(app.execution.property_animation_pending());
    assert_eq!(
        app.execution.timeline(),
        noon::integration::TimelineWakeState::Quiescent
    );
    assert!(app.session().pointer_selection_highlight().is_none());
    let origin = app.effect_previous_tick.unwrap();
    let old_display = app.pointer.presented.clone().unwrap();
    app.advance_realtime_property_animations(origin + Duration::from_millis(500))
        .unwrap();
    assert_eq!(
        app.session().frame().objects[0].transform.scale,
        Vec2::new(1.2, 1.2)
    );
    assert_eq!(app.session().frame().objects[1], before.objects[1]);
    assert!(old_display
        .validate_current(app.session(), old_display.view())
        .is_err());
    assert_eq!(app.session().frame().time, 0.0);
    app.advance_realtime_property_animations(origin + Duration::from_secs(1))
        .unwrap();
    assert_eq!(app.session().frame(), &before);
    assert_eq!(
        app.session().publication_context().scene_revision(),
        revision
    );
    assert!(!app.execution.property_animation_pending());
    assert!(app.effect_previous_tick.is_none());
    present(&mut app);
    assert!(!app.publication_pending());
    assert!(app.session().wake_state().is_quiescent());
    assert_eq!(resumes.get(), 1);
}

#[test]
fn new_click_does_not_inherit_earlier_interval_or_reset_existing_effect() {
    let (mut app, _) = fixture();
    present(&mut app);
    let origin = Instant::now();
    click_at(&mut app, 0.0, origin);
    present(&mut app);
    let before = app.session().publication_context();
    occurrence_at(&mut app, 2.0, true, origin + Duration::from_millis(400)).unwrap();
    assert_eq!(app.session().publication_context(), before);
    assert_eq!(app.effect_previous_tick, Some(origin));
    occurrence_at(&mut app, 2.0, false, origin + Duration::from_millis(400)).unwrap();
    assert!(app.session().frame().objects[0].transform.scale.x > 1.0);
    assert_eq!(
        app.session().frame().objects[1].transform.scale,
        Vec2::new(1.0, 1.0)
    );
    app.advance_realtime_property_animations(origin + Duration::from_millis(500))
        .unwrap();
    assert_eq!(
        app.session().frame().objects[0].transform.scale,
        Vec2::new(1.2, 1.2)
    );
    let second_scale = app.session().frame().objects[1].transform.scale.x;
    assert!(second_scale > 1.0 && second_scale < 1.2);
    app.advance_realtime_property_animations(origin + Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        app.session().frame().objects[0].transform.scale,
        Vec2::new(1.0, 1.0)
    );
    assert!(app.execution.property_animation_pending());
    app.advance_realtime_property_animations(origin + Duration::from_millis(1400))
        .unwrap();
    assert!(!app.execution.property_animation_pending());
    assert_eq!(app.session().frame().time, 0.0);
}

#[test]
fn idle_time_is_not_charged_to_a_new_click() {
    let (mut app, _) = fixture();
    let origin = Instant::now();
    app.advance_realtime_property_animations(origin).unwrap();
    present(&mut app);
    let activation = origin + Duration::from_secs(3600);
    click_at(&mut app, 0.0, activation);
    assert_eq!(
        app.session().frame().objects[0].transform.scale,
        Vec2::new(1.0, 1.0)
    );
    app.advance_realtime_property_animations(activation + Duration::from_millis(500))
        .unwrap();
    assert_eq!(
        app.session().frame().objects[0].transform.scale,
        Vec2::new(1.2, 1.2)
    );
}

#[test]
fn backwards_delivery_is_rejected_without_changing_input_clock_or_frame() {
    let (mut app, _) = fixture();
    present(&mut app);
    let origin = Instant::now();
    click_at(&mut app, 0.0, origin);
    present(&mut app);
    let before = app.session().publication_context();
    let sequence = app.next_input_sequence;
    let backwards = origin.checked_sub(Duration::from_secs(1)).unwrap();
    assert!(app.advance_realtime_property_animations(backwards).is_err());
    assert!(occurrence_at(&mut app, 0.0, true, backwards).is_err());
    assert_eq!(app.session().publication_context(), before);
    assert_eq!(app.effect_previous_tick, Some(origin));
    assert_eq!(app.next_input_sequence, sequence);
}

#[test]
fn required_callback_gap_preserves_effect_and_reanchors_delivery_on_settlement() {
    let mut scene = Scene::new();
    let object = scene.circle(0.5).unwrap();
    scene.add(&object).unwrap();
    let mut session = scene.execution_session().unwrap();
    let effect = scene
        .live(&mut session)
        .start_indicate_effect(
            &object,
            noon::IndicateOptions::default(),
            noon_core::AnimationOptions::new(),
        )
        .unwrap()
        .unwrap();
    let mut app = NativeApp::new(session, NativeViewportConfig::default());
    let origin = Instant::now();
    app.advance_realtime_property_animations(origin).unwrap();
    app.advance_realtime_property_animations(origin + Duration::from_millis(250))
        .unwrap();
    let overlay = app
        .static_session_mut()
        .begin_required_callback_phase(0.0, [object.node_id()])
        .unwrap();
    assert!(!app.execution.property_animation_pending());
    app.advance_realtime_property_animations(origin + Duration::from_secs(30))
        .unwrap();
    assert!(app.effect_previous_tick.is_none());
    assert_eq!(app.session().property_animation_elapsed(effect), Some(0.25));
    app.static_session_mut()
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    app.advance_realtime_property_animations(origin + Duration::from_secs(60))
        .unwrap();
    assert_eq!(app.session().property_animation_elapsed(effect), Some(0.25));
    app.advance_realtime_property_animations(origin + Duration::from_millis(60250))
        .unwrap();
    assert_eq!(app.session().property_animation_elapsed(effect), Some(0.5));
}

#[test]
fn failed_acquire_retains_effect_publication_without_replaying_elapsed() {
    let (mut app, _) = fixture();
    present(&mut app);
    let origin = Instant::now();
    click_at(&mut app, 0.0, origin);
    app.advance_realtime_property_animations(origin + Duration::from_millis(500))
        .unwrap();
    let expected = app.session().frame().clone();
    assert!(NativeApp::take_renderer_publication_after_acquire(
        app.execution.as_mut(),
        false,
        None::<()>
    )
    .is_none());
    assert!(app.publication_pending());
    app.advance_realtime_property_animations(origin + Duration::from_millis(500))
        .unwrap();
    assert_eq!(app.session().frame(), &expected);
    present(&mut app);
    assert!(!app.publication_pending());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires an X11 display and a working native wgpu adapter"]
fn native_surface_smoke_presents_click_effect_zoom_and_retrigger() {
    use winit::application::ApplicationHandler;
    use winit::event::{MouseScrollDelta, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::x11::EventLoopBuilderExtX11;
    use winit::window::WindowId;
    struct Harness {
        app: NativeApp,
        stage: u8,
        presentations: usize,
        baseline: noon_core::Transform2D,
        second_peak: bool,
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
                frame.publication(),
                self.app.session().publication_context()
            );
            assert!(self.app.session().pointer_selection_highlight().is_none());
            assert!(self.app.last_geometry_draw_calls > 0);
            self.presentations += 1;
            let window = self.app.window.as_ref().unwrap();
            let size = window.inner_size();
            let scale = window.scale_factor();
            let transform = self.app.session().frame().objects[0].transform;
            match self.stage {
                0 => {
                    collector_click(&mut self.app, size, scale);
                    assert!(self.app.execution.property_animation_pending());
                    self.stage = 1;
                }
                1 if transform.scale.x > 1.02 => {
                    self.app
                        .dispatch_inspection_scroll(
                            MouseScrollDelta::PixelDelta(PhysicalPosition::new(
                                0.0,
                                std::f64::consts::LN_2 * 500.0 * scale,
                            )),
                            size,
                            scale,
                        )
                        .unwrap();
                    self.stage = 2;
                }
                2 if !self.app.execution.property_animation_pending() => {
                    assert_eq!(transform, self.baseline);
                    assert!(self.app.execution.camera().unwrap().height < 8.0);
                    collector_click(&mut self.app, size, scale);
                    assert!(self.app.execution.property_animation_pending());
                    self.stage = 3;
                }
                3 => {
                    self.second_peak |= transform.scale.x > 1.02;
                    if !self.app.execution.property_animation_pending() {
                        assert!(self.second_peak);
                        assert_eq!(transform, self.baseline);
                        assert!(self.app.effect_previous_tick.is_none());
                        assert!(self.app.session().wake_state().is_quiescent());
                        self.stage = 4;
                        event_loop.exit();
                    }
                }
                _ => {}
            }
        }
    }
    let mut builder = EventLoop::builder();
    builder.with_x11().with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let (app, resumes) = fixture();
    let baseline = app.session().frame().objects[0].transform;
    let mut harness = Harness {
        app,
        stage: 0,
        presentations: 0,
        baseline,
        second_peak: false,
    };
    event_loop.run_app(&mut harness).unwrap();
    assert!(harness.app.error.is_none(), "{:?}", harness.app.error);
    assert_eq!(harness.stage, 4);
    assert!(harness.presentations >= 5);
    assert_eq!(resumes.get(), 1);
}

#[test]
fn post_admission_action_failure_commits_native_sequence_before_surfacing_error() {
    // The authored factor is representable, but applying it to a scaled object
    // exceeds the render domain. Picking/input remain valid; action lowering fails.
    let action = SemanticPointerClickAction::indicate(f64::from(f32::MAX), noon_core::YELLOW, 1.0);
    let (mut app, _) = fixture_with_action(action, 2.0);
    present(&mut app);
    let now = Instant::now();
    occurrence_at(&mut app, 0.0, true, now).unwrap();
    let sequence = app.next_input_sequence;
    assert!(occurrence_at(&mut app, 0.0, false, now).is_err());
    assert_eq!(app.next_input_sequence, sequence + 1);
    assert!(!app.execution.property_animation_pending());
    assert!(app.effect_previous_tick.is_none());
    assert_eq!(app.session().frame().time, 0.0);
    let frame = app.pointer.presented.as_ref().unwrap();
    let token = frame.input_token(app.session(), frame.view()).unwrap();
    let position = frame.position(Vec2::new(400.0, 200.0)).unwrap();
    let repeated = NativePointerInput::new(
        sequence,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        NativePointerInputKind::Release {
            position,
            button: 0,
        },
    );
    assert!(app
        .submit_pointer_occurrence_at(&token, repeated, sequence + 1, now)
        .is_err());
    assert_eq!(app.next_input_sequence, sequence + 1);
}
