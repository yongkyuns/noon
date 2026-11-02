//! Read-only historical facade over the ordinary shared execution session.
//!
//! The session owns the sole historical runtime, picking index and interaction
//! state. The receiver owns only ordered-delivery/readiness metadata. No mutable
//! session/runtime accessor or authoring/causal input entry point is exposed.
use crate::ExecutionSession;
use noon_core::{ObjectId, PublicationContext, SemanticClickIndicate};
use noon_runtime::{
    apply_execution_slot_membership_changes, preflight_execution_slot_membership_changes,
    FrameState, PrefixBatch, PrefixBatchStats, PrefixError, PrefixReceiver,
    PreparedTransientAnimation, RendererPublication, ReplayStats, RuntimeIdentity,
};

mod input;
mod presentation;

#[derive(Debug)]
pub struct PrefixReader {
    session: ExecutionSession,
    receiver: PrefixReceiver,
}

impl PrefixReader {
    pub(super) fn matches_publisher(&self, publisher: &noon_runtime::PrefixPublisher) -> bool {
        self.receiver.matches_publisher(publisher)
    }
    pub(super) fn new(session: ExecutionSession, receiver: PrefixReceiver) -> Self {
        Self { session, receiver }
    }

    pub(crate) fn session(&self) -> &ExecutionSession {
        &self.session
    }

    pub(crate) fn set_native_state_input(
        &mut self,
        source: noon_core::NativeStateSource,
        value: noon_core::NativeInputValue,
    ) -> Result<(), crate::ExecutionSessionInputError> {
        // Source admission excludes semantic input routes. Preserve ordinary
        // validation and ambient platform occurrence ordering in this session.
        self.session
            .set_native_state_input(source, value)
            .map(|_| ())
    }

    pub(crate) fn emit_native_event(
        &mut self,
        event: noon_core::NativeEventOccurrence,
    ) -> Result<(), crate::ExecutionSessionInputError> {
        self.session.emit_native_event(event).map(|_| ())
    }

    pub fn contains_time(&self, time: f64) -> bool {
        self.receiver.contains_time(time)
    }

    pub fn runtime_identity(&self) -> RuntimeIdentity {
        self.session.runtime_identity()
    }
    pub fn publication_context(&self) -> PublicationContext {
        self.session.publication_context()
    }
    pub fn frontier(&self) -> f64 {
        self.receiver.frontier()
    }
    pub fn end_time(&self) -> Option<f64> {
        self.receiver.end_time()
    }
    pub fn frame(&self) -> &FrameState {
        self.session.frame()
    }
    pub fn replay_stats(&self) -> ReplayStats {
        self.session.replay_stats()
    }
    /// Logical immutable-resource payloads pinned by this history scope.
    /// This excludes revision/storage capacity, allocator overhead and GPU state.
    pub fn retained_resource_bytes(&self) -> usize {
        self.receiver.retained_resource_bytes()
    }
    pub fn resource_budget_bytes(&self) -> usize {
        self.receiver.resource_budget_bytes()
    }

    /// Explicit seek cancels transient gestures/effects through the same session
    /// policy as ordinary replay. Invalid/unavailable times change nothing.
    pub fn seek(&mut self, time: f64) -> Result<&FrameState, PrefixError> {
        self.receiver
            .prepare_seek(&mut self.session.runtime, time)?;
        self.session.seek(time)?;
        Ok(self.session.frame())
    }

    /// Forward playback never silently rewinds. Readiness is admitted before the
    /// shared session advances; disjoint transient effects keep their own clock.
    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, PrefixError> {
        self.receiver
            .prepare_advance(&mut self.session.runtime, time)?;
        self.session.advance_to(time)?;
        Ok(self.session.frame())
    }

    /// Explicit allocation barrier for both runtime rows and their derived input
    /// slots. Partial allocation failure changes capacity only, not live state.
    pub fn reserve_object_capacity(&mut self, total: usize) -> Result<usize, PrefixError> {
        // Validate the runtime's bounded capacity before allocating the slot map.
        let capacity = self
            .receiver
            .reserve_object_capacity(&mut self.session.runtime, total)?;
        self.session
            .slots
            .reserve_object_capacity(capacity)
            .map_err(PrefixError::Slots)?;
        Ok(capacity)
    }

    /// Explicit storage-growth barrier for a borrowed, source-bound batch.
    /// Reserves only required object slots and immutable glyph-plan tables.
    /// It never consumes output, evaluates a frame or invokes source code.
    pub fn reserve_append_capacity(&mut self, batch: &PrefixBatch) -> Result<(), PrefixError> {
        self.receiver
            .reserve_append_capacity(&mut self.session.runtime, batch)?;
        self.session
            .slots
            .reserve_object_capacity(batch.required_object_capacity())
            .map_err(PrefixError::Slots)?;
        Ok(())
    }

    /// Observe delivery admission without consuming the batch or publishing its
    /// future targets. Slot work is proportional only to newly delivered rows.
    pub fn preflight_append(&self, batch: &PrefixBatch) -> Result<PrefixBatchStats, PrefixError> {
        let stats = self
            .receiver
            .preflight_append(&self.session.runtime, batch)?;
        if batch.required_object_capacity() > self.session.slots.reserved_object_capacity() {
            return Err(PrefixError::CapacityRequired {
                object_slots: batch.required_object_capacity(),
            });
        }
        let new_objects: Vec<_> = batch.new_object_ids().collect();
        preflight_execution_slot_membership_changes(&self.session.slots, &[], &new_objects)
            .map_err(PrefixError::Slots)?;
        Ok(stats)
    }

    /// Adopt history and pin new derived identities, never new live input targets.
    /// The displayed publication, pending gesture, selection and effect survive.
    /// Explicit seeking/advancement alone selects a newer historical revision.
    pub fn append(&mut self, batch: PrefixBatch) -> Result<PrefixBatchStats, PrefixError> {
        self.preflight_append(&batch)?;
        let new_objects: Vec<_> = batch.new_object_ids().collect();
        let stats = self.receiver.append(&mut self.session.runtime, batch)?;
        apply_execution_slot_membership_changes(&mut self.session.slots, &[], &new_objects)
            .expect("same-owner prefix identity batch was preflighted before adoption");
        Ok(stats)
    }

    pub fn prepare_click_indicate(
        &self,
        object: ObjectId,
        binding: SemanticClickIndicate,
    ) -> Result<Option<PreparedTransientAnimation>, PrefixError> {
        self.receiver
            .prepare_click_indicate(&self.session.runtime, object, binding)
    }
    pub fn prepare_bound_click_indicate(
        &self,
        object: ObjectId,
    ) -> Result<Option<PreparedTransientAnimation>, PrefixError> {
        self.receiver
            .prepare_bound_click_indicate(&self.session.runtime, object)
    }
    pub fn start_transient_animation(
        &mut self,
        prepared: PreparedTransientAnimation,
    ) -> Result<(), PrefixError> {
        self.session
            .runtime
            .start_transient_animation(prepared)
            .map_err(PrefixError::Interaction)
    }
    pub fn advance_interactions(&mut self, wall_time: f64) -> Result<(), PrefixError> {
        self.session
            .runtime
            .advance_interactions(wall_time)
            .map_err(PrefixError::Interaction)
    }
    pub fn interactions_active(&self) -> bool {
        self.session.interactions_active()
    }
    /// Borrow the selected historical frame for fallible device preparation.
    /// Delivery, dirty changes and the displayed history cursor remain pending.
    pub fn resource_publication(&self) -> RendererPublication<'_> {
        self.session.resource_publication()
    }

    pub fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        self.session.take_renderer_publication()
    }
}
