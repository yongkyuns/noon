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
    fn adapter_rejects_unsupported_glow_profile_without_poisoning_publication() {
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

#[cfg(test)]
mod playback_tests {
    use super::super::*;
    use crate::retained_execution_transport::RetainedExecutionDeltaEnvelope;
    use noon::{
        effects::{EffectDefinition, GlowUpdate, Pixels},
        AnimationOptions, Color, RateFunction,
    };

    #[test]
    fn adapter_reuses_the_live_player_for_glow_targets_completion_and_removal() {
        let mut context = CanonicalAuthoringScene::default();
        let mut source = context.scene.circle(0.4).unwrap();
        source.disable_stroke().unwrap();
        source.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &source).unwrap();
        context.live_player(1.0).unwrap();
        let player = context.active_live_player().unwrap();
        let identity = player.ownership_identity();
        player
            .live_set_glow(
                &source,
                GlowUpdate::default()
                    .color(Color::RED)
                    .radius(Pixels(3.25))
                    .intensity(0.4),
            )
            .unwrap();
        let original = source.get_effect("glow").unwrap();
        let initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
        assert_eq!(initial.objects.len(), 1);
        assert_eq!(
            initial.objects[0].glow.unwrap().attachment,
            original.node_id()
        );
        let target = player.live_target_editor(&source).unwrap();
        assert_ne!(
            target.get_effect("glow").unwrap().node_id(),
            original.node_id()
        );
        assert_eq!(
            target
                .get_effect("glow")
                .unwrap()
                .authored_definition()
                .unwrap(),
            original.authored_definition().unwrap()
        );
        player.live_set_translation(&target, 2.0, 0.0).unwrap();
        player
            .live_set_glow(
                &target,
                GlowUpdate::default()
                    .color(Color::BLUE)
                    .radius(Pixels(6.5))
                    .intensity(1.4),
            )
            .unwrap();
        let endpoint = player
            .live_declare_and_activate_composition(
                &noon::AnimationCompositionRequest::TransformTo(noon::TransformToRequest::new(
                    &source,
                    &target,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )),
                AnimationOptions::new(),
            )
            .unwrap();
        assert_eq!(endpoint, 1.0);
        player.live_advance_segment_to(0.5).unwrap();
        let midpoint: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&player.drain_delta_json().unwrap().unwrap()).unwrap();
        assert_eq!(midpoint.objects.len(), 1);
        let glow = midpoint.objects[0].glow.unwrap();
        assert_eq!(glow.attachment, original.node_id());
        assert_eq!(glow.intensity, 0.4 + (1.4 - 0.4) * 0.5);
        assert_eq!(glow.radius, Pixels(4.875).into());
        assert_eq!(midpoint.objects[0].transform.translation.x, 1.0);
        player.live_advance_segment_to(endpoint).unwrap();
        player.live_complete_segment().unwrap();
        let EffectDefinition::Glow(completed) = original.authored_definition().unwrap();
        assert_eq!(completed.intensity(), 1.4);
        assert_eq!(completed.radius(), Pixels(6.5).into());
        assert_eq!(completed.color(), Color::BLUE);
        player.live_remove_glow(&source).unwrap();
        let removed: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&player.drain_delta_json().unwrap().unwrap()).unwrap();
        assert_eq!(removed.objects.len(), 1);
        assert!(removed.objects[0].glow.is_none());
        assert!(
            removed.removed_slots.is_empty(),
            "effect removal is not source retirement"
        );
        assert_eq!(removed.objects[0].slot, initial.objects[0].slot);
        assert_eq!(removed.objects[0].object, initial.objects[0].object);
        assert!(original.authored_definition().is_err());
        assert_eq!(player.ownership_identity(), identity);
        assert_eq!(player.time(), endpoint);
    }
}
