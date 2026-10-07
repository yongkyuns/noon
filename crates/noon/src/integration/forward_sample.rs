//! Cooperative forward observation over the existing Rust live-program owner.

use std::{error::Error, fmt};

use noon_core::PublicationContext;
use noon_runtime::RendererPublication;

use crate::{
    ExecutionSession, LiveContinuation, LiveProgram, LiveProgramError, LiveProgramStatus,
    RustHostCallbackTable,
};

/// Requested time and the actual coherent runtime observation remain distinct.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleObservation {
    pub requested_time: f64,
    pub published_time: f64,
    pub publication: PublicationContext,
}

/// One bounded cooperative step. No platform event loop or encoder is involved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ForwardSampleStatus {
    /// One source resume or shared segment drive completed; call `advance` again.
    Progress,
    /// Consume this exact endpoint publication, then acknowledge its admission.
    /// It is not an additional output frame.
    PublicationPending(PublicationContext),
    /// Required callbacks and same-time source work have settled at the request.
    Ready(SampleObservation),
    /// The finite source completed at or before the request. Do not relabel its
    /// endpoint as a later frame; the outer export range/hold policy decides.
    SourceFinished(SampleObservation),
}

#[derive(Clone, Copy, Debug)]
enum Phase {
    Driving,
    Endpoint {
        context: PublicationContext,
        consumed: bool,
    },
    Observed {
        observation: SampleObservation,
        finished: bool,
        consumed: bool,
    },
    Failed,
}

/// One forward-sample operation borrowing, not replacing, the canonical program.
///
/// The borrow prevents unrelated source/session mutation while a frame or an
/// endpoint publication is being consumed. Hosts can await GPU/sink readiness
/// between calls; repeated polling at a publication or observation does no new
/// runtime work. There are no sleeps, threads, GPU dependencies, scene copies,
/// or independent animation/callback schedules here.
///
/// This is the direct Rust continuation adapter. FrameGrid is language-neutral;
/// native-Python/Pyodide binding integration is a separate export milestone.
/// A source callback is ordinary synchronous Rust: cooperative limits cannot
/// preempt a callback or continuation that never returns.
///
/// On a sink failure, cancel/drop this request and abandon the export. Taking a
/// publication consumes its accumulated invalidation; this operation does not
/// offer retry/replay of a failed renderer or an encoder.
pub struct ForwardSample<'a, C: LiveContinuation> {
    program: &'a mut LiveProgram<C>,
    callbacks: &'a mut RustHostCallbackTable,
    requested_time: f64,
    remaining_transitions: u32,
    phase: Phase,
}

impl<'a, C: LiveContinuation> ForwardSample<'a, C> {
    pub fn new(
        program: &'a mut LiveProgram<C>,
        callbacks: &'a mut RustHostCallbackTable,
        requested_time: f64,
        max_transitions: u32,
    ) -> Result<Self, ForwardSampleError<C::Error>> {
        let current_time = program.session().frame().time;
        if !requested_time.is_finite() || requested_time < 0.0 || requested_time < current_time {
            return Err(ForwardSampleError::InvalidTime {
                requested: requested_time,
                current: current_time,
            });
        }
        if max_transitions == 0 {
            return Err(ForwardSampleError::TransitionLimit);
        }
        Ok(Self {
            program,
            callbacks,
            requested_time,
            remaining_transitions: max_transitions,
            phase: Phase::Driving,
        })
    }

    /// Observe the existing runtime without exposing mutable session authority.
    pub fn session(&self) -> &ExecutionSession {
        self.program.session()
    }

    fn observation(&self) -> SampleObservation {
        SampleObservation {
            requested_time: self.requested_time,
            published_time: self.session().frame().time,
            publication: self.session().publication_context(),
        }
    }

    fn spend_transition(&mut self) -> Result<(), ForwardSampleError<C::Error>> {
        if self.remaining_transitions == 0 {
            self.phase = Phase::Failed;
            return Err(ForwardSampleError::TransitionLimit);
        }
        self.remaining_transitions -= 1;
        Ok(())
    }

