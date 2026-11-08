//! Desktop event pumping for an ordinary Rust authoring stack.

use std::cell::RefCell;
use std::rc::Rc;

use noon::integration::{
    BlockingBoundary, BlockingContinuation, BlockingSceneHost, BlockingSceneSetup,
};
use noon::{
    BlockingScene, LiveProgram, PlaybackProgram, RustHostCallbackTable, Scene, ScriptError,
};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};

use crate::execution_source::{NativeAuthoringAccess, PlaybackExecutionSource};
use crate::{admit_source, NativeApp, NativeHostError, NativeViewportConfig};

#[cfg(test)]
mod tests;

/// Run an ordinary Rust scene function in a native desktop window.
///
/// `play` and `wait` service the existing window, input, renderer, and callback
/// loop before returning to the same Rust stack. Ordinary helpers and loops need
/// no async annotation. Call this once on the application's main thread; the
/// viewport remains open after the source finishes, until the user closes it.
/// Closing during a boundary cancels the source instead of completing that wait.
///
/// The source runs outside window callbacks. CPU work between boundaries runs
/// synchronously too, so long calculations should be kept off the UI thread.
pub fn run_scene(
    source: impl FnOnce(&BlockingScene) -> Result<(), ScriptError>,
) -> Result<(), NativeHostError> {
    run_scene_with_config(source, NativeViewportConfig::default())
}

/// Run an ordinary Rust scene function with explicit desktop viewport settings.
///
/// This runner is available on Windows, Linux, and macOS. It uses one Winit event
/// loop for the invocation and retains the same native clock across boundaries.
pub fn run_scene_with_config(
    source: impl FnOnce(&BlockingScene) -> Result<(), ScriptError>,
    config: NativeViewportConfig,
) -> Result<(), NativeHostError> {
    run_with_host(source, Rc::new(NativeBlockingHost::new(config)))
}

fn run_with_host(
    source: impl FnOnce(&BlockingScene) -> Result<(), ScriptError>,
    host: Rc<NativeBlockingHost>,
) -> Result<(), NativeHostError> {
    let erased: Rc<dyn BlockingSceneHost> = host.clone();
    let scene = BlockingScene::for_host(Rc::downgrade(&erased));
    drop(erased);

    // A panic in an author helper can also unwind through a platform pump. End
    // that invocation explicitly before propagating its original Rust panic.
    let authored = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| source(&scene)));
    let result = match authored {
        Ok(result) => result.and_then(|()| host.finish_and_run()),
        Err(panic) => {
            if let Err(error) = host.retire(true) {
                eprintln!("native source retirement after an author panic failed: {error}");
            }
            std::panic::resume_unwind(panic)
        }
    };
    host.retire(result.is_err())?;
    result.map_err(|error| NativeHostError::Program(error.to_string()))
}

struct BlockingAuthoring;

impl NativeAuthoringAccess<BlockingContinuation> for BlockingAuthoring {
    fn with_program(
        program: &mut PlaybackProgram<BlockingContinuation>,
        operation: &mut dyn FnMut(
            &mut LiveProgram<BlockingContinuation>,
        ) -> Result<(), ScriptError>,
    ) -> Result<(), ScriptError> {
        program.with_observed_program(operation).ok_or_else(|| {
            ScriptError::Host("synchronous authoring requires its original observed source".into())
        })?
    }
}

enum HostState {
    Cold(Box<BlockingSceneSetup>),
    Running(Box<NativeRunning>),
    Failed(NativeHostError),
    Retired,
}

struct NativeRunning {
    app: NativeApp,
    event_loop: EventLoop<()>,
    started: bool,
    exited: bool,
}

impl NativeRunning {
    fn authoring(
        &mut self,
        operation: &mut dyn FnMut(
            &mut LiveProgram<BlockingContinuation>,
        ) -> Result<(), ScriptError>,
    ) -> Result<(), ScriptError> {
        if self.exited {
            return Err(ScriptError::Cancelled);
        }
        self.app.execution.with_authoring_program(operation)
    }

    fn boundary_complete(&mut self) -> Result<bool, ScriptError> {
        let mut complete = false;
        self.authoring(&mut |program| {
            complete = program.boundary_complete();
            Ok(())
        })?;
        Ok(complete)
    }

    fn wake(&self) {
        if let Some(window) = self.app.window.as_ref() {
            // The preceding boundary may have left Winit waiting indefinitely.
            // Let the ordinary handler observe the new work and its deadline.
            window.request_redraw();
        }
    }

    fn pump(&mut self) -> Result<(), ScriptError> {
        if self.exited {
            return Err(ScriptError::Cancelled);
        }
        // Winit may run Init and then poll again in its first pump. Return from
        // that bootstrap even if a zero-duration boundary completed during Init;
        // later pumps use the existing handler's sleeping deadline policy.
        let timeout = (!self.started).then_some(std::time::Duration::ZERO);
        self.started = true;
        let status = self.event_loop.pump_app_events(timeout, &mut self.app);
        self.exited = matches!(status, PumpStatus::Exit(_));
        if let Some(error) = self.app.error.as_ref() {
            return Err(ScriptError::Host(error.to_string()));
        }
        if let PumpStatus::Exit(code) = status {
            if code != 0 {
                return Err(ScriptError::Host(format!(
                    "native event loop exited with status {code}"
                )));
            }
        }
        Ok(())
    }
}

