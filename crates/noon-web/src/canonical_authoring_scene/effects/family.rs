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

    #[wasm_bindgen(js_name = liveSetFamilyGlow)]
    pub fn live_set_family_glow(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        update: &WasmEffectUpdate,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_set_glow(family.semantic_family_ref(), update.value.clone())
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = liveAddFamilyEffect)]
    pub fn live_add_family_effect(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        definition: &WasmEffectDefinition,
        name: &str,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_add_effect(family.semantic_family_ref(), definition.definition(), name)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = liveSetFamilyEffect)]
    pub fn live_set_family_effect(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        name: &str,
        update: &WasmEffectUpdate,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_set_effect(
                family.semantic_family_ref(),
                name.into(),
                update.value.clone(),
            )
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = liveSetFamilyEffectHandle)]
    pub fn live_set_family_effect_handle(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        effect: &WasmEffectHandle,
        update: &WasmEffectUpdate,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_set_effect(
                family.semantic_family_ref(),
                (&effect.handle).into(),
                update.value.clone(),
            )
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = liveRemoveFamilyEffect)]
    pub fn live_remove_family_effect(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        name: &str,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_remove_effect(family.semantic_family_ref(), name.into())
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = liveRemoveFamilyEffectHandle)]
    pub fn live_remove_family_effect_handle(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
        effect: &WasmEffectHandle,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_remove_effect(family.semantic_family_ref(), (&effect.handle).into())
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = liveRemoveFamilyGlow)]
    pub fn live_remove_family_glow(
        &mut self,
        family: &WasmAuthoringFamilyHandle,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_remove_glow(family.semantic_family_ref())
            .map_err(js_error)
    }
}
