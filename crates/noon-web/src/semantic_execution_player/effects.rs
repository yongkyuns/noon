use super::SemanticExecutionPlayer;
use crate::AuthoringFailure;
use noon::effects::{EffectDefinition, EffectSelector, EffectUpdate};
use noon::MobjectTarget;

impl SemanticExecutionPlayer {
    pub(crate) fn live_set_glow<'owner>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        update: EffectUpdate,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_glow(object, update))
    }

    pub(crate) fn live_add_effect<'owner>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        definition: EffectDefinition,
        name: &str,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.add_effect(object, definition, name))
    }

    pub(crate) fn require_effect_resource_preparation(&mut self) {
        self.session.require_effect_resource_preparation();
    }

    pub(crate) fn prepare_effect_addition<'owner>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        definition: EffectDefinition,
        name: &str,
    ) -> Result<noon::StagedEffectPublication, AuthoringFailure> {
        self.with_live_session(|live| live.prepare_effect_attachments([(object, definition, name)]))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn effect_preparation_bytes(
        &self,
        pending: &noon::StagedEffectPublication,
    ) -> Result<Vec<u8>, String> {
        crate::retained_resource_transport::shader_programs::encode_shader_preparation(
            pending.programs(),
            |program| self.encoder.shader_program_handle(program),
        )
    }

    pub(crate) fn publish_effect_addition(
        &mut self,
        staged: noon::StagedEffectPublication,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.publish_staged_effects(staged))
    }

    pub(crate) fn live_set_effect<'owner>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        selector: EffectSelector<'_>,
        update: EffectUpdate,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_effect(object, selector, update))
    }

    pub(crate) fn live_remove_effect<'owner>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        selector: EffectSelector<'_>,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.remove_effect(object, selector))
    }

    pub(crate) fn live_remove_glow<'owner>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.remove_glow(object))
    }
}
