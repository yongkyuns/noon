//! Synchronous native-input admission and publication on the existing session.
//!
//! This module owns no queue, scheduler, scene, or gesture recognizer. Accepted
//! pointer records publish sampled state and subscribed event counters in one
//! runtime transaction. Rejected records are not retained or acknowledged.

use noon_core::{
    NativeEventOccurrence, NativeEventSource, NativeInputRuntimeError, NativeInputValue,
    NativePointerContext, NativePointerId, NativePointerInput, NativePointerInputKind,
    NativeStateSource, NativeStateUpdate, PublicationContext, ReactiveError, ReactiveValue,
    SemanticNodeId, SignalId,
};
use noon_runtime::{EvaluationError, FrameState, RuntimeIdentity};

use super::ExecutionSession;

pub(super) const NATIVE_EVENT_SEQUENCE_WRAP: f32 = 1_000_000.0;

// Keep the existing input error at the session's public re-export.
// INPUT_ERROR_DEFINITION

/// Session-issued association between a collector's coordinates and a publication.
///
/// The runtime identity and binding generation cannot be supplied by a caller.
/// Capture this token when converting an occurrence's coordinates, not when later
/// draining it. Moving a session preserves tokens; cloning/replacing it does not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePointerInputToken {
    runtime: RuntimeIdentity,
    generation: u64,
    pointer: NativePointerId,
    context: NativePointerContext,
}

impl NativePointerInputToken {
    pub const fn pointer(&self) -> NativePointerId {
        self.pointer
    }

    pub const fn context(&self) -> NativePointerContext {
        self.context
    }
}

/// An accepted occurrence and the actual before/after runtime publications.
///
/// The original coordinates, modifiers, identity, sequence and cancellation reason
/// remain available without consulting a later sampled pointer position. This is
/// a delivery receipt, not a queued interaction action or a rendered-frame fence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativePointerInputPublication {
    input: NativePointerInput,
    previous: PublicationContext,
    current: PublicationContext,
}

impl NativePointerInputPublication {
    pub const fn input(self) -> NativePointerInput {
        self.input
    }

    pub const fn previous_publication(self) -> PublicationContext {
        self.previous
    }

    pub const fn publication(self) -> PublicationContext {
        self.current
    }
}

#[derive(Clone, Copy, Debug)]
struct PointerBinding {
    pointer: NativePointerId,
    view_revision: u64,
    generation: u64,
}

/// Only ingress configuration is retained here. Sampled values stay in the
/// existing reactive runtime, and sequence order stays session-global.
#[derive(Clone, Debug)]
pub(super) struct PointerInputState {
    binding: Option<PointerBinding>,
    next_generation: Option<u64>,
}

impl Default for PointerInputState {
    fn default() -> Self {
        Self {
            binding: None,
            next_generation: Some(0),
        }
    }
}

impl ExecutionSession {
    /// Select the one exact pointer projected into the current unkeyed native
    /// pointer signals. Other pointers are rejected, never implicitly merged.
    ///
    /// Call again for a replaced collector or changed coordinate mapping. Even
    /// identical arguments establish a new binding generation. Existing button
    /// states are cleared atomically without synthesizing releases or coordinates;
    /// only after that succeeds are previous tokens invalidated. The host owns
    /// coordinate conversion and must change `view_revision` when that mapping
    /// changes. This operation does not select scene objects or acquire OS capture.
    pub fn configure_native_pointer_input(
        &mut self,
        pointer: NativePointerId,
        view_revision: u64,
    ) -> Result<NativePointerInputToken, ExecutionSessionInputError> {
        self.ensure_direct_input_ingress_available()?;
        let generation = self
            .pointer_input
            .next_generation
            .ok_or(ExecutionSessionInputError::PointerBindingSequenceExhausted)?;
        let inputs = self.pointer_button_reset_inputs();
        if !inputs.is_empty() {
            self.apply_reactive_input_batch(inputs)?;
        }
        self.pointer_input.binding = Some(PointerBinding {
            pointer,
            view_revision,
            generation,
        });
        self.pointer_input.next_generation = generation.checked_add(1);
        self.native_pointer_input_token()
    }

    /// Observe the configured mapping and the exact current effective publication.
    /// A token does not pin historical geometry or imply that the frame was shown.
    pub fn native_pointer_input_token(
        &self,
    ) -> Result<NativePointerInputToken, ExecutionSessionInputError> {
        let binding = self
            .pointer_input
            .binding
            .ok_or(ExecutionSessionInputError::PointerNotConfigured)?;
        Ok(NativePointerInputToken {
            runtime: self.runtime_identity(),
            generation: binding.generation,
            pointer: binding.pointer,
            context: NativePointerContext {
                publication: self.publication_context(),
                view_revision: binding.view_revision,
            },
        })
    }

