//! Thin browser host for the same recorded signal train used by native capture.

use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;

use crate::WasmExecutionCanvasRenderer;

#[wasm_bindgen(js_name = createDirectSignalTrainsRenderer)]
pub async fn create_direct_signal_trains_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::line_passing_flash::train_playback()
        .map_err(|error| JsValue::from_str(&error))?;
    WasmExecutionCanvasRenderer::create_from_scene_execution(canvas, session).await
}
