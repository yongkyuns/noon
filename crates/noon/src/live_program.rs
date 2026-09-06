//! Target-neutral ownership for realtime Rust authoring continuations.
//!
//! A [`LiveProgram`] keeps the consumed semantic [`Scene`] paired with the one
//! [`ExecutionSession`] lowered from it. It owns only the control-plane state
//! needed to await one existing [`ExecutionSegment`]. Timeline evaluation,
//! callback ordering, semantic publication, and renderer-facing invalidation
//! remain in their existing owners.

use std::error::Error;

use noon_core::PublicationContext;

use crate::{
    ExecutionSegment, ExecutionSegmentState, ExecutionSession, LiveSession, RendererPublication,
    RustHostCallbackError, RustHostCallbackTable, Scene,
};

/// One application-authored continuation result.
///
/// The continuation owns ordinary Rust locals and semantic handles. It supplies
/// an existing segment created through the borrowed [`LiveSession`], never a
/// host-defined interval, clock, scheduler entry, or semantic identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContinuationStep {
    Await(ExecutionSegment),
    Finished,
}

/// Resumable direct Rust authoring over one shared live scene/session pair.
pub trait LiveContinuation {
    type Error;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error>;
}

/// Observable control-plane state for one live Rust program.
///
/// `PublicationPending` means endpoint completion has coherently published, but
/// the host has not yet admitted that exact publication. The program cannot
/// resume authoring until [`LiveProgram::admit_publication`] receives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LiveProgramStatus {
    ReadyToResume,
    Awaiting(ExecutionSegmentState),
    PublicationPending(PublicationContext),
    Finished,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum LiveProgramPhase {
    ReadyToResume,
    Awaiting(ExecutionSegment),
    PublicationPending(PublicationContext),
    Finished,
    Terminal,
}

/// Failure while driving or resuming one live Rust program.
#[derive(Debug)]
pub enum LiveProgramError<E> {
    InvalidState {
        operation: &'static str,
        state: LiveProgramStatus,
    },
    PublicationMismatch {
        expected: PublicationContext,
        actual: PublicationContext,
    },
    PublicationStillPending {
        expected: PublicationContext,
    },
    Callback(RustHostCallbackError),
    Completion(crate::LiveSessionError),
    Continuation(E),
}

impl<E: std::fmt::Display> std::fmt::Display for LiveProgramError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidState { operation, state } => {
                write!(
                    formatter,
                    "cannot {operation} while live program is {state:?}"
                )
            }
            Self::PublicationMismatch { expected, actual } => write!(
                formatter,
                "cannot resume from publication {actual:?}; expected {expected:?}"
            ),
            Self::PublicationStillPending { expected } => write!(
                formatter,
                "cannot resume before publication {expected:?} is admitted by the host"
            ),
            Self::Callback(error) => error.fmt(formatter),
            Self::Completion(error) => error.fmt(formatter),
            Self::Continuation(error) => error.fmt(formatter),
        }
    }
}

impl<E: Error + 'static> Error for LiveProgramError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Callback(error) => Some(error),
            Self::Completion(error) => Some(error),
            Self::Continuation(error) => Some(error),
            Self::InvalidState { .. }
            | Self::PublicationMismatch { .. }
            | Self::PublicationStillPending { .. } => None,
        }
    }
}

/// One consumed semantic scene, its matching runtime, and one application continuation.
///
/// The program has no public constructor from independent parts, so a host cannot
/// accidentally pair a semantic store with a foreign runtime. It retains at most
/// one existing segment and never creates another timeline or cursor.
pub struct LiveProgram<C> {
    scene: Scene,
    session: ExecutionSession,
    continuation: C,
    phase: LiveProgramPhase,
}

impl Scene {
    /// Consume this scene into one paired live program.
    ///
    /// Lowering happens once before the continuation starts. All later semantic
    /// work is available only through the program's temporary [`LiveSession`].
    pub fn into_live_program<C: LiveContinuation>(
        self,
        continuation: C,
    ) -> Result<LiveProgram<C>, noon_compile::SemanticExecutionLoweringError> {
        let session = self.execution_session()?;
        Ok(LiveProgram {
            scene: self,
            session,
            continuation,
            phase: LiveProgramPhase::ReadyToResume,
        })
    }
}

