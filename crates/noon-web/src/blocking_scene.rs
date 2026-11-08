//! Experimental ordinary Rust stacks over the existing direct canvas host.
//!
//! JSPI parks source execution, not the browser. The one Scene/program/renderer
//! stays in Rust; JavaScript supplies only browser wake and cancellation Promises.

// wasm-bindgen's JSPI attributes generate sibling warning items, so acknowledge
// the experimental API at this dedicated, opt-in platform module boundary.
#![allow(deprecated)]

use std::{cell::RefCell, rc::Rc};

use noon::integration::{BlockingBoundary, BlockingSceneHost, BlockingSceneSetup};
use noon::{BlockingScene, RustHostCallbackTable, Scene, ScriptError};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, jspi_block_on_promise, JsFuture};
use web_sys::{AbortController, AbortSignal, OffscreenCanvas};

use crate::{BrowserHostWake, WasmExecutionCanvasRenderer};

pub(crate) type AuthoringOperation<'a> =
    dyn FnMut(&mut Scene, &mut RustHostCallbackTable) -> Result<(), ScriptError> + 'a;

#[wasm_bindgen(inline_js = r#"
export function blockingSceneWake(frame, delay, signal) {
    return new Promise((resolve, reject) => {
        if (signal.aborted) { reject(signal.reason); return; }
        let handle;
        const abort = () => {
            if (frame) cancelAnimationFrame(handle); else clearTimeout(handle);
            reject(signal.reason);
        };
        const done = time => {
            signal.removeEventListener('abort', abort);
            resolve(time);
        };
        signal.addEventListener('abort', abort, { once: true });
        try {
            handle = frame ? requestAnimationFrame(done)
                : setTimeout(() => done(performance.now()), Math.ceil(Math.min(delay, 2147483647)));
        } catch (error) {
            signal.removeEventListener('abort', abort);
            reject(error);
        }
    });
}
export function blockingSceneAbortable(promise, signal) {
    return new Promise((resolve, reject) => {
        if (signal.aborted) { reject(signal.reason); return; }
        const abort = () => reject(signal.reason);
        signal.addEventListener('abort', abort, { once: true });
        promise.then(value => {
            signal.removeEventListener('abort', abort);
            resolve(value);
        }, error => {
            signal.removeEventListener('abort', abort);
            reject(error);
        });
    });
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = blockingSceneWake)]
    fn browser_wake(frame: bool, delay: f64, signal: &AbortSignal) -> js_sys::Promise;
    #[wasm_bindgen(js_name = blockingSceneAbortable)]
    fn abortable(promise: &js_sys::Promise, signal: &AbortSignal) -> js_sys::Promise;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Created,
    Authoring,
    Waiting,
    Finished,
    Cancelled,
}

struct State {
    setup: Option<BlockingSceneSetup>,
    renderer: Option<WasmExecutionCanvasRenderer>,
    preparing: bool,
    phase: Phase,
    presented_frames: u32,
    #[cfg(any(debug_assertions, feature = "renderer-smoke"))]
    boundary_clock_anchors: Vec<Option<(f64, f64)>>,
}

struct BrowserBlockingHost {
    state: Rc<RefCell<State>>,
    canvas: OffscreenCanvas,
    abort: AbortController,
}

