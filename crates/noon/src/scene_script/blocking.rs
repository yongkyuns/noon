//! Ordinary Rust call stacks over a host-serviced scene completion boundary.

use std::{
    cell::RefCell,
    ops::Deref,
    rc::{Rc, Weak},
};

use super::{BoundaryOperation, ScriptBridge, ScriptError, ScriptScene};
use crate::{AnimationOptions, IntoAnimationRequest, RustHostCallbackTable, Scene};

/// One synchronous source boundary. Its operation is admitted by the shared
/// program before the host waits; it contains no clock or alternate scene state.
pub struct BlockingBoundary {
    pub(crate) operation: Box<BoundaryOperation>,
}

/// Platform integration for an ordinary Rust authoring stack.
///
/// Implementations keep the one scene/program in the existing host. Every
/// `with_scene` borrow must finish before `wait` pumps events or suspends a stack.
/// A successful wait means the program's logical completion and required
/// publication have both been admitted. Cancellation must return an error.
pub trait BlockingSceneHost {
    /// Invoke this short operation exactly once with the current scene and its
    /// source-owned callable table. Reject calls while a boundary is outstanding
    /// or after the authoring source has finished or been cancelled.
    fn with_scene(
        &self,
        operation: &mut dyn FnMut(
            &mut Scene,
            &mut RustHostCallbackTable,
        ) -> Result<(), ScriptError>,
    ) -> Result<(), ScriptError>;

    /// Admit and complete this boundary using the existing execution host.
    /// No engine or renderer borrow may remain on the suspended author's stack.
    fn wait(&self, boundary: BlockingBoundary) -> Result<(), ScriptError>;
}

/// Scene authoring through ordinary synchronous Rust functions.
///
/// `play` and `wait` return after their logical completion and publication
/// boundary. The platform host keeps servicing rendering, input, and callbacks
/// while the author's stack waits. Ordinary scene operations are shared with
/// [`ScriptScene`]; no scene or runtime is copied into this capability.
///
/// Obtain this capability from a synchronous platform runner. Cloning it aliases
/// the same invocation without keeping the host alive. It cannot be used from
/// callbacks during a pending boundary, or after that invocation is retired.
#[derive(Clone)]
pub struct BlockingScene {
    scene: ScriptScene,
    // Own only the access capability/registration identity, never the engine.
    bridge: Rc<RefCell<ScriptBridge>>,
}

impl BlockingScene {
    /// Construct a scoped capability for a platform host. Scene authors should
    /// use their host's runner, which owns this integration and its teardown.
    #[doc(hidden)]
    pub fn for_host(host: Weak<dyn BlockingSceneHost>) -> Self {
        let bridge = Rc::new(RefCell::new(ScriptBridge {
            scene: None,
            callbacks: None,
            boundary: None,
            host: Some(host),
        }));
        Self {
            scene: ScriptScene {
                bridge: Rc::downgrade(&bridge),
            },
            bridge,
        }
    }

    /// Play one animation or a parallel tuple/collection and wait for completion.
    /// Effective target values are captured when this call is admitted.
    pub fn play(&self, request: impl IntoAnimationRequest) -> Result<(), ScriptError> {
        self.play_with(request, AnimationOptions::default())
    }

    /// Play with explicit shared animation options, then resume ordinary code.
    pub fn play_with(
        &self,
        request: impl IntoAnimationRequest,
        options: AnimationOptions,
    ) -> Result<(), ScriptError> {
        let mut boundary = self.scene.play_with(request, options);
        self.wait_boundary(BlockingBoundary {
            operation: boundary
                .operation
                .take()
                .expect("new boundary owns its operation"),
        })
    }

    /// Wait in authored time while the host services its ordinary event loop.
    pub fn wait(&self, seconds: f64) -> Result<(), ScriptError> {
        let mut boundary = self.scene.wait(seconds);
        self.wait_boundary(BlockingBoundary {
            operation: boundary
                .operation
                .take()
                .expect("new boundary owns its operation"),
        })
    }

    fn wait_boundary(&self, boundary: BlockingBoundary) -> Result<(), ScriptError> {
        let host = self
            .bridge
            .try_borrow()
            .map_err(|_| ScriptError::Reentrant)?
            .host
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or(ScriptError::Inactive)?;
        // The bridge borrow ended before entering a platform wait/reentrancy point.
        host.wait(boundary)
    }
}

impl Deref for BlockingScene {
    type Target = ScriptScene;

    fn deref(&self) -> &Self::Target {
        &self.scene
    }
}
