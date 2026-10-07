#![cfg(not(target_arch = "wasm32"))]

use std::cell::RefCell;
use std::convert::Infallible;
use std::rc::Rc;

use noon::integration::{
    ExportFrameOptions, ExportStop, FrameRate, HostCallbackId, SemanticMutationTransaction,
};
use noon::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveSession,
    LiveSessionError, Mobject, RateFunction, RustHostCallbackTable, Scene,
};
use noon_export::{capture_frames, Backends, CaptureOptions, CaptureRunError, CaptureSummary};

struct Source {
    object: Mobject,
    target: Mobject,
    camera: Mobject,
    callback: bool,
    stage: usize,
}

impl LiveContinuation for Source {
    type Error = LiveSessionError;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let stage = self.stage;
        self.stage += 1;
        match stage {
            0 if !self.callback => Ok(ContinuationStep::Await(
                live.declare_and_activate_transform_to(
                    &self.object,
                    &self.target,
                    AnimationOptions::new()
                        .run_time(0.105)
                        .rate_func(RateFunction::Linear),
                )?,
            )),
            0 => Ok(ContinuationStep::Await(live.wait_segment(0.105)?)),
            1 => {
                live.set_translation(&self.camera, 1.0, 0.5)?;
                Ok(ContinuationStep::Await(live.wait_segment(0.207)?))
            }
            _ => {
                live.set_translation(&self.object, 2.0, -1.0)?;
                Ok(ContinuationStep::Finished)
            }
        }
    }
}

struct Fixture {
    program: LiveProgram<Source>,
    callbacks: RustHostCallbackTable,
    trace: Rc<RefCell<Vec<(f64, f64)>>>,
}

fn fixture(callback: bool) -> Fixture {
    let mut scene = Scene::new();
    let mut camera = scene.camera_frame().unwrap();
    camera.manim_scale(0.75, 0.75).unwrap();
    let mut object = scene.square(1.0).unwrap();
    object.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    object.set_stroke_width(0.0).unwrap();
    object.set_translation(-2.0, 1.0).unwrap();
    scene.add(&object).unwrap();
    let mut target = object.target_editor().unwrap();
    target.set_translation(1.0, 1.0).unwrap();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = RustHostCallbackTable::new();
    if callback {
        let id = HostCallbackId::new(189_602);
        let mut tx = SemanticMutationTransaction::new();
        tx.add_updater(object.node_id(), id, 0.0, None);
        tx.apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let calls = Rc::clone(&trace);
        callbacks
            .insert(id, move |context| {
                calls
                    .borrow_mut()
                    .push((context.time(), context.delta_time()));
                let mut transform = context.target_state().transform;
                transform.translation.x += (context.delta_time().powi(2) + 0.03) as f32;
                context.set_target_transform(transform)
            })
            .unwrap();
    }
    let program = scene
        .into_live_program(Source {
            object,
            target,
            camera,
            callback,
            stage: 0,
        })
        .unwrap();
    Fixture {
        program,
        callbacks,
        trace,
    }
}

fn frames() -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 64,
        max_transitions_per_sample: 64,
        final_hold_seconds: 0.0,
    }
}

fn software(width: u32, height: u32) -> CaptureOptions {
    let mut options = CaptureOptions::new(width, height);
    options.backends = Backends::VULKAN;
    options.force_fallback_adapter = true;
    options
}

struct Run {
    pixels: Vec<Vec<u8>>,
    trace: Vec<(f64, f64)>,
    summary: CaptureSummary,
}

