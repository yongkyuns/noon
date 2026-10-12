//! Exercise the callback-yield boundary used by interpreter-owned sessions.
//! User callback logic is shared with the Rust reference, not its host driver.
//! This is a Rust integration test, not evidence of a completed Python binding.
#![cfg(not(target_arch = "wasm32"))]

use std::{cell::RefCell, rc::Rc};

use noon::integration::{
    CallbackAdvance, CallbackRegionAdvance, ExportFrameOptions, ExportFramePolicy,
    ExportFramePolicyStatus, ExportFrameSummary, ExportStop, FrameRate, HostCallbackId,
    SampleObservation, SemanticMutationTransaction,
};
use noon::{
    ContinuationStep, ExecutionSegment, ExecutionSession, LiveContinuation, LiveSession,
    LiveSessionError, Mobject, RustHostCallbackTable, Scene, Transform2D,
};
use noon_export::{capture_frames, Backends, CaptureOptions, SessionCapture};

const FIRST: HostCallbackId = HostCallbackId::new(189_641);
const SECOND: HostCallbackId = HostCallbackId::new(189_642);
type Trace = Rc<RefCell<Vec<(u64, f64, f64, f32, f32)>>>;

#[derive(Debug, PartialEq)]
struct Image {
    source_index: u64,
    pts: u64,
    requested: f64,
    published: f64,
    held: bool,
    rgba: Vec<u8>,
}

