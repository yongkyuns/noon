//! Explicit execution maintenance barriers.

use super::ExecutionSession;
use noon_runtime::{RuntimeCompactionError, RuntimeCompactionStats};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionSessionMaintenanceError {
    RequiredCallbackPending,
    SegmentCompletionPending,
    CallbacksConfigured,
    DerivedDisplayActive,
    InteractionActive,
    Runtime(RuntimeCompactionError),
}

impl std::fmt::Display for ExecutionSessionMaintenanceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequiredCallbackPending => {
                formatter.write_str("cannot compact while a required callback is pending")
            }
            Self::SegmentCompletionPending => {
                formatter.write_str("cannot compact while an animation completion is pending")
            }
            Self::CallbacksConfigured => {
                formatter.write_str("cannot compact while host callback scheduling is configured")
            }
            Self::DerivedDisplayActive => {
                formatter.write_str("cannot compact while a derived display plan is active")
            }
            Self::InteractionActive => {
                formatter.write_str("cannot compact while an interaction effect is active")
            }
            Self::Runtime(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecutionSessionMaintenanceError {}

impl ExecutionSession {
    /// Reclaim tombstoned rows and eligible static resources at a maintenance barrier.
    ///
    /// The session keeps its semantic store and runtime identities, current
    /// effective frame and authored time. Compaction advances only the execution
    /// and frame revisions, which invalidates any receipt or prepared work pinned
    /// to the previous row layout. A replay scope is rejected only when it retains
    /// tombstoned rows; an active scope with no reclaimable rows remains a no-op.
    pub fn reclaim_retired_object_slots(
        &mut self,
    ) -> Result<RuntimeCompactionStats, ExecutionSessionMaintenanceError> {
        if self.pending_callback.is_some() {
            return Err(ExecutionSessionMaintenanceError::RequiredCallbackPending);
        }
        if self.pending_segment_completion.is_some() {
            return Err(ExecutionSessionMaintenanceError::SegmentCompletionPending);
        }
        // A configured callback plan is keyed to semantic targets. Resource-only
        // pruning cannot relocate its execution rows, but row compaction still
        // needs a separate schedule-remapping contract.
        if !self.callback_schedule.is_empty() && self.runtime.has_retired_object_slots() {
            return Err(ExecutionSessionMaintenanceError::CallbacksConfigured);
        }
        if self.derived_display_plan.is_some() {
            return Err(ExecutionSessionMaintenanceError::DerivedDisplayActive);
        }
        if self.interactions_active() || self.translation_drag_active() {
            return Err(ExecutionSessionMaintenanceError::InteractionActive);
        }
        let stats = self
            .runtime
            .reclaim_retired_object_slots()
            .map_err(ExecutionSessionMaintenanceError::Runtime)?;
        if stats.compiled.object_slots_reclaimed != 0
            || stats.compiled.resource_entries_reclaimed != 0
        {
            self.last_callback_receipt = None;
            let time = self.frame().time;
            let publication = self.publication_context();
            self.callback_schedule
                .carry_completed_publication(time, publication);
        }
        if stats.compiled.object_slots_reclaimed != 0 {
            self.sync_spatial_index();
            self.reconcile_pointer_selection();
        }
        Ok(stats)
    }
}
