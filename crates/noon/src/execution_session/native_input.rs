//! Native ingress shares the session's existing reactive transaction and sequence.
//!
//! This synchronous seam owns no queue: rejection consumes neither input nor
//! sequence. A producer must retain/retry a rejected occurrence or surface a fault;
//! it must not advance to later input after a required callback blocks delivery.

use noon_core::{
    NativeEventOccurrence, NativeEventSource, NativeInputValue, NativePointerContext,
    NativePointerId, NativePointerInput, NativePointerInputKind, NativeStateSource,
    NativeStateUpdate, PublicationContext, ReactiveValue, SignalId,
};
use noon_runtime::{FrameState, RuntimeIdentity};

use super::{ExecutionSession, ExecutionSessionInputError};

pub(super) const NATIVE_EVENT_SEQUENCE_WRAP: f32 = 1_000_000.0;

/// One explicit source for the existing unkeyed pointer signals, not a picked target.
#[derive(Clone, Debug)]
pub(super) struct PointerBinding {
    pointer: NativePointerId,
    view_revision: u64,
    // Fixed 256-button vocabulary, independent of subscriptions and scene size.
    pressed: [u64; 4],
}

impl PointerBinding {
    fn is_pressed(&self) -> bool {
        self.pressed.iter().any(|word| *word != 0)
    }

    fn commit(&mut self, kind: NativePointerInputKind) {
        match kind {
            NativePointerInputKind::Press { button, .. } => {
                self.pressed[usize::from(button / 64)] |= 1_u64 << (button % 64);
            }
            NativePointerInputKind::Release { button, .. } => {
                self.pressed[usize::from(button / 64)] &= !(1_u64 << (button % 64));
            }
            NativePointerInputKind::Cancel(_) => self.pressed = [0; 4],
            NativePointerInputKind::Move(_) => {}
        }
    }
}

/// Admission failure before any pointer state, event counter or sequence commits.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionPointerInputError {
    Unconfigured,
    ForeignRuntime {
        expected: RuntimeIdentity,
        actual: RuntimeIdentity,
    },
    PointerMismatch {
        expected: NativePointerId,
        actual: NativePointerId,
    },
    StaleContext {
        expected: NativePointerContext,
        actual: NativePointerContext,
    },
    PointerStillPressed,
    ViewRevisionNotIncreasing {
        previous: u64,
        next: u64,
    },
    Input(ExecutionSessionInputError),
}

impl std::fmt::Display for ExecutionSessionPointerInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unconfigured => formatter.write_str("no native pointer source is selected"),
            Self::ForeignRuntime { expected, actual } => write!(
                formatter,
                "pointer runtime {actual:?} does not match {expected:?}"
            ),
            Self::PointerMismatch { expected, actual } => write!(
                formatter,
                "pointer {actual:?} does not match selected source {expected:?}"
            ),
            Self::StaleContext { expected, actual } => write!(
                formatter,
                "pointer context {actual:?} does not match current context {expected:?}"
            ),
            Self::PointerStillPressed => formatter.write_str(
                "release or cancel pressed pointer state before changing its source/view",
            ),
            Self::ViewRevisionNotIncreasing { previous, next } => write!(
                formatter,
                "pointer view revision must increase: previous {previous}, next {next}"
            ),
            Self::Input(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecutionSessionPointerInputError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ExecutionSessionInputError> for ExecutionSessionPointerInputError {
    fn from(error: ExecutionSessionInputError) -> Self {
        Self::Input(error)
    }
}

/// Accepted occurrence and the coherent publication after its native projection.
///
/// The original record retains cancellation and occurrence-local motion evidence.
/// This is not a click, a picking result or an independently draining event queue.
/// It does not retain the pre-input frame or authorize delayed target selection.
/// C5 interpretation must join session ordering, not replay receipts against a newer frame.
#[must_use = "consume the original occurrence, including cancellation, in session order"]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativePointerInputReceipt {
    input: NativePointerInput,
    runtime: RuntimeIdentity,
    publication: PublicationContext,
}

impl NativePointerInputReceipt {
    pub const fn input(self) -> NativePointerInput {
        self.input
    }
    pub const fn runtime(self) -> RuntimeIdentity {
        self.runtime
    }
    pub const fn publication(self) -> PublicationContext {
        self.publication
    }
}

