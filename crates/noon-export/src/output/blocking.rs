//! A bounded host for explicit, once-only ordinary Rust source preparation.

use std::{
    cell::{Cell, RefCell},
    error::Error,
    fmt, io,
    panic::{catch_unwind, resume_unwind, AssertUnwindSafe},
    rc::Rc,
};

use noon::integration::{
    BlockingBoundary, BlockingContinuation, BlockingReplayError, BlockingSceneHost,
    BlockingSceneSetup, ExportFrameOptions, ExportFramePolicy, ReplayLimits,
};
use noon::{
    BlockingScene, LiveProgram, LiveProgramStatus, RustHostCallbackTable, Scene, SceneExecution,
    ScriptError,
};

use super::{
    export_scene_file, FileExportSummary, OutputFormat, OutputOptions, SessionFileExportError,
};
use crate::{gpu::NativeCapture, CaptureError, CaptureOptions};

#[cfg(test)]
mod tests;

/// Bounds on source preparation, independent of the number of exported frames.
#[derive(Clone, Copy, Debug)]
pub struct BlockingSourceLimits {
    /// Each constructor, mutation, query and boundary consumes one host operation.
    pub max_operations: u64,
    /// Includes zero-duration waits and resource-only boundaries.
    pub max_boundaries: u64,
    /// The existing runtime's sparse revision and immutable payload budgets.
    pub history: ReplayLimits,
}

impl Default for BlockingSourceLimits {
    fn default() -> Self {
        Self {
            max_operations: 100_000,
            max_boundaries: 10_000,
            history: ReplayLimits::default(),
        }
    }
}

/// Export an ordinary synchronous scene without a window or a source rewrite.
///
/// Accepts the same synchronous source signature as `noon_native::run_scene`
/// and executes that function exactly once.
/// Each `play`/`wait` completes through its existing engine-owned boundary and
/// retains the resulting publication. On source completion, the original Scene
/// and runtime transfer together to [`super::export_scene_file`]; sampling and
/// rendering use the sealed history and never call the source again.
/// Preparation is live from time zero: use the supplied `BlockingScene` for
/// mutations throughout, so each edit receives ordinary coherent publication.
///
/// This explicit finite visual profile rejects required callbacks, unrecorded
/// input, effective display producers and unsupported history, rather than
/// executing their work only at segment endpoints. Retained-history audio is
/// unsupported. Resource preparation and ordinary transactional constructor
/// failures keep their normal admission rules.
///
/// Defaults bound source operations, boundaries and retained history separately
/// from `frame_options.max_frames`. Cancellation is checked before the source,
/// between host operations, at every boundary and during output. Arbitrary Rust
/// code between host calls cannot be preempted. A source panic retires its
/// capability before unwinding. Retained capability clones cannot write after
/// success, failure or cancellation. Files use the existing output transaction;
/// source/preparation failure does not touch the destination.
pub fn export_blocking_scene(
    construct: impl FnOnce(&BlockingScene) -> Result<(), ScriptError>,
    frame_options: ExportFrameOptions,
    capture_options: CaptureOptions,
    output: OutputOptions,
) -> Result<FileExportSummary, BlockingFileExportError> {
    export_blocking_scene_with_limits(
        construct,
        frame_options,
        capture_options,
        output,
        BlockingSourceLimits::default(),
    )
}

