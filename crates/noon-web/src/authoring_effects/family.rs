//! Family syntax reaches the same checked Rust attachment operations as objects.
use crate::{
    authoring_error::js_error, WasmAuthoringFamilyHandle, WasmEffectDefinition, WasmEffectHandle,
    WasmEffectUpdate,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    #[wasm_bindgen(js_name = setGlow)]
    pub fn set_glow(&self, update: &WasmEffectUpdate) -> Result<(), JsValue> {
        self.semantic_family()?
            .set_glow(update.value)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = addEffect)]
    pub fn add_effect(&self, definition: &WasmEffectDefinition, name: &str) -> Result<(), JsValue> {
        self.semantic_family()?
            .add_effect(definition.definition(), name)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = getEffect)]
    pub fn get_effect(&self, name: &str) -> Result<WasmEffectHandle, JsValue> {
        self.semantic_family()?
            .get_effect(name)
            .map(|handle| WasmEffectHandle { handle })
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setEffect)]
    pub fn set_effect(&self, name: &str, update: &WasmEffectUpdate) -> Result<(), JsValue> {
        self.semantic_family()?
            .set_effect(name, update.value)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setEffectHandle)]
    pub fn set_effect_handle(
        &self,
        effect: &WasmEffectHandle,
        update: &WasmEffectUpdate,
    ) -> Result<(), JsValue> {
        self.semantic_family()?
            .set_effect(&effect.handle, update.value)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = removeEffect)]
    pub fn remove_effect(&self, name: &str) -> Result<(), JsValue> {
        self.semantic_family()?
            .remove_effect(name)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = removeEffectHandle)]
    pub fn remove_effect_handle(&self, effect: &WasmEffectHandle) -> Result<(), JsValue> {
        self.semantic_family()?
            .remove_effect(&effect.handle)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = removeGlow)]
    pub fn remove_glow(&self) -> Result<(), JsValue> {
        self.semantic_family()?.remove_glow().map_err(js_error)
    }
}
