//! Monotonic platform-time delivery, independent of the authored realtime clock.
//! No operation list, interpolation, duration or completion policy lives here.

use crate::{NativeApp, NativeHostError};
use noon::integration::{NativePointerInput, NativePointerInputToken};
use std::time::Instant;

impl NativeApp {
    fn property_animation_delta(&self, now: Instant) -> Result<f64, NativeHostError> {
        if !self.execution.property_animation_pending() {
            return Ok(0.0);
        }
        let Some(previous) = self.effect_previous_tick else {
            return Ok(0.0);
        };
        now.checked_duration_since(previous)
            .map(|elapsed| elapsed.as_secs_f64())
            .ok_or_else(|| NativeHostError::Platform("effect timestamp went backwards".to_owned()))
    }

    fn record_property_animation_delivery(&mut self, now: Instant) {
        self.effect_previous_tick = self.execution.property_animation_pending().then_some(now);
    }

    pub(super) fn advance_realtime_property_animations(
        &mut self,
        now: Instant,
    ) -> Result<(), NativeHostError> {
        if self.execution.property_animation_pending() {
            let elapsed = self.property_animation_delta(now)?;
            self.execution.advance_property_animations_by(elapsed)?;
        }
        self.record_property_animation_delivery(now);
        Ok(())
    }

    pub(super) fn submit_pointer_occurrence_at(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
        next_sequence: u64,
        now: Instant,
    ) -> Result<(), NativeHostError> {
        let elapsed = self.property_animation_delta(now)?;
        let publication = self
            .execution
            .submit_native_pointer_input(token, input, elapsed)?;
        // Commit delivery bookkeeping before surfacing an action failure. An
        // accepted release must not become retryable or leave our sequence behind.
        self.next_input_sequence = next_sequence;
        if publication.effect_time_sampled {
            self.record_property_animation_delivery(now);
        }
        publication.action
    }
}

#[cfg(test)]
mod tests;
