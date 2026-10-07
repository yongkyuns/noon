//! A native language host keeps its original ExecutionSession. Capture consumes
//! that session's publication without replacing it with a Rust continuation.
#![cfg(not(target_arch = "wasm32"))]

use noon::integration::{
    CallbackAdvance, ExportFrameOptions, ExportStop, FrameGrid, FrameRate, HostCallbackId,
    SemanticMutationTransaction,
};
use noon::{
    AnimationOptions, ContinuationStep, ExecutionSegment, ExecutionSession, LiveContinuation,
    LiveSession, LiveSessionError, Mobject, RateFunction, RustHostCallbackTable, Scene,
};
use noon_export::{capture_frames, Backends, CaptureOptions, SessionCapture, SessionCaptureError};

fn options() -> CaptureOptions {
    let mut options = CaptureOptions::new(257, 129);
    options.backends = Backends::VULKAN;
    options.force_fallback_adapter = true;
    options
}

fn make_scene(with_callback: bool) -> (Scene, Mobject, Mobject) {
    let mut scene = Scene::new();
    let mut object = scene.square(1.0).unwrap();
    object.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    object.set_stroke_width(0.0).unwrap();
    object.set_translation(-2.0, 1.0).unwrap();
    scene.add(&object).unwrap();
    let mut target = object.target_editor().unwrap();
    target.set_translation(2.0, -1.0).unwrap();
    if with_callback {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(object.node_id(), HostCallbackId::new(189_604), 0.0, None);
        transaction
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
    }
    (scene, object, target)
}

fn animation() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(0.3)
        .rate_func(RateFunction::Linear)
}

struct Motion {
    object: Mobject,
    target: Mobject,
    started: bool,
}

impl LiveContinuation for Motion {
    type Error = LiveSessionError;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        if self.started {
            return Ok(ContinuationStep::Finished);
        }
        self.started = true;
        live.declare_and_activate_transform_to(&self.object, &self.target, animation())
            .map(ContinuationStep::Await)
    }
}

fn external_session() -> (Scene, ExecutionSession, ExecutionSegment) {
    let (mut scene, object, target) = make_scene(false);
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(&object, &target, animation())
        .unwrap();
    (scene, session, segment)
}

#[test]
fn invalid_configuration_and_precancelled_session_fail_before_gpu_creation() {
    let (mut scene, _, _) = make_scene(false);
    let session = scene.execution_session().unwrap();
    let mut invalid = options();
    invalid.width = 0;
    assert!(matches!(
        SessionCapture::new(&session, invalid),
        Err(SessionCaptureError::Capture(_))
    ));
    let cancelled = options();
    cancelled.cancellation.cancel();
    assert!(matches!(
        SessionCapture::new(&session, cancelled),
        Err(SessionCaptureError::Cancelled)
    ));
}

