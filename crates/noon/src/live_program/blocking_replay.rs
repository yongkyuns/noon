//! Explicit finite preparation of an ordinary source on its existing owner.

use super::{LiveProgram, LiveProgramError, LiveProgramPhase, LiveProgramStatus};
use crate::{
    scene_script::BlockingContinuation, RustHostCallbackTable, SceneExecution, SceneExecutionError,
    ScriptError, SourceExecutionRequirement,
};
use noon_runtime::{ReplayError, ReplayLimits};

/// Failure to retain or transfer an explicitly executed synchronous source.
#[derive(Debug)]
pub enum BlockingReplayError {
    Program(LiveProgramError<ScriptError>),
    /// This source needs the ordinary observed callback/input host.
    RequiresObservedExecution(SourceExecutionRequirement),
    Replay(ReplayError),
    Scene(SceneExecutionError),
}

impl std::fmt::Display for BlockingReplayError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Program(error) => error.fmt(formatter),
            Self::RequiresObservedExecution(requirement) => write!(
                formatter,
                "deterministic source preparation requires observed execution: {requirement:?}"
            ),
            Self::Replay(error) => error.fmt(formatter),
            Self::Scene(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for BlockingReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Program(error) => Some(error),
            Self::Replay(error) => Some(error),
            Self::Scene(error) => Some(error),
            Self::RequiresObservedExecution(_) => None,
        }
    }
}

impl LiveProgram<BlockingContinuation> {
    /// Opt into bounded history before invoking an ordinary finite source.
    ///
    /// This is explicit source execution, not speculative preparation of an
    /// opaque continuation. The host calls the source once and checks
    /// [`Self::validate_retained_authoring`] after each source operation.
    pub fn begin_retained_authoring(
        &mut self,
        limits: ReplayLimits,
    ) -> Result<(), BlockingReplayError> {
        if !self.boundary_complete() || self.session().frame().time != 0.0 {
            return Err(BlockingReplayError::Program(self.invalid_state(
                "begin finite blocking source retention at time zero",
            )));
        }
        self.scene
            .owned_execution_mut()
            .begin_replay_retention(limits)
            .map_err(BlockingReplayError::Replay)?;
        self.validate_retained_authoring()
    }

    /// Reuse the engine's callback/input/domain admission and original journal.
    ///
    /// A host must check before advancing a boundary, including after each
    /// source edit. Failure never executes a callback, advances time or repairs
    /// an invalidated history. The host must then retire this invocation.
    pub fn validate_retained_authoring(&self) -> Result<(), BlockingReplayError> {
        if self.phase == LiveProgramPhase::Terminal {
            return Err(BlockingReplayError::Program(
                self.invalid_state("retain a cancelled blocking source"),
            ));
        }
        if let Some(requirement) = self.session().source_execution_requirement() {
            return Err(BlockingReplayError::RequiresObservedExecution(requirement));
        }
        self.session()
            .replay_retention_status()
            .map_err(BlockingReplayError::Replay)
    }

    /// Complete the current engine-owned boundary and admit its retained result.
    ///
    /// No sampling clock is supplied by the host: the existing segment supplies
    /// its exact endpoint and shared completion applies the final semantic state.
    /// Retention admits the matching publication; this is not a displayed-frame
    /// receipt. Pending external resources must be prepared and published through
    /// the ordinary resource handshake before calling this method.
    pub fn complete_retained_boundary(&mut self) -> Result<(), BlockingReplayError> {
        self.validate_retained_authoring()?;
        match self.phase {
            LiveProgramPhase::Awaiting(segment) => {
                self.drive_to(&mut RustHostCallbackTable::new(), segment.end_time())
                    .map_err(BlockingReplayError::Program)?;
            }
            LiveProgramPhase::PublicationPending(_) => {}
            _ => {
                return Err(BlockingReplayError::Program(
                    self.invalid_state("complete a retained blocking boundary"),
                ));
            }
        }
        self.validate_retained_authoring()?;
        let needs_admission = matches!(self.phase, LiveProgramPhase::PublicationPending(_));
        let publication = self.take_renderer_publication().context();
        if needs_admission {
            self.admit_publication(publication)
                .map_err(BlockingReplayError::Program)?;
        }
        debug_assert!(self.boundary_complete());
        Ok(())
    }