impl ExecutionSession {
    /// Select one pointer for the existing single-pointer native signal vocabulary.
    ///
    /// A changed source or coordinate map requires a strictly newer view revision
    /// and no held buttons. Cancel through the current binding before resize/rebind.
    /// Selecting the exact current binding is a no-op. The platform still owns the
    /// coordinate map and must update this registration when that map changes.
    pub fn configure_native_pointer_input(
        &mut self,
        pointer: NativePointerId,
        view_revision: u64,
    ) -> Result<(), ExecutionSessionPointerInputError> {
        if let Some(binding) = &self.native_pointer {
            if binding.pointer == pointer && binding.view_revision == view_revision {
                return Ok(());
            }
            if binding.is_pressed() {
                return Err(ExecutionSessionPointerInputError::PointerStillPressed);
            }
            if view_revision <= binding.view_revision {
                return Err(
                    ExecutionSessionPointerInputError::ViewRevisionNotIncreasing {
                        previous: binding.view_revision,
                        next: view_revision,
                    },
                );
            }
        }
        // Do not adopt a held legacy signal or a newly declared held button with
        // no matching contextual press. Its current owner must release it first.
        let any_pressed = (0..=u8::MAX).any(|button| {
            self.reactive_projection
                .native_state_targets(&NativeStateSource::PointerButton { button })
                .iter()
                .any(|signal| {
                    self.runtime.reactive_value(*signal) == Some(&ReactiveValue::Bool(true))
                })
        });
        if any_pressed {
            return Err(ExecutionSessionPointerInputError::PointerStillPressed);
        }
        self.native_pointer = Some(PointerBinding {
            pointer,
            view_revision,
            pressed: [0; 4],
        });
        Ok(())
    }

    /// Current context for an immediate occurrence captured using the registered view.
    ///
    /// This does not authenticate the host's coordinate conversion or retain older
    /// rendered frames. Capture the runtime identity when installing the collector;
    /// do not replace that identity with a new session's identity at delivery time.
    pub fn native_pointer_context(&self) -> Option<NativePointerContext> {
        self.native_pointer
            .as_ref()
            .map(|binding| NativePointerContext {
                publication: self.publication_context(),
                view_revision: binding.view_revision,
            })
    }

    /// Admit one contextual occurrence through the existing session input lane.
    ///
    /// Position, button state and subscribed event counters evaluate together and
    /// publish at most one frame epoch. Moves, buttons, cancellations and existing
    /// discrete inputs share one ingress sequence. Authored time is unchanged.
    ///
    /// This first synchronous contract requires the exact current publication and
    /// registered view. It does not guess a new position for stale buffered input.
    /// Required callbacks remain explicitly unsupported by direct input; a pending
    /// manual callback phase rejects without consuming the occurrence or sequence.
    pub fn apply_native_pointer_input(
        &mut self,
        runtime: RuntimeIdentity,
        input: NativePointerInput,
    ) -> Result<NativePointerInputReceipt, ExecutionSessionPointerInputError> {
        if runtime != self.runtime_identity() {
            return Err(ExecutionSessionPointerInputError::ForeignRuntime {
                expected: self.runtime_identity(),
                actual: runtime,
            });
        }
        let binding = self
            .native_pointer
            .as_ref()
            .ok_or(ExecutionSessionPointerInputError::Unconfigured)?;
        if binding.pointer != input.pointer() {
            return Err(ExecutionSessionPointerInputError::PointerMismatch {
                expected: binding.pointer,
                actual: input.pointer(),
            });
        }
        let expected = self.native_pointer_context().expect("binding was checked");
        if input.context() != expected {
            return Err(ExecutionSessionPointerInputError::StaleContext {
                expected,
                actual: input.context(),
            });
        }
        self.validate_native_input_sequence(input.sequence())?;
        self.ensure_direct_input_ingress_available()?;

        let mut values = Vec::new();
        for update in input.state_updates() {
            self.stage_native_state(update, &mut values)?;
        }
        if matches!(input.kind(), NativePointerInputKind::Cancel(_)) {
            // Clear every routed button, including bindings added after a press.
            // This is bounded by the u8 input vocabulary, never by scene size.
            // There is deliberately no PointerUp event and no invented position.
            for button in 0..=u8::MAX {
                self.stage_native_state(
                    NativeStateUpdate {
                        source: NativeStateSource::PointerButton { button },
                        value: NativeInputValue::Bool(false),
                    },
                    &mut values,
                )?;
            }
        }
        if let Some(event) = input.button_event() {
            self.stage_native_event(&event, &mut values);
        }
        if !values.is_empty() {
            self.apply_reactive_input_batch(values)?;
        }
        // All fallible validation/evaluation finished before admission bookkeeping.
        self.native_pointer
            .as_mut()
            .expect("binding was checked")
            .commit(input.kind());
        self.last_native_input_sequence = Some(input.sequence());
        Ok(NativePointerInputReceipt {
            input,
            runtime,
            publication: self.publication_context(),
        })
    }

