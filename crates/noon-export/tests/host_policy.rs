//! The externally owned session and Rust continuation use one output policy.
//! This is a Rust host-integration oracle, not a Python implementation or pass.
#![cfg(not(target_arch = "wasm32"))]

use noon::integration::{
    ExportFrameOptions, ExportFramePolicy, ExportFramePolicyStatus, ExportFrameSummary, ExportStop,
    FrameRate, SampleObservation,
};
use noon::{
    ContinuationStep, LiveContinuation, LiveSession, LiveSessionError, Mobject,
    RustHostCallbackTable, Scene,
};
use noon_export::{capture_frames, Backends, CaptureOptions, SessionCapture};

fn scene() -> (Scene, Mobject) {
    let mut scene = Scene::new();
    let mut marker = scene.square(1.0).unwrap();
    marker.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    marker.set_stroke_width(0.0).unwrap();
    marker.set_translation(-2.0, 0.0).unwrap();
    scene.add(&marker).unwrap();
    (scene, marker)
}

fn capture_options() -> CaptureOptions {
    let mut options = CaptureOptions::new(65, 33);
    options.backends = Backends::VULKAN;
    options.force_fallback_adapter = true;
    options
}

struct Source {
    marker: Mobject,
    stage: usize,
}

impl LiveContinuation for Source {
    type Error = LiveSessionError;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let stage = self.stage;
        self.stage += 1;
        match stage {
            0 => Ok(ContinuationStep::Await(live.wait_segment(0.105)?)),
            1 => {
                live.set_translation(&self.marker, 0.5, 1.0)?;
                Ok(ContinuationStep::Await(live.wait_segment(0.207)?))
            }
            _ => {
                live.set_translation(&self.marker, 2.0, -1.0)?;
                Ok(ContinuationStep::Finished)
            }
        }
    }
}

fn direct(options: ExportFrameOptions) -> (Vec<Vec<u8>>, ExportFrameSummary) {
    let (scene, marker) = scene();
    let mut program = scene.into_live_program(Source { marker, stage: 0 }).unwrap();
    let mut frames = Vec::new();
    let summary = capture_frames(
        &mut program,
        &mut RustHostCallbackTable::new(),
        options,
        capture_options(),
        |frame| {
            assert_eq!(frame.frame.pts, frames.len() as u64);
            frames.push(frame.rgba.to_vec());
            Ok::<_, std::convert::Infallible>(())
        },
    )
    .unwrap();
    (frames, summary.sampling)
}

fn external(options: ExportFrameOptions) -> (Vec<Vec<u8>>, ExportFrameSummary) {
    let (scene, marker) = scene();
    let mut session = scene.execution_session().unwrap();
    let mut target = SessionCapture::new(&session, capture_options()).unwrap();
    let mut policy = ExportFramePolicy::new(options).unwrap();
    let mut stage = 0;
    let mut segment = Some(scene.live(&mut session).wait_segment(0.105).unwrap());
    let mut frames = Vec::new();
    for _ in 0..1_000 {
        match policy.status().unwrap() {
            ExportFramePolicyStatus::NeedsSample(requested) => {
                // This test's explicit sequential source has two static waits.
                // The runtime still owns segment evaluation and completion;
                // the policy does not round endpoints onto the output grid.
                while let Some(current) = segment {
                    session
                        .advance_segment_to(current, requested.min(current.end_time()))
                        .unwrap();
                    if session.frame().time < current.end_time() {
                        break;
                    }
                    scene.live(&mut session).complete_segment(current).unwrap();
                    let receipt = target.render(&mut session).unwrap();
                    assert_eq!(receipt.published_time, current.end_time());
                    if stage == 0 {
                        stage = 1;
                        let mut live = scene.live(&mut session);
                        live.set_translation(&marker, 0.5, 1.0).unwrap();
                        segment = Some(live.wait_segment(0.207).unwrap());
                    } else {
                        scene
                            .live(&mut session)
                            .set_translation(&marker, 2.0, -1.0)
                            .unwrap();
                        segment = None;
                    }
                }
                policy
                    .observe(
                        SampleObservation {
                            requested_time: requested,
                            published_time: session.frame().time,
                            publication: session.publication_context(),
                        },
                        segment.is_none(),
                    )
                    .unwrap();
            }
            ExportFramePolicyStatus::SampleReady(sample) => {
                if let Some(frame) = sample.frame {
                    let pixels = target.capture(&mut session).unwrap();
                    assert_eq!(pixels.receipt.publication, sample.observation.publication);
                    assert_eq!(
                        pixels.receipt.published_time,
                        sample.observation.published_time
                    );
                    assert_eq!(frame.pts, frames.len() as u64);
                    frames.push(pixels.rgba.to_vec());
                } else {
                    target.render(&mut session).unwrap();
                }
                policy.acknowledge_sample(sample).unwrap();
            }
            ExportFramePolicyStatus::Complete(summary) => return (frames, summary),
        }
    }
    panic!("host-policy fixture exceeded its finite request budget");
}

#[test]
#[ignore = "requires software Vulkan; selected by native output gate"]
fn external_session_policy_matches_direct_rust_pixels_for_crops_and_holds() {
    let base = ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 32,
        max_transitions_per_sample: 32,
        final_hold_seconds: 0.0,
    };
    for (start_frame, final_hold_seconds, expected) in [(0, 0.0, 10), (4, 0.0, 6), (0, 0.1, 13)] {
        let options = ExportFrameOptions {
            start_frame,
            final_hold_seconds,
            ..base
        };
        let (direct_pixels, direct_summary) = direct(options);
        let (external_pixels, external_summary) = external(options);
        assert_eq!(direct_pixels.len(), expected);
        assert_eq!(external_pixels.len(), expected);
        assert_eq!(external_summary, direct_summary);
        for (index, (a, b)) in direct_pixels.iter().zip(&external_pixels).enumerate() {
            assert!(a == b, "different source-host pixels at output frame {index}");
        }
        if final_hold_seconds > 0.0 {
            assert!(external_pixels[10..].iter().all(|p| p == &external_pixels[10]));
            assert!(
                external_pixels[9] != external_pixels[10],
                "final source edit was not captured"
            );
        }
    }
}
