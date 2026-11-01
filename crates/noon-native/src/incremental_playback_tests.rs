//! Native clock and physical-receipt policy over the actual shared player.
use super::*;
use noon::{ContinuationStep, LiveSession, LiveSessionError, Mobject, Scene};
use std::{cell::Cell, rc::Rc};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton},
};
const SIZE: PhysicalSize<u32> = PhysicalSize::new(800, 400);

fn present(app: &mut NativeApp) {
    app.execution.take_renderer_publication();
    let frame = app.capture_pointer_presentation(SIZE, 1.0).unwrap();
    app.execution.admit_presented_frame(&frame).unwrap();
    app.pointer.presented = Some(frame);
    app.pointer.refresh_pending = false;
    app.force_full_redraw = false;
    app.playback_picture_pending = false;
}
struct DelayedPrefix {
    marker: Mobject,
    target: Mobject,
    calls: Rc<Cell<usize>>,
}
impl LiveContinuation for DelayedPrefix {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let step = self.calls.get();
        self.calls.set(step + 1);
        Ok(match step {
            0 => ContinuationStep::Await(live.wait_segment(1.0)?),
            2 => ContinuationStep::Await(
                live.declare_and_activate_transform_to(
                    &self.marker,
                    &self.target,
                    noon::AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(noon::RateFunction::Linear),
                )?,
            ),
            1 => ContinuationStep::Await(live.wait_segment(0.0)?),
            3 => ContinuationStep::Finished,
            _ => panic!("source restarted"),
        })
    }
}
#[test]
fn pause_at_an_unknown_frontier_preserves_intent_and_excludes_buffered_wall_time() {
    let calls = Rc::new(Cell::new(0));
    let mut scene = Scene::new();
    let marker = scene.circle(0.5).unwrap();
    scene.add(&marker).unwrap();
    let mut target = marker.target_editor().unwrap();
    target.set_translation(2.0, 0.0).unwrap();
    let program = scene
        .into_live_program(DelayedPrefix {
            marker,
            target,
            calls: calls.clone(),
        })
        .unwrap();
    let source = PlaybackExecutionSource::new(program, RustHostCallbackTable::new()).unwrap();
    let mut app = NativeApp::from_source(
        Box::new(source),
        NativeViewportConfig {
            playback_controls: true,
            ..NativeViewportConfig::default()
        },
    );
    present(&mut app);
    let origin = Instant::now();
    app.advance_realtime_timeline(origin).unwrap();
    app.advance_realtime_timeline(origin + Duration::from_millis(1500))
        .unwrap();
    assert!(app.execution.buffering());
    assert_eq!(app.execution.frame_time(), 0.0);
    assert!(app.realtime_clock.is_none());
    assert_eq!(calls.get(), 2, "one zero-duration source call yields");
    app.pause_playback(origin + Duration::from_secs(2)).unwrap();
    assert!(app.playback_paused);
    app.resume_ready_preserving_clock(origin + Duration::from_secs(50))
        .unwrap();
    assert!(!app.execution.buffering());
    assert_eq!(app.execution.frame_time(), 0.0);
    assert!(app.playback_paused);
    assert!(app.realtime_clock.is_none());
    assert_eq!(calls.get(), 3);
    app.resume_playback().unwrap();
    present(&mut app);
    let resumed = origin + Duration::from_secs(100);
    app.advance_realtime_timeline(resumed).unwrap();
    assert_eq!(app.realtime_clock.unwrap().wall_origin, resumed);
    app.advance_realtime_timeline(resumed + Duration::from_secs(1))
        .unwrap();
    assert_eq!(app.execution.frame_time(), 1.0);
    assert_eq!(app.realtime_clock.unwrap().wall_origin, resumed);
    assert_eq!(calls.get(), 3);
    app.advance_realtime_timeline(resumed + Duration::from_millis(1500))
        .unwrap();
    assert_eq!(app.execution.frame_time(), 1.5);
    assert!((app.session().frame().objects[0].transform.translation.x - 1.0).abs() < 1e-6);
    assert_eq!(calls.get(), 4);
}