    /// Deliver sampled native state through the compiler-owned signal routes.
    /// Unbound sources remain no-ops. A contextual pointer cannot bypass its lane.
    pub fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        let update = NativeStateUpdate::new(source, value)?;
        if self.native_pointer.is_some()
            && matches!(
                update.source,
                NativeStateSource::PointerPosition | NativeStateSource::PointerButton { .. }
            )
        {
            return Err(ExecutionSessionInputError::ContextualPointerInputRequired);
        }
        let mut values = Vec::new();
        self.stage_native_state(update, &mut values)?;
        if values.is_empty() {
            return Ok(self.runtime.frame());
        }
        self.ensure_direct_input_ingress_available()?;
        self.apply_reactive_input_batch(values)
    }

    /// Deliver one ordered discrete occurrence using the shared ingress sequence.
    /// Repeated events retain the existing bounded scalar-counter convention.
    pub fn emit_native_event(
        &mut self,
        occurrence: NativeEventOccurrence,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        if self.native_pointer.is_some()
            && matches!(
                occurrence.source,
                NativeEventSource::PointerDown { .. } | NativeEventSource::PointerUp { .. }
            )
        {
            return Err(ExecutionSessionInputError::ContextualPointerInputRequired);
        }
        self.validate_native_input_sequence(occurrence.sequence)?;
        let mut values = Vec::new();
        self.stage_native_event(&occurrence, &mut values);
        if !values.is_empty() {
            self.ensure_direct_input_ingress_available()?;
            self.apply_reactive_input_batch(values)?;
        }
        self.last_native_input_sequence = Some(occurrence.sequence);
        Ok(self.runtime.frame())
    }

    fn validate_native_input_sequence(&self, next: u64) -> Result<(), ExecutionSessionInputError> {
        if let Some(previous) = self.last_native_input_sequence {
            if next <= previous {
                return Err(ExecutionSessionInputError::NativeEventOutOfOrder { previous, next });
            }
        }
        Ok(())
    }

    fn stage_native_state(
        &self,
        update: NativeStateUpdate,
        values: &mut Vec<(SignalId, ReactiveValue)>,
    ) -> Result<(), ExecutionSessionInputError> {
        let update = NativeStateUpdate::new(update.source, update.value)?;
        values.extend(
            self.reactive_projection
                .native_state_targets(&update.source)
                .iter()
                .map(|signal| (*signal, reactive_value_from_native(update.value))),
        );
        Ok(())
    }

    fn stage_native_event(
        &self,
        event: &NativeEventOccurrence,
        values: &mut Vec<(SignalId, ReactiveValue)>,
    ) {
        for signal in self.reactive_projection.native_event_targets(&event.source) {
            let Some(ReactiveValue::Scalar(current)) = self.runtime.reactive_value(*signal) else {
                unreachable!("lowered native event targets are live scalar input signals")
            };
            let next = if *current >= NATIVE_EVENT_SEQUENCE_WRAP {
                0.0
            } else {
                *current + 1.0
            };
            values.push((*signal, ReactiveValue::Scalar(next)));
        }
    }
}

fn reactive_value_from_native(value: NativeInputValue) -> ReactiveValue {
    match value {
        NativeInputValue::Scalar(value) => ReactiveValue::Scalar(value),
        NativeInputValue::Bool(value) => ReactiveValue::Bool(value),
        NativeInputValue::Vec2(value) => ReactiveValue::Vec2(value),
    }
}

#[cfg(test)]
mod tests;
