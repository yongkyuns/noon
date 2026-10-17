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

#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
#[test]
#[ignore = "requires software Vulkan and bundled fonts; run by Native Host Smoke"]
fn native_capture_preserves_diagram_composition_and_seek_replay() {
    const WIDTH: usize = 640;
    const HEIGHT: usize = 360;
    let mut session = noon::example_scenes::diagram_composition::session().unwrap();
    let mut capture =
        noon_export::SessionCapture::new(&session, software(WIDTH as u32, HEIGHT as u32)).unwrap();
    let samples = [
        (0.0, 0.0, 1.4),
        (0.75, 0.375, 1.4),
        (1.25, 0.75, 1.4),
        (2.0, 0.75, 1.9),
        (2.5, 0.75, 2.4),
        (3.0, 0.75, 2.4),
    ];
    let mut forward = Vec::new();
    for (index, &(time, y, right_x)) in samples.iter().enumerate() {
        session.advance_to(time).unwrap();
        let frame = capture.capture(&mut session).unwrap();
        assert_eq!(frame.receipt.published_time, time);
        assert_eq!(frame.rgba.len(), WIDTH * HEIGHT * 4);
        assert!(frame.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));

        // Check visible content in independent scene-space regions. These broad
        // pixel thresholds tolerate antialiasing while catching missing boxes,
        // native labels, or a connection during either stage of the motion.
        let count =
            |x_min: f32, x_max: f32, y_min: f32, y_max: f32, matches: fn(&[u8; 4]) -> bool| {
                let scale = HEIGHT as f32 / noon::DEFAULT_FRAME_HEIGHT;
                frame
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .enumerate()
                    .filter(|(i, pixel)| {
                        let x = ((*i % WIDTH) as f32 + 0.5 - WIDTH as f32 / 2.0) / scale;
                        let y = (HEIGHT as f32 / 2.0 - (*i / WIDTH) as f32 - 0.5) / scale;
                        x >= x_min && x <= x_max && y >= y_min && y <= y_max && matches(pixel)
                    })
                    .count()
            };
        let blue = count(-2.25, -0.55, y - 0.45, y + 0.45, |p| {
            p[2] > 100 && p[1] > p[0].saturating_add(20)
        });
        let orange = count(right_x - 0.85, right_x + 0.85, y - 0.45, y + 0.45, |p| {
            p[0] > 100 && p[0] > p[1].saturating_add(30) && p[1] > p[2].saturating_add(20)
        });
        assert!(blue > 60, "missing IMU box at t={time}: {blue} pixels");
        assert!(orange > 60, "missing EKF box at t={time}: {orange} pixels");
        let white: fn(&[u8; 4]) -> bool = |p| p[0] > 128 && p[1] > 128 && p[2] > 128;
        for (label, x) in [("IMU", -1.4), ("EKF", right_x)] {
            let pixels = count(x - 0.6, x + 0.6, y - 0.3, y + 0.3, white);
            assert!(
                pixels > 12,
                "missing {label} label at t={time}: {pixels} pixels"
            );
        }
        let arrow = count(-0.55, right_x - 0.85, y - 0.13, y + 0.13, white);
        assert!(arrow > 15, "missing connection at t={time}: {arrow} pixels");

        proof(
            &format!("diagram-composition-{index:02}"),
            frame.rgba,
            WIDTH,
            HEIGHT,
        );
        forward.push(frame.rgba.to_vec());
    }
    assert_ne!(
        forward[0], forward[1],
        "common group motion must be visible"
    );
    assert_ne!(forward[2], forward[3], "member motion must be visible");
    assert_eq!(
        forward[4], forward[5],
        "the final hold must preserve pixels"
    );
    for index in [4, 2, 1, 0, 3, 5] {
        let time = samples[index].0;
        session.seek(time).unwrap();
        let frame = capture.capture(&mut session).unwrap();
        assert_eq!(
            frame.rgba, forward[index],
            "rendered replay differs at t={time}"
        );
    }
}