struct LateDrag {
    target: Mobject,
    calls: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
}
impl Drop for LateDrag {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
impl LiveContinuation for LateDrag {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let step = self.calls.get();
        self.calls.set(step + 1);
        if step == 1 {
            live.set_translation_drag_targets([&self.target])?;
        }
        Ok(match step {
            0 | 1 => ContinuationStep::Await(live.wait_segment(1.0)?),
            2 => ContinuationStep::Finished,
            _ => panic!("source restarted"),
        })
    }
}
#[test]
fn late_drag_requires_a_new_presented_frame_and_a_fresh_contact_after_handoff() {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.5).unwrap();
    target.set_fill(0.0, 0.4, 1.0, 1.0).unwrap();
    target.disable_stroke().unwrap();
    scene.add(&target).unwrap();
    scene
        .on_click_indicate(
            &target,
            noon::IndicateOptions::default(),
            noon::AnimationOptions::new(),
        )
        .unwrap();
    let calls = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let source = PlaybackExecutionSource::new(
        scene
            .into_live_program(LateDrag {
                target,
                calls: calls.clone(),
                drops: drops.clone(),
            })
            .unwrap(),
        RustHostCallbackTable::new(),
    )
    .unwrap();
    let mut app = NativeApp::from_source(Box::new(source), NativeViewportConfig::default());
    present(&mut app);
    app.dispatch_pointer_position(PhysicalPosition::new(400.0, 200.0), SIZE, 1.0)
        .unwrap();
    present(&mut app);
    app.dispatch_pointer_button(MouseButton::Left, ElementState::Pressed, SIZE, 1.0)
        .unwrap();
    present(&mut app);
    let old_frame = app.pointer.presented.clone().unwrap();
    let old_identity = app.session().runtime_identity();
    let origin = Instant::now();
    app.advance_realtime_timeline(origin).unwrap();
    app.advance_realtime_timeline(origin + Duration::from_millis(1250))
        .unwrap();
    assert_ne!(app.session().runtime_identity(), old_identity);
    assert!(app.pointer.presented.is_none());
    assert!(app.publication_pending());
    assert_eq!(calls.get(), 2);
    assert_eq!(drops.get(), 0);
    assert!(old_frame
        .validate_presentation(app.session(), old_frame.view())
        .is_err());
    app.dispatch_pointer_button(MouseButton::Left, ElementState::Released, SIZE, 1.0)
        .unwrap();
    assert_eq!(
        app.session().frame().objects[0].transform.translation.x,
        0.0
    );
    present(&mut app);
    app.dispatch_pointer_position(PhysicalPosition::new(400.0, 200.0), SIZE, 1.0)
        .unwrap();
    present(&mut app);
    app.dispatch_pointer_button(MouseButton::Left, ElementState::Pressed, SIZE, 1.0)
        .unwrap();
    present(&mut app);
    app.dispatch_pointer_position(PhysicalPosition::new(475.0, 200.0), SIZE, 1.0)
        .unwrap();
    present(&mut app);
    app.dispatch_pointer_button(MouseButton::Left, ElementState::Released, SIZE, 1.0)
        .unwrap();
    assert_eq!(
        app.session().frame().objects[0].transform.translation.x,
        1.5
    );
    assert_eq!(calls.get(), 2);
    drop(app);
    assert_eq!(drops.get(), 1);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires X11 and a real raster adapter; included in native host smoke"]
fn native_surface_smoke_incremental_first_picture_precedes_source_completion() {
    use winit::platform::x11::EventLoopBuilderExtX11;
    let mut scene = Scene::new();
    let mut marker = scene.circle(0.5).unwrap();
    marker.set_fill(0.1, 0.4, 1.0, 1.0).unwrap();
    scene.add(&marker).unwrap();
    let mut target = marker.target_editor().unwrap();
    target.set_translation(2.0, 0.0).unwrap();
    let calls = Rc::new(Cell::new(0));
    let program = scene
        .into_live_program(DelayedPrefix {
            marker,
            target,
            calls: calls.clone(),
        })
        .unwrap();
    let source = PlaybackExecutionSource::new(program, RustHostCallbackTable::new()).unwrap();
    let app = NativeApp::from_source(
        Box::new(source),
        NativeViewportConfig {
            width: 320,
            height: 180,
            ..NativeViewportConfig::default()
        },
    );
    struct FirstPicture(NativeApp);
    impl ApplicationHandler for FirstPicture {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            self.0.resumed(event_loop);
        }
        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            self.0.about_to_wait(event_loop);
        }
        fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
            self.0.window_event(event_loop, id, event);
            if self.0.presented_frame_time.is_some() {
                event_loop.exit();
            }
        }
    }
    let mut harness = FirstPicture(app);
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(&mut harness).unwrap();
    let app = harness.0;
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.presented_frame_time.is_some());
    assert!(app.execution.source_active());
    assert!(
        calls.get() < 4,
        "first surface picture must precede full source completion"
    );
    assert!(app.last_geometry_draw_calls > 0);
    let shown = app.pointer.presented.as_ref().unwrap();
    assert_eq!(shown.publication(), app.session().publication_context());
}
