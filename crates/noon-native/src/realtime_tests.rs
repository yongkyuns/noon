//! Actual native host/source timing, not a model of the clock algorithm.
use super::*;
use noon::{
    AnimationOptions, ContinuationStep, LiveSession, LiveSessionError, Mobject, RateFunction,
};
#[cfg(target_os = "linux")]
use std::{cell::Cell, rc::Rc};

struct WaitThenMove {
    marker: Mobject,
    target: Mobject,
    waits_left: usize,
    moving: bool,
}

impl LiveContinuation for WaitThenMove {
    type Error = LiveSessionError;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        if self.waits_left != 0 {
            self.waits_left -= 1;
            return live.wait_segment(1.0 / 64.0).map(ContinuationStep::Await);
        }
        if self.moving {
            return Ok(ContinuationStep::Finished);
        }
        self.moving = true;
        live.declare_and_activate_transform_to(
            &self.marker,
            &self.target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .map(ContinuationStep::Await)
    }
}

fn moving_app(waits_left: usize) -> NativeApp {
    let mut scene = noon::Scene::new();
    let marker = scene.square(0.5).unwrap();
    scene.add(&marker).unwrap();
    let mut target = marker.target_editor().unwrap();
    target.set_translation(4.0, 0.0).unwrap();
    let program = scene
        .into_live_program(WaitThenMove {
            marker,
            target,
            waits_left,
            moving: false,
        })
        .unwrap();
    let source = LiveProgramExecutionSource::new(program, RustHostCallbackTable::new()).unwrap();
    let mut app = NativeApp::from_source(Box::new(source), NativeViewportConfig::default());
    // This CPU fixture consumes only the initial publication. It never admits
    // an animated endpoint without the explicit renderer boundary below.
    let _ = app.execution.take_renderer_publication();
    app
}

#[test]
fn foreground_frame_density_does_not_scale_native_animation_speed() {
    for fps in [5_u32, 15, 24, 30, 60, 120] {
        let mut app = moving_app(0);
        let origin = Instant::now();
        for frame in 0..=2 * fps {
            let elapsed = f64::from(frame) / f64::from(fps);
            app.advance_realtime_timeline(origin + Duration::from_secs_f64(elapsed))
                .unwrap();
            let state = app.session().frame();
            assert!((state.time - elapsed).abs() < 1.0e-8);
            let actual_x = f64::from(state.objects[0].transform.translation.x);
            assert!((actual_x - 2.0 * elapsed).abs() < 1.0e-5);
            assert_eq!(app.realtime_clock.unwrap().wall_origin, origin);
        }
        assert_eq!(app.session().frame().time, 2.0);
        assert_eq!(app.session().frame().objects[0].transform.translation.x, 4.0);
        assert!(app.execution.pending_endpoint().is_some());
    }
}

#[test]
fn jitter_and_late_short_waits_share_one_native_playback_epoch() {
    let mut app = moving_app(32);
    let origin = Instant::now();
    app.advance_realtime_timeline(origin).unwrap();
    for ms in [7_u64, 21, 203, 499, 937, 1_701, 2_500] {
        let elapsed = ms as f64 / 1_000.0;
        app.advance_realtime_timeline(origin + Duration::from_millis(ms))
            .unwrap();
        // The 32 exact short waits total 0.5 seconds, not 32 host frames.
        let expected_x = 2.0 * (elapsed - 0.5).clamp(0.0, 2.0);
        let actual_x = f64::from(app.session().frame().objects[0].transform.translation.x);
        assert!((actual_x - expected_x).abs() < 1.0e-5);
        assert_eq!(app.realtime_clock.unwrap().wall_origin, origin);
    }
    assert_eq!(app.session().frame().time, 2.5);
    assert_eq!(app.session().frame().objects[0].transform.translation.x, 4.0);
}

#[test]
fn pending_renderer_endpoint_does_not_erase_late_wall_time() {
    let mut app = moving_app(0);
    let origin = Instant::now();
    app.advance_realtime_timeline(origin).unwrap();
    app.advance_realtime_timeline(origin + Duration::from_secs(7))
        .unwrap();
    let endpoint = app.execution.pending_endpoint().unwrap();
    assert_eq!(app.session().frame().time, 2.0);
    assert_eq!(app.realtime_clock.unwrap().wall_origin, origin);
    assert!(app.execution.admit_retained_publication(endpoint).is_err());
    for delay in [8, 9, 10] {
        app.advance_realtime_timeline(origin + Duration::from_secs(delay))
            .unwrap();
        assert_eq!(app.execution.pending_endpoint(), Some(endpoint));
        assert_eq!(app.realtime_clock.unwrap().wall_origin, origin);
    }
    // This checks the runtime receipt contract, not GPU success. The surface
    // fixture below exercises real renderer preparation before admission.
    let consumed = app.execution.take_renderer_publication().context();
    assert_eq!(consumed, endpoint);
    app.execution.admit_retained_publication(consumed).unwrap();
    app.resume_ready_preserving_clock(origin + Duration::from_secs(10))
        .unwrap();
    assert!(!app.execution.source_active());
    assert!(app.realtime_clock.is_none());
}