    /// Admit one contextual pointer record and publish all of its native effects.
    ///
    /// Delivery is synchronous and unbuffered: every error leaves the occurrence
    /// unaccepted, with no sequence acknowledgement or partial native publication.
    /// Required callbacks must be resolved through their existing session path;
    /// this entry point neither queues around nor bypasses that barrier.
    ///
    /// Positional input currently requires the exact current publication. A stale
    /// record must be rejected or explicitly cancelled, not retagged as current.
    /// Cancellation has no coordinates and may clear buttons after frame advance,
    /// but still requires the same runtime, pointer and binding generation.
    /// Native event subscribers observe every accepted down/up occurrence; cancel
    /// is never projected into a successful release. Picking/click/drag policy is
    /// intentionally not implemented here.
    pub fn submit_native_pointer_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, ExecutionSessionInputError> {
        self.ensure_direct_input_ingress_available()?;
        if token.runtime != self.runtime_identity() {
            return Err(ExecutionSessionInputError::ForeignPointerRuntime);
        }
        let binding = self
            .pointer_input
            .binding
            .ok_or(ExecutionSessionInputError::PointerNotConfigured)?;
        if token.generation != binding.generation {
            return Err(ExecutionSessionInputError::StalePointerBinding);
        }
        if input.pointer() != binding.pointer {
            return Err(ExecutionSessionInputError::WrongPointer {
                expected: binding.pointer,
                actual: input.pointer(),
            });
        }
        if input.context() != token.context {
            return Err(ExecutionSessionInputError::PointerContextMismatch);
        }
        let previous = self.publication_context();
        if input.position().is_some() && input.context().publication != previous {
            return Err(ExecutionSessionInputError::StalePointerPublication {
                expected: previous,
                actual: input.context().publication,
            });
        }
        self.require_native_event_sequence(input.sequence())?;

        let mut inputs = if matches!(input.kind(), NativePointerInputKind::Cancel(_)) {
            self.pointer_button_reset_inputs()
        } else {
            Vec::new()
        };
        for update in input.state_updates() {
            self.append_native_state_inputs(update, &mut inputs);
        }
        if let Some(event) = input.button_event() {
            self.append_native_event_inputs(&event, &mut inputs);
        }
        if !inputs.is_empty() {
            self.apply_reactive_input_batch(inputs)?;
        }
        self.last_native_event_sequence = Some(input.sequence());
        Ok(NativePointerInputPublication {
            input,
            previous,
            current: self.publication_context(),
        })
    }

    /// Deliver one normalized sampled native state through signal-owned routes.
    /// Unbound sources remain no-ops. Once contextual pointer input is configured,
    /// pointer state must use `submit_native_pointer_input` rather than a second lane.
    pub fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        let update = NativeStateUpdate::new(source, value)?;
        if self.pointer_input.binding.is_some()
            && matches!(
                update.source,
                NativeStateSource::PointerPosition | NativeStateSource::PointerButton { .. }
            )
        {
            return Err(ExecutionSessionInputError::ContextualPointerRequired);
        }
        let mut inputs = Vec::new();
        self.append_native_state_inputs(update, &mut inputs);
        if !inputs.is_empty() {
            self.ensure_direct_input_ingress_available()?;
            self.apply_reactive_input_batch(inputs)?;
        }
        Ok(self.runtime.frame())
    }

    /// Deliver one explicitly ordered discrete native event occurrence.
    /// All native events and contextual pointer records share one sequence domain.
    pub fn emit_native_event(
        &mut self,
        occurrence: NativeEventOccurrence,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        if self.pointer_input.binding.is_some()
            && matches!(
                occurrence.source,
                NativeEventSource::PointerDown { .. } | NativeEventSource::PointerUp { .. }
            )
        {
            return Err(ExecutionSessionInputError::ContextualPointerRequired);
        }
        self.require_native_event_sequence(occurrence.sequence)?;
        let mut inputs = Vec::new();
        self.append_native_event_inputs(&occurrence, &mut inputs);
        if !inputs.is_empty() {
            self.ensure_direct_input_ingress_available()?;
            self.apply_reactive_input_batch(inputs)?;
        }
        self.last_native_event_sequence = Some(occurrence.sequence);
        Ok(self.runtime.frame())
    }

    fn require_native_event_sequence(
        &self,
        next: u64,
    ) -> Result<(), ExecutionSessionInputError> {
        if let Some(previous) = self.last_native_event_sequence {
            if next <= previous {
                return Err(ExecutionSessionInputError::NativeEventOutOfOrder { previous, next });
            }
        }
        Ok(())
    }

    fn append_native_state_inputs(
        &self,
        update: NativeStateUpdate,
        inputs: &mut Vec<(SignalId, ReactiveValue)>,
    ) {
        let value = reactive_value_from_native(update.value);
        inputs.extend(
            self.reactive_projection
                .native_state_targets(&update.source)
                .iter()
                .map(|&signal| (signal, value.clone())),
        );
    }

    fn append_native_event_inputs(
        &self,
        occurrence: &NativeEventOccurrence,
        inputs: &mut Vec<(SignalId, ReactiveValue)>,
    ) {
        for &signal in self
            .reactive_projection
            .native_event_targets(&occurrence.source)
        {
            let value = self
                .runtime
                .reactive_value(signal)
                .expect("lowered native event target must remain a live reactive signal");
            let ReactiveValue::Scalar(current) = value else {
                unreachable!("semantic native event declaration validates a scalar input signal")
            };
            let next = if *current >= NATIVE_EVENT_SEQUENCE_WRAP {
                0.0
            } else {
                *current + 1.0
            };
            inputs.push((signal, ReactiveValue::Scalar(next)));
        }
    }

    fn pointer_button_reset_inputs(&self) -> Vec<(SignalId, ReactiveValue)> {
        let mut inputs = Vec::new();
        // The button vocabulary is exactly u8. A bounded 256-source lookup resets
        // even pre-existing/initially-true routes, without copying sampled state or
        // scanning the scene. No position or release-event value is manufactured.
        for button in 0..=u8::MAX {
            self.append_native_state_inputs(
                NativeStateUpdate {
                    source: NativeStateSource::PointerButton { button },
                    value: NativeInputValue::Bool(false),
                },
                &mut inputs,
            );
        }
        inputs
    }

    // INPUT_BATCH_DEFINITION
}

// NATIVE_VALUE_CONVERSION

#[cfg(test)]
mod tests;
