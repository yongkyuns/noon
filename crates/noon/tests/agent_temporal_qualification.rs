//! Native counterpart of the MCP agent_temporal_translation.py fixture.
//! Exercise typed authoring, shared execution, and production retained rendering;
//! this is not a native clone of the browser preview service.

use noon::{AnimationOptions, Color, ExecutionSession, RateFunction, Scene};

mod raster_support;
use raster_support::{Raster, SIZE, WORLD_SIZE};

fn sample_translation(y: f64, mut observe: impl FnMut(&mut ExecutionSession, f64, f64)) {
    let mut scene = Scene::new();
    let mut marker = scene.square(1.0).unwrap();
    let blue = Color::BLUE;
    marker
        .set_fill(blue.red.into(), blue.green.into(), blue.blue.into(), 1.0)
        .unwrap();
    marker.set_stroke_width(0.0).unwrap();
    marker.set_translation(-3.0, y).unwrap();
    scene.add(&marker).unwrap();
    let mut target = marker.target_editor().unwrap();
    target.set_translation(3.0, y).unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(
            &marker,
            &target,
            AnimationOptions::new()
                .run_time(3.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();

    for (time, expected_x) in [(0.0, -3.0), (1.0, -1.0), (2.0, 1.0), (3.0, 3.0)] {
        session.advance_segment_to(segment, time).unwrap();
        assert_eq!(session.frame().time, time, "engine-published timestamp");
        assert_eq!(session.frame().objects.len(), 1);
        let position = session.frame().objects[0].transform.translation;
        assert!((f64::from(position.x) - expected_x).abs() < 1.0e-6);
        assert_eq!(position.y, y as f32);
        observe(&mut session, time, expected_x);
    }
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(marker.center().unwrap(), (3.0, y));
    assert_eq!(session.frame().time, 3.0);
}

#[test]
fn temporal_translation_has_expected_published_states() {
    sample_translation(0.0, |_, _, _| {});
}

fn blue_centroid(pixels: &[u8], width: u32, height: u32) -> (f64, f64) {
    assert_eq!(pixels.len(), (width * height * 4) as usize);
    let mut total_weight = 0.0;
    let mut weighted_x = 0.0;
    let mut weighted_y = 0.0;
    for (index, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
        let blue_excess = pixel[2].saturating_sub(pixel[0].max(pixel[1]));
        let weight = f64::from(blue_excess) * f64::from(pixel[3]) / 255.0;
        total_weight += weight;
        weighted_x += (index % width as usize) as f64 * weight;
        weighted_y += (index / width as usize) as f64 * weight;
    }
    assert!(
        total_weight > 0.0,
        "rendered frame must contain the blue marker"
    );
    (weighted_x / total_weight, weighted_y / total_weight)
}

#[test]
#[ignore = "requires software Vulkan; executed by Native Host Smoke"]
fn native_temporal_translation_matches_published_states() {
    let capture_run = |width, height, y| {
        let mut raster = if (width, height) == (SIZE, SIZE) {
            pollster::block_on(Raster::new())
        } else {
            pollster::block_on(Raster::with_dimensions(width, height))
        };
        let mut frames = Vec::new();
        sample_translation(y, |session, time, expected_x| {
            let context = session.publication_context();
            let pixels = raster.capture(&session.take_renderer_publication());
            let (centroid_x, centroid_y) = blue_centroid(&pixels, width, height);
            // Pixel indices locate centers half a pixel below viewport coordinates.
            // The original 256-square oracle still expects x=31.5,95.5,159.5,223.5.
            // Odd, non-square targets additionally expose row padding and inversion.
            let density = f64::from(height) / f64::from(WORLD_SIZE);
            let expected_pixel_x = expected_x * density + f64::from(width) / 2.0 - 0.5;
            let expected_pixel_y = -y * density + f64::from(height) / 2.0 - 0.5;
            assert!(
                (centroid_x - expected_pixel_x).abs() <= 1.0,
                "{width}x{height}, time {time}: x {centroid_x}, expected {expected_pixel_x}"
            );
            assert!(
                (centroid_y - expected_pixel_y).abs() <= 1.0,
                "{width}x{height}, time {time}: y {centroid_y}, expected {expected_pixel_y}"
            );
            let repeated = raster.capture(&session.take_renderer_publication());
            assert!(pixels == repeated, "repeat capture changed pixels at {time}");
            assert_eq!(session.publication_context(), context);
            assert_eq!(
                session.frame().time,
                time,
                "capture must not advance execution"
            );
            eprintln!(
                "native temporal sample: {width}x{height}, time={time}, centroid=({centroid_x},{centroid_y})"
            );
            frames.push(pixels);
        });
        frames
    };
    for (width, height, y) in [(SIZE, SIZE, 0.0), (257, 129, 2.0), (65, 33, -2.0)] {
        let first = capture_run(width, height, y);
        let second = capture_run(width, height, y);
        assert_eq!(first.len(), 4);
        assert_eq!(second.len(), 4);
        for (index, (left, right)) in first.iter().zip(&second).enumerate() {
            assert!(
                left == right,
                "{width}x{height}: fresh-session pixels differ at sample {index}"
            );
        }
    }
    qualify_export_sampling_pixels();
}

// Couple the real retained raster path to P1 without promoting this software-GPU
// fixture into a production host. Endpoint/prefix images are consumed, not output.
fn qualify_export_sampling_pixels() {
    use std::{cell::RefCell, rc::Rc};

    use noon::integration::{
        ExportFrameOptions, ExportFrameSummary, ExportFrames, ExportFramesStatus, ExportStop,
        FrameRate, HostCallbackId, SemanticMutationTransaction,
    };
    use noon::{ContinuationStep, LiveContinuation, LiveSession, LiveSessionError, Mobject};

    struct Source {
        object: Mobject,
        stage: usize,
    }

    impl LiveContinuation for Source {
        type Error = LiveSessionError;

        fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            let stage = self.stage;
            self.stage += 1;
            match stage {
                0 => Ok(ContinuationStep::Await(live.wait_segment(0.105)?)),
                1 => Ok(ContinuationStep::Await(live.wait_segment(0.207)?)),
                _ => {
                    live.set_translation(&self.object, 2.0, -1.0)?;
                    Ok(ContinuationStep::Finished)
                }
            }
        }
    }

    struct Capture {
        pts: u64,
        source_index: u64,
        requested: f64,
        published: f64,
        held: bool,
        pixels: Vec<u8>,
    }

    struct Run {
        frames: Vec<Capture>,
        trace: Vec<(f64, f64)>,
        summary: ExportFrameSummary,
    }

    fn capture_run(start_frame: u64, hold: f64) -> Run {
        let mut scene = Scene::new();
        let mut object = scene.square(1.0).unwrap();
        object.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
        object.set_stroke_width(0.0).unwrap();
        object.set_translation(-2.0, 1.0).unwrap();
        scene.add(&object).unwrap();
        let callback = HostCallbackId::new(189_604);
        let mut registration = SemanticMutationTransaction::new();
        registration.add_updater(object.node_id(), callback, 0.0, None);
        registration
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let trace = Rc::new(RefCell::new(Vec::new()));
        let callback_trace = Rc::clone(&trace);
        let mut callbacks = noon::RustHostCallbackTable::new();
        callbacks
            .insert(callback, move |context| {
                callback_trace
                    .borrow_mut()
                    .push((context.time(), context.delta_time()));
                let mut transform = context.target_state().transform;
                transform.translation.x += (context.delta_time().powi(2) + 0.03) as f32;
                context.set_target_transform(transform)
            })
            .unwrap();
        let mut program = scene
            .into_live_program(Source { object, stage: 0 })
            .unwrap();
        let options = ExportFrameOptions {
            frame_rate: FrameRate::new(30, 1).unwrap(),
            start_frame,
            stop: ExportStop::SourceEnd,
            max_frames: 32,
            max_transitions_per_sample: 32,
            final_hold_seconds: hold,
        };
        let mut export = ExportFrames::new(&mut program, &mut callbacks, options).unwrap();
        let mut raster = pollster::block_on(Raster::with_dimensions(257, 129));
        let mut frames = Vec::new();
        for _ in 0..1_000 {
            match export.advance().unwrap() {
                ExportFramesStatus::Progress => {}
                ExportFramesStatus::PublicationPending(context) => {
                    {
                        let publication = export.take_renderer_publication().unwrap();
                        assert_eq!(publication.context(), context);
                        drop(raster.capture(&publication));
                    }
                    export.admit_endpoint(context).unwrap();
                }
                ExportFramesStatus::SampleReady(sample) => {
                    let trace_before = trace.borrow().clone();
                    let pixels = {
                        let publication = export.take_renderer_publication().unwrap();
                        assert_eq!(publication.context(), sample.observation.publication);
                        raster.capture(&publication)
                    };
                    assert_eq!(*trace.borrow(), trace_before, "capture invoked a callback");
                    assert_eq!(
                        export.session().frame().time,
                        sample.observation.published_time
                    );
                    if let Some(frame) = sample.frame {
                        assert_eq!(
                            frame.source_sample.authored_time(),
                            sample.observation.requested_time
                        );
                        assert_eq!(pixels.len(), 257 * 129 * 4);
                        frames.push(Capture {
                            pts: frame.pts,
                            source_index: frame.source_sample.index(),
                            requested: sample.observation.requested_time,
                            published: sample.observation.published_time,
                            held: frame.held,
                            pixels,
                        });
                    }
                    export.acknowledge_sample(sample).unwrap();
                }
                ExportFramesStatus::Complete(summary) => {
                    assert_eq!(summary.frames, frames.len() as u64);
                    let trace = trace.borrow().clone();
                    return Run {
                        frames,
                        trace,
                        summary,
                    };
                }
            }
        }
        panic!("real-GPU export fixture exhausted its cooperative step budget");
    }

    let full = capture_run(0, 0.0);
    let crop = capture_run(4, 0.0);
    let held = capture_run(0, 0.1);
    assert_eq!(
        (full.frames.len(), crop.frames.len(), held.frames.len()),
        (10, 6, 13)
    );
    assert_eq!(full.trace, crop.trace);
    assert_eq!(full.trace, held.trace);
    for (index, frame) in full.frames.iter().enumerate() {
        assert_eq!(frame.pts, index as u64);
        assert_eq!(frame.source_index, index as u64);
        assert_eq!(frame.requested, index as f64 / 30.0);
        assert_eq!(frame.published, frame.requested);
        assert!(!frame.held);
        assert!(
            frame.pixels == held.frames[index].pixels,
            "hold changed prefix pixels"
        );
    }
    for (index, (actual, expected)) in crop.frames.iter().zip(&full.frames[4..]).enumerate() {
        assert_eq!(actual.pts, index as u64);
        assert_eq!(actual.source_index, expected.source_index);
        assert_eq!(actual.requested, expected.requested);
        assert_eq!(actual.published, expected.published);
        assert!(
            actual.pixels == expected.pixels,
            "crop pixels differ at {index}"
        );
    }
    let source_end = held.summary.source_end.unwrap();
    for frame in &held.frames[10..] {
        assert!(frame.held && frame.requested > source_end);
        assert_eq!(frame.published, source_end);
        assert!(
            frame.pixels == held.frames[10].pixels,
            "terminal hold changed pixels"
        );
        let (x, y) = blue_centroid(&frame.pixels, 257, 129);
        assert!((x - (128.0 + 2.0 * 129.0 / 8.0)).abs() <= 1.0);
        assert!((y - (64.0 + 129.0 / 8.0)).abs() <= 1.0);
    }
    eprintln!("native export: 10 full / 6 cropped / 13 held frames; identical callback traces");
}
