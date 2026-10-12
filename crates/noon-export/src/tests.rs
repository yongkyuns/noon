use std::cell::Cell;
use std::convert::Infallible;
use std::rc::Rc;

use noon::integration::{ExportStop, FrameRate};
use noon::{ContinuationStep, LiveSession, Scene};

use super::*;

fn frame_options() -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 100,
        max_transitions_per_sample: 32,
        final_hold_seconds: 0.0,
    }
}

struct NeverRun(Rc<Cell<usize>>);

impl LiveContinuation for NeverRun {
    type Error = Infallible;

    fn resume(&mut self, _: &mut LiveSession<'_>) -> Result<ContinuationStep, Infallible> {
        self.0.set(self.0.get() + 1);
        Ok(ContinuationStep::Finished)
    }
}

#[test]
fn invalid_capture_options_fail_before_gpu_creation_or_source_execution() {
    let mut invalid = vec![CaptureOptions::new(0, 1), CaptureOptions::new(1, 0)];
    let mut options = CaptureOptions::new(65, 3);
    options.max_readback_bytes = 1535;
    invalid.push(options);
    let mut options = CaptureOptions::new(65, 3);
    options.gpu_wait_timeout = Duration::ZERO;
    invalid.push(options);
    let mut options = CaptureOptions::new(65, 3);
    options.background[1] = f64::NAN;
    invalid.push(options);
    let mut options = CaptureOptions::new(65, 3);
    options.backends = Backends::empty();
    invalid.push(options);
    for options in invalid {
        let resumes = Rc::new(Cell::new(0));
        let mut program = Scene::new()
            .into_live_program(NeverRun(Rc::clone(&resumes)))
            .unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        let result = capture_frames(
            &mut program,
            &mut callbacks,
            frame_options(),
            options,
            |_| Ok::<_, Infallible>(()),
        );
        assert!(matches!(result, Err(CaptureRunError::Capture(_))));
        assert_eq!(resumes.get(), 0);
    }
}

#[test]
fn pre_cancelled_capture_does_not_create_gpu_or_invoke_source() {
    let resumes = Rc::new(Cell::new(0));
    let mut program = Scene::new()
        .into_live_program(NeverRun(Rc::clone(&resumes)))
        .unwrap();
    let options = CaptureOptions::new(65, 3);
    let token = options.cancellation.clone();
    token.cancel();
    assert!(options.cancellation.is_cancelled());
    let result = capture_frames(
        &mut program,
        &mut RustHostCallbackTable::new(),
        frame_options(),
        options,
        |_| Ok::<_, Infallible>(()),
    );
    assert!(matches!(result, Err(CaptureRunError::Cancelled)));
    assert_eq!(resumes.get(), 0);
}

#[test]
fn invalid_frame_bounds_fail_without_invoking_source() {
    let resumes = Rc::new(Cell::new(0));
    let mut program = Scene::new()
        .into_live_program(NeverRun(Rc::clone(&resumes)))
        .unwrap();
    let options = ExportFrameOptions {
        max_frames: 0,
        ..frame_options()
    };
    let result = capture_frames(
        &mut program,
        &mut RustHostCallbackTable::new(),
        options,
        CaptureOptions::new(65, 3),
        |_| Ok::<_, Infallible>(()),
    );
    assert!(matches!(result, Err(CaptureRunError::Sampling(_))));
    assert_eq!(resumes.get(), 0);
}
