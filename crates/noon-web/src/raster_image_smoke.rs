//! Opt-in image qualification on the ordinary typed native/WASM engine path.
use crate::WasmExecutionCanvasRenderer;
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;

/// The same image lifecycle runs natively and directly in Rust/WASM.
#[wasm_bindgen(js_name = createDirectRasterImageSmokeRenderer)]
pub async fn create_direct_raster_image_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let program = noon::example_scenes::raster_image::program().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}

/// Filter/opacity qualification uses the same native typed fixture.
#[wasm_bindgen(js_name = createDirectRasterImageSamplingRenderer)]
pub async fn create_direct_raster_image_sampling_renderer(
    canvas: OffscreenCanvas,
    sampling: &str,
    opacity: f64,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::raster_image::sampling_session(
        crate::authoring_image::sampling(sampling)?,
        opacity,
    )
    .map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}