impl BrowserBlockingHost {
    fn enter_wait(&self) -> Result<(), ScriptError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ScriptError::Reentrant)?;
        match state.phase {
            Phase::Authoring => state.phase = Phase::Waiting,
            Phase::Cancelled => return Err(ScriptError::Cancelled),
            _ => return Err(ScriptError::Inactive),
        }
        Ok(())
    }

    fn leave_wait(&self) {
        let mut state = self.state.borrow_mut();
        if state.phase == Phase::Waiting {
            state.phase = Phase::Authoring;
        }
    }

    fn suspend(&self, promise: &js_sys::Promise) -> Result<(), ScriptError> {
        // No Ref/RefMut, Scene, runtime, or renderer loan crosses this call.
        let result = jspi_block_on_promise(promise);
        if self.abort.signal().aborted() {
            return Err(ScriptError::Cancelled);
        }
        result.map(|_| ()).map_err(host_error)
    }

    fn ensure_renderer(&self) -> Result<(), ScriptError> {
        let setup = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| ScriptError::Reentrant)?;
            if state.renderer.is_some() {
                return Ok(());
            }
            let setup = state.setup.take().ok_or(ScriptError::Inactive)?;
            state.preparing = true;
            setup
        };
        let program = match setup.into_program() {
            Ok(program) => program,
            Err(error) => {
                let mut state = self.state.borrow_mut();
                state.preparing = false;
                state.phase = Phase::Cancelled;
                return Err(error);
            }
        };
        let state = Rc::clone(&self.state);
        let canvas = self.canvas.clone();
        let preparation = future_to_promise(async move {
            let result =
                WasmExecutionCanvasRenderer::create_from_blocking_program(canvas, program).await;
            match result {
                Ok(mut renderer) => {
                    let mut state = state.borrow_mut();
                    state.preparing = false;
                    if state.phase == Phase::Cancelled {
                        renderer.blocking_cancel().map_err(js_error)?;
                        return Err(js_error(ScriptError::Cancelled));
                    }
                    state.renderer = Some(renderer);
                    Ok(JsValue::UNDEFINED)
                }
                Err(error) => {
                    let mut state = state.borrow_mut();
                    state.preparing = false;
                    state.phase = Phase::Cancelled;
                    Err(error)
                }
            }
        });
        // Browser device preparation may finish after cancellation. It retains
        // its single owner until then and retires it instead of publishing late.
        self.suspend(&abortable(&preparation, &self.abort.signal()))
    }

    fn drive(&self, final_frame: bool) -> Result<(), ScriptError> {
        let state = Rc::clone(&self.state);
        let signal = self.abort.signal();
        let completion = future_to_promise(async move {
            drive_until_complete(state, signal, final_frame)
                .await
                .map_err(js_error)?;
            Ok(JsValue::UNDEFINED)
        });
        self.suspend(&completion)
    }

    fn finish(&self) -> Result<(), ScriptError> {
        self.enter_wait()?;
        self.ensure_renderer()?;
        self.state
            .borrow_mut()
            .renderer
            .as_mut()
            .ok_or(ScriptError::Inactive)?
            .blocking_finish()?;
        self.drive(true)?;
        self.state.borrow_mut().phase = Phase::Finished;
        Ok(())
    }

    fn cancel(&self) -> Result<(), ScriptError> {
        let result = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| ScriptError::Reentrant)?;
            if state.phase == Phase::Cancelled {
                Ok(())
            } else {
                state.phase = Phase::Cancelled;
                state.setup = None;
                state
                    .renderer
                    .as_mut()
                    .map_or(Ok(()), |renderer| renderer.blocking_cancel())
            }
        };
        // Abort listeners settle platform Promises only after the owner borrow ends.
        self.abort.abort();
        result
    }
}

impl BlockingSceneHost for BrowserBlockingHost {
    fn with_scene(&self, operation: &mut AuthoringOperation<'_>) -> Result<(), ScriptError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ScriptError::Reentrant)?;
        match state.phase {
            Phase::Authoring => {}
            Phase::Cancelled => return Err(ScriptError::Cancelled),
            _ => return Err(ScriptError::Inactive),
        }
        if let Some(setup) = state.setup.as_mut() {
            setup.with_scene(operation)
        } else {
            state
                .renderer
                .as_mut()
                .ok_or(ScriptError::Inactive)?
                .blocking_with_scene(operation)
        }
    }

    fn wait(&self, boundary: BlockingBoundary) -> Result<(), ScriptError> {
        self.enter_wait()?;
        let result = (|| {
            self.ensure_renderer()?;
            // Admission failures are catchable by ordinary Rust code. The same
            // initialized program remains in this host for its next operation.
            self.state
                .borrow_mut()
                .renderer
                .as_mut()
                .ok_or(ScriptError::Inactive)?
                .blocking_begin(boundary)?;
            let result = self.drive(false);
            if result.is_err() {
                self.cancel()?;
            }
            result
        })();
        self.leave_wait();
        result
    }
}

