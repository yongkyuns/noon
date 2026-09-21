//! Finite, opt-in historical execution. Semantic membership stays at the authored
//! frontier; the existing execution projection selects a time-qualified revision.
use super::ExecutionSession;
use noon_core::ObjectId;
use noon_runtime::{
    apply_execution_slot_membership_changes, ReplayError, ReplayLimits, ReplayStats,
};

impl ExecutionSession {
    /// Begin finite history retention before driving a deterministic source program.
    /// Ordinary long-lived editable sessions do not retain history by default.
    pub fn begin_replay_retention(&mut self, limits: ReplayLimits) -> Result<(), ReplayError> {
        if self.pending_segment_completion.is_some() || self.pending_callback.is_some() {
            return Err(ReplayError::Incomplete);
        }
        self.runtime.begin_replay_retention(limits)?;
        if !self.callback_schedule.is_empty()
            || !self.signal_timeline.is_empty()
            || self.derived_display_plan.is_some()
        {
            self.runtime.invalidate_replay_domain();
        }
        self.replay_pinned_exits = Some(Default::default());
        Ok(())
    }
    /// Admit backward playback only after all retained revisions and completion
    /// barriers are coherent. Failure never changes the current authored scene.
    pub fn seal_replay(&mut self) -> Result<(), ReplayError> {
        if self.pending_segment_completion.is_some() || self.pending_callback.is_some() {
            return Err(ReplayError::Incomplete);
        }
        if !self.callback_schedule.is_empty()
            || !self.signal_timeline.is_empty()
            || self.derived_display_plan.is_some()
        {
            return Err(ReplayError::UnsupportedDomain);
        }
        self.runtime.seal_replay()
    }
    pub fn replay_scope_active(&self) -> bool {
        self.runtime.replay_scope_active()
    }
    pub fn replay_is_sealed(&self) -> bool {
        self.runtime.replay_is_sealed()
    }
    pub fn replay_stats(&self) -> ReplayStats {
        self.runtime.replay_stats()
    }
    /// Explicitly return to the authored frontier and release history and pinned
    /// execution identities. The historical projection cannot be edited in place.
    pub fn discard_replay_retention(&mut self) {
        self.runtime.discard_replay_retention();
        self.pointer_selection.reset();
        self.sync_spatial_index();
        self.release_replay_slots();
    }
    fn release_replay_slots(&mut self) {
        if let Some(pinned) = self.replay_pinned_exits.take() {
            for object in pinned {
                self.slots
                    .remove_object(object)
                    .expect("replay pins own existing slots");
            }
        }
    }
    /// Membership is published once through the normal slot table. During a finite
    /// recording, removed identities remain pinned for earlier renderer frames;
    /// re-entry uses the same slot instead of an unrelated recycled generation.
    pub(super) fn publish_replay_membership(&mut self, exited: &[ObjectId], entered: &[ObjectId]) {
        if let Some(pinned) = self.replay_pinned_exits.as_mut() {
            pinned.extend(exited.iter().copied());
            let additions: Vec<_> = entered
                .iter()
                .copied()
                .filter(|id| !pinned.remove(id))
                .collect();
            apply_execution_slot_membership_changes(&mut self.slots, &[], &additions)
                .expect("exact membership is a subset of the preflighted structural shape");
            if !self.runtime.replay_retention_valid() {
                // Capacity/unsupported-domain failure ends retention immediately,
                // rather than leaking slots until an arbitrarily late source end.
                self.release_replay_slots();
            }
        } else {
            apply_execution_slot_membership_changes(&mut self.slots, exited, entered)
                .expect("exact membership is a subset of the preflighted structural shape");
        }
    }
}