struct NativeBlockingHost {
    state: RefCell<HostState>,
    config: NativeViewportConfig,
    #[cfg(test)]
    event_loop: RefCell<Option<EventLoop<()>>>,
    #[cfg(test)]
    close_after_authoring: std::cell::Cell<bool>,
}

impl NativeBlockingHost {
    fn new(config: NativeViewportConfig) -> Self {
        Self {
            state: RefCell::new(HostState::Cold(Box::default())),
            config,
            #[cfg(test)]
            event_loop: RefCell::new(None),
            #[cfg(test)]
            close_after_authoring: std::cell::Cell::new(false),
        }
    }

    fn create_event_loop(&self) -> Result<EventLoop<()>, NativeHostError> {
        #[cfg(test)]
        if let Some(event_loop) = self.event_loop.borrow_mut().take() {
            return Ok(event_loop);
        }
        EventLoop::new().map_err(|error| NativeHostError::Platform(error.to_string()))
    }

    fn start(&self, setup: BlockingSceneSetup) -> Result<NativeRunning, NativeHostError> {
        let program = setup
            .into_program()
            .map_err(|error| NativeHostError::Program(error.to_string()))?;
        // The empty external relay is observed immediately. Lowering is complete
        // even if the author's first request is rejected and subsequently caught.
        let source = PlaybackExecutionSource::<_, BlockingAuthoring>::with_authoring_access(
            program,
            RustHostCallbackTable::new(),
        )?;
        admit_source(&source, &self.config)?;
        let event_loop = self.create_event_loop()?;
        event_loop.set_control_flow(ControlFlow::Wait);
        Ok(NativeRunning {
            app: NativeApp::from_source(Box::new(source), self.config.clone()),
            event_loop,
            started: false,
            exited: false,
        })
    }

    fn with_running<R>(
        &self,
        operation: impl FnOnce(&mut NativeRunning) -> Result<R, ScriptError>,
    ) -> Result<R, ScriptError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ScriptError::Reentrant)?;
        if matches!(*state, HostState::Cold(_)) {
            let HostState::Cold(setup) = std::mem::replace(&mut *state, HostState::Retired) else {
                unreachable!("cold source was checked above")
            };
            *state = match self.start(*setup) {
                Ok(running) => HostState::Running(Box::new(running)),
                Err(error) => HostState::Failed(error),
            };
        }
        match &mut *state {
            HostState::Running(running) => operation(running),
            HostState::Failed(error) => Err(ScriptError::Host(error.to_string())),
            HostState::Retired => Err(ScriptError::Inactive),
            HostState::Cold(_) => unreachable!("cold source was consumed above"),
        }
    }

    fn finish_and_run(&self) -> Result<(), ScriptError> {
        self.with_running(|running| {
            running.authoring(&mut |program| program.finish_authoring())?;
            #[cfg(test)]
            if self.close_after_authoring.get() {
                running.app.exit_after_present = Some(running.app.execution.frame_time());
            }
            running.wake();
            Ok(())
        })?;
        loop {
            let exited = self.with_running(|running| {
                running.pump()?;
                Ok(running.exited)
            })?;
            if exited {
                return Ok(());
            }
        }
    }

    fn retire(&self, cancelled: bool) -> Result<(), NativeHostError> {
        let mut state = self.state.try_borrow_mut().map_err(|_| {
            NativeHostError::Program(
                "native authoring owner is still borrowed at retirement".into(),
            )
        })?;
        let old = std::mem::replace(&mut *state, HostState::Retired);
        match old {
            HostState::Running(mut running) => {
                #[cfg(feature = "audio")]
                running.app.stop_audio();
                let retired = if cancelled {
                    running
                        .app
                        .execution
                        .with_authoring_program(&mut |program| program.cancel_authoring())
                        .map_err(|error| NativeHostError::Program(error.to_string()))
                } else {
                    running.app.execution.retire_execution_source()
                };
                if let Some(error) = running.app.error.take() {
                    return Err(error);
                }
                retired
            }
            HostState::Failed(error) => Err(error),
            HostState::Cold(_) | HostState::Retired => Ok(()),
        }
    }
}

impl BlockingSceneHost for NativeBlockingHost {
    fn with_scene(
        &self,
        operation: &mut dyn FnMut(
            &mut Scene,
            &mut RustHostCallbackTable,
        ) -> Result<(), ScriptError>,
    ) -> Result<(), ScriptError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ScriptError::Reentrant)?;
        match &mut *state {
            HostState::Cold(setup) => setup.with_scene(operation),
            HostState::Running(running) => {
                running.authoring(&mut |program| program.with_authoring_scene(operation))
            }
            HostState::Failed(error) => Err(ScriptError::Host(error.to_string())),
            HostState::Retired => Err(ScriptError::Inactive),
        }
    }

    fn wait(&self, boundary: BlockingBoundary) -> Result<(), ScriptError> {
        let mut boundary = Some(boundary);
        self.with_running(|running| {
            running.authoring(&mut |program| {
                program.begin_boundary(boundary.take().expect("one boundary admission"))
            })?;
            running.wake();
            Ok(())
        })?;
        loop {
            let complete = self.with_running(|running| {
                if running.boundary_complete()? {
                    return Ok(true);
                }
                // NativeApp keeps rendering, input, callbacks, and lifecycle
                // handling inside Winit's synchronous ApplicationHandler calls.
                running.pump()?;
                running.boundary_complete()
            })?;
            if complete {
                return Ok(());
            }
        }
    }
}
