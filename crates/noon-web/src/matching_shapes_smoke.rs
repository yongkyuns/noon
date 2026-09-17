use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;

use crate::WasmExecutionCanvasRenderer;

/// Direct browser host for the same matching-shapes lifecycle used by the native test.
#[wasm_bindgen(js_name = createDirectTransformMatchingShapesSmokeRenderer)]
pub async fn create_direct_transform_matching_shapes_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let program = noon::example_scenes::family_transform_indicate::matching_shapes::program()
        .map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}

/// Direct browser host for duplicate keys and the default unmatched fade paths.
#[wasm_bindgen(js_name = createDirectTransformMatchingShapesBreadthSmokeRenderer)]
pub async fn create_direct_transform_matching_shapes_breadth_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let program =
        noon::example_scenes::family_transform_indicate::matching_shapes::breadth_program()
            .map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}
