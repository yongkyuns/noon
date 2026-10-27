//! Synchronous native-input admission and publication on the existing session.
//!
//! This module owns no queue, scheduler, scene, or gesture recognizer. Accepted
//! pointer records publish sampled state and subscribed event counters in one
//! runtime transaction. Rejected records are not retained or acknowledged.

use noon_core::{
    NativeEventOccurrence, NativeEventSource, NativeInputRuntimeError, NativeInputValue,
    NativePointerContext, NativePointerId, NativePointerInput, NativePointerInputKind,
    NativeStateSource, NativeStateUpdate, PublicationContext, ReactiveError, ReactiveValue,
    SemanticNodeId, SemanticStore, SignalId,
};
use noon_runtime::{EffectivePropertyWrite, EvaluationError, FrameState, RuntimeIdentity};

use super::ExecutionSession;

mod presentation;
pub use presentation::{PointerFrameError, PointerFrameSnapshot, PointerFrameView};

pub(super) const NATIVE_EVENT_SEQUENCE_WRAP: f32 = 1_000_000.0;

// Keep the existing input error at the session's public re-export.
/// Error produced when semantic/native reactive input cannot be applied to this execution session.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionInputError {
    InvalidPointerClickTolerance,
    Interaction(String),
    Publication(super::ExecutionSessionPublicationError),
    PointerNotConfigured,
    ForeignPointerRuntime,
    StalePointerBinding,
    PointerContextMismatch,
    StalePointerPublication {
        expected: PublicationContext,
        actual: PublicationContext,
    },
    WrongPointer {
        expected: NativePointerId,
        actual: NativePointerId,
    },
    PointerBindingSequenceExhausted,
    ContextualPointerRequired,
    RequiredCallbackPending,
    RequiredCallbacksConfigured,
    UnknownSemanticSignal(SemanticNodeId),
    NativeInput(NativeInputRuntimeError),
    NativeEventOutOfOrder {
        previous: u64,
        next: u64,
    },
    Reactive(ReactiveError),
    Evaluation(EvaluationError),
    PreparedCommit(noon_runtime::PreparedFrameCommitError),
    TimelineOwnedSignal {
        signal: SemanticNodeId,
    },
    NativeOwnedSignal {
        signal: SemanticNodeId,
    },
}

impl std::fmt::Display for ExecutionSessionInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Interaction(message) => formatter.write_str(message),
            Self::Publication(error) => error.fmt(formatter),
            Self::InvalidPointerClickTolerance => formatter.write_str("click tolerance must be finite and nonnegative logical pixels"),
            Self::PointerNotConfigured => formatter.write_str("contextual pointer input is not configured"),
            Self::ForeignPointerRuntime => formatter.write_str("pointer token belongs to another runtime incarnation"),
            Self::StalePointerBinding => formatter.write_str("pointer binding has been replaced"),
            Self::PointerContextMismatch => formatter.write_str("pointer record does not match its captured view/publication context"),
            Self::StalePointerPublication { expected, actual } => write!(formatter, "pointer publication {actual:?} is not the current publication {expected:?}"),
            Self::WrongPointer { expected, actual } => write!(formatter, "pointer {actual:?} is not the configured pointer {expected:?}"),
            Self::PointerBindingSequenceExhausted => formatter.write_str("pointer binding sequence is exhausted"),
            Self::ContextualPointerRequired => formatter.write_str("configured pointer input requires an occurrence-local context"),
            Self::RequiredCallbackPending => {
                formatter.write_str("a required callback publication is pending")
            }
            Self::RequiredCallbacksConfigured => formatter.write_str(
                "direct native/reactive input is unsupported while required callbacks are configured",
            ),
            Self::UnknownSemanticSignal(signal) => write!(
                formatter,
                "semantic signal {}:{} is not present in this execution session",
                signal.slot(),
                signal.generation()
            ),
            Self::NativeInput(error) => error.fmt(formatter),
            Self::NativeEventOutOfOrder { previous, next } => write!(
                formatter,
                "native input event sequence must increase: previous {previous}, next {next}"
            ),
            Self::Reactive(error) => error.fmt(formatter),
            Self::Evaluation(error) => error.fmt(formatter),
            Self::PreparedCommit(error) => error.fmt(formatter),
            Self::TimelineOwnedSignal { signal } => write!(
                formatter,
                "semantic signal {}:{} is timeline-owned and cannot be set directly",
                signal.slot(),
                signal.generation()
            ),
            Self::NativeOwnedSignal { signal } => write!(
                formatter,
                "semantic signal {}:{} is native-owned and cannot be set directly",
                signal.slot(),
                signal.generation()
            ),
        }
    }
}

