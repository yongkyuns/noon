//! Checked family mutation routing through the existing live execution owner.
use super::*;
use crate::WasmAuthoringFamilyHandle;

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = prepareFamilyMembership)]
    pub fn prepare_family_membership(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        batch: WasmSceneMembershipBatch,
    ) -> Result<WasmStagedEffectPublication, JsValue> {
        self.prepare_membership_target(family.semantic_family_ref().into(), batch)
    }

    #[wasm_bindgen(js_name = prepareFamilyEffectAddition)]
    pub fn prepare_family_effect_addition(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        definition: &WasmEffectDefinition,
        name: &str,
    ) -> Result<WasmStagedEffectPublication, JsValue> {
        let pending = self
            .inner
            .active_live_player()
            .map_err(js_error)?
            .prepare_effect_addition(family.semantic_family_ref(), definition.definition(), name)
            .map_err(js_error)?;
        Ok(WasmStagedEffectPublication {
            pending: Some(pending),
            reservations: Default::default(),
        })
    }

    #[wasm_bindgen(js_name = setFamilyGlow)]
    pub fn set_family_glow(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        update: &WasmEffectUpdate,
    ) -> Result<(), JsValue> {
        self.inner
            .edit_scene(|scene| scene.set_glow(family.semantic_family_ref(), update.value.clone()))
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = addFamilyEffect)]
    pub fn add_family_effect(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        definition: &WasmEffectDefinition,
        name: &str,
    ) -> Result<(), JsValue> {
        self.inner
            .edit_scene(|scene| {
                scene.add_effect(family.semantic_family_ref(), definition.definition(), name)
            })
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setFamilyEffect)]
    pub fn set_family_effect(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        name: &str,
        update: &WasmEffectUpdate,
    ) -> Result<(), JsValue> {
        self.inner
            .edit_scene(|scene| {
                scene.set_effect(family.semantic_family_ref(), name, update.value.clone())
            })
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setFamilyEffectHandle)]
    pub fn set_family_effect_handle(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        effect: &WasmEffectHandle,
        update: &WasmEffectUpdate,
    ) -> Result<(), JsValue> {
        self.inner
            .edit_scene(|scene| {
                scene.set_effect(
                    family.semantic_family_ref(),
                    &effect.handle,
                    update.value.clone(),
                )
            })
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = removeFamilyEffect)]
    pub fn remove_family_effect(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        name: &str,
    ) -> Result<(), JsValue> {
        self.inner
            .edit_scene(|scene| scene.remove_effect(family.semantic_family_ref(), name))
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = removeFamilyEffectHandle)]
    pub fn remove_family_effect_handle(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        effect: &WasmEffectHandle,
    ) -> Result<(), JsValue> {
        self.inner
            .edit_scene(|scene| scene.remove_effect(family.semantic_family_ref(), &effect.handle))
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = removeFamilyGlow)]
    pub fn remove_family_glow(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
    ) -> Result<(), JsValue> {
        self.inner
            .edit_scene(|scene| scene.remove_glow(family.semantic_family_ref()))
            .map_err(js_error)
    }
}