impl<C: LiveContinuation> LiveProgram<C> {
    /// Borrow the authoritative runtime for frame, wake, camera, and query observations.
    pub const fn session(&self) -> &ExecutionSession {
        &self.session
    }

    /// Consume the runtime's current renderer-facing invalidation without copying state.
    pub fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        self.session.take_renderer_publication()
    }

    pub fn status(&self) -> LiveProgramStatus {
        match self.phase {
            LiveProgramPhase::ReadyToResume => LiveProgramStatus::ReadyToResume,
            LiveProgramPhase::Awaiting(segment) => {
                LiveProgramStatus::Awaiting(self.session.segment_state(segment))
            }
            LiveProgramPhase::PublicationPending(publication) => {
                LiveProgramStatus::PublicationPending(publication)
            }
            LiveProgramPhase::Finished => LiveProgramStatus::Finished,
            LiveProgramPhase::Terminal => LiveProgramStatus::Terminal,
        }
    }

    /// Invoke the application continuation exactly once from a ready barrier.
    ///
    /// A returned segment becomes the only segment this program can drive. A
    /// continuation error terminally closes the program and is never retried.
    pub fn resume(&mut self) -> Result<LiveProgramStatus, LiveProgramError<C::Error>> {
        if self.phase != LiveProgramPhase::ReadyToResume {
            return Err(self.invalid_state("resume authoring"));
        }
        let result = {
            let mut live = self.scene.live(&mut self.session);
            self.continuation.resume(&mut live)
        };
        match result {
            Ok(ContinuationStep::Await(segment)) => {
                self.phase = LiveProgramPhase::Awaiting(segment);
                Ok(self.status())
            }
            Ok(ContinuationStep::Finished) => {
                self.phase = LiveProgramPhase::Finished;
                Ok(self.status())
            }
            Err(error) => {
                self.phase = LiveProgramPhase::Terminal;
                Err(LiveProgramError::Continuation(error))
            }
        }
    }

    /// Drive the current segment through shared callback and completion logic.
    ///
    /// Mid-segment calls remain `Awaiting`. At the exact endpoint, completion is
    /// reconciled once through [`LiveSession`]. If that leaves renderer-facing
    /// work pending, the exact publication must be admitted before `resume`;
    /// otherwise, including an unchanged pure wait, authoring is immediately ready.
    pub fn drive_to(
        &mut self,
        callbacks: &mut RustHostCallbackTable,
        requested_time: f64,
    ) -> Result<LiveProgramStatus, LiveProgramError<C::Error>> {
        let LiveProgramPhase::Awaiting(segment) = self.phase else {
            return Err(self.invalid_state("drive a segment"));
        };
        if let Err(error) = callbacks.advance_segment_to(&mut self.session, segment, requested_time)
        {
            self.phase = LiveProgramPhase::Terminal;
            return Err(LiveProgramError::Callback(error));
        }
        if self.session.frame().time < segment.end_time() {
            return Ok(self.status());
        }

        let completion = {
            let mut live = self.scene.live(&mut self.session);
            live.complete_segment(segment)
        };
        if let Err(error) = completion {
            self.phase = LiveProgramPhase::Terminal;
            return Err(LiveProgramError::Completion(error));
        }
        debug_assert!(self.session.segment_state(segment).is_complete());

        self.phase = if self.session.wake_state().frame_pending() {
            LiveProgramPhase::PublicationPending(self.session.publication_context())
        } else {
            LiveProgramPhase::ReadyToResume
        };
        Ok(self.status())
    }

    /// Acknowledge admission of the exact endpoint publication.
    ///
    /// Rendering and presentation stay host responsibilities. This method only
    /// pins the host acknowledgement to the runtime publication that completion
    /// produced; it cannot be used to skip segment advancement or completion.
    pub fn admit_publication(
        &mut self,
        publication: PublicationContext,
    ) -> Result<LiveProgramStatus, LiveProgramError<C::Error>> {
        let LiveProgramPhase::PublicationPending(expected) = self.phase else {
            return Err(self.invalid_state("admit an endpoint publication"));
        };
        if publication != expected {
            return Err(LiveProgramError::PublicationMismatch {
                expected,
                actual: publication,
            });
        }
        if self.session.wake_state().frame_pending() {
            return Err(LiveProgramError::PublicationStillPending { expected });
        }
        self.phase = LiveProgramPhase::ReadyToResume;
        Ok(self.status())
    }

    fn invalid_state(&self, operation: &'static str) -> LiveProgramError<C::Error> {
        LiveProgramError::InvalidState {
            operation,
            state: self.status(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, convert::Infallible, rc::Rc};

    use noon_core::{
        AnimationOptions, HostCallbackId, RateFunction, SemanticMutationTransaction, SemanticVec3,
        Vec2,
    };

    use super::*;

    struct AnimatedContinuation {
        source: crate::Mobject,
        target: crate::Mobject,
        resumes: Rc<RefCell<usize>>,
        stage: usize,
    }

    impl LiveContinuation for AnimatedContinuation {
        type Error = crate::LiveSessionError;

        fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            *self.resumes.borrow_mut() += 1;
            match self.stage {
                0 => {
                    self.stage = 1;
                    let segment = live.declare_and_activate_transform_to(
                        &self.source,
                        &self.target,
                        AnimationOptions::new()
                            .run_time(2.0)
                            .rate_func(RateFunction::Linear),
                    )?;
                    Ok(ContinuationStep::Await(segment))
                }
                1 => {
                    self.stage = 2;
                    live.set_translation(&self.source, 3.0, -1.0)?;
                    assert_eq!(
                        live.effective(&self.source)?.transform.translation,
                        Vec2::new(3.0, -1.0)
                    );
                    Ok(ContinuationStep::Finished)
                }
                _ => unreachable!("finished continuation must never resume again"),
            }
        }
    }

    #[test]
    fn animated_completion_requires_exact_publication_before_resuming_once() {
        let mut scene = Scene::new();
        let source = scene.circle(0.4).unwrap();
        scene.add(&source).unwrap();
        let mut target = source.target_editor().unwrap();
        target.set_translation(2.0, -1.0).unwrap();
        let resumes = Rc::new(RefCell::new(0));
        let mut program = scene
            .into_live_program(AnimatedContinuation {
                source: source.clone(),
                target,
                resumes: Rc::clone(&resumes),
                stage: 0,
            })
            .unwrap();
        // Admit the initial frame independently of any continuation barrier.
        let initial = program.take_renderer_publication().context();

        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.drive_to(&mut callbacks, 1.0).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let LiveProgramStatus::PublicationPending(endpoint) =
            program.drive_to(&mut callbacks, 9.0).unwrap()
        else {
            panic!("animated endpoint must await its renderer publication")
        };
        assert_eq!(*resumes.borrow(), 1);
        assert!(matches!(
            program.resume(),
            Err(LiveProgramError::InvalidState { .. })
        ));
        assert!(matches!(
            program.admit_publication(initial),
            Err(LiveProgramError::PublicationMismatch { .. })
        ));
        assert!(matches!(
            program.admit_publication(endpoint),
            Err(LiveProgramError::PublicationStillPending { .. })
        ));

        let admitted = program.take_renderer_publication().context();
        assert_eq!(admitted, endpoint);
        assert_eq!(
            program.admit_publication(admitted).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(*resumes.borrow(), 2);
        assert!(matches!(
            program.resume(),
            Err(LiveProgramError::InvalidState { .. })
        ));
        assert_eq!(*resumes.borrow(), 2);
        assert_eq!(
            source.state().unwrap().transform.translation,
            SemanticVec3::new(3.0, -1.0, 0.0)
        );
    }

    struct WaitContinuation {
        stage: usize,
        resumes: Rc<RefCell<usize>>,
    }

    impl LiveContinuation for WaitContinuation {
        type Error = Infallible;

        fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            *self.resumes.borrow_mut() += 1;
            if self.stage == 0 {
                self.stage = 1;
                Ok(ContinuationStep::Await(live.wait_segment(1.0).unwrap()))
            } else {
                Ok(ContinuationStep::Finished)
            }
        }
    }

    #[test]
    fn unchanged_wait_resumes_without_inventing_a_renderer_publication() {
        let mut scene = Scene::new();
        let object = scene.circle(0.4).unwrap();
        scene.add(&object).unwrap();
        let resumes = Rc::new(RefCell::new(0));
        let mut program = scene
            .into_live_program(WaitContinuation {
                stage: 0,
                resumes: Rc::clone(&resumes),
            })
            .unwrap();
        program.take_renderer_publication();
        program.resume().unwrap();
        let LiveProgramStatus::Awaiting(waiting) = program.status() else {
            panic!("wait continuation must expose its shared deadline")
        };
        assert_eq!(waiting.timeline(), crate::TimelineWakeState::Deadline(1.0));
        program
            .drive_to(&mut RustHostCallbackTable::new(), 0.5)
            .unwrap();
        assert_eq!(*resumes.borrow(), 1);
        assert_eq!(
            program
                .drive_to(&mut RustHostCallbackTable::new(), 1.0)
                .unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(*resumes.borrow(), 2);
    }

    #[test]
    fn callback_failure_is_terminal_and_never_retries_continuation() {
        const CALLBACK: HostCallbackId = HostCallbackId::new(71);
        let mut scene = Scene::new();
        let object = scene.circle(0.4).unwrap();
        scene.add(&object).unwrap();
        let mut registration = SemanticMutationTransaction::new();
        registration.add_updater(object.node_id(), CALLBACK, 0.0, None);
        registration.apply(&mut scene.store().borrow_mut()).unwrap();
        let resumes = Rc::new(RefCell::new(0));
        let mut program = scene
            .into_live_program(WaitContinuation {
                stage: 0,
                resumes: Rc::clone(&resumes),
            })
            .unwrap();
        program.take_renderer_publication();
        program.resume().unwrap();

        let error = program
            .drive_to(&mut RustHostCallbackTable::new(), 1.0)
            .unwrap_err();
        assert!(matches!(
            error,
            LiveProgramError::Callback(RustHostCallbackError::UnknownCallback { .. })
        ));
        assert_eq!(program.session().frame().time, 0.0);
        assert_eq!(program.status(), LiveProgramStatus::Terminal);
        assert!(matches!(
            program.resume(),
            Err(LiveProgramError::InvalidState { .. })
        ));
        assert_eq!(*resumes.borrow(), 1);
    }

    #[test]
    fn segment_drive_reuses_ordered_callback_loop_before_publication_admission() {
        const CALLBACK: HostCallbackId = HostCallbackId::new(72);
        let mut scene = Scene::new();
        let object = scene.circle(0.4).unwrap();
        scene.add(&object).unwrap();
        let mut registration = SemanticMutationTransaction::new();
        registration.add_updater(object.node_id(), CALLBACK, 0.0, None);
        registration.apply(&mut scene.store().borrow_mut()).unwrap();
        let callback_count = Rc::new(RefCell::new(0));
        let mut callbacks = RustHostCallbackTable::new();
        let observed_count = Rc::clone(&callback_count);
        callbacks
            .insert(CALLBACK, move |context| {
                *observed_count.borrow_mut() += 1;
                let mut transform = context.target_state().transform;
                transform.translation.y = 1.0;
                context.set_target_transform(transform)
            })
            .unwrap();
        let resumes = Rc::new(RefCell::new(0));
        let mut program = scene
            .into_live_program(WaitContinuation {
                stage: 0,
                resumes: Rc::clone(&resumes),
            })
            .unwrap();
        program.take_renderer_publication();
        program.resume().unwrap();

        let LiveProgramStatus::PublicationPending(endpoint) =
            program.drive_to(&mut callbacks, 1.0).unwrap()
        else {
            panic!("callback effective write must await its endpoint publication")
        };
        assert!(*callback_count.borrow() > 0);
        assert_eq!(*resumes.borrow(), 1);
        let admitted = program.take_renderer_publication().context();
        assert_eq!(admitted, endpoint);
        program.admit_publication(admitted).unwrap();
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(*resumes.borrow(), 2);
    }
}
