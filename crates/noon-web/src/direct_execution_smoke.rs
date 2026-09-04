use noon::ExecutionSession;
use noon_core::{SemanticObjectProperty, SemanticObjectState, SemanticStore, StoredGeometry};
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;

use crate::WasmExecutionCanvasRenderer;

/// Debug-only browser proof that authoritative semantic state can reach the existing
/// canvas renderer without a scene document, execution delta, or transport mirror.
#[wasm_bindgen(js_name = createDirectExecutionSmokeRenderer)]
pub async fn create_direct_execution_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let mut store = SemanticStore::new();
    let opacity = store
        .insert_semantic_input_signal(0.65_f64)
        .map_err(js_error)?;
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.5,
    }));
    store.attach_to_scene(object).map_err(js_error)?;
    store
        .bind_semantic_signal(opacity, object, SemanticObjectProperty::ObjectOpacity)
        .map_err(js_error)?;

    let session = ExecutionSession::from_semantic_store(&store).map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}