impl std::error::Error for ExecutionSessionInputError {}

impl From<NativeInputRuntimeError> for ExecutionSessionInputError {
    fn from(value: NativeInputRuntimeError) -> Self {
        Self::NativeInput(value)
    }
}

impl From<ReactiveError> for ExecutionSessionInputError {
    fn from(value: ReactiveError) -> Self {
        Self::Reactive(value)
    }
}

impl From<EvaluationError> for ExecutionSessionInputError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

impl From<super::ExecutionSessionPublicationError> for ExecutionSessionInputError {
    fn from(value: super::ExecutionSessionPublicationError) -> Self {
        Self::Publication(value)
    }
}

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
    selection_query: Option<super::PointerFillQuery>,
    selection_click: Option<super::PointerSelectionClick>,
    selection_changed: bool,
    playback_control: Option<noon_core::PlaybackControl>,
}

impl NativePointerInputPublication {
    /// Endpoint pick and locality evidence, present only when selection queried.
    pub const fn selection_query(self) -> Option<super::PointerFillQuery> {
        self.selection_query
    }

    /// Exactly-once successful click, only after the whole input commit succeeds.
    pub const fn selection_click(self) -> Option<super::PointerSelectionClick> {
        self.selection_click
    }

    /// A scene-authored viewer request from this committed click. Dispatch it
    /// after input returns through the existing admitted playback path, never
    /// while the input transaction or its displayed-frame checks are pending.
    /// Observing this receipt does not seek, play audio, or mutate authored state.
    pub const fn playback_control(self) -> Option<noon_core::PlaybackControl> {
        self.playback_control
    }

