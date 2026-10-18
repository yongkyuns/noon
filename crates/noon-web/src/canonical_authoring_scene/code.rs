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
        if self.inner.player_ownership.is_unstarted() {
            noon::Code::new(&mut self.inner.scene, source, options.options)
                .map(WasmCodeHandle::new)
                .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_code(source, options.options)
                .map(WasmCodeHandle::new)
                .map_err(js_error)
        }
    }
}
