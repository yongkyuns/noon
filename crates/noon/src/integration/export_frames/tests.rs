use std::{cell::RefCell, rc::Rc};

use super::*;
use crate::integration::{HostCallbackId, SemanticMutationTransaction};
use crate::{ContinuationStep, LiveSession, LiveSessionError, Mobject, Scene};

const CALLBACK: HostCallbackId = HostCallbackId::new(1_896);

struct Source {
    object: Mobject,
    stage: usize,
    durations: [f64; 2],
}

impl LiveContinuation for Source {
    type Error = LiveSessionError;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let stage = self.stage;
        self.stage += 1;
        if stage < self.durations.len() {
            Ok(ContinuationStep::Await(
                live.wait_segment(self.durations[stage])?,
            ))
        } else {
            let y = f64::from(live.effective(&self.object)?.transform.translation.y);
            live.set_translation(&self.object, 7.0, y)?;
            Ok(ContinuationStep::Finished)
        }
    }
}

struct Fixture {
    program: LiveProgram<Source>,
    callbacks: RustHostCallbackTable,
    trace: Rc<RefCell<Vec<(f64, f64)>>>,
}

fn fixture(durations: [f64; 2], with_callback: bool) -> Fixture {
    let mut scene = Scene::new();
    let object = scene.square(0.5).unwrap();
    scene.add(&object).unwrap();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = RustHostCallbackTable::new();
    if with_callback {
        let mut registration = SemanticMutationTransaction::new();
        registration.add_updater(object.node_id(), CALLBACK, 0.0, None);
        registration
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let trace = Rc::clone(&trace);
        callbacks
            .insert(CALLBACK, move |context| {
                trace
                    .borrow_mut()
                    .push((context.time(), context.delta_time()));
                let mut transform = context.target_state().transform;
                // Nonlinear-in-dt update exposes skipped prefix samples, unlike y += dt.
                transform.translation.y += (context.delta_time().powi(2) + 1.0) as f32;
                context.set_target_transform(transform)
            })
            .unwrap();
    }
    let program = scene
        .into_live_program(Source {
            object,
            stage: 0,
            durations,
        })
        .unwrap();
    Fixture {
        program,
        callbacks,
        trace,
    }
}

fn options() -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 1_000,
        max_transitions_per_sample: 32,
        final_hold_seconds: 0.0,
    }
}

#[derive(Debug, PartialEq)]
struct Captured {
    pts: u64,
    source_index: u64,
    time: f64,
    actual: f64,
    held: bool,
    x: f32,
    y: f32,
}

fn drive(
    fixture: &mut Fixture,
    options: ExportFrameOptions,
    delayed: bool,
) -> Result<(Vec<Captured>, ExportFrameSummary), ExportFramesError<LiveSessionError>> {
    let mut export = ExportFrames::new(&mut fixture.program, &mut fixture.callbacks, options)?;
    let mut frames = Vec::new();
    for _ in 0..20_000 {
        match export.advance()? {
            ExportFramesStatus::Progress => {}
            ExportFramesStatus::PublicationPending(context) => {
                let publication = export.take_renderer_publication()?;
                assert_eq!(publication.context(), context);
                drop(publication);
                export.admit_endpoint(context)?;
            }
            ExportFramesStatus::SampleReady(sample) => {
                assert!(matches!(
                    export.acknowledge_sample(sample),
                    Err(ExportFramesError::Sample(
                        ForwardSampleError::PublicationNotConsumed
                    ))
                ));
                let trace_before = fixture.trace.borrow().clone();
                if delayed {
                    for _ in 0..7 {
                        assert_eq!(
                            export.advance()?,
                            ExportFramesStatus::SampleReady(sample)
                        );
                    }
                }
                let publication = export.take_renderer_publication()?;
                assert_eq!(publication.context(), sample.observation.publication);
                if let Some(frame) = sample.frame {
                    assert_eq!(sample.kind, ExportSampleKind::Output);
                    let state = publication.frame().objects[0].transform.translation;
                    frames.push(Captured {
                        pts: frame.pts,
                        source_index: frame.source_sample.index(),
                        time: frame.source_sample.authored_time(),
                        actual: sample.observation.published_time,
                        held: frame.held,
                        x: state.x,
                        y: state.y,
                    });
                }
                drop(publication);
                if delayed {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    assert_eq!(export.advance()?, ExportFramesStatus::SampleReady(sample));
                }
                assert_eq!(*fixture.trace.borrow(), trace_before);
                export.acknowledge_sample(sample)?;
            }
            ExportFramesStatus::Complete(summary) => {
                assert_eq!(frames.len() as u64, summary.frames);
                assert_eq!(export.advance()?, ExportFramesStatus::Complete(summary));
                assert!(matches!(
                    export.take_renderer_publication(),
                    Err(ExportFramesError::Inactive)
                ));
                return Ok((frames, summary));
            }
        }
    }
    panic!("test run exceeded its bounded cooperative steps");
}