#[test]
#[ignore = "requires software Vulkan; selected by native output gate"]
fn session_capture_matches_live_program_pixels_without_advancing_source() {
    let rate = FrameRate::new(10, 1).unwrap();
    let grid = FrameGrid::new(rate, 0.0).unwrap();
    let (scene, object, target) = make_scene(false);
    let mut program = scene
        .into_live_program(Motion {
            object,
            target,
            started: false,
        })
        .unwrap();
    let mut reference = Vec::new();
    let summary = capture_frames(
        &mut program,
        &mut RustHostCallbackTable::new(),
        ExportFrameOptions {
            frame_rate: rate,
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: 10,
            max_transitions_per_sample: 32,
            final_hold_seconds: 0.0,
        },
        options(),
        |frame| {
            reference.push(frame.rgba.to_vec());
            Ok::<(), std::convert::Infallible>(())
        },
    )
    .unwrap();
    assert_eq!(summary.sampling.frames, 3);
    let (mut scene, mut session, segment) = external_session();
    let mut capture = SessionCapture::new(&session, options()).unwrap();
    let mut address = None;
    for (index, expected) in reference.iter().enumerate() {
        let time = grid.sample(index as u64).unwrap().authored_time();
        session.advance_segment_to(segment, time).unwrap();
        let context = session.publication_context();
        {
            let frame = capture.capture(&mut session).unwrap();
            assert_eq!(frame.receipt.publication, context);
            assert_eq!(frame.receipt.published_time, time);
            assert_eq!((frame.width, frame.height), (257, 129));
            assert_eq!(frame.rgba.len(), 257 * 129 * 4);
            assert!(
                frame.rgba == expected,
                "capture entry points differ at frame {index}"
            );
            let previous = address.get_or_insert(frame.rgba.as_ptr());
            assert_eq!(*previous, frame.rgba.as_ptr());
        }
        assert_eq!(session.frame().time, time);
        assert_eq!(session.publication_context(), context);
        assert!(capture.capture(&mut session).unwrap().rgba == expected);
        // A new capture target must obtain a complete image even though another
        // renderer has already consumed this publication's accumulated changes.
        let mut late_target = SessionCapture::new(&session, options()).unwrap();
        assert!(late_target.capture(&mut session).unwrap().rgba == expected);
    }
    session.advance_segment_to(segment, 0.3).unwrap();
    let context = session.publication_context();
    let receipt = capture.render(&mut session).unwrap();
    assert_eq!(receipt.publication, context);
    assert_eq!(receipt.published_time, 0.3);
    assert_eq!(session.frame().time, 0.3);
    scene.live(&mut session).complete_segment(segment).unwrap();
}

#[test]
#[ignore = "requires software Vulkan; selected by native output gate"]
fn foreign_runtime_and_cancellation_poison_capture_without_mutating_session() {
    let (_first_scene, mut first, _) = external_session();
    let (_other_scene, mut other, _) = external_session();
    let first_context = first.publication_context();
    let other_context = other.publication_context();
    let mut capture = SessionCapture::new(&first, options()).unwrap();
    assert!(matches!(
        capture.capture(&mut other),
        Err(SessionCaptureError::WrongRuntime)
    ));
    assert_eq!(other.publication_context(), other_context);
    assert!(matches!(
        capture.capture(&mut first),
        Err(SessionCaptureError::Inactive)
    ));
    assert_eq!(first.publication_context(), first_context);

    let configuration = options();
    let cancellation = configuration.cancellation.clone();
    let mut capture = SessionCapture::new(&first, configuration).unwrap();
    cancellation.cancel();
    assert!(matches!(
        capture.capture(&mut first),
        Err(SessionCaptureError::Cancelled)
    ));
    assert!(matches!(
        capture.render(&mut first),
        Err(SessionCaptureError::Inactive)
    ));
    assert_eq!(first.publication_context(), first_context);
    assert_eq!(first.frame().time, 0.0);
}

#[test]
#[ignore = "requires software Vulkan; selected by native output gate"]
fn pending_callback_is_not_captured_completed_or_acknowledged() {
    let (mut scene, _object, _target) = make_scene(true);
    let mut session = scene.execution_session().unwrap();
    let mut capture = SessionCapture::new(&session, options()).unwrap();
    let segment = scene.live(&mut session).wait_segment(0.5).unwrap();
    assert!(matches!(
        session
            .advance_segment_to_callback_barrier(segment, 0.1)
            .unwrap(),
        CallbackAdvance::HostRequired { .. }
    ));
    let token = session.pending_callback_token();
    assert!(token.is_some());
    let context = session.publication_context();
    let time = session.frame().time;
    assert!(matches!(
        capture.capture(&mut session),
        Err(SessionCaptureError::UnsettledCallback)
    ));
    assert_eq!(session.pending_callback_token(), token);
    assert_eq!(session.publication_context(), context);
    assert_eq!(session.frame().time, time);
    assert!(matches!(
        SessionCapture::new(&session, options()),
        Err(SessionCaptureError::UnsettledCallback)
    ));
}
