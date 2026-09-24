//! Independent effects through the one consumed live scene/session owner.

use super::{LiveContinuation, LiveProgram, LiveProgramError};
use crate::{IndicateOptions, Mobject};
use noon_core::AnimationOptions;
use noon_runtime::PropertyAnimationToken;

impl<C: LiveContinuation> LiveProgram<C> {
    /// Start an action without resuming source or advancing its awaited segment.
    /// Target handles are checked against this program's actual semantic store.
    pub fn start_indicate_effect(
        &mut self,
        target: &Mobject,
        indication: IndicateOptions,
        options: AnimationOptions,
    ) -> Result<Option<PropertyAnimationToken>, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("start an independent Indicate")?;
        let token = self
            .scene
            .owned_live()
            .start_indicate_effect(target, indication, options)
            .map_err(LiveProgramError::Effect)?;
        self.refresh_pending_publication();
        Ok(token)
    }

    /// Deliver elapsed input to Runtime's independent property-effect domain.
    /// Source completion and authored play/pause are not changed by this call.
    pub fn advance_property_animations_by(
        &mut self,
        delta: f64,
    ) -> Result<(), LiveProgramError<C::Error>> {
        self.ensure_host_input_available("advance independent property animations")?;
        self.scene
            .owned_execution_mut()
            .advance_property_animations_by(delta)
            .map_err(|error| LiveProgramError::Effect(error.into()))?;
        self.refresh_pending_publication();
        Ok(())
    }

    /// Cancel exactly one runtime-bound operation and refresh any pending endpoint.
    pub fn cancel_property_animation(
        &mut self,
        token: PropertyAnimationToken,
    ) -> Result<(), LiveProgramError<C::Error>> {
        self.ensure_host_input_available("cancel an independent property animation")?;
        self.scene
            .owned_execution_mut()
            .cancel_property_animation(token)
            .map_err(|error| LiveProgramError::Effect(error.into()))?;
        self.refresh_pending_publication();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
