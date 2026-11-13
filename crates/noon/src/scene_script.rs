//! Sequential compiled Rust authoring over the existing live-program protocol.
//!
//! The capability borrows the one Scene only during a source poll. An awaitable
//! yields an existing execution/resource boundary; LiveProgram alone advances,
//! completes and admits it before polling source code again.

use std::{
    cell::{Cell, RefCell},
    error::Error,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};

use crate::{
    live_program::ContinuationScene, AuthoringError, ContinuationStep,
    ExecutionSessionCallbackError, LiveContinuation, LiveProgram, LiveSessionError,
    RustHostCallbackError, RustHostCallbackTable, Scene,
};

mod blocking;
mod blocking_program;
mod constructors;
mod family;
mod operations;
pub use blocking::BlockingScene;
pub use blocking::{BlockingBoundary, BlockingSceneHost};
pub use blocking_program::{BlockingContinuation, BlockingSceneSetup};
pub use operations::{UpdaterFrame, UpdaterRegistration, UpdaterTarget};

#[cfg(test)]
mod tests;

/// An error from sequential scene authoring or its scoped capability.
#[derive(Debug)]
pub enum ScriptError {
    Authoring(AuthoringError),
    /// Shared text preparation or semantic resource admission failed.
    #[cfg(any(feature = "native-text", feature = "typst"))]
    Text(crate::TextAuthoringError),
    /// Shared code-listing preparation or publication failed.
    #[cfg(feature = "code")]
    Code(crate::CodeAuthoringError),
    /// Shared coordinate-family preparation or publication failed.
    Coordinate(crate::CoordinateAuthoringError),
    LiveSession(LiveSessionError),
    Callback(RustHostCallbackError),
    CallbackSession(ExecutionSessionCallbackError),
    Lowering(noon_compile::SemanticExecutionLoweringError),
    /// The capability belongs to a stopped source or is used outside its poll.
    Inactive,
    /// A nested operation attempted to borrow the same scene before an edit returned.
    Reentrant,
    /// Source code yielded without awaiting a Noon execution/resource boundary.
    UnsupportedSuspension,
    /// A second execution/resource boundary was polled before the first completed.
    ConcurrentBoundary,
    /// A polled execution/resource boundary was abandoned without awaiting completion.
    UnawaitedBoundary,
    /// A completed awaitable was polled again.
    CompletedBoundary,
    /// An internal awaitable did not return an execution/resource boundary.
    InvalidBoundary,
    /// An updater registration belongs to another script source.
    ForeignUpdaterRegistration,
    /// No additional host callable identity can be allocated in this source.
    CallbackIdentityExhausted,
    /// The host closed or cancelled a synchronous authoring invocation.
    Cancelled,
    /// A platform host failed while servicing a synchronous scene boundary.
    Host(String),
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(error) => error.fmt(formatter),
            #[cfg(any(feature = "native-text", feature = "typst"))]
            Self::Text(error) => error.fmt(formatter),
            #[cfg(feature = "code")]
            Self::Code(error) => error.fmt(formatter),
            Self::Coordinate(error) => error.fmt(formatter),
            Self::LiveSession(error) => error.fmt(formatter),
            Self::Callback(error) => error.fmt(formatter),
            Self::CallbackSession(error) => error.fmt(formatter),
            Self::Lowering(error) => error.fmt(formatter),
            Self::Inactive => formatter.write_str(
                "scene authoring is only available while this scene's script is running",
            ),
            Self::Reentrant => formatter.write_str(
                "finish the current scene operation before starting a nested scene operation",
            ),
            Self::UnsupportedSuspension => formatter.write_str(
                "scene scripts may suspend only at Noon play, wait, or resource boundaries",
            ),
            Self::ConcurrentBoundary => formatter.write_str(
                "await the current scene boundary before starting another; compose simultaneous animations in one play",
            ),
            Self::UnawaitedBoundary => formatter.write_str(
                "a started scene boundary was dropped or left without awaiting its completion",
            ),
            Self::CompletedBoundary => formatter.write_str(
                "a completed scene awaitable cannot be polled again",
            ),
            Self::InvalidBoundary => formatter.write_str(
                "a scene awaitable must produce an execution or resource boundary",
            ),
            Self::ForeignUpdaterRegistration => formatter.write_str(
                "this updater registration belongs to another scene script",
            ),
            Self::CallbackIdentityExhausted => formatter.write_str(
                "this scene script has exhausted its host callback identities",
            ),
            Self::Cancelled => formatter.write_str("scene authoring was cancelled by its host"),
            Self::Host(message) => formatter.write_str(message),
        }
    }
}

