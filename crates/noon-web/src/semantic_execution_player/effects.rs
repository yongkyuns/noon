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
