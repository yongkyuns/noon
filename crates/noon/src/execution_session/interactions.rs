//! Time-qualified compiled click actions in the existing session input lane.
use super::{ExecutionSession, ExecutionSessionInputError, PointerSelectionClick};
use noon_runtime::PreparedTransientAnimation;

impl ExecutionSession {
    pub fn interactions_active(&self) -> bool {
        self.runtime.interactions_active()
    }

    /// Advance only input-triggered effective animation using platform monotonic seconds.
    pub fn advance_interactions(
        &mut self,
        wall_time_seconds: f64,
    ) -> Result<(), ExecutionSessionInputError> {
        self.runtime
            .advance_interactions(wall_time_seconds)
            .map_err(ExecutionSessionInputError::Interaction)
    }

    pub(super) fn prepare_click_animation(
        &self,
        click: Option<PointerSelectionClick>,
    ) -> Result<Option<PreparedTransientAnimation>, ExecutionSessionInputError> {
        let Some(node) = click.and_then(|click| click.target()) else {
            return Ok(None);
        };
        // Runtime-owned channel arbitration admits disjoint playback and defers
        // an effect when an authored/native driver owns one of its channels.
        if self.pending_callback.is_some() {
            return Ok(None);
        }
        let Some(object) = self.execution_object_id(node) else {
            return Ok(None);
        };
        let Some(binding) = self.runtime.click_indicate_binding(object) else {
            return Ok(None);
        };
        if self.runtime.object_has_effective_driver(object) {
            return Ok(None);
        }
        self.runtime
            .prepare_click_indicate(object, binding)
            .map_err(ExecutionSessionInputError::Interaction)
    }
}

#[cfg(test)]
mod tests;