    /// An interactive presentation may use this to request overlay work. It does
    /// not manufacture ordinary scene/renderer dirtiness or advance authored time.
    pub const fn selection_changed(self) -> bool {
        self.selection_changed
    }

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

/// One unpublished native-input batch; its frame uses the existing sparse runtime
/// preparation, not a copied session. Used when a view change must validate the
/// effects of cancelling held buttons before either operation commits.
pub(super) struct PreparedInputPublication {
    pub(super) frame: noon_runtime::PreparedFrameEvaluation,
    effective: noon_runtime::PreparedEffectivePropertyBatch,
    timeline: Option<noon_runtime::SignalTimelinePreview>,
    pub(super) drag_cancellation:
        Option<super::translation_drag::PreparedTranslationDragCancellation>,
}

/// One fully validated native occurrence split at the existing runtime
/// publication boundary.  It owns no queue or snapshot: the prepared frame is
/// still the runtime's sparse evaluation and metadata is committed only after
/// that frame (or a shared authored publication) succeeds.
pub(super) struct PreparedNativePointerPublication {
    frame: Option<noon_runtime::PreparedFrameEvaluation>,
    effective: Option<noon_runtime::PreparedEffectivePropertyBatch>,
    timeline: Option<noon_runtime::SignalTimelinePreview>,
    input: NativePointerInput,
    previous: PublicationContext,
    selection: super::selection::PreparedPointerSelection,
    action: Option<super::interactions::PreparedClickAction>,
}

pub(super) struct PreparedNativePointerMetadata {
    input: NativePointerInput,
    previous: PublicationContext,
    selection: super::selection::PreparedPointerSelection,
    action: Option<super::interactions::PreparedClickAction>,
}

impl ExecutionSession {
    /// Whether a native reactive route or transient selection consumes this pointer.
    ///
    /// This bounded lookup inspects lowered routes, not objects or authored scene
    /// graphs. It is an adapter-interest query, not admission: explicit typed
    /// callers still deliver unbound records through the normal session contract.
    /// Interaction interest is owned here alongside lowered native routes.
    pub fn has_native_pointer_subscribers(&self) -> bool {
        !self.interaction_bindings.is_empty()
            || self.translation_drag.has_targets()
            || self.pointer_selection.enabled()
            || !self
                .reactive_projection
                .native_state_targets(&NativeStateSource::PointerPosition)
                .is_empty()
            || (0..=u8::MAX).any(|button| {
                !self
                    .reactive_projection
                    .native_state_targets(&NativeStateSource::PointerButton { button })
                    .is_empty()
                    || !self
                        .reactive_projection
                        .native_event_targets(&NativeEventSource::PointerDown { button })
                        .is_empty()
                    || !self
                        .reactive_projection
                        .native_event_targets(&NativeEventSource::PointerUp { button })
                        .is_empty()
            })
    }

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
        if self.translation_drag_active() {
            return Err(ExecutionSessionInputError::Interaction(
                "cancel the active translation drag before replacing its pointer mapping".into(),
            ));
        }
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
        self.pointer_selection.cancel_press();
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
    /// is never projected into a successful release. Opt-in transient fill
    /// selection prepares through the shared picker and commits only after all
    /// native effects succeed. Authored click actions use the same admitted occurrence.
    pub fn submit_native_pointer_input(
        &mut self,
        store: &mut SemanticStore,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, ExecutionSessionInputError> {
        let effective = self
            .runtime
            .prepare_effective_property_batch(&[])
            .expect("an empty effective-property batch is always valid");
        self.submit_native_pointer_input_with_effective(store, token, input, effective, false)
    }

    /// Commit one typed native occurrence and a preflighted scoped effective
    /// write in the same prepared-frame publication. Gesture policy lives in
    /// sibling session modules; this remains the sole pointer admission lane.
    pub(super) fn submit_native_pointer_input_with_effective(
        &mut self,
        store: &mut SemanticStore,
        token: &NativePointerInputToken,
        input: NativePointerInput,
        effective: noon_runtime::PreparedEffectivePropertyBatch,
        suppress_click_action: bool,
    ) -> Result<NativePointerInputPublication, ExecutionSessionInputError> {
        self.require_published_store(store)?;
        let prepared = self.prepare_native_pointer_input_with_effective(
            token,
            input,
            effective,
            suppress_click_action,
        )?;
        self.commit_prepared_native_pointer_input(store, prepared)
    }

    pub(super) fn prepare_native_pointer_input_with_effective(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
        effective: noon_runtime::PreparedEffectivePropertyBatch,
        suppress_click_action: bool,
    ) -> Result<PreparedNativePointerPublication, ExecutionSessionInputError> {
        let previous = self.preflight_native_pointer_input(token, input)?;
        let selection = self.prepare_pointer_selection(token, input)?;
        let action = (!suppress_click_action)
            .then(|| self.prepare_click_action(selection.click))
            .transpose()?
            .flatten();

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
        let (frame, effective, timeline) = if inputs.is_empty() && effective.is_empty() {
            (None, None, None)
        } else {
            let mut prepared = self.prepare_reactive_input_batch(inputs)?;
            prepared.effective = effective;
            self.runtime
                .preflight_prepared_frame_commit(&prepared.frame, &prepared.effective)
                .map_err(ExecutionSessionInputError::PreparedCommit)?;
            (
                Some(prepared.frame),
                Some(prepared.effective),
                prepared.timeline,
            )
        };
        Ok(PreparedNativePointerPublication {
            frame,
            effective,
            timeline,
            input,
            previous,
            selection,
            action,
        })
    }

    pub(super) fn commit_prepared_native_pointer_input(
        &mut self,
        store: &mut SemanticStore,
        prepared: PreparedNativePointerPublication,
    ) -> Result<NativePointerInputPublication, ExecutionSessionInputError> {
        let (frame, effective, timeline, mut metadata) = prepared.into_parts();
        match metadata.action.take() {
            Some(super::interactions::PreparedClickAction::GlowPulse { target, binding }) => {
                self.require_published_store(store)?;
                let object = self.execution_object_id(target).ok_or_else(|| {
                    ExecutionSessionInputError::Interaction(
                        "click target retired before publication".into(),
                    )
                })?;
                let attachment = store
                    .effect_by_name(target, "glow")
                    .map_err(|error| ExecutionSessionInputError::Interaction(error.to_string()))?;
                let glow = self
                    .runtime
                    .effective_object(object)
                    .and_then(|row| row.effects.as_ref())
                    .and_then(|stack| attachment.and_then(|id| stack.get(id)));
                if let Some(glow) = glow {
                    if store.effect_by_name(target, "glow").ok().flatten() != Some(glow.attachment)
                    {
                        return Err(ExecutionSessionInputError::Interaction(
                            "click glow does not match its semantic attachment".into(),
                        ));
                    }
                    metadata.action = self
                        .runtime
                        .prepare_click_glow_pulse(
                            object,
                            binding,
                            glow,
                            false,
                            self.publication_context(),
                        )
                        .map_err(ExecutionSessionInputError::Interaction)?
                        .map(super::interactions::PreparedClickAction::Animation);
                } else {
                    if attachment.is_some() {
                        return Err(ExecutionSessionInputError::Interaction(
                            "click glow does not match its effective attachment".into(),
                        ));
                    }
                    use super::publication::{
                        PreparedRuntimePublication, SemanticPublicationPurpose,
                    };
                    self.require_publication_ready(
                        SemanticPublicationPurpose::TransientGlowLifecycle,
                    )?;
                    let neutral =
                        noon_core::Glow::new(noon_core::EffectUpdate::default().intensity(0.0))
                            .expect("neutral glow is valid");
                    let mut transaction = noon_core::SemanticMutationTransaction::new();
                    let token = transaction.create_effect(target, "glow", neutral);
                    let prepared = transaction
                        .prepare(store)
                        .map_err(super::ExecutionSessionPublicationError::Semantic)?;
                    let attachment = prepared
                        .planned_node_id(token)
                        .expect("prepared glow has an identity");
                    let current = self.publication_context();
                    let proposed = PublicationContext::new(
                        prepared.proposed_scene_revision(),
                        current.execution_revision().checked_next().ok_or_else(|| {
                            ExecutionSessionInputError::Interaction(
                                "execution revision exhausted".into(),
                            )
                        })?,
                        current.frame_epoch().checked_next().ok_or_else(|| {
                            ExecutionSessionInputError::Interaction("frame epoch exhausted".into())
                        })?,
                    );
                    let animation = self
                        .runtime
                        .prepare_click_glow_pulse(
                            object,
                            binding,
                            noon_compile::CompiledEffect {
                                attachment,
                                definition: neutral.into(),
                            },
                            true,
                            proposed,
                        )
                        .map_err(ExecutionSessionInputError::Interaction)?
                        .ok_or_else(|| {
                            ExecutionSessionInputError::Interaction(
                                "click glow admission changed before publication".into(),
                            )
                        })?;
                    self.apply_prepared_semantic_transaction_with_execution_contract(
                        prepared,
                        Vec::new(),
                        PreparedRuntimePublication { effective, frame },
                        SemanticPublicationPurpose::TransientGlowLifecycle,
                        None,
                        None,
                    )?;
                    if let Some(pending) = self.pending_segment_completion.as_mut() {
                        pending.activation_scene_revision = store.scene_revision();
                    }
                    metadata.action = Some(super::interactions::PreparedClickAction::Animation(
                        animation,
                    ));
                    return self.commit_prepared_native_pointer_metadata(timeline, metadata);
                }
            }
            action => metadata.action = action,
        }
        if let (Some(frame), Some(effective)) = (frame, effective) {
            self.runtime
                .commit_prepared_frame(frame, effective)
                .map_err(ExecutionSessionInputError::PreparedCommit)?;
        }
        self.commit_prepared_native_pointer_metadata(timeline, metadata)
    }

    pub(super) fn commit_prepared_native_pointer_metadata(
        &mut self,
        timeline: Option<noon_runtime::SignalTimelinePreview>,
        metadata: PreparedNativePointerMetadata,
    ) -> Result<NativePointerInputPublication, ExecutionSessionInputError> {
        if let Some(preview) = timeline {
            self.signal_timeline.commit(preview);
        }
        let PreparedNativePointerMetadata {
            input,
            previous,
            selection,
            action,
        } = metadata;
        self.last_native_event_sequence = Some(input.sequence());
        self.pointer_selection = selection.state;
        self.pointer_selection.hover.accept(input);
        let playback_control = match action {
            Some(super::interactions::PreparedClickAction::Animation(animation)) => {
                self.runtime
                    .start_transient_animation(animation)
                    .expect("admitted native input cannot change authored revision or time");
                None
            }
            Some(super::interactions::PreparedClickAction::Playback(control)) => Some(control),
            Some(super::interactions::PreparedClickAction::GlowPulse { .. }) => {
                unreachable!("glow admission is prepared before input commit")
            }
            None => None,
        };
        Ok(NativePointerInputPublication {
            input,
            previous,
            current: self.publication_context(),
            selection_query: selection.query,
            selection_click: selection.click,
            selection_changed: selection.changed,
            playback_control,
        })
    }

    /// Shared read-only admission check for input consumers. Picking does not
    /// acknowledge a sequence, project signals, or relax callback/context gates.
    pub(super) fn preflight_native_pointer_input(
        &self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<PublicationContext, ExecutionSessionInputError> {
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

        Ok(previous)
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
                &update.source,
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
                &occurrence.source,
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

    fn require_native_event_sequence(&self, next: u64) -> Result<(), ExecutionSessionInputError> {
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

    pub(super) fn apply_reactive_input_batch(
        &mut self,
        inputs: Vec<(noon_core::SignalId, ReactiveValue)>,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        let prepared = self.prepare_reactive_input_batch(inputs)?;
        self.commit_reactive_input_batch(prepared)
    }

    fn prepare_reactive_input_batch(
        &mut self,
        inputs: Vec<(noon_core::SignalId, ReactiveValue)>,
    ) -> Result<PreparedInputPublication, ExecutionSessionInputError> {
        let current = self.runtime.frame().time;
        let signal_timeline = (!self.signal_timeline.is_empty()
            && !self.signal_timeline.is_coherent_at(current, current))
        .then(|| self.signal_timeline.preview(current, current));
        let mut combined = signal_timeline
            .as_ref()
            .map_or_else(Vec::new, |preview| preview.inputs().to_vec());
        combined.extend(inputs);
        let frame = self
            .runtime
            .prepare_advance_to_with_reactive_inputs(current, &combined)?;
        let effective = self
            .runtime
            .prepare_effective_property_batch(&[])
            .expect("an empty effective-property batch is always valid");
        Ok(PreparedInputPublication {
            frame,
            effective,
            timeline: signal_timeline,
            drag_cancellation: None,
        })
    }

    pub(super) fn commit_reactive_input_batch(
        &mut self,
        prepared: PreparedInputPublication,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        let PreparedInputPublication {
            frame,
            effective,
            timeline,
            drag_cancellation,
        } = prepared;
        if let Some(cancellation) = drag_cancellation {
            self.commit_translation_drag_cancellation(cancellation, frame, effective)?;
        } else {
            self.runtime
                .commit_prepared_frame(frame, effective)
                .map_err(ExecutionSessionInputError::PreparedCommit)?;
        }
        if let Some(preview) = timeline {
            self.signal_timeline.commit(preview);
        }
        Ok(self.runtime.frame())
    }

    pub(super) fn prepare_pointer_view_cancellation(
        &mut self,
    ) -> Result<Option<PreparedInputPublication>, ExecutionSessionInputError> {
        self.ensure_direct_input_ingress_available()?;
        let mut inputs = self.pointer_button_reset_inputs();
        // No input publication is needed for already released buttons. In
        // particular inspection alone must not invalidate finite replay history.
        inputs.retain(|(signal, value)| self.runtime.reactive_value(*signal) != Some(value));
        let drag_cancellation = self.prepare_translation_drag_cancellation()?;
        if inputs.is_empty() && drag_cancellation.is_none() {
            Ok(None)
        } else {
            let mut prepared = self.prepare_reactive_input_batch(inputs)?;
            if let Some(cancellation) = drag_cancellation {
                prepared.effective = self
                    .runtime
                    .prepare_effective_property_batch(&[EffectivePropertyWrite::Translation {
                        object: cancellation.object,
                        translation: cancellation.base,
                    }])
                    .map_err(|_| {
                        ExecutionSessionInputError::Interaction(
                            "captured translation drag conflicts with cancellation".into(),
                        )
                    })?;
                prepared.drag_cancellation = Some(cancellation);
            }
            Ok(Some(prepared))
        }
    }

    pub(super) fn retire_pointer_view_binding(&mut self) {
        self.pointer_input.binding = None;
        self.pointer_selection.cancel_press();
    }
}

impl PreparedNativePointerPublication {
    pub(super) fn into_parts(
        self,
    ) -> (
        Option<noon_runtime::PreparedFrameEvaluation>,
        Option<noon_runtime::PreparedEffectivePropertyBatch>,
        Option<noon_runtime::SignalTimelinePreview>,
        PreparedNativePointerMetadata,
    ) {
        (
            self.frame,
            self.effective,
            self.timeline,
            PreparedNativePointerMetadata {
                input: self.input,
                previous: self.previous,
                selection: self.selection,
                action: self.action,
            },
        )
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
