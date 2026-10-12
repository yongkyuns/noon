//! FPS controls sample density, never authored duration (#1948).
//! CPU/runtime proof; this does not qualify live host clock lifecycle or pixels.
use noon::integration::{
    ExportFrameOptions, ExportFrameSummary, ExportFrames, ExportFramesStatus, ExportStop, FrameRate,
};
use noon::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveSession, LiveSessionError, Mobject,
    RateFunction, RustHostCallbackTable, Scene,
};
use std::time::Duration;

const RATES: [(u32, u32); 8] = [
    (5, 1),
    (15, 1),
    (24, 1),
    (30, 1),
    (60, 1),
    (120, 1),
    (30_000, 1_001),
    (60_000, 1_001),
];

struct Source {
    marker: Mobject,
    target: Mobject,
    waits_left: usize,
    started: bool,
}

impl LiveContinuation for Source {
    type Error = LiveSessionError;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        if self.waits_left > 0 {
            self.waits_left -= 1;
            // Exactly representable, but off-grid at every integer FPS below.
            return live.wait_segment(1.0 / 64.0).map(ContinuationStep::Await);
        }
        if self.started {
            return Ok(ContinuationStep::Finished);
        }
        self.started = true;
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

#[derive(Debug, PartialEq)]
struct Observed {
    pts: u64,
    authored_time: f64,
    x: f32,
}

fn sample(p: u32, q: u32, prefix: usize, delayed: bool) -> (Vec<Observed>, ExportFrameSummary) {
    let mut scene = Scene::new();
    let mut marker = scene.square(0.5).unwrap();
    marker.set_translation(-2.0, 0.0).unwrap();
    scene.add(&marker).unwrap();
    let mut target = marker.target_editor().unwrap();
    target.set_translation(2.0, 0.0).unwrap();
    let mut source = scene
        .into_live_program(Source {
            marker,
            target,
            waits_left: prefix,
            started: false,
        })
        .unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    let options = ExportFrameOptions {
        frame_rate: FrameRate::new(p, q).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 512,
        max_transitions_per_sample: 256,
        final_hold_seconds: 0.0,
    };
    let mut export = ExportFrames::new(&mut source, &mut callbacks, options).unwrap();
    let mut observations = Vec::new();
    for _ in 0..20_000 {
        match export.advance().unwrap() {
            ExportFramesStatus::Progress => {}
            ExportFramesStatus::PublicationPending(expected) => {
                let received = export.take_renderer_publication().unwrap().context();
                assert_eq!(received, expected);
                export.admit_endpoint(received).unwrap();
            }
            ExportFramesStatus::SampleReady(sample) => {
                if let Some(frame) = sample.frame {
                    if delayed && frame.pts.is_multiple_of(7) {
                        // Real consumer delay and repeated polling must not advance a sample.
                        std::thread::sleep(Duration::from_millis(2));
                        for _ in 0..3 {
                            assert_eq!(
                                export.advance().unwrap(),
                                ExportFramesStatus::SampleReady(sample)
                            );
                        }
                    }
                    observations.push(Observed {
                        pts: frame.pts,
                        authored_time: sample.observation.published_time,
                        x: export.session().frame().objects[0].transform.translation.x,
                    });
                }
                let _ = export.take_renderer_publication().unwrap();
                export.acknowledge_sample(sample).unwrap();
            }
            ExportFramesStatus::Complete(summary) => return (observations, summary),
        }
    }
    panic!("finite duration fixture exceeded its cooperative transition budget");
}

#[test]
fn two_second_movement_has_rate_independent_positions_and_end_time() {
    for (p, q) in RATES {
        let (frames, summary) = sample(p, q, 0, false);
        assert_eq!(summary.source_end, Some(2.0));
        assert_eq!(summary.end_time, 2.0);
        let expected = (2 * u64::from(p)).div_ceil(u64::from(q));
        assert_eq!(summary.frames, expected);
        assert_eq!(frames.len() as u64, expected);
        for (index, frame) in frames.iter().enumerate() {
            let time = index as f64 * f64::from(q) / f64::from(p);
            assert_eq!(frame.pts, index as u64);
            assert!((frame.authored_time - time).abs() < 1.0e-12);
            assert!((f64::from(frame.x) - (-2.0 + 2.0 * time)).abs() < 1.0e-5);
        }
        let duration = expected as f64 * f64::from(q) / f64::from(p);
        assert!((summary.scheduled_duration - duration).abs() < 1.0e-12);
        assert!(duration >= 2.0 && duration - 2.0 < f64::from(q) / f64::from(p));
    }
}

#[test]
fn slow_consumer_does_not_change_authored_samples_or_movie_duration() {
    for (p, q) in [(5, 1), (60, 1), (60_000, 1_001)] {
        let normal = sample(p, q, 0, false);
        let delayed = sample(p, q, 0, true);
        assert_eq!(normal, delayed, "consumer delay changed {p}/{q} output");
    }
}

#[test]
fn short_sequential_waits_are_not_rounded_individually_to_video_frames() {
    for (p, q) in RATES {
        // 32 * (1/64) + 2 = 2.5 seconds, even at 5 FPS.
        let (frames, summary) = sample(p, q, 32, false);
        assert_eq!(summary.source_end, Some(2.5));
        assert_eq!(summary.end_time, 2.5);
        let expected = (5 * u64::from(p)).div_ceil(2 * u64::from(q));
        assert_eq!(summary.frames, expected);
        for frame in frames {
            let progress = (frame.authored_time - 0.5).max(0.0) / 2.0;
            assert!((f64::from(frame.x) - (-2.0 + 4.0 * progress)).abs() < 1.0e-5);
        }
    }
}