/// The ordinary source exporter with explicit finite preparation budgets.
pub fn export_blocking_scene_with_limits(
    construct: impl FnOnce(&BlockingScene) -> Result<(), ScriptError>,
    frame_options: ExportFrameOptions,
    capture_options: CaptureOptions,
    output: OutputOptions,
    limits: BlockingSourceLimits,
) -> Result<FileExportSummary, BlockingFileExportError> {
    // Validate host options before invoking effectful application code. The final
    // finite range is admitted after the one source execution supplies its end.
    capture_options
        .layout()
        .map_err(BlockingFileExportError::Capture)?;
    output
        .validate(
            frame_options.frame_rate,
            capture_options.width,
            capture_options.height,
        )
        .map_err(|error| BlockingFileExportError::Export(SessionFileExportError::Output(error)))?;
    if output.format == OutputFormat::PngSequence && frame_options.max_frames > i32::MAX as u64 {
        return Err(BlockingFileExportError::Export(
            SessionFileExportError::Output(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PNG frame limit exceeds image2 numbering",
            )),
        ));
    }
    ExportFramePolicy::new(frame_options).map_err(|error| {
        BlockingFileExportError::Export(SessionFileExportError::Sampling(error))
    })?;
    if capture_options.cancellation.is_cancelled() {
        return Err(BlockingFileExportError::Cancelled);
    }

    let host = BlockingExportHost::new(capture_options.clone(), limits)?;
    let source = host.capability();
    let result = catch_unwind(AssertUnwindSafe(|| construct(&source)));
    let source_result = match result {
        Ok(result) => result,
        Err(panic) => {
            host.retire();
            resume_unwind(panic);
        }
    };
    if let Some(error) = host.take_failure() {
        host.retire();
        return Err(error);
    }
    if let Err(error) = source_result {
        host.retire();
        return Err(BlockingFileExportError::Source(error));
    }
    let mut scene = host.finish()?;
    let result = catch_unwind(AssertUnwindSafe(|| {
        export_scene_file(&mut scene, frame_options, capture_options, output)
    }));
    let retirement = scene.retire_execution_source();
    match result {
        Ok(Ok(summary)) => {
            retirement.map_err(|error| {
                BlockingFileExportError::Export(SessionFileExportError::Playback(error))
            })?;
            Ok(summary)
        }
        Ok(Err(error)) => Err(BlockingFileExportError::Export(error)),
        Err(panic) => resume_unwind(panic),
    }
}

struct HostState {
    program: Option<Box<LiveProgram<BlockingContinuation>>>,
    resource_gpu: Option<Box<NativeCapture>>,
    operations: u64,
    boundaries: u64,
    failure: Option<BlockingFileExportError>,
}

struct BlockingExportHost {
    active: Cell<bool>,
    panicked: Cell<bool>,
    state: RefCell<HostState>,
    capture: CaptureOptions,
    limits: BlockingSourceLimits,
}

impl BlockingExportHost {
    fn new(
        capture: CaptureOptions,
        limits: BlockingSourceLimits,
    ) -> Result<Rc<Self>, BlockingFileExportError> {
        // Start history before any source edit, including time-zero construction.
        // Every operation therefore uses ordinary live publication and exactly
        // one canonical semantic owner; there is no cold/live duplicate state.
        let mut program = BlockingSceneSetup::new()
            .into_program()
            .map_err(BlockingFileExportError::Source)?;
        program.require_effect_resource_preparation();
        program
            .begin_retained_authoring(limits.history)
            .map_err(BlockingFileExportError::Preparation)?;
        Ok(Rc::new(Self {
            active: Cell::new(true),
            panicked: Cell::new(false),
            state: RefCell::new(HostState {
                program: Some(Box::new(program)),
                resource_gpu: None,
                operations: 0,
                boundaries: 0,
                failure: None,
            }),
            capture,
            limits,
        }))
    }

    fn capability(self: &Rc<Self>) -> BlockingScene {
        let host: Rc<dyn BlockingSceneHost> = self.clone();
        BlockingScene::for_host(Rc::downgrade(&host))
    }

    fn fail(&self, state: &mut HostState, error: BlockingFileExportError) -> ScriptError {
        self.active.set(false);
        let message = error.to_string();
        if state.failure.is_none() {
            state.failure = Some(error);
        }
        if let Some(program) = state.program.as_mut() {
            let _ = program.cancel_authoring();
        }
        ScriptError::Host(message)
    }