impl Error for ScriptError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Authoring(error) => Some(error),
            #[cfg(any(feature = "native-text", feature = "typst"))]
            Self::Text(error) => Some(error),
            #[cfg(feature = "code")]
            Self::Code(error) => Some(error),
            Self::Coordinate(error) => Some(error),
            Self::LiveSession(error) => Some(error),
            Self::Callback(error) => Some(error),
            Self::CallbackSession(error) => Some(error),
            Self::Lowering(error) => Some(error),
            Self::Inactive
            | Self::Reentrant
            | Self::UnsupportedSuspension
            | Self::ConcurrentBoundary
            | Self::UnawaitedBoundary
            | Self::CompletedBoundary
            | Self::InvalidBoundary
            | Self::ForeignUpdaterRegistration
            | Self::CallbackIdentityExhausted
            | Self::Cancelled
            | Self::Host(_) => None,
        }
    }
}

impl From<AuthoringError> for ScriptError {
    fn from(error: AuthoringError) -> Self {
        Self::Authoring(error)
    }
}

#[cfg(any(feature = "native-text", feature = "typst"))]
impl From<crate::TextAuthoringError> for ScriptError {
    fn from(error: crate::TextAuthoringError) -> Self {
        Self::Text(error)
    }
}

#[cfg(feature = "code")]
impl From<crate::CodeAuthoringError> for ScriptError {
    fn from(error: crate::CodeAuthoringError) -> Self {
        Self::Code(error)
    }
}

impl From<crate::CoordinateAuthoringError> for ScriptError {
    fn from(error: crate::CoordinateAuthoringError) -> Self {
        Self::Coordinate(error)
    }
}

impl From<LiveSessionError> for ScriptError {
    fn from(error: LiveSessionError) -> Self {
        Self::LiveSession(error)
    }
}

impl From<RustHostCallbackError> for ScriptError {
    fn from(error: RustHostCallbackError) -> Self {
        Self::Callback(error)
    }
}

impl From<ExecutionSessionCallbackError> for ScriptError {
    fn from(error: ExecutionSessionCallbackError) -> Self {
        Self::CallbackSession(error)
    }
}

impl From<noon_compile::SemanticExecutionLoweringError> for ScriptError {
    fn from(error: noon_compile::SemanticExecutionLoweringError) -> Self {
        Self::Lowering(error)
    }
}

/// A cloneable capability for synchronous scene operations inside an async script.
///
/// Cloning aliases the same scene. The capability neither owns a second scene nor
/// keeps a stopped source alive. Only `play`/`wait`-class operations suspend; normal
/// construction, edits and coherent queries complete before returning.
#[derive(Clone)]
pub struct ScriptScene {
    pub(super) bridge: Weak<RefCell<ScriptBridge>>,
}

impl ScriptScene {
    pub(super) fn with_scene<T>(
        &self,
        operation: impl FnOnce(&mut Scene) -> Result<T, ScriptError>,
    ) -> Result<T, ScriptError> {
        self.with_scene_and_callbacks(|scene, _callbacks| operation(scene))
    }

    pub(super) fn with_scene_and_callbacks<T>(
        &self,
        operation: impl FnOnce(&mut Scene, &mut RustHostCallbackTable) -> Result<T, ScriptError>,
    ) -> Result<T, ScriptError> {
        let bridge = self.bridge.upgrade().ok_or(ScriptError::Inactive)?;
        let mut bridge = bridge
            .try_borrow_mut()
            .map_err(|_| ScriptError::Reentrant)?;
        if let Some(host) = &bridge.host {
            let host = host.upgrade().ok_or(ScriptError::Inactive)?;
            drop(bridge);
            let mut operation = Some(operation);
            let mut result = None;
            host.with_scene(&mut |scene, callbacks| {
                let operation = operation.take().ok_or(ScriptError::Reentrant)?;
                result = Some(operation(scene, callbacks)?);
                Ok(())
            })?;
            return result.ok_or(ScriptError::Inactive);
        }
        let ScriptBridge {
            scene, callbacks, ..
        } = &mut *bridge;
        operation(
            scene.as_mut().ok_or(ScriptError::Inactive)?,
            callbacks.as_mut().ok_or(ScriptError::Inactive)?,
        )
    }

