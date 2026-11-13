use super::wasm::CanonicalAuthoringSceneContext;
use crate::{authoring_error::js_error, WasmCodeHandle, WasmCodeOptions};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = liveCreateCode)]
    pub fn live_create_code(
        &mut self,
        source: &str,
        options: WasmCodeOptions,
    ) -> Result<WasmCodeHandle, JsValue> {
        noon::Code::new(
            self.inner.local_authoring_scene().map_err(js_error)?,
            source,
            options.options,
        )
        .map(WasmCodeHandle::new)
        .map_err(js_error)
    }
}
