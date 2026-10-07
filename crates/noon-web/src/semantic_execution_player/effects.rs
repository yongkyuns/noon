use super::SemanticExecutionPlayer;
use crate::AuthoringFailure;
use noon::effects::{EffectDefinition, EffectSelector, GlowUpdate};
use noon::Mobject;

impl SemanticExecutionPlayer {
    pub(crate) fn live_set_glow(
        &mut self,
        object: &Mobject,
        update: GlowUpdate,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_glow(object, update))
    }

    pub(crate) fn live_add_effect(
        &mut self,
        object: &Mobject,
        definition: EffectDefinition,
        name: &str,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.add_effect(object, definition, name))
    }

    pub(crate) fn live_set_effect(
        &mut self,
        object: &Mobject,
        selector: EffectSelector<'_>,
        update: GlowUpdate,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_effect(object, selector, update))
    }

    pub(crate) fn live_remove_effect(
        &mut self,
        object: &Mobject,
        selector: EffectSelector<'_>,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.remove_effect(object, selector))
    }

    pub(crate) fn live_remove_glow(&mut self, object: &Mobject) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.remove_glow(object))
    }
}