    fn check_cancelled(&self, state: &mut HostState) -> Result<(), ScriptError> {
        if self.capture.cancellation.is_cancelled() {
            return Err(self.fail(state, BlockingFileExportError::Cancelled));
        }
        Ok(())
    }

    fn admit_operation(&self, state: &mut HostState, boundary: bool) -> Result<(), ScriptError> {
        self.check_cancelled(state)?;
        if state.operations >= self.limits.max_operations {
            return Err(self.fail(
                state,
                BlockingFileExportError::SourceLimit {
                    kind: "operations",
                    limit: self.limits.max_operations,
                },
            ));
        }
        if boundary && state.boundaries >= self.limits.max_boundaries {
            return Err(self.fail(
                state,
                BlockingFileExportError::SourceLimit {
                    kind: "boundaries",
                    limit: self.limits.max_boundaries,
                },
            ));
        }
        state.operations += 1;
        state.boundaries += u64::from(boundary);
        Ok(())
    }

    fn validate(&self, state: &mut HostState) -> Result<(), ScriptError> {
        if self.panicked.get() {
            return Err(self.fail(state, BlockingFileExportError::SourcePanicked));
        }
        let result = state
            .program
            .as_ref()
            .ok_or(ScriptError::Inactive)?
            .validate_retained_authoring();
        if let Err(error) = result {
            return Err(self.fail(state, BlockingFileExportError::Preparation(error)));
        }
        self.check_cancelled(state)
    }

    fn take_failure(&self) -> Option<BlockingFileExportError> {
        self.state.borrow_mut().failure.take().or_else(|| {
            self.panicked
                .get()
                .then_some(BlockingFileExportError::SourcePanicked)
        })
    }

    fn finish(&self) -> Result<SceneExecution, BlockingFileExportError> {
        self.active.set(false);
        // Take the owner before dropping any source-held values, so retained
        // capabilities cannot reenter it during teardown or final validation.
        let (mut program, resource_gpu) = {
            let mut state = self.state.borrow_mut();
            (
                state
                    .program
                    .take()
                    .expect("active source owns its program"),
                state.resource_gpu.take(),
            )
        };
        drop(resource_gpu);
        let result = (|| {
            if self.capture.cancellation.is_cancelled() {
                return Err(BlockingFileExportError::Cancelled);
            }
            program
                .finish_authoring()
                .map_err(BlockingFileExportError::Source)?;
            program
                .validate_retained_authoring()
                .map_err(BlockingFileExportError::Preparation)
        })();
        if let Err(error) = result {
            let _ = program.cancel_authoring();
            return Err(error);
        }
        program
            .into_retained_execution()
            .map_err(BlockingFileExportError::Preparation)
    }

    fn retire(&self) {
        self.active.set(false);
        let (program, resource_gpu) = {
            let mut state = self.state.borrow_mut();
            (state.program.take(), state.resource_gpu.take())
        };
        if let Some(mut program) = program {
            let _ = program.cancel_authoring();
        }
        drop(resource_gpu);
    }
}

impl Drop for BlockingExportHost {
    fn drop(&mut self) {
        self.retire();
    }
}

// Declared before each mutable state borrow, so an unwind releases that borrow
// before this guard retires the host. The independent latch also covers a nested
// panic while another operation still owns the borrow. Catching such a panic in
// application code cannot turn a partially completed host call into success.
struct HostCallGuard<'a>(&'a BlockingExportHost);

impl Drop for HostCallGuard<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.active.set(false);
            self.0.panicked.set(true);
            if let Ok(mut state) = self.0.state.try_borrow_mut() {
                self.0
                    .fail(&mut state, BlockingFileExportError::SourcePanicked);
            }
        }
    }
}