    /// Perform at most one source resume or existing shared segment drive.
    ///
    /// At an exact boundary, admit the incoming endpoint before resuming source
    /// work. Continue through same-time edits/zero waits before reporting Ready.
    /// The existing LiveProgram performs all callback and completion ordering.
    pub fn advance(&mut self) -> Result<ForwardSampleStatus, ForwardSampleError<C::Error>> {
        match self.phase {
            Phase::Endpoint { context, .. } => {
                return Ok(ForwardSampleStatus::PublicationPending(context));
            }
            Phase::Observed {
                observation,
                finished,
                ..
            } => {
                return Ok(if finished {
                    ForwardSampleStatus::SourceFinished(observation)
                } else {
                    ForwardSampleStatus::Ready(observation)
                });
            }
            Phase::Failed => return Err(ForwardSampleError::Inactive),
            Phase::Driving => {}
        }

        match self.program.status() {
            LiveProgramStatus::PublicationPending(context) => {
                self.phase = Phase::Endpoint {
                    context,
                    consumed: false,
                };
                Ok(ForwardSampleStatus::PublicationPending(context))
            }
            LiveProgramStatus::Finished => {
                let observation = self.observation();
                self.phase = Phase::Observed {
                    observation,
                    finished: true,
                    consumed: false,
                };
                Ok(ForwardSampleStatus::SourceFinished(observation))
            }
            LiveProgramStatus::Terminal => {
                self.phase = Phase::Failed;
                Err(ForwardSampleError::Inactive)
            }
            LiveProgramStatus::ReadyToResume => {
                self.spend_transition()?;
                if let Err(error) = self.program.resume() {
                    self.phase = Phase::Failed;
                    return Err(ForwardSampleError::Program(error));
                }
                Ok(ForwardSampleStatus::Progress)
            }
            LiveProgramStatus::Awaiting(_) => {
                self.spend_transition()?;
                let status = match self.program.drive_to(self.callbacks, self.requested_time) {
                    Ok(status) => status,
                    Err(error) => {
                        self.phase = Phase::Failed;
                        return Err(ForwardSampleError::Program(error));
                    }
                };
                if matches!(status, LiveProgramStatus::Awaiting(_)) {
                    let observation = self.observation();
                    if observation.published_time != self.requested_time {
                        self.phase = Phase::Failed;
                        return Err(ForwardSampleError::InvalidTime {
                            requested: self.requested_time,
                            current: observation.published_time,
                        });
                    }
                    self.phase = Phase::Observed {
                        observation,
                        finished: false,
                        consumed: false,
                    };
                    Ok(ForwardSampleStatus::Ready(observation))
                } else {
                    Ok(ForwardSampleStatus::Progress)
                }
            }
        }
    }

    /// Consume one coherent publication at an endpoint or a settled observation.
    ///
    /// A host must prepare/retain the publication before acknowledging an
    /// endpoint; a successful call alone does not prove a GPU submission.
    /// Borrowed resource lifetimes stay enforced by the existing publication API.
    pub fn take_renderer_publication(
        &mut self,
    ) -> Result<RendererPublication<'_>, ForwardSampleError<C::Error>> {
        let consumed = match &mut self.phase {
            Phase::Endpoint { consumed, .. } | Phase::Observed { consumed, .. } => consumed,
            _ => return Err(ForwardSampleError::NoPublication),
        };
        if *consumed {
            return Err(ForwardSampleError::PublicationAlreadyConsumed);
        }
        *consumed = true;
        Ok(self.program.take_renderer_publication())
    }

    /// Admit exactly the endpoint delivered above, without producing a video frame.
    pub fn admit_endpoint(
        &mut self,
        context: PublicationContext,
    ) -> Result<(), ForwardSampleError<C::Error>> {
        let Phase::Endpoint {
            context: expected,
            consumed,
        } = self.phase
        else {
            return Err(ForwardSampleError::NoPublication);
        };
        if context != expected {
            return Err(ForwardSampleError::WrongPublication);
        }
        if !consumed {
            return Err(ForwardSampleError::PublicationNotConsumed);
        }
        // Do not bypass LiveProgram's exact-context and pending-dirty checks.
        self.program
            .admit_publication(context)
            .map_err(ForwardSampleError::Program)?;
        self.phase = Phase::Driving;
        Ok(())
    }

    /// Stop this request cooperatively. No future advance will invoke user code.
    /// The outer host owns failure cleanup and must not publish a successful file.
    pub fn cancel(&mut self) {
        self.phase = Phase::Failed;
    }
}

#[derive(Debug)]
pub enum ForwardSampleError<E> {
    InvalidTime { requested: f64, current: f64 },
    TransitionLimit,
    Inactive,
    NoPublication,
    WrongPublication,
    PublicationNotConsumed,
    PublicationAlreadyConsumed,
    Program(LiveProgramError<E>),
}

impl<E: fmt::Display> fmt::Display for ForwardSampleError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTime { requested, current } => {
                write!(
                    f,
                    "invalid forward sample {requested} at runtime time {current}"
                )
            }
            Self::TransitionLimit => write!(f, "forward sample exhausted its transition budget"),
            Self::Inactive => write!(f, "forward sample or source is inactive"),
            Self::NoPublication => write!(f, "no publication is available in this sample phase"),
            Self::WrongPublication => write!(f, "endpoint receipt names a different publication"),
            Self::PublicationNotConsumed => write!(f, "consume the endpoint before admitting it"),
            Self::PublicationAlreadyConsumed => {
                write!(f, "sample publication was already consumed")
            }
            Self::Program(error) => error.fmt(f),
        }
    }
}

impl<E: Error + 'static> Error for ForwardSampleError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Program(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
