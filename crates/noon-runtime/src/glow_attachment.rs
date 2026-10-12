//! Attachment lifetime on the existing execution publication and scheduler.
use std::sync::Arc;

use noon_compile::{CompilePatchError, CompiledChannelKey, CompiledGlow, ExecutionPatch};
use noon_core::ObjectId;

use crate::{EvaluationStats, RuntimePatchStats, SceneInstance};

impl SceneInstance {
    pub(crate) fn apply_glow_attachment_patch(
        &mut self,
        patch: &ExecutionPatch,
        object: ObjectId,
        next: Option<Arc<CompiledGlow>>,
    ) -> Result<(), CompilePatchError> {
        let index = self
            .compiled
            .object_index(object)
            .ok_or(CompilePatchError::UnknownObject(object))?;
        // Compiler validation completes before either authority is changed.
        let compiled = self.compiled.apply_execution_patch_with_stats(patch)?;
        let mut stats = RuntimePatchStats {
            track_locators_removed: compiled.track_locators_removed,
            ..RuntimePatchStats::default()
        };
        for property in CompiledGlow::PARAMETER_PROPERTIES {
            let channel = CompiledChannelKey::new(index, property);
            if self.groups.remove(&channel).is_some() {
                let retired = self.timeline_scheduler.relower_channel(channel, &[]);
                stats.channels_relowered += retired.groups_relowered;
                stats.scheduler_events_removed += retired.events_removed;
                stats.scheduler_events_inserted += retired.events_inserted;
            }
        }
        let index = index as usize;
        self.frame.objects[index].glow = next;
        // Halo coverage is renderer-derived. Semantic geometry, spatial queries,
        // object motion, painter order and all unrelated rows stay untouched.
        self.changes.insert(index);
        self.last_stats = EvaluationStats::default();
        self.last_patch_stats = stats;
        Ok(())
    }
}