struct Run {
    images: Vec<Image>,
    trace: Vec<(u64, f64, f64, f32, f32)>,
    summary: ExportFrameSummary,
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

fn with_callbacks() -> (Scene, Mobject, Trace) {
    let mut scene = Scene::new();
    let mut marker = scene.square(1.0).unwrap();
    marker.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    marker.set_stroke_width(0.0).unwrap();
    marker.set_translation(-2.0, 0.0).unwrap();
    scene.add(&marker).unwrap();
    let mut registration = SemanticMutationTransaction::new();
    registration.add_updater(marker.node_id(), FIRST, 0.0, None);
    // The second callback becomes active between output samples, not on the
    // global frame grid. An early Ready result must not become a stale image.
    registration.add_updater(marker.node_id(), SECOND, 0.047, None);
    registration
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    (scene, marker, Rc::new(RefCell::new(Vec::new())))
}

fn user_callback(
    trace: &Trace,
    id: HostCallbackId,
    time: f64,
    dt: f64,
    mut transform: Transform2D,
) -> Transform2D {
    trace.borrow_mut().push((
        id.get(),
        time,
        dt,
        transform.translation.x,
        transform.translation.y,
    ));
    if id == FIRST {
        // Skipping a prefix sample or invoking the callback twice changes state.
        transform.translation.x += (dt.powi(2) + 0.125) as f32;
    } else {
        assert_eq!(id, SECOND);
        // Observe the first callback's write: reversing invocation order fails.
        transform.translation.y += transform.translation.x * 0.015 + dt as f32 * 0.1;
    }
    transform
}

fn reference(options: ExportFrameOptions) -> Run {
    let (scene, marker, trace) = with_callbacks();
    let mut callbacks = RustHostCallbackTable::new();
    for id in [FIRST, SECOND] {
        let trace = Rc::clone(&trace);
        callbacks
            .insert(id, move |context| {
                let transform = user_callback(
                    &trace,
                    context.callback_id(),
                    context.time(),
                    context.delta_time(),
                    context.target_state().transform,
                );
                context.set_target_transform(transform)
            })
            .unwrap();
    }
    let mut program = scene
        .into_live_program(Source { marker, stage: 0 })
        .unwrap();
    let mut images = Vec::new();
    let summary = capture_frames(
        &mut program,
        &mut callbacks,
        options,
        capture_options(),
        |frame| {
            images.push(Image {
                source_index: frame.frame.source_sample.index(),
                pts: frame.frame.pts,
                requested: frame.observation.requested_time,
                published: frame.observation.published_time,
                held: frame.frame.held,
                rgba: frame.rgba.to_vec(),
            });
            Ok::<_, std::convert::Infallible>(())
        },
    )
    .unwrap();
    let trace = trace.borrow().clone();
    Run {
        images,
        trace,
        summary: summary.sampling,
    }
}

// Test-only interpreter-style host: Rust selects phases/occurrences/regions;
// the host invokes user code and returns its writes through the canonical API.
fn host_advance(
    session: &mut ExecutionSession,
    segment: ExecutionSegment,
    requested: f64,
    trace: &Trace,
    delayed: bool,
) {
    let target = requested.min(segment.end_time());
    for _ in 0..128 {
        let mut region = match session
            .advance_segment_to_callback_barrier(segment, target)
            .unwrap()
        {
            CallbackAdvance::Ready(frame) => {
                if frame.time == target {
                    return;
                }
                continue;
            }
            CallbackAdvance::HostRequired {
                invocations,
                overlay,
            } => CallbackRegionAdvance::HostRequired {
                invocations,
                overlay,
            },
        };
        let mut committed = false;
        for _ in 0..128 {
            region = match region {
                CallbackRegionAdvance::HostRequired {
                    invocations,
                    mut overlay,
                } => {
                    let token = overlay.token();
                    let before = session.publication_context();
                    assert_eq!(session.pending_callback_token(), Some(token));
                    if delayed {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                    for invocation in invocations {
                        let id = invocation.target();
                        let transform = user_callback(
                            trace,
                            invocation.callback_id(),
                            overlay.time(),
                            overlay.delta_time(),
                            overlay.object(id).unwrap().transform,
                        );
                        overlay.set_transform(id, transform).unwrap();
                    }
                    // Host-local edits must not publish an incomplete frame.
                    assert_eq!(session.publication_context(), before);
                    session
                        .submit_required_callback_region(overlay.finish())
                        .unwrap()
                }
                CallbackRegionAdvance::Complete(batch) => {
                    session.commit_required_callback_phase(batch).unwrap();
                    committed = true;
                    break;
                }
            };
        }
        assert!(committed, "callback region budget exhausted");
    }
    panic!("callback request budget exhausted");
}

fn interpreter_style(options: ExportFrameOptions, delayed: bool) -> Run {
    let (scene, marker, trace) = with_callbacks();
    let mut session = scene.execution_session().unwrap();
    let mut target = SessionCapture::new(&session, capture_options()).unwrap();
    let mut policy = ExportFramePolicy::new(options).unwrap();
    let mut stage = 0;
    let mut segment = Some(scene.live(&mut session).wait_segment(0.105).unwrap());
    let mut images = Vec::new();
    for _ in 0..1_000 {
        match policy.status().unwrap() {
            ExportFramePolicyStatus::NeedsSample(requested) => {
                while let Some(current) = segment {
                    host_advance(&mut session, current, requested, &trace, delayed);
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
                let trace_before = trace.borrow().len();
                for _ in 0..3 {
                    assert_eq!(
                        policy.status().unwrap(),
                        ExportFramePolicyStatus::SampleReady(sample)
                    );
                }
                if let Some(frame) = sample.frame {
                    let pixels = target.capture(&mut session).unwrap();
                    assert_eq!(pixels.receipt.publication, sample.observation.publication);
                    assert_eq!(frame.pts, images.len() as u64);
                    images.push(Image {
                        source_index: frame.source_sample.index(),
                        pts: frame.pts,
                        requested: sample.observation.requested_time,
                        published: pixels.receipt.published_time,
                        held: frame.held,
                        rgba: pixels.rgba.to_vec(),
                    });
                } else {
                    target.render(&mut session).unwrap();
                }
                assert_eq!(
                    trace.borrow().len(),
                    trace_before,
                    "capture invoked user code"
                );
                policy.acknowledge_sample(sample).unwrap();
            }
            ExportFramePolicyStatus::Complete(summary) => {
                let trace = trace.borrow().clone();
                return Run {
                    images,
                    trace,
                    summary,
                };
            }
        }
    }
    panic!("interpreter-style fixture exhausted request budget");
}

#[test]
#[ignore = "requires software Vulkan; selected by native output gate"]
fn callback_yields_preserve_pixels_timestamps_and_source_history() {
    let base = ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 64,
        max_transitions_per_sample: 64,
        final_hold_seconds: 0.0,
    };
    for (options, expected) in [
        (base, 10),
        (
            ExportFrameOptions {
                start_frame: 4,
                ..base
            },
            6,
        ),
        (
            ExportFrameOptions {
                final_hold_seconds: 0.1,
                ..base
            },
            13,
        ),
        (
            ExportFrameOptions {
                stop: ExportStop::EndTime(0.115),
                ..base
            },
            4,
        ),
        (
            ExportFrameOptions {
                frame_rate: FrameRate::new(60_000, 1_001).unwrap(),
                ..base
            },
            19,
        ),
    ] {
        let normal = reference(options);
        let external = interpreter_style(options, true);
        assert_eq!(normal.images.len(), expected);
        assert_eq!(external.images.len(), expected);
        assert!(!normal.trace.is_empty());
        assert!(normal.trace.iter().any(|entry| entry.0 == FIRST.get()));
        assert!(normal.trace.iter().any(|entry| entry.0 == SECOND.get()));
        assert_eq!(
            normal.trace, external.trace,
            "callback order/dt/read history diverged"
        );
        assert_eq!(normal.summary, external.summary);
        // Keep failing logs bounded: a full RGBA-vector debug dump is not useful.
        for (index, (a, b)) in normal.images.iter().zip(&external.images).enumerate() {
            assert!(
                a == b,
                "callback-host frame metadata/pixels differ at {index}"
            );
        }
    }
}