async fn drive_until_complete(
    state: Rc<RefCell<State>>,
    signal: AbortSignal,
    final_frame: bool,
) -> Result<(), ScriptError> {
    let performance = web_sys::window()
        .and_then(|window| window.performance())
        .ok_or_else(|| {
            ScriptError::Host("blocking canvas host requires Window performance timing".into())
        })?;
    loop {
        let wake = {
            let mut state = state.try_borrow_mut().map_err(|_| ScriptError::Reentrant)?;
            if state.phase == Phase::Cancelled {
                return Err(ScriptError::Cancelled);
            }
            let renderer = state.renderer.as_mut().ok_or(ScriptError::Inactive)?;
            let now = performance.now();
            renderer.advance_direct_realtime(now).map_err(host_error)?;
            let presented = renderer.render().map_err(host_error)?;
            let (present_now, wake) = renderer.direct_wake_directive(now).map_err(host_error)?;
            let complete = final_frame || renderer.blocking_complete()?;
            #[cfg(any(debug_assertions, feature = "renderer-smoke"))]
            let anchor = renderer.blocking_clock_anchor();
            state.presented_frames += u32::from(presented);
            if complete && !present_now {
                #[cfg(any(debug_assertions, feature = "renderer-smoke"))]
                if !final_frame {
                    state.boundary_clock_anchors.push(anchor);
                }
                return Ok(());
            }
            if present_now {
                BrowserHostWake::AnimationFrame
            } else {
                wake
            }
        };
        let promise = match wake {
            BrowserHostWake::AnimationFrame => browser_wake(true, 0.0, &signal),
            BrowserHostWake::TimerAfterMilliseconds(delay) => browser_wake(false, delay, &signal),
            BrowserHostWake::Idle => {
                return Err(ScriptError::Host(
                    "an incomplete blocking boundary has no runtime wake".into(),
                ))
            }
        };
        JsFuture::from(promise).await.map_err(host_error)?;
    }
}

/// Experimental JSPI host for ordinary synchronous Rust scene functions.
///
/// The feature requires a JSPI-capable browser. Construct a runner first so its
/// cancellation handle exists while source execution is suspended. Keep the
/// runner after completion to retain its final canvas frame.
#[wasm_bindgen(js_name = BlockingSceneRunner)]
pub struct WasmBlockingSceneRunner {
    host: Rc<BrowserBlockingHost>,
}