#[test]
fn natural_completion_drains_off_grid_end_without_adding_a_frame() {
    let mut fixture = fixture([0.105, 0.207], true);
    let (frames, summary) = drive(&mut fixture, options(), false).unwrap();
    assert_eq!(frames.len(), 10);
    assert_eq!(summary.reason, ExportEndReason::SourceEnd);
    assert!((summary.end_time - 0.312).abs() < 1.0e-12);
    assert_eq!(summary.source_end, Some(summary.end_time));
    assert_eq!(summary.scheduled_duration, 10.0 / 30.0);
    assert_eq!(fixture.program.status(), LiveProgramStatus::Finished);
    assert!(frames
        .iter()
        .all(|f| f.time == f.actual && !f.held && f.x == 0.0));
    assert_eq!(
        fixture.program.session().frame().objects[0]
            .transform
            .translation
            .x,
        7.0
    );
}

#[test]
fn cropped_prefix_preserves_stateful_history_and_rebases_pts() {
    let mut full = fixture([0.105, 0.207], true);
    let mut crop = fixture([0.105, 0.207], true);
    let (full_frames, _) = drive(&mut full, options(), false).unwrap();
    let config = ExportFrameOptions {
        start_frame: 4,
        ..options()
    };
    let (cropped, summary) = drive(&mut crop, config, true).unwrap();
    assert_eq!(cropped.len(), 6);
    assert_eq!(summary.start_time, 4.0 / 30.0);
    assert_eq!(*full.trace.borrow(), *crop.trace.borrow());
    for (pts, (actual, expected)) in cropped.iter().zip(&full_frames[4..]).enumerate() {
        assert_eq!(actual.pts, pts as u64);
        assert_eq!(actual.source_index, expected.source_index);
        assert_eq!(
            (actual.time, actual.actual, actual.y),
            (expected.time, expected.actual, expected.y)
        );
    }
}

#[test]
fn explicit_non_grid_stop_never_drives_callbacks_past_the_requested_end() {
    let mut fixture = fixture([0.105, 2.0], true);
    let config = ExportFrameOptions {
        stop: ExportStop::EndTime(0.115),
        ..options()
    };
    let (frames, summary) = drive(&mut fixture, config, false).unwrap();
    assert_eq!(frames.len(), 4);
    assert_eq!(summary.reason, ExportEndReason::RequestedStop);
    assert_eq!(summary.source_end, None);
    assert_eq!(summary.end_time, 0.115);
    assert_eq!(fixture.program.session().frame().time, 0.115);
    assert!(fixture.trace.borrow().iter().all(|&(t, _)| t <= 0.115));
}

#[test]
fn explicit_frame_count_is_relative_to_crop_and_does_not_reset_the_grid() {
    let mut fixture = fixture([1.0, 1.0], true);
    let config = ExportFrameOptions {
        start_frame: 3,
        stop: ExportStop::FrameCount(2),
        ..options()
    };
    let (frames, summary) = drive(&mut fixture, config, false).unwrap();
    assert_eq!(
        frames.iter().map(|f| f.source_index).collect::<Vec<_>>(),
        [3, 4]
    );
    assert_eq!(summary.end_time, 5.0 / 30.0);
    assert_eq!(summary.reason, ExportEndReason::RequestedStop);
}

#[test]
fn exact_cap_completion_succeeds_but_an_active_source_at_cap_fails() {
    let mut complete = fixture([0.5, 0.5], false);
    let config = ExportFrameOptions {
        max_frames: 30,
        ..options()
    };
    let (frames, summary) = drive(&mut complete, config, false).unwrap();
    assert_eq!(frames.len(), 30);
    assert_eq!(summary.reason, ExportEndReason::SourceEnd);
    let mut active = fixture([0.5, 1.0], false);
    assert!(matches!(
        drive(&mut active, config, false),
        Err(ExportFramesError::FrameLimit)
    ));
}

#[test]
fn final_hold_freezes_the_final_source_edit_and_does_not_call_updaters() {
    let mut plain = fixture([0.105, 0.207], true);
    let mut held = fixture([0.105, 0.207], true);
    drive(&mut plain, options(), false).unwrap();
    let config = ExportFrameOptions {
        final_hold_seconds: 0.1,
        ..options()
    };
    let (frames, summary) = drive(&mut held, config, false).unwrap();
    assert_eq!(frames.len(), 13);
    assert_eq!(*plain.trace.borrow(), *held.trace.borrow());
    let end = summary.source_end.unwrap();
    assert!(frames[10..]
        .iter()
        .all(|f| f.held && f.x == 7.0 && f.actual == end && f.time > end));
    assert_eq!(summary.end_time, end + 0.1);
}

