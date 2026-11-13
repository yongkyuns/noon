//! Cold bootstrap and boundary relay for a host-serviced ordinary Rust stack.

use super::{BlockingBoundary, ScriptError};
use crate::{
    ContinuationStep, LiveContinuation, LiveProgram, LiveProgramError, LiveProgramStatus,
    RustHostCallbackTable, Scene,
};

/// The single cold Scene and callable table before a blocking source's first boundary.
///
/// Construction and updater registration finish here before lowering. Converting
/// this setup consumes both owners once; later edits use the same live program.
pub struct BlockingSceneSetup {
    scene: Scene,
    callbacks: RustHostCallbackTable,
}

impl Default for BlockingSceneSetup {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockingSceneSetup {
    pub fn new() -> Self {
        Self {
            scene: Scene::new(),
            callbacks: RustHostCallbackTable::new(),
        }
    }

    /// Run one short authoring operation before bootstrap.
    pub fn with_scene(
        &mut self,
        operation: &mut dyn FnMut(
            &mut Scene,
            &mut RustHostCallbackTable,
        ) -> Result<(), ScriptError>,
    ) -> Result<(), ScriptError> {
        operation(&mut self.scene, &mut self.callbacks)
    }

    /// Lower the initial setup once and retain its original callable identities.
    ///
    /// An empty relay is idle, not finished. The first boundary or explicit source
    /// completion supplies its first continuation step through the live program.
    pub fn into_program(self) -> Result<LiveProgram<BlockingContinuation>, ScriptError> {
        self.scene
            .into_live_program(BlockingContinuation {
                pending: None,
                callbacks: self.callbacks,
            })
            .map_err(Into::into)
    }
}

/// One pending existing boundary and the source's original callback table.
///
/// The synchronous source runs outside this continuation on its ordinary call
/// stack. This relay never invokes source code, advances time, or owns a second
/// Scene. Its default opaque effect contract prevents speculative source work.
pub struct BlockingContinuation {
    pending: Option<ContinuationStep>,
    callbacks: RustHostCallbackTable,
}

impl LiveContinuation for BlockingContinuation {
    type Error = ScriptError;

    fn is_ready(&self) -> bool {
        self.pending.is_some()
    }

    fn resume(
        &mut self,
        _scene: &mut crate::ContinuationScene,
    ) -> Result<ContinuationStep, Self::Error> {
        self.pending.take().ok_or(ScriptError::Inactive)
    }

    fn callbacks(&mut self) -> Option<&mut RustHostCallbackTable> {
        Some(&mut self.callbacks)
    }
}

impl LiveProgram<BlockingContinuation> {
    /// Run one short source operation only after the preceding boundary is admitted.
    ///
    /// The closure borrows this program's existing Scene and callable table. It
    /// must return before the platform host services or suspends the author stack.
    pub fn with_authoring_scene(
        &mut self,
        operation: &mut dyn FnMut(
            &mut Scene,
            &mut RustHostCallbackTable,
        ) -> Result<(), ScriptError>,
    ) -> Result<(), ScriptError> {
        self.with_external_authoring(|scene, continuation| {
            operation(scene, &mut continuation.callbacks)
        })
        .map_err(source_error)?
    }

    /// Validate and activate one boundary before handing it to the existing program.
    ///
    /// Admission errors return to the ordinary source without terminalizing an
    /// otherwise ready program. Successful admission retains the normal callback,
    /// logical completion, and exact renderer-publication barriers.
    pub fn begin_boundary(&mut self, boundary: BlockingBoundary) -> Result<(), ScriptError> {
        self.with_external_authoring(|scene, continuation| {
            let step = (boundary.operation)(scene)?;
            if matches!(step, ContinuationStep::Finished) {
                return Err(ScriptError::InvalidBoundary);
            }
            continuation.pending = Some(step);
            Ok(())
        })
        .map_err(source_error)??;
        self.resume().map(|_| ()).map_err(source_error)
    }

    /// Whether the previous boundary has completed and its publication was admitted.
    /// An initial empty program also permits authoring without inventing a wait.
    pub fn boundary_complete(&self) -> bool {
        self.status() == LiveProgramStatus::ReadyToResume && !self.can_resume()
    }

    /// Finish explicitly, including a source that never requested a boundary.
    /// Pending final renderer work remains available to the owning platform host.
    pub fn finish_authoring(&mut self) -> Result<(), ScriptError> {
        self.with_external_authoring(|_scene, continuation| {
            continuation.pending = Some(ContinuationStep::Finished);
        })
        .map_err(source_error)?;
        self.resume().map(|_| ()).map_err(source_error)
    }

    /// Retire input-owned work and reject all later source writes or boundaries.
    pub fn cancel_authoring(&mut self) -> Result<(), ScriptError> {
        self.cancel_external_authoring().map_err(source_error)
    }
}

fn source_error(error: LiveProgramError<ScriptError>) -> ScriptError {
    match error {
        LiveProgramError::Continuation(error) => error,
        LiveProgramError::Callback(error) => ScriptError::Callback(error),
        LiveProgramError::Completion(error) => ScriptError::LiveSession(error),
        LiveProgramError::InvalidState {
            state: LiveProgramStatus::Terminal,
            ..
        } => ScriptError::Cancelled,
        LiveProgramError::InvalidState {
            state: LiveProgramStatus::Finished,
            ..
        } => ScriptError::Inactive,
        other => ScriptError::Host(other.to_string()),
    }
}
