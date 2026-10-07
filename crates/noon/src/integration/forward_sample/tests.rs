//! P1 fake-sink qualification: real typed source/runtime, no GPU or encoder.
//! Endpoint publications are consumed separately from the global output grid.

use std::{cell::RefCell, rc::Rc, time::Duration};

use crate::integration::{
    ForwardSample, ForwardSampleError, ForwardSampleStatus, FrameGrid, FrameRate,
    HostCallbackId, SemanticMutationTransaction,
};
use crate::{
    ContinuationStep, LiveContinuation, LiveProgram, LiveProgramStatus, LiveSession,
    LiveSessionError, Mobject, RustHostCallbackTable, Scene,
};

const CALLBACK: HostCallbackId = HostCallbackId::new(1_896);
const FIRST_END: f64 = 0.105;
const SECOND_DURATION: f64 = 0.207;

struct Segments {
    marker: Mobject,
    stage: usize,
    resumes: Rc<RefCell<Vec<usize>>>,
}

impl LiveContinuation for Segments {
    type Error = LiveSessionError;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let stage = self.stage;
        self.stage += 1;
        self.resumes.borrow_mut().push(stage);
        match stage {
            0 => Ok(ContinuationStep::Await(live.wait_segment(FIRST_END)?)),
            1 => {
                let y = f64::from(live.effective(&self.marker)?.transform.translation.y);
                live.set_translation(&self.marker, 2.0, y)?;
                Ok(ContinuationStep::Await(live.wait_segment(SECOND_DURATION)?))
            }
            2 => Ok(ContinuationStep::Await(live.wait_segment(0.0)?)),
            3 => {
                let y = f64::from(live.effective(&self.marker)?.transform.translation.y);
                live.set_translation(&self.marker, 9.0, y)?;
                Ok(ContinuationStep::Finished)
            }
            _ => panic!("a completed source must never resume again"),
        }
    }
}

struct Fixture {
    program: LiveProgram<Segments>,
    callbacks: RustHostCallbackTable,
    trace: Rc<RefCell<Vec<(f64, f64)>>>,
    resumes: Rc<RefCell<Vec<usize>>>,
}

fn fixture() -> Fixture {
    let mut scene = Scene::new();
    let marker = scene.square(0.5).unwrap();
    scene.add(&marker).unwrap();
    let mut registration = SemanticMutationTransaction::new();
    registration.add_updater(marker.node_id(), CALLBACK, 0.0, None);
    registration.apply(&mut scene.integration_store().borrow_mut()).unwrap();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let observed_trace = Rc::clone(&trace);
    let mut callbacks = RustHostCallbackTable::new();
    callbacks.insert(CALLBACK, move |context| {
        observed_trace.borrow_mut().push((context.time(), context.delta_time()));
        let mut transform = context.target_state().transform;
        transform.translation.y += context.delta_time() as f32;
        context.set_target_transform(transform)
    }).unwrap();
    let resumes = Rc::new(RefCell::new(Vec::new()));
    let program = scene.into_live_program(Segments {
        marker,
        stage: 0,
        resumes: Rc::clone(&resumes),
    }).unwrap();
    Fixture { program, callbacks, trace, resumes }
}

#[derive(Clone, Debug, PartialEq)]
struct Observation {
    requested: f64,
    published: f64,
    x: f32,
    y: f32,
    finished: bool,
}