#[test]
fn cooperative_work_budget_preserves_debt_instead_of_restarting_time() {
    let mut app = moving_app(512);
    let origin = Instant::now();
    app.advance_realtime_timeline(origin).unwrap();
    let late = origin + Duration::from_millis(8_500);
    app.advance_realtime_timeline(late).unwrap();
    assert!(app.session().frame().time < 8.5);
    assert_eq!(app.realtime_clock.unwrap().wall_origin, origin);
    // Reusing the same timestamp drains bounded source work; it must not need
    // fresh wall time to make logical progress or allocate a backlog of frames.
    for _ in 0..32 {
        if app.session().frame().time == 8.5 {
            break;
        }
        app.advance_realtime_timeline(late).unwrap();
        assert_eq!(app.realtime_clock.unwrap().wall_origin, origin);
    }
    assert_eq!(app.session().frame().time, 8.5);
    assert_eq!(app.session().frame().objects[0].transform.translation.x, 1.0);
}

#[cfg(target_os = "linux")]
struct ShortAnimatedSequence {
    marker: Mobject,
    targets: Vec<Mobject>,
    next: usize,
    resumes: Rc<Cell<usize>>,
}

#[cfg(target_os = "linux")]
impl LiveContinuation for ShortAnimatedSequence {
    type Error = LiveSessionError;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        self.resumes.set(self.resumes.get() + 1);
        let Some(target) = self.targets.get(self.next) else {
            return Ok(ContinuationStep::Finished);
        };
        self.next += 1;
        live.declare_and_activate_transform_to(
            &self.marker,
            target,
            AnimationOptions::new()
                .run_time(1.0 / 16.0)
                .rate_func(RateFunction::Linear),
        )
        .map(ContinuationStep::Await)
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires X11 and WGPU; selected by authoring-native-time qualification"]
fn late_native_surface_retains_all_endpoints_but_presents_only_the_caught_up_frame() {
    use winit::platform::x11::EventLoopBuilderExtX11;

    let mut scene = noon::Scene::new();
    let marker = scene.square(0.5).unwrap();
    scene.add(&marker).unwrap();
    let targets = (1..=32)
        .map(|index| {
            let mut target = marker.target_editor().unwrap();
            target
                .set_translation(f64::from(index) / 16.0, 0.0)
                .unwrap();
            target
        })
        .collect();
    let resumes = Rc::new(Cell::new(0));
    let program = scene
        .into_live_program(ShortAnimatedSequence {
            marker,
            targets,
            next: 0,
            resumes: Rc::clone(&resumes),
        })
        .unwrap();
    let source = LiveProgramExecutionSource::new(program, RustHostCallbackTable::new()).unwrap();
    let mut app = NativeApp::from_source(
        Box::new(source),
        NativeViewportConfig {
            title: "Noon late native duration proof".to_owned(),
            width: 320,
            height: 180,
            inspection_zoom: false,
        },
    );
    // Begin with the source already overdue. Exit after the FIRST physical
    // presentation, not after waiting for 32 subsequent vsyncs to hide drift.
    app.realtime_clock = Some(RealtimeClock::new(
        Instant::now().checked_sub(Duration::from_secs(10)).unwrap(),
        0.0,
    ));
    app.exit_after_present = Some(0.0);
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    event_loop.run_app(&mut app).unwrap();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.presented_frame_time, Some(2.0));
    assert_eq!(app.session().frame().time, 2.0);
    assert_eq!(app.session().frame().objects[0].transform.translation.x, 2.0);
    assert_eq!(resumes.get(), 33);
    assert!(!app.execution.source_active());
    assert!(app.realtime_clock.is_none());
    assert!(app.last_geometry_draw_calls > 0);
    let shown = app.pointer.presented.as_ref().unwrap();
    assert_eq!(shown.publication(), app.session().publication_context());
    eprintln!(
        "first physical frame: authored=2s, admitted resumes={}",
        resumes.get()
    );
}
