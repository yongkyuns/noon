use crate::WasmExecutionCanvasRenderer;
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;
fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// Same typed scene and input contract as the native example, with no worker codec.
#[wasm_bindgen(js_name = createDirectPointerSelectionRenderer)]
pub async fn create_direct_pointer_selection_renderer(
    canvas: OffscreenCanvas,
    live_program: bool,
    animated: bool,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let scene = if animated {
        noon::example_scenes::pointer_selection::click_indicate_scene()
    } else {
        noon::example_scenes::pointer_selection::scene()
    }
    .map_err(js_error)?;
    if live_program {
        struct Finished;
        impl noon::LiveContinuation for Finished {
            type Error = std::convert::Infallible;
            fn resume(
                &mut self,
                _: &mut noon::LiveSession<'_>,
            ) -> Result<noon::ContinuationStep, Self::Error> {
                Ok(noon::ContinuationStep::Finished)
            }
        }
        let program = scene.into_live_program(Finished).map_err(js_error)?;
        WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
    } else {
        let session = scene.execution_session().map_err(js_error)?;
        WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
    }
}

/// Direct counterpart of the Python-authored moving selection qualification.
#[wasm_bindgen(js_name = createDirectMovingPointerSelectionRenderer)]
pub async fn create_direct_moving_pointer_selection_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session =
        noon::example_scenes::pointer_selection::moving_selection_session().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Direct-browser counterpart for click-Indicate during authored motion.
#[wasm_bindgen(js_name = createDirectMovingClickIndicateRenderer)]
pub async fn create_direct_moving_click_indicate_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::pointer_selection::moving_click_indicate_session()
        .map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Direct-browser counterpart of the native collector's shared Rust-authored
/// pointer fixture. It retains the fixture's pointer state/event subscriptions
/// and enables the same paused-session selection policy used by qualification.
#[wasm_bindgen(js_name = createDirectPointerInputTraceRenderer)]
pub async fn create_direct_pointer_input_trace_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let fixture = noon::example_scenes::pointer_input_trace::Fixture::new();
    let session = noon::ExecutionSession::from_semantic_root(&fixture.store, fixture.root)
        .map_err(js_error)?;
    let mut session = session;
    session
        .enable_pointer_fill_selection(4.0)
        .map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Qualification-only live animation; not click-action dispatch. Inspection must
/// remain independent while the normal shared Indicate segment owns the shape.
#[wasm_bindgen(js_name = createDirectInspectionIndicateRenderer)]
pub async fn create_direct_inspection_indicate_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    struct Once {
        circle: noon::Mobject,
        started: bool,
    }
    impl noon::LiveContinuation for Once {
        type Error = String;
        fn resume(
            &mut self,
            live: &mut noon::LiveSession<'_>,
        ) -> Result<noon::ContinuationStep, String> {
            if self.started {
                return Ok(noon::ContinuationStep::Finished);
            }
            let segment = live
                .declare_and_activate_indicate(
                    &self.circle,
                    noon::IndicateOptions::default(),
                    noon::AnimationOptions::new().run_time(4.0),
                )
                .map_err(|error| error.to_string())?;
            self.started = true;
            Ok(noon::ContinuationStep::Await(segment))
        }
    }
    let mut scene = noon::Scene::new();
    let mut circle = scene.circle(0.8).map_err(js_error)?;
    circle.set_fill(0.0, 0.0, 1.0, 1.0).map_err(js_error)?;
    circle.set_stroke_width(0.0).map_err(js_error)?;
    scene.add(&circle).map_err(js_error)?;
    let program = scene
        .into_live_program(Once {
            circle,
            started: false,
        })
        .map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}
