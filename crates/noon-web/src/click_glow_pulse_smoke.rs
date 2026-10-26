use crate::WasmExecutionCanvasRenderer;
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;

/// Direct typed counterpart of the native and Python click-to-GlowPulse example.
#[wasm_bindgen(js_name = createDirectClickGlowPulseRenderer)]
pub async fn create_direct_click_glow_pulse_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let execution = noon::example_scenes::click_glow_pulse::execution()
        .map_err(|error| JsValue::from_str(&error))?;
    WasmExecutionCanvasRenderer::create_from_scene_execution(canvas, execution).await
}
