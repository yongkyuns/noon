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
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
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
        let program = noon::example_scenes::pointer_selection::scene()
            .map_err(js_error)?
            .into_live_program(Finished)
            .map_err(js_error)?;
        WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
    } else {
        let session = noon::example_scenes::pointer_selection::session().map_err(js_error)?;
        WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
    }
}