    /// Construct a lazy source boundary; no scene operation runs until first poll.
    pub(super) fn boundary(
        &self,
        operation: impl FnOnce(&mut Scene) -> Result<ContinuationStep, ScriptError> + 'static,
    ) -> ScriptBoundary {
        ScriptBoundary {
            scene: self.clone(),
            operation: Some(Box::new(operation)),
            receipt: Rc::new(Cell::new(BoundaryStatus::New)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BoundaryStatus {
    New,
    Waiting,
    Completed,
    Consumed,
    Abandoned,
}

struct RequestedBoundary {
    receipt: Rc<Cell<BoundaryStatus>>,
    step: Option<ContinuationStep>,
    // The first boundary is activated only after the cold prelude has been
    // lowered once. Later boundaries activate in the already-running Scene.
    operation: Option<Box<BoundaryOperation>>,
    // Initial activation happens after lowering. Return its failure through the
    // same awaitable so author code can handle errors at every source position.
    failure: Option<ScriptError>,
}

pub(super) struct ScriptBridge {
    pub(super) scene: Option<Scene>,
    pub(super) callbacks: Option<RustHostCallbackTable>,
    boundary: Option<RequestedBoundary>,
    // A synchronous source borrows its host's canonical owner only for one
    // ordinary operation. The weak reference cannot keep a retired host alive.
    host: Option<Weak<dyn BlockingSceneHost>>,
}

type BoundaryOperation = dyn FnOnce(&mut Scene) -> Result<ContinuationStep, ScriptError>;

/// One lazy play, wait, or resource boundary in a sequential scene script.
///
/// Await it before starting another boundary. Simultaneous animations belong in
/// one composed play operation. Dropping it before first poll has no scene effect.
#[must_use = "await this operation to run it through the scene's completion boundary"]
pub struct ScriptBoundary {
    scene: ScriptScene,
    operation: Option<Box<BoundaryOperation>>,
    receipt: Rc<Cell<BoundaryStatus>>,
}

impl Future for ScriptBoundary {
    type Output = Result<(), ScriptError>;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let Some(bridge) = this.scene.bridge.upgrade() else {
            return Poll::Ready(Err(ScriptError::Inactive));
        };
        let mut bridge = match bridge.try_borrow_mut() {
            Ok(bridge) => bridge,
            Err(_) => return Poll::Ready(Err(ScriptError::Reentrant)),
        };
        if bridge.scene.is_none() {
            return Poll::Ready(Err(ScriptError::Inactive));
        }

        match this.receipt.get() {
            BoundaryStatus::New => {
                if bridge.boundary.is_some() {
                    this.receipt.set(BoundaryStatus::Consumed);
                    return Poll::Ready(Err(ScriptError::ConcurrentBoundary));
                }
                let operation = this
                    .operation
                    .take()
                    .expect("unpolled scene boundary retains its operation");
                if bridge
                    .scene
                    .as_ref()
                    .expect("active scene checked above")
                    .running_execution()
                    .is_none()
                {
                    this.receipt.set(BoundaryStatus::Waiting);
                    bridge.boundary = Some(RequestedBoundary {
                        receipt: Rc::clone(&this.receipt),
                        step: None,
                        operation: Some(operation),
                        failure: None,
                    });
                    return Poll::Pending;
                }
                let result = operation(bridge.scene.as_mut().expect("active scene checked above"));
                let step = match result {
                    Ok(ContinuationStep::Finished) => {
                        this.receipt.set(BoundaryStatus::Consumed);
                        return Poll::Ready(Err(ScriptError::InvalidBoundary));
                    }
                    Ok(step) => step,
                    Err(error) => {
                        this.receipt.set(BoundaryStatus::Consumed);
                        return Poll::Ready(Err(error));
                    }
                };
                this.receipt.set(BoundaryStatus::Waiting);
                bridge.boundary = Some(RequestedBoundary {
                    receipt: Rc::clone(&this.receipt),
                    step: Some(step),
                    operation: None,
                    failure: None,
                });
                Poll::Pending
            }
            BoundaryStatus::Waiting => Poll::Pending,
            BoundaryStatus::Completed => {
                let matches = bridge
                    .boundary
                    .as_ref()
                    .is_some_and(|boundary| Rc::ptr_eq(&boundary.receipt, &this.receipt));
                if !matches {
                    return Poll::Ready(Err(ScriptError::Inactive));
                }
                let boundary = bridge.boundary.take().expect("boundary matched above");
                this.receipt.set(BoundaryStatus::Consumed);
                Poll::Ready(boundary.failure.map_or(Ok(()), Err))
            }
            BoundaryStatus::Consumed => Poll::Ready(Err(ScriptError::CompletedBoundary)),
            BoundaryStatus::Abandoned => Poll::Ready(Err(ScriptError::UnawaitedBoundary)),
        }
    }
}

impl Drop for ScriptBoundary {
    fn drop(&mut self) {
        if matches!(
            self.receipt.get(),
            BoundaryStatus::Waiting | BoundaryStatus::Completed
        ) {
            self.receipt.set(BoundaryStatus::Abandoned);
        }
    }
}

/// A compiled async scene source driven by [`LiveProgram`].
///
/// The source has no executor, authored clock, or runtime copy. It may suspend only
/// at Noon boundaries, which reuse the live program's existing callback,
/// completion, and renderer-publication protocol on native and WASM targets.
pub struct SceneScript {
    future: Pin<Box<dyn Future<Output = Result<(), ScriptError>>>>,
    bridge: Rc<RefCell<ScriptBridge>>,
    callbacks: Option<RustHostCallbackTable>,
    finished: bool,
    started: bool,
}

impl Scene {
    /// Create a scene authored by ordinary sequential async Rust code.
    ///
    /// Construction runs the source through its initial scene setup to the first
    /// Noon await. That scene is lowered once, including its initial updaters.
    /// Resuming the returned program starts the first awaited operation. Later
    /// source code runs only after its logical completion and publication barrier.
    ///
    /// Only Noon play/wait/resource awaitables may suspend this source; unrelated
    /// pending futures return [`ScriptError::UnsupportedSuspension`].
    pub fn script<F, Fut>(source: F) -> Result<LiveProgram<SceneScript>, ScriptError>
    where
        F: FnOnce(ScriptScene) -> Fut + 'static,
        Fut: Future<Output = Result<(), ScriptError>> + 'static,
    {
        Self::new().into_script(source)
    }

    // The public constructor starts with a fresh scene and callable table. An
    // existing scene may already reference host callables, which must not be
    // silently rebound to this source's newly allocated callback identities.
    fn into_script<F, Fut>(self, source: F) -> Result<LiveProgram<SceneScript>, ScriptError>
    where
        F: FnOnce(ScriptScene) -> Fut + 'static,
        Fut: Future<Output = Result<(), ScriptError>> + 'static,
    {
        let bridge = Rc::new(RefCell::new(ScriptBridge {
            scene: None,
            callbacks: None,
            boundary: None,
            host: None,
        }));
        let scene = ScriptScene {
            bridge: Rc::downgrade(&bridge),
        };
        let mut script = SceneScript {
            future: Box::pin(async move { source(scene).await }),
            bridge,
            callbacks: Some(RustHostCallbackTable::new()),
            finished: false,
            started: false,
        };
        let mut owner = ContinuationScene::new(self);
        {
            let _loan = ScriptLoan::new(&mut owner, &mut script.callbacks, &script.bridge)?;
            if matches!(
                poll_source(script.future.as_mut(), &script.bridge)?,
                SourcePoll::Finished
            ) {
                script.finished = true;
            }
        }
        owner.take().into_live_program(script).map_err(Into::into)
    }
}

enum SourcePoll {
    Boundary,
    Finished,
}

fn poll_source(
    future: Pin<&mut dyn Future<Output = Result<(), ScriptError>>>,
    bridge: &Rc<RefCell<ScriptBridge>>,
) -> Result<SourcePoll, ScriptError> {
    // LiveProgram's ready barrier is the wakeup mechanism. Pending without a
    // Noon boundary is rejected instead of adding an executor or a second clock.
    let mut context = Context::from_waker(Waker::noop());
    let polled = future.poll(&mut context);
    let bridge = bridge.borrow();
    match polled {
        Poll::Pending => {
            let boundary = bridge
                .boundary
                .as_ref()
                .ok_or(ScriptError::UnsupportedSuspension)?;
            if boundary.receipt.get() == BoundaryStatus::Abandoned {
                return Err(ScriptError::UnawaitedBoundary);
            }
            if boundary.step.is_none() && boundary.operation.is_none() {
                return Err(ScriptError::UnsupportedSuspension);
            }
            Ok(SourcePoll::Boundary)
        }
        Poll::Ready(result) => {
            result?;
            if bridge.boundary.is_some() {
                Err(ScriptError::UnawaitedBoundary)
            } else {
                Ok(SourcePoll::Finished)
            }
        }
    }
}

/// Restore both owners before returning to a host, including during unwinding.
struct ScriptLoan<'a> {
    owner: &'a mut ContinuationScene,
    callbacks: &'a mut Option<RustHostCallbackTable>,
    bridge: Rc<RefCell<ScriptBridge>>,
}

impl<'a> ScriptLoan<'a> {
    fn new(
        owner: &'a mut ContinuationScene,
        callbacks: &'a mut Option<RustHostCallbackTable>,
        bridge: &Rc<RefCell<ScriptBridge>>,
    ) -> Result<Self, ScriptError> {
        {
            let mut borrowed = bridge
                .try_borrow_mut()
                .map_err(|_| ScriptError::Reentrant)?;
            if borrowed.scene.is_some() || borrowed.callbacks.is_some() {
                return Err(ScriptError::Reentrant);
            }
            let callbacks = callbacks.take().ok_or(ScriptError::Inactive)?;
            borrowed.scene = Some(owner.take());
            borrowed.callbacks = Some(callbacks);
        }
        Ok(Self {
            owner,
            callbacks,
            bridge: Rc::clone(bridge),
        })
    }
}

impl Drop for ScriptLoan<'_> {
    fn drop(&mut self) {
        let mut bridge = self.bridge.borrow_mut();
        self.owner.restore(
            bridge
                .scene
                .take()
                .expect("source poll retains the one loaned scene"),
        );
        *self.callbacks = Some(
            bridge
                .callbacks
                .take()
                .expect("source poll retains its loaned callback table"),
        );
    }
}

impl LiveContinuation for SceneScript {
    type Error = ScriptError;

