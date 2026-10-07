#[cfg(target_arch = "wasm32")]
use super::wasm::CanonicalAuthoringSceneContext;
#[cfg(target_arch = "wasm32")]
use crate::{
    authoring_error::js_error, WasmAuthoringMobjectHandle, WasmEffectHandle, WasmGlow,
    WasmGlowUpdate,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = liveSetGlow)]
    pub fn live_set_glow(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
        update: &WasmGlowUpdate,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_set_glow(object.semantic_mobject(), update.value)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = liveAddEffect)]
    pub fn live_add_effect(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
        definition: &WasmGlow,
        name: &str,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_add_effect(object.semantic_mobject(), definition.definition(), name)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = liveSetEffect)]
    pub fn live_set_effect(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
        name: &str,
        update: &WasmGlowUpdate,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_set_effect(object.semantic_mobject(), name.into(), update.value)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = liveSetEffectHandle)]
    pub fn live_set_effect_handle(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
        effect: &WasmEffectHandle,
        update: &WasmGlowUpdate,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_set_effect(
                object.semantic_mobject(),
                (&effect.handle).into(),
                update.value,
            )
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = liveRemoveEffect)]
    pub fn live_remove_effect(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
        name: &str,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_remove_effect(object.semantic_mobject(), name.into())
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = liveRemoveEffectHandle)]
    pub fn live_remove_effect_handle(
        &mut self,
        object: &WasmAuthoringMobjectHandle,
        effect: &WasmEffectHandle,
    ) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_remove_effect(object.semantic_mobject(), (&effect.handle).into())
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = liveRemoveGlow)]
    pub fn live_remove_glow(&mut self, object: &WasmAuthoringMobjectHandle) -> Result<(), JsValue> {
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_remove_glow(object.semantic_mobject())
            .map_err(js_error)
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use noon::effects::{Glow, GlowUpdate};

    #[test]
    fn adapter_live_effect_calls_use_the_existing_publication_guard() {
        let mut context = CanonicalAuthoringScene::default();
        let dot = context.scene.circle(0.08).unwrap();
        context.bind_mobject(ObjectId::new(0), &dot).unwrap();
        context.live_player(1.0).unwrap();
        let revision = context.scene.revision();
        let player = context.active_live_player().unwrap();
        let identity = player.ownership_identity();
        let time = player.time();
        assert_eq!(
            player
                .live_set_glow(&dot, GlowUpdate::default())
                .unwrap_err()
                .category,
            "unsupported_operation"
        );
        assert_eq!(
            player
                .live_add_effect(&dot, Glow::default().into(), "accent")
                .unwrap_err()
                .category,
            "unsupported_operation"
        );
        assert!(player
            .live_set_effect(&dot, "missing".into(), GlowUpdate::default())
            .is_err());
        assert!(player.live_remove_effect(&dot, "missing".into()).is_err());
        player.live_remove_glow(&dot).unwrap();
        assert_eq!(player.ownership_identity(), identity);
        assert_eq!(player.time(), time);
        assert_eq!(context.scene.revision(), revision);
        assert!(dot.get_effect("glow").is_err());
        assert!(dot.get_effect("accent").is_err());
        // A rejected appearance edit does not poison ordinary live edits.
        context
            .active_live_player()
            .unwrap()
            .live_set_translation(&dot, 2.0, 0.0)
            .unwrap();
        assert_eq!(context.mobject_layout(&dot).unwrap().0, 2.0);
    }
}