#[wasm_bindgen(js_class = BlockingSceneRunner)]
impl WasmBlockingSceneRunner {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: OffscreenCanvas) -> Result<Self, JsValue> {
        Ok(Self {
            host: Rc::new(BrowserBlockingHost {
                state: Rc::new(RefCell::new(State {
                    setup: Some(BlockingSceneSetup::new()),
                    renderer: None,
                    preparing: false,
                    phase: Phase::Created,
                    presented_frames: 0,
                    #[cfg(any(debug_assertions, feature = "renderer-smoke"))]
                    boundary_clock_anchors: Vec::new(),
                })),
                canvas,
                abort: AbortController::new()?,
            }),
        })
    }

    /// Run the same ordinary Rust example as the native synchronous host.
    #[wasm_bindgen(jspi, js_name = runExample)]
    pub fn run_example(&self) -> Result<(), JsValue> {
        self.run(noon::example_scenes::blocking_authoring::construct)
            .map_err(js_error)
    }

    /// Qualification fixture: a rejected first boundary must keep its original
    /// scene and allow a later valid boundary and final immediate authored edit.
    #[cfg(any(debug_assertions, feature = "renderer-smoke"))]
    #[wasm_bindgen(jspi, js_name = runInvalidWaitExample)]
    pub fn run_invalid_wait_example(&self) -> Result<(), JsValue> {
        self.run(|scene| {
            let square = scene.square(1.0)?;
            scene.set_fill(&square, noon::Color::BLUE, 1.0)?;
            scene.add(&square)?;
            if scene.wait(-1.0).is_ok() {
                return Err(ScriptError::Host(
                    "negative wait unexpectedly succeeded".into(),
                ));
            }
            assert_eq!(scene.time()?, 0.0);
            assert_eq!(scene.effective_layout(&square)?.center.0, 0.0);
            scene.play_with(
                square.animate().shift(noon::Vec2::new(1.0, 0.0)),
                noon::AnimationOptions::new().run_time(0.25),
            )?;
            assert_eq!(scene.time()?, 0.25);
            scene.set_fill(&square, noon::Color::ORANGE, 1.0)?;
            Ok(())
        })
        .map_err(js_error)
    }

    /// Qualification fixture: source without a wait still publishes its final frame.
    #[cfg(any(debug_assertions, feature = "renderer-smoke"))]
    #[wasm_bindgen(jspi, js_name = runStaticExample)]
    pub fn run_static_example(&self) -> Result<(), JsValue> {
        self.run(|scene| {
            let square = scene.square(1.0)?;
            scene.set_fill(&square, noon::Color::BLUE, 1.0)?;
            scene.add(&square)
        })
        .map_err(js_error)
    }

    /// Cancel this invocation. Await its rejected run Promise before freeing it.
    pub fn cancel(&self) -> Result<(), JsValue> {
        self.host.cancel().map_err(js_error)
    }

    pub fn time(&self) -> f64 {
        self.host
            .state
            .borrow()
            .renderer
            .as_ref()
            .map_or(0.0, |renderer| renderer.time())
    }

    #[wasm_bindgen(js_name = presentedFrames)]
    pub fn presented_frames(&self) -> u32 {
        self.host.state.borrow().presented_frames
    }

    #[wasm_bindgen(js_name = rendererBackend)]
    pub fn renderer_backend(&self) -> Option<String> {
        self.host
            .state
            .borrow()
            .renderer
            .as_ref()
            .map(|renderer| renderer.renderer_backend())
    }

    #[cfg(any(debug_assertions, feature = "renderer-smoke"))]
    #[wasm_bindgen(js_name = debugSelectionFrameJson)]
    pub fn debug_selection_frame_json(&self) -> Option<String> {
        self.host
            .state
            .borrow()
            .renderer
            .as_ref()
            .map(|renderer| renderer.debug_selection_frame_json())
    }

    /// Host lifecycle evidence only; never used to advance or admit engine work.
    #[cfg(any(debug_assertions, feature = "renderer-smoke"))]
    #[wasm_bindgen(js_name = debugLifecycleJson)]
    pub fn debug_lifecycle_json(&self) -> String {
        let state = self.host.state.borrow();
        let phase = match state.phase {
            Phase::Created => "created",
            Phase::Authoring => "authoring",
            Phase::Waiting => "waiting",
            Phase::Finished => "finished",
            Phase::Cancelled => "cancelled",
        };
        serde_json::json!({
            "phase": phase,
            "preparing": state.preparing,
            "hasRenderer": state.renderer.is_some(),
            "presentedFrames": state.presented_frames,
            "clockAnchor": state.renderer.as_ref().and_then(|renderer| renderer.blocking_clock_anchor()),
            "boundaryClockAnchors": state.boundary_clock_anchors,
        })
        .to_string()
    }
}

impl WasmBlockingSceneRunner {
    /// Run a normal Rust function from a JSPI-enabled activation. The source
    /// itself requires no async function, Future, or scene-authoring macro.
    pub fn run(
        &self,
        source: impl FnOnce(&BlockingScene) -> Result<(), ScriptError>,
    ) -> Result<(), ScriptError> {
        {
            let mut state = self
                .host
                .state
                .try_borrow_mut()
                .map_err(|_| ScriptError::Reentrant)?;
            if state.phase != Phase::Created {
                return Err(ScriptError::Inactive);
            }
            state.phase = Phase::Authoring;
        }
        let host: Rc<dyn BlockingSceneHost> = self.host.clone();
        let scene = BlockingScene::for_host(Rc::downgrade(&host));
        let result = source(&scene).and_then(|()| self.host.finish());
        if result.is_err() {
            self.host.cancel()?;
        }
        result
    }
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&error.to_string()).into()
}

fn host_error(error: JsValue) -> ScriptError {
    ScriptError::Host(error.as_string().unwrap_or_else(|| format!("{error:?}")))
}