    /// Consume a finished source into its exact sealed scene/runtime pair.
    ///
    /// The caller first retires its authoring capability. Every unfinished,
    /// cancelled, resource-pending or unsupported source is rejected. Failure
    /// consumes and retires this invocation; it must not be resumed or retried.
    /// No Scene is copied or lowered again, and playback never reruns the source.
    pub fn into_retained_execution(mut self) -> Result<SceneExecution, BlockingReplayError> {
        let ready = (|| {
            if self.status() != LiveProgramStatus::Finished || self.pending_effects.is_some() {
                return Err(BlockingReplayError::Program(
                    self.invalid_state("transfer a finished blocking source"),
                ));
            }
            self.validate_retained_authoring()?;
            self.retire_execution_source()
                .map_err(BlockingReplayError::Program)
        })();
        if let Err(error) = ready {
            let _ = self.cancel_external_authoring();
            return Err(error);
        }

        let Self {
            mut scene,
            continuation,
            pending_effects,
            ..
        } = self;
        // Callable captures are source-owned values, so their destructors run
        // before final admission and owner validation, not after a successful
        // transfer. The relay itself never executes the ordinary source.
        drop(continuation);
        drop(pending_effects);
        scene
            .owned_execution_mut()
            .seal_replay_for_viewer()
            .map_err(BlockingReplayError::Replay)?;
        scene
            .take()
            .into_execution()
            .map_err(BlockingReplayError::Scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        integration::{BlockingBoundary, BlockingSceneSetup},
        ContinuationStep,
    };
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn only_a_completed_and_finished_source_can_transfer_its_owner() {
        for pending in [false, true] {
            let mut program = BlockingSceneSetup::new().into_program().unwrap();
            program
                .begin_retained_authoring(ReplayLimits::default())
                .unwrap();
            if pending {
                program
                    .begin_boundary(BlockingBoundary {
                        operation: Box::new(|scene| {
                            Ok(ContinuationStep::Await(
                                scene.owned_live().wait_segment(1.0)?,
                            ))
                        }),
                    })
                    .unwrap();
            }
            assert!(matches!(
                program.into_retained_execution(),
                Err(BlockingReplayError::Program(
                    LiveProgramError::InvalidState { .. }
                ))
            ));
        }
        let mut program = BlockingSceneSetup::new().into_program().unwrap();
        program
            .begin_retained_authoring(ReplayLimits::default())
            .unwrap();
        program
            .begin_boundary(BlockingBoundary {
                operation: Box::new(|scene| {
                    Ok(ContinuationStep::Await(
                        scene.owned_live().wait_segment(1.0)?,
                    ))
                }),
            })
            .unwrap();
        let identity = program.session().runtime_identity();
        program.complete_retained_boundary().unwrap();
        assert!(program.boundary_complete());
        assert_eq!(program.session().frame().time, 1.0);
        program.finish_authoring().unwrap();
        let scene = program.into_retained_execution().unwrap();
        assert_eq!(scene.session().runtime_identity(), identity);
        assert_eq!(scene.session().replay_end_time(), Some(1.0));
    }

    #[test]
    fn an_existing_required_callback_is_classified_before_any_headless_advance() {
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let mut setup = BlockingSceneSetup::new();
        let mut observed = Some(observed);
        setup
            .with_scene(&mut |scene, callbacks| {
                let square = scene.square(0.8)?;
                scene.add(&square)?;
                let id = noon_core::HostCallbackId::new(812);
                let observed = observed.take().unwrap();
                callbacks.insert(id, move |_| {
                    observed.set(observed.get() + 1);
                    Ok::<(), ScriptError>(())
                })?;
                let mut transaction = noon_core::SemanticMutationTransaction::new();
                transaction.add_updater(square.node_id(), id, 0.0, None);
                scene.apply_semantic_transaction(transaction)?;
                Ok(())
            })
            .unwrap();
        let mut program = setup.into_program().unwrap();
        assert!(matches!(
            program.begin_retained_authoring(ReplayLimits::default()),
            Err(BlockingReplayError::RequiresObservedExecution(
                SourceExecutionRequirement::HostCallbacks
            ))
        ));
        assert_eq!(calls.get(), 0);
        assert_eq!(program.session().frame().time, 0.0);
        assert!(program.into_retained_execution().is_err());
        assert_eq!(calls.get(), 0);
    }
}