#[test]
fn zero_duration_requires_explicit_hold_and_never_invents_one_frame() {
    let mut empty = fixture([0.0, 0.0], false);
    assert!(matches!(
        drive(&mut empty, options(), false),
        Err(ExportFramesError::EmptyInterval)
    ));
    let mut held = fixture([0.0, 0.0], false);
    let config = ExportFrameOptions {
        final_hold_seconds: 0.1,
        ..options()
    };
    let (frames, summary) = drive(&mut held, config, false).unwrap();
    assert_eq!(frames.len(), 3);
    assert_eq!(summary.source_end, Some(0.0));
    assert!(frames.iter().all(|f| f.held && f.actual == 0.0 && f.x == 7.0));
}

#[test]
fn fractional_rate_and_static_waits_keep_all_output_pts() {
    let mut fixture = fixture([0.5, 0.5], false);
    let config = ExportFrameOptions {
        frame_rate: FrameRate::new(60_000, 1_001).unwrap(),
        ..options()
    };
    let (frames, summary) = drive(&mut fixture, config, false).unwrap();
    assert_eq!(frames.len(), 60);
    assert_eq!(summary.frame_rate.time_base(), (1_001, 60_000));
    for (index, frame) in frames.iter().enumerate() {
        assert_eq!(frame.pts, index as u64);
        assert_eq!(frame.time, frame.actual);
    }
}

#[test]
fn slow_output_does_not_change_frames_or_callbacks() {
    let mut fast = fixture([0.105, 0.207], true);
    let mut slow = fixture([0.105, 0.207], true);
    assert_eq!(
        drive(&mut fast, options(), false).unwrap(),
        drive(&mut slow, options(), true).unwrap()
    );
    assert_eq!(*fast.trace.borrow(), *slow.trace.borrow());
}

#[test]
fn invalid_configuration_does_not_invoke_source() {
    for config in [
        ExportFrameOptions {
            max_frames: 0,
            ..options()
        },
        ExportFrameOptions {
            start_frame: 1_000,
            ..options()
        },
        ExportFrameOptions {
            stop: ExportStop::FrameCount(0),
            ..options()
        },
        ExportFrameOptions {
            stop: ExportStop::FrameCount(u64::MAX),
            ..options()
        },
        ExportFrameOptions {
            stop: ExportStop::EndTime(f64::NAN),
            ..options()
        },
        ExportFrameOptions {
            final_hold_seconds: -1.0,
            ..options()
        },
        ExportFrameOptions {
            final_hold_seconds: f64::INFINITY,
            ..options()
        },
        ExportFrameOptions {
            max_transitions_per_sample: 0,
            ..options()
        },
    ] {
        let mut fixture = fixture([1.0, 1.0], true);
        assert!(ExportFrames::new(&mut fixture.program, &mut fixture.callbacks, config).is_err());
        assert_eq!(fixture.program.status(), LiveProgramStatus::ReadyToResume);
        assert!(fixture.trace.borrow().is_empty());
    }
}

#[test]
fn cancellation_and_wrong_acknowledgements_do_not_advance_source() {
    let mut fixture = fixture([1.0, 1.0], true);
    let mut export =
        ExportFrames::new(&mut fixture.program, &mut fixture.callbacks, options()).unwrap();
    for _ in 0..32 {
        if let ExportFramesStatus::SampleReady(sample) = export.advance().unwrap() {
            let mut wrong = sample;
            wrong.observation.requested_time += 1.0;
            assert!(matches!(
                export.acknowledge_sample(wrong),
                Err(ExportFramesError::WrongSample)
            ));
            let trace = fixture.trace.borrow().clone();
            drop(export.take_renderer_publication().unwrap());
            export.cancel();
            assert!(matches!(
                export.acknowledge_sample(sample),
                Err(ExportFramesError::Inactive)
            ));
            assert!(matches!(
                export.advance(),
                Err(ExportFramesError::Inactive)
            ));
            assert_eq!(*fixture.trace.borrow(), trace);
            return;
        }
    }
    panic!("sample did not become ready");
}

#[test]
fn already_started_program_is_rejected_instead_of_skipping_prefix() {
    let mut fixture = fixture([1.0, 1.0], false);
    fixture.program.resume().unwrap();
    assert!(matches!(
        ExportFrames::new(&mut fixture.program, &mut fixture.callbacks, options()),
        Err(ExportFramesError::SourceAlreadyStarted)
    ));
}
