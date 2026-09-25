//! Authored click declarations and action dispatch through the existing live owner.
//!
//! Input admission and action execution are separate outcomes: an action failure
//! never turns an already-accepted physical event into a retryable input error.

use std::rc::Rc;

use super::{IndicateOptions, LiveSession, LiveSessionError};
use crate::integration::{NativePointerInputPublication, NativePointerInputToken};
use crate::{AuthoringError, ExecutionSessionAnimationError, Mobject};
use noon_core::{
    AnimationOptions, NativePointerInput, PublicationContext, SemanticMutationTransaction,
    SemanticMutationTransactionResult, SemanticPointerClickAction,
};
use noon_runtime::{PropertyAnimationError, PropertyAnimationToken};

/// Result of dispatching the bound action after accepted input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerClickActionOutcome {
    /// No decided click hit or no authored action on its exact semantic target.
    None,
    Started(PropertyAnimationToken),
    /// The request has no changed channels, so no operation was allocated.
    Unchanged,
    /// Runtime declined an overlapping claim without restarting its owner.
    /// Any separately supplied preceding effect interval has already been delivered.
    /// This is an explicit arbitration outcome, not suppression of arbitrary errors.
    Busy,
}

/// Accepted input plus its action outcome. `current` is the final publication,
/// which may be newer than the input receipt when activation acquires claims.
#[derive(Debug)]
pub struct PointerActionPublication {
    input: NativePointerInputPublication,
    action: Result<PointerClickActionOutcome, LiveSessionError>,
    current: PublicationContext,
    effect_time_sampled: bool,
}

impl PointerActionPublication {
    pub const fn input(&self) -> &NativePointerInputPublication {
        &self.input
    }
    pub fn action(&self) -> Result<PointerClickActionOutcome, &LiveSessionError> {
        self.action.as_ref().copied()
    }
    pub const fn current(&self) -> PublicationContext {
        self.current
    }
    /// A decided click samples the host's elapsed-time delivery before activation.
    /// Motion, press, cancellation and rejected gestures do not consume that delta.
    /// Hosts advance their delivery baseline only after this accepted boundary.
    pub fn effect_time_sampled(&self) -> bool {
        self.effect_time_sampled
    }
}

impl Mobject {
    /// Author a primary-click action before execution. For an already-running
    /// scene, use LiveSession's setter so the semantic revision is published.
    pub fn set_pointer_click_action(
        &self,
        action: Option<SemanticPointerClickAction>,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.validate()?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_pointer_click_action(self.node_id(), action);
        let mut store = self.integration_store().borrow_mut();
        Ok(transaction.prepare(&mut store)?.commit())
    }
}

impl LiveSession<'_> {
    pub fn set_pointer_click_action(
        &mut self,
        target: &Mobject,
        action: Option<SemanticPointerClickAction>,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(target)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_pointer_click_action(target.node_id(), action);
        self.apply(transaction)
    }

    /// Admit ordered native input, then dispatch at most one scene-authored action
    /// for its accepted click. The classifier must be explicitly configured via
    /// `enable_pointer_fill_clicks` (recognition only) or editor fill selection.
    ///
    /// The outer error means no input was accepted. An inner action error means
    /// input was accepted and must not be retried. Action options/target are read
    /// from the same authoritative semantic publication, never a host registry.
    pub fn submit_pointer_input_with_actions(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<PointerActionPublication, LiveSessionError> {
        self.submit_pointer_input_with_actions_after_elapsed(token, input, 0.0)
    }

    /// Admit against the displayed publication first. For a decided click only,
    /// deliver elapsed time to existing effects before activating its new action.
    /// This prevents both stale-before-admission rejection and charging a newly
    /// started operation for wall time that preceded its click. An ordinary motion
    /// must not tick effects and invalidate the following edge's displayed receipt.
    ///
    /// `elapsed` is finite, nonnegative seconds since the host's last successful
    /// effect delivery. A failed outer admission consumes none of it. Inspect
    /// `effect_time_sampled` on success before updating the host delivery baseline;
    /// an inner action error is post-admission and must never retry the occurrence.
    pub fn submit_pointer_input_with_actions_after_elapsed(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
        elapsed: f64,
    ) -> Result<PointerActionPublication, LiveSessionError> {
        if !elapsed.is_finite() || elapsed < 0.0 {
            return Err(ExecutionSessionAnimationError::PropertyAnimation(
                PropertyAnimationError::InvalidDelta(elapsed),
            )
            .into());
        }
        self.session
            .require_published_store(&self.store.borrow())
            .map_err(ExecutionSessionAnimationError::AuthoredPublication)?;
        let receipt = self
            .session
            .submit_native_pointer_input(token, input)
            .map_err(ExecutionSessionAnimationError::EffectInput)?;
        let clicked = receipt.selection_click().is_some();
        let elapsed_result = if clicked && elapsed > 0.0 && self.session.has_property_animations() {
            self.session
                .advance_property_animations_by(elapsed)
                .map(|_| ())
                .map_err(LiveSessionError::from)
        } else {
            Ok(())
        };
        let effect_time_sampled = clicked && elapsed_result.is_ok();
        let action = elapsed_result.and_then(|()| self.dispatch_pointer_click(&receipt));
        Ok(PointerActionPublication {
            input: receipt,
            action,
            current: self.session.publication_context(),
            effect_time_sampled,
        })
    }

    // Only called on a newly accepted occurrence. No public receipt-replay entrypoint.
    fn dispatch_pointer_click(
        &mut self,
        receipt: &NativePointerInputPublication,
    ) -> Result<PointerClickActionOutcome, LiveSessionError> {
        let Some(node) = receipt.selection_click().and_then(|click| click.target()) else {
            return Ok(PointerClickActionOutcome::None);
        };
        let action = self
            .store
            .borrow()
            .semantic_object_state_checked(node)
            .map_err(AuthoringError::from)?
            .pointer_click_action();
        let Some(action) = action else {
            return Ok(PointerClickActionOutcome::None);
        };
        let target = Mobject::from_node(Rc::clone(self.store), node)?;
        let result = match action {
            SemanticPointerClickAction::Indicate {
                scale_factor,
                color,
                run_time,
            } => self.start_indicate_effect(
                &target,
                IndicateOptions::new(scale_factor, color),
                AnimationOptions::new().run_time(run_time),
            ),
        };
        match result {
            Ok(Some(token)) => Ok(PointerClickActionOutcome::Started(token)),
            Ok(None) => Ok(PointerClickActionOutcome::Unchanged),
            Err(LiveSessionError::Activation(
                ExecutionSessionAnimationError::PropertyAnimation(
                    PropertyAnimationError::ChannelBusy { .. },
                ),
            )) => Ok(PointerClickActionOutcome::Busy),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests;