    fn resume(&mut self, scene: &mut ContinuationScene) -> Result<ContinuationStep, Self::Error> {
        let _loan = ScriptLoan::new(scene, &mut self.callbacks, &self.bridge)?;
        let complete_previous = if !self.started {
            self.started = true;
            if self.finished {
                return Ok(ContinuationStep::Finished);
            }
            let mut bridge = self.bridge.borrow_mut();
            let ScriptBridge {
                scene, boundary, ..
            } = &mut *bridge;
            let boundary = boundary.as_mut().ok_or(ScriptError::UnawaitedBoundary)?;
            if boundary.receipt.get() != BoundaryStatus::Waiting {
                self.finished = true;
                return Err(ScriptError::UnawaitedBoundary);
            }
            if let Some(step) = boundary.step.take() {
                return Ok(step);
            }
            let operation = boundary
                .operation
                .take()
                .ok_or(ScriptError::UnawaitedBoundary)?;
            let result = operation(
                scene
                    .as_mut()
                    .expect("initial boundary has its loaned scene"),
            );
            let failure = match result {
                Ok(ContinuationStep::Finished) => ScriptError::InvalidBoundary,
                Ok(step) => return Ok(step),
                Err(error) => error,
            };
            boundary.failure = Some(failure);
            boundary.receipt.set(BoundaryStatus::Completed);
            false
        } else {
            true
        };
        if self.finished {
            return Err(ScriptError::Inactive);
        }
        if complete_previous {
            let bridge = self.bridge.borrow();
            if let Some(boundary) = &bridge.boundary {
                // LiveProgram permits this poll only after exact logical
                // completion and any required renderer publication admission.
                if boundary.step.is_some()
                    || boundary.operation.is_some()
                    || boundary.receipt.get() != BoundaryStatus::Waiting
                {
                    self.finished = true;
                    return Err(ScriptError::UnawaitedBoundary);
                }
                boundary.receipt.set(BoundaryStatus::Completed);
            }
        }

        match poll_source(self.future.as_mut(), &self.bridge) {
            Ok(SourcePoll::Boundary) => self
                .bridge
                .borrow_mut()
                .boundary
                .as_mut()
                .and_then(|boundary| boundary.step.take())
                .ok_or(ScriptError::UnsupportedSuspension),
            Ok(SourcePoll::Finished) => {
                self.finished = true;
                Ok(ContinuationStep::Finished)
            }
            Err(error) => {
                self.finished = true;
                Err(error)
            }
        }
    }

    fn callbacks(&mut self) -> Option<&mut RustHostCallbackTable> {
        self.callbacks.as_mut()
    }
}
