//! Independent effects through the one consumed live scene/session owner.

use super::{LiveContinuation, LiveProgram, LiveProgramError};
use crate::{IndicateOptions, Mobject};
use noon_core::AnimationOptions;
use noon_runtime::PropertyAnimationToken;

impl<C: LiveContinuation> LiveProgram<C> {
    /// Configure click recognition without changing editor selection presentation.
    pub fn set_pointer_fill_clicks(
        &mut self,
        max_movement: Option<f32>,
    ) -> Result<(), LiveProgramError<C::Error>> {
        self.ensure_host_input_available("configure pointer clicks")?;
        let session = self.scene.owned_execution_mut();
        match max_movement {
            Some(value) => session.enable_pointer_fill_clicks(value),
            None => session.disable_pointer_fill_selection(),
        }
        .map_err(LiveProgramError::Input)
    }

    /// Admit input and execute its authored action through this program's live owner.
    /// Action failures are retained inside the accepted-input publication.
    pub fn submit_pointer_input_with_actions(
        &mut self,
        token: &crate::integration::NativePointerInputToken,
        input: noon_core::NativePointerInput,
    ) -> Result<crate::PointerActionPublication, LiveProgramError<C::Error>> {
        self.submit_pointer_input_with_actions_after_elapsed(token, input, 0.0)
    }

    /// Deliver an accepted click's preceding effect interval without advancing
    /// authored time or bypassing the program's endpoint/publication ownership.
    /// See `LiveSession::submit_pointer_input_with_actions_after_elapsed` for the
    /// distinction between admission failure and a post-admission action error.
    pub fn submit_pointer_input_with_actions_after_elapsed(
        &mut self,
        token: &crate::integration::NativePointerInputToken,
        input: noon_core::NativePointerInput,
        elapsed: f64,
    ) -> Result<crate::PointerActionPublication, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("dispatch pointer input actions")?;
        let publication = self
            .scene
            .owned_live()
            .submit_pointer_input_with_actions_after_elapsed(token, input, elapsed)
            .map_err(LiveProgramError::Effect)?;
        self.refresh_pending_publication();
        Ok(publication)
    }

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
