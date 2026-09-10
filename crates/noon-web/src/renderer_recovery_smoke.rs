//! Typed platform fixtures, available in debug builds or explicit release qualification.

use crate::WasmExecutionCanvasRenderer;
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;

/// Named platform-test fixtures stay entirely within the typed Rust engine.
#[wasm_bindgen(js_name = createDirectRecoverySmokeRenderer)]
pub async fn create_direct_recovery_smoke_renderer(
    canvas: OffscreenCanvas,
    fixture: &str,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session = match fixture {
        "circle" => noon::example_scenes::renderer_recovery::circle(),
        "four-animated" => noon::example_scenes::renderer_recovery::four_animated(),
        "camera-density" => noon::example_scenes::renderer_recovery::camera_density(),
        _ => Err(format!("unknown recovery fixture: {fixture}")),
    }
    .map_err(|error| JsValue::from_str(&error))?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}