impl BlockingSceneHost for BlockingExportHost {
    fn with_scene(
        &self,
        operation: &mut dyn FnMut(
            &mut Scene,
            &mut RustHostCallbackTable,
        ) -> Result<(), ScriptError>,
    ) -> Result<(), ScriptError> {
        if !self.active.get() {
            return Err(ScriptError::Cancelled);
        }
        let _guard = HostCallGuard(self);
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ScriptError::Reentrant)?;
        self.admit_operation(&mut state, false)?;
        let result = state
            .program
            .as_mut()
            .ok_or(ScriptError::Inactive)?
            .with_authoring_scene(operation);
        // A normal transactional constructor failure can be caught by source.
        // An invalid history or unsupported callback/input cannot be repaired by
        // swallowing the host error and continuing toward a successful export.
        self.validate(&mut state)?;
        result
    }

    fn wait(&self, boundary: BlockingBoundary) -> Result<(), ScriptError> {
        if !self.active.get() {
            return Err(ScriptError::Cancelled);
        }
        let _guard = HostCallGuard(self);
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ScriptError::Reentrant)?;
        self.admit_operation(&mut state, true)?;
        let result = state
            .program
            .as_mut()
            .ok_or(ScriptError::Inactive)?
            .begin_boundary(boundary);
        self.validate(&mut state)?;
        result?;
        let preparation = (|| {
            if state.program.as_ref().unwrap().status() == LiveProgramStatus::PreparingResources {
                if state.resource_gpu.is_none() {
                    let layout = self
                        .capture
                        .layout()
                        .map_err(BlockingFileExportError::Capture)?;
                    let gpu = pollster::block_on(NativeCapture::new(&self.capture, layout))
                        .map_err(BlockingFileExportError::Capture)?;
                    state.resource_gpu = Some(Box::new(gpu));
                }
                let HostState {
                    program,
                    resource_gpu,
                    ..
                } = &mut *state;
                resource_gpu
                    .as_mut()
                    .unwrap()
                    .prepare_effect_modules(
                        program
                            .as_ref()
                            .unwrap()
                            .pending_effect_programs()
                            .expect("resource boundary owns programs"),
                    )
                    .map_err(BlockingFileExportError::Capture)?;
                if self.capture.cancellation.is_cancelled() {
                    return Err(BlockingFileExportError::Cancelled);
                }
                program
                    .as_mut()
                    .unwrap()
                    .publish_prepared_effects()
                    .map_err(|error| {
                        BlockingFileExportError::Preparation(BlockingReplayError::Program(error))
                    })?;
            }
            state
                .program
                .as_mut()
                .unwrap()
                .complete_retained_boundary()
                .map_err(BlockingFileExportError::Preparation)
        })();
        if let Err(error) = preparation {
            return Err(self.fail(&mut state, error));
        }
        self.validate(&mut state)
    }
}

#[derive(Debug)]
pub enum BlockingFileExportError {
    Source(ScriptError),
    Preparation(BlockingReplayError),
    SourceLimit { kind: &'static str, limit: u64 },
    SourcePanicked,
    Cancelled,
    Capture(CaptureError),
    Export(SessionFileExportError),
}

impl fmt::Display for BlockingFileExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "blocking scene source: {error}"),
            Self::Preparation(error) => write!(formatter, "blocking scene preparation: {error}"),
            Self::SourceLimit { kind, limit } => write!(
                formatter,
                "blocking scene exceeds its {limit} source {kind} limit"
            ),
            Self::SourcePanicked => formatter
                .write_str("blocking scene host operation panicked; the source was retired"),
            Self::Cancelled => formatter.write_str("blocking scene export cancelled"),
            Self::Capture(error) => error.fmt(formatter),
            Self::Export(error) => error.fmt(formatter),
        }
    }
}

impl Error for BlockingFileExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Preparation(error) => Some(error),
            Self::Capture(error) => Some(error),
            Self::Export(error) => Some(error),
            Self::SourceLimit { .. } | Self::SourcePanicked | Self::Cancelled => None,
        }
    }
}