fn observe(fixture: &mut Fixture, requested: f64, slow: bool, endpoints: &mut Vec<f64>) -> Observation {
    let trace = Rc::clone(&fixture.trace);
    let mut sample = ForwardSample::new(&mut fixture.program, &mut fixture.callbacks, requested, 32).unwrap();
    for _ in 0..100 {
        let status = sample.advance().unwrap();
        match status {
            ForwardSampleStatus::Progress => {}
            ForwardSampleStatus::PublicationPending(expected) => {
                assert!(matches!(sample.admit_endpoint(expected), Err(ForwardSampleError::PublicationNotConsumed)));
                let time = sample.session().frame().time;
                let trace_before = trace.borrow().clone();
                if slow {
                    std::thread::sleep(Duration::from_millis(1));
                    for _ in 0..7 {
                        assert_eq!(sample.advance().unwrap(), status);
                    }
                }
                assert_eq!(*trace.borrow(), trace_before);
                let receipt = sample.take_renderer_publication().unwrap().context();
                assert_eq!(receipt, expected);
                assert!(matches!(sample.take_renderer_publication(), Err(ForwardSampleError::PublicationAlreadyConsumed)));
                sample.admit_endpoint(receipt).unwrap();
                endpoints.push(time);
            }
            ForwardSampleStatus::Ready(metadata) | ForwardSampleStatus::SourceFinished(metadata) => {
                let frame = sample.session().frame();
                let object = &frame.objects[0];
                let observation = Observation {
                    requested: metadata.requested_time,
                    published: metadata.published_time,
                    x: object.transform.translation.x,
                    y: object.transform.translation.y,
                    finished: matches!(status, ForwardSampleStatus::SourceFinished(_)),
                };
                let trace_before = trace.borrow().clone();
                assert_eq!(sample.take_renderer_publication().unwrap().context(), metadata.publication);
                if slow {
                    std::thread::sleep(Duration::from_millis(1));
                }
                // Neither observation/capture nor a stalled output sink invokes an updater.
                for _ in 0..7 {
                    assert_eq!(sample.advance().unwrap(), status);
                }
                assert_eq!(*trace.borrow(), trace_before);
                assert_eq!(sample.session().frame().time, observation.published);
                return observation;
            }
        }
    }
    panic!("bounded sample did not settle");
}

#[derive(Debug, PartialEq)]
struct Run {
    frames: Vec<Observation>,
    terminal: Observation,
    endpoints: Vec<f64>,
    callbacks: Vec<(f64, f64)>,
    resumes: Vec<usize>,
}

fn run(slow: bool) -> Run {
    let mut fixture = fixture();
    let grid = FrameGrid::new(FrameRate::new(30, 1).unwrap(), 0.0).unwrap();
    let mut frames = Vec::new();
    let mut endpoints = Vec::new();
    for index in 0..64 {
        let request = grid.sample(index).unwrap();
        let observation = observe(&mut fixture, request.authored_time(), slow, &mut endpoints);
        if observation.finished {
            assert_eq!(grid.frame_count_before(observation.published, 64).unwrap(), frames.len() as u64);
            assert_eq!(fixture.program.status(), LiveProgramStatus::Finished);
            let callback_trace = fixture.trace.borrow().clone();
            let resume_trace = fixture.resumes.borrow().clone();
            return Run {
                frames,
                terminal: observation,
                endpoints,
                callbacks: callback_trace,
                resumes: resume_trace,
            };
        }
        assert_eq!(request.pts(), index);
        assert_eq!(request.time_base(), (1, 30));
        assert_eq!(observation.requested, observation.published);
        frames.push(observation);
    }
    panic!("source failed to finish within the frame cap");
}

#[test]
fn non_grid_endpoints_and_callbacks_do_not_insert_output_frames() {
    let run = run(false);
    assert_eq!(run.frames.len(), 10);
    assert_eq!(run.resumes, [0, 1, 2, 3]);
    assert!(run.endpoints.contains(&FIRST_END));
    assert!(run.endpoints.iter().any(|&t| (t - 0.312).abs() < 1.0e-12));
    for (index, frame) in run.frames.iter().enumerate() {
        let expected_time = index as f64 / 30.0;
        assert_eq!(frame.published, expected_time);
        assert_eq!(frame.x, if expected_time < FIRST_END { 0.0 } else { 2.0 });
        assert!((f64::from(frame.y) - expected_time).abs() < 1.0e-6);
    }
    assert_eq!(run.terminal.x, 9.0);
    assert!((run.terminal.published - 0.312).abs() < 1.0e-12);
    assert!(run.terminal.requested > run.terminal.published);
    assert!((run.callbacks.iter().map(|(_, dt)| dt).sum::<f64>() - 0.312).abs() < 1.0e-12);
}