fn run(callback: bool, start: u64, hold: f64, delayed: bool) -> Run {
    let mut fixture = fixture(callback);
    let mut pixels = Vec::new();
    let mut buffer = None;
    let options = ExportFrameOptions {
        start_frame: start,
        final_hold_seconds: hold,
        ..frames()
    };
    let summary = capture_frames(
        &mut fixture.program,
        &mut fixture.callbacks,
        options,
        software(257, 129),
        |frame| {
            assert_eq!(
                (frame.width, frame.height, frame.rgba.len()),
                (257, 129, 257 * 129 * 4)
            );
            assert_eq!(frame.frame.pts, pixels.len() as u64);
            assert_eq!(
                frame.frame.source_sample.index(),
                pixels.len() as u64 + start
            );
            assert_eq!(
                frame.frame.source_sample.authored_time(),
                frame.observation.requested_time
            );
            if !frame.frame.held {
                assert_eq!(
                    frame.observation.requested_time,
                    frame.observation.published_time
                );
            }
            assert!(frame.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
            if let Some(address) = buffer {
                assert_eq!(
                    frame.rgba.as_ptr(),
                    address,
                    "CPU output buffer was reallocated"
                );
            }
            buffer = Some(frame.rgba.as_ptr());
            let trace = fixture.trace.borrow().clone();
            if delayed {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert_eq!(
                *fixture.trace.borrow(),
                trace,
                "sink latency advanced callbacks"
            );
            pixels.push(frame.rgba.to_vec());
            Ok::<_, Infallible>(())
        },
    )
    .unwrap();
    assert_eq!(summary.readbacks, summary.sampling.frames);
    assert!(summary.rendered_publications >= summary.readbacks);
    assert_eq!(summary.staging_buffer_bytes, 1280 * 129);
    let trace = fixture.trace.borrow().clone();
    Run {
        pixels,
        trace,
        summary,
    }
}

fn centroid(pixels: &[u8], width: usize) -> (f64, f64) {
    let mut count = 0.0;
    let mut x = 0.0;
    let mut y = 0.0;
    for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
        let weight = f64::from(p[2].saturating_sub(p[0].max(p[1])));
        count += weight;
        x += (i % width) as f64 * weight;
        y += (i / width) as f64 * weight;
    }
    assert!(count > 0.0);
    (x / count, y / count)
}

fn proof(name: &str, pixels: &[u8], width: usize, height: usize) {
    let Some(dir) = std::env::var_os("NOON_CAPTURE_PROOF_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
    for p in pixels.as_chunks::<4>().0 {
        ppm.extend_from_slice(&p[..3]);
    }
    std::fs::write(dir.join(format!("{name}.ppm")), ppm).unwrap();
}

#[test]
#[ignore = "requires software Vulkan; run by Native Host Smoke without DISPLAY"]
fn native_capture_preserves_timing_pixels_crops_holds_and_backpressure() {
    for callback in [false, true] {
        let full = run(callback, 0, 0.0, false);
        let slow = run(callback, 0, 0.0, true);
        let crop = run(callback, 4, 0.0, false);
        let held = run(callback, 0, 0.1, false);
        assert_eq!(
            (full.pixels.len(), crop.pixels.len(), held.pixels.len()),
            (10, 6, 13)
        );
        assert_eq!(full.pixels, slow.pixels);
        assert_eq!(crop.pixels, full.pixels[4..]);
        assert_eq!(held.pixels[..10], full.pixels);
        assert_eq!(full.trace, slow.trace);
        assert_eq!(full.trace, crop.trace);
        assert_eq!(full.trace, held.trace);
        for image in &held.pixels[10..] {
            assert_eq!(image, &held.pixels[10]);
            let (x, y) = centroid(image, 257);
            // Camera center moved to (1, 0.5), height 6; final object at (2, -1).
            assert!((x - (128.0 + 129.0 / 6.0)).abs() <= 1.0);
            assert!((y - (64.0 + 1.5 * 129.0 / 6.0)).abs() <= 1.0);
        }
        assert!(
            full.pixels[0] != full.pixels[4],
            "camera/motion did not change output"
        );
        assert_eq!(full.summary.sampling.frames, 10);
        proof(
            &format!("callback-{callback}-first"),
            &full.pixels[0],
            257,
            129,
        );
        proof(
            &format!("callback-{callback}-hold"),
            &held.pixels[12],
            257,
            129,
        );
    }
}

#[test]
#[ignore = "requires software Vulkan; run by Native Host Smoke without DISPLAY"]
fn native_capture_consumer_failure_and_cancellation_stop_without_more_callbacks() {
    let mut failed = fixture(true);
    let mut delivered = 0;
    let result = capture_frames(
        &mut failed.program,
        &mut failed.callbacks,
        frames(),
        software(65, 33),
        |_| {
            delivered += 1;
            Err("intentional sink failure")
        },
    );
    assert!(matches!(
        result,
        Err(CaptureRunError::Consumer("intentional sink failure"))
    ));
    assert_eq!(delivered, 1);
    assert_eq!(failed.program.session().frame().time, 0.0);
    let mut cancelled = fixture(true);
    let options = software(65, 33);
    let token = options.cancellation.clone();
    let result = capture_frames(
        &mut cancelled.program,
        &mut cancelled.callbacks,
        frames(),
        options,
        |_| {
            token.cancel();
            Ok::<_, Infallible>(())
        },
    );
    assert!(matches!(result, Err(CaptureRunError::Cancelled)));
    assert_eq!(cancelled.program.session().frame().time, 0.0);
    // A fresh run after either failure must remain healthy.
    assert_eq!(run(false, 0, 0.0, false).pixels.len(), 10);
}

#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
#[test]
#[ignore = "requires software Vulkan and bundled fonts; run by Native Host Smoke"]
fn native_capture_composes_shared_text_image_transients_and_zoomed_views() {
    let capture = || {
        let (mut program, mut callbacks) =
            noon::example_scenes::moving_zoomed_scene_around::program().unwrap();
        let mut pixels = Vec::new();
        let options = ExportFrameOptions {
            frame_rate: FrameRate::new(2, 1).unwrap(),
            ..frames()
        };
        let summary = capture_frames(
            &mut program,
            &mut callbacks,
            options,
            software(320, 180),
            |frame| {
                assert_eq!(frame.frame.pts, pixels.len() as u64);
                assert_eq!(frame.rgba.len(), 320 * 180 * 4);
                assert!(frame.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
                assert!(frame
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[0] > 32 || p[1] > 32 || p[2] > 32));
                pixels.push(frame.rgba.to_vec());
                Ok::<_, Infallible>(())
            },
        )
        .unwrap();
        assert_eq!(summary.sampling.frames, 24);
        pixels
    };
    let first = capture();
    let second = capture();
    assert_eq!(first, second);
    assert!(first[0] != first[3]);
    for index in [0, 3, 5, 8, 15, 23] {
        proof(
            &format!("zoom-text-image-{index:02}"),
            &first[index],
            320,
            180,
        );
    }
}