#[test]
fn slow_sink_and_repeated_polling_preserve_frame_and_callback_traces() {
    assert_eq!(run(false), run(true));
}

#[test]
fn exact_boundary_observes_post_completion_source_edits() {
    let mut fixture = fixture();
    let observed = observe(&mut fixture, FIRST_END, false, &mut Vec::new());
    assert!(!observed.finished);
    assert_eq!(observed.published, FIRST_END);
    assert_eq!(observed.x, 2.0);
    assert_eq!(*fixture.resumes.borrow(), [0, 1]);
}

#[test]
fn invalid_times_do_not_resume_source() {
    let mut fixture = fixture();
    for time in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(ForwardSample::new(&mut fixture.program, &mut fixture.callbacks, time, 32), Err(ForwardSampleError::InvalidTime { .. })));
    }
    assert!(fixture.resumes.borrow().is_empty());
    observe(&mut fixture, 0.1, false, &mut Vec::new());
    assert!(matches!(ForwardSample::new(&mut fixture.program, &mut fixture.callbacks, 0.0, 32), Err(ForwardSampleError::InvalidTime { .. })));
}

#[test]
fn cancellation_and_premature_capture_do_not_invoke_source() {
    let mut fixture = fixture();
    let mut sample = ForwardSample::new(&mut fixture.program, &mut fixture.callbacks, 0.1, 32).unwrap();
    assert!(matches!(sample.take_renderer_publication(), Err(ForwardSampleError::NoPublication)));
    sample.cancel();
    assert!(matches!(sample.advance(), Err(ForwardSampleError::Inactive)));
    assert!(fixture.resumes.borrow().is_empty());
}

#[test]
fn missing_callback_is_terminal_and_not_retried() {
    let mut fixture = fixture();
    fixture.callbacks = RustHostCallbackTable::new();
    let mut sample = ForwardSample::new(&mut fixture.program, &mut fixture.callbacks, 0.1, 32).unwrap();
    assert_eq!(sample.advance().unwrap(), ForwardSampleStatus::Progress);
    assert!(matches!(sample.advance(), Err(ForwardSampleError::Program(crate::LiveProgramError::Callback(_)))));
    assert!(matches!(sample.advance(), Err(ForwardSampleError::Inactive)));
    assert_eq!(*fixture.resumes.borrow(), [0]);
}

struct ZeroWaits;

impl LiveContinuation for ZeroWaits {
    type Error = LiveSessionError;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        Ok(ContinuationStep::Await(live.wait_segment(0.0)?))
    }
}

#[test]
fn nonprogressing_zero_time_source_exhausts_the_transition_budget() {
    let mut program = Scene::new().into_live_program(ZeroWaits).unwrap();
    drop(program.take_renderer_publication());
    let mut callbacks = RustHostCallbackTable::new();
    let mut sample = ForwardSample::new(&mut program, &mut callbacks, 1.0, 4).unwrap();
    for _ in 0..4 {
        assert_eq!(sample.advance().unwrap(), ForwardSampleStatus::Progress);
    }
    assert!(matches!(sample.advance(), Err(ForwardSampleError::TransitionLimit)));
    assert!(matches!(sample.advance(), Err(ForwardSampleError::Inactive)));
}

#[test]
fn a_foreign_receipt_cannot_release_the_endpoint() {
    let other = fixture();
    let mut fixture = fixture();
    let foreign = other.program.session().publication_context();
    let mut sample = ForwardSample::new(&mut fixture.program, &mut fixture.callbacks, 0.2, 32).unwrap();
    for _ in 0..32 {
        if let ForwardSampleStatus::PublicationPending(expected) = sample.advance().unwrap() {
            assert_ne!(expected, foreign);
            let receipt = sample.take_renderer_publication().unwrap().context();
            assert!(matches!(sample.admit_endpoint(foreign), Err(ForwardSampleError::WrongPublication)));
            assert_eq!(sample.advance().unwrap(), ForwardSampleStatus::PublicationPending(expected));
            sample.admit_endpoint(receipt).unwrap();
            return;
        }
    }
    panic!("fixture never reached its endpoint");
}
