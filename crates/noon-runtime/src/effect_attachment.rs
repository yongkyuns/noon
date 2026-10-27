//! Attachment lifetime on the existing execution publication and scheduler.
use std::sync::Arc;

use noon_compile::{CompilePatchError, CompiledChannelKey, CompiledEffect, ExecutionPatch};
use noon_core::ObjectId;

use crate::{EvaluationStats, RuntimePatchStats, SceneInstance};

impl SceneInstance {
    pub(crate) fn apply_effect_attachment_patch(
        &mut self,
        patch: &ExecutionPatch,
        object: ObjectId,
        next: Option<Arc<CompiledEffect>>,
    ) -> Result<(), CompilePatchError> {
        let index = self
            .compiled
            .object_index(object)
            .ok_or(CompilePatchError::UnknownObject(object))?;
        let previous = self.compiled.objects()[index as usize]
            .effect
            .as_ref()
            .map(|effect| effect.attachment);
        // Compiler validation completes before either authority is changed.
        let compiled = self.compiled.apply_execution_patch_with_stats(patch)?;
        let mut stats = RuntimePatchStats {
            track_locators_removed: compiled.track_locators_removed,
            ..RuntimePatchStats::default()
        };
        for property in CompiledEffect::PARAMETER_PROPERTIES {
            let Some(attachment) = previous else { break };
            let channel = CompiledChannelKey::effect(index, property, attachment);
            if self.groups.remove(&channel).is_some() {
                let retired = self.timeline_scheduler.relower_channel(channel, &[]);
                stats.channels_relowered += retired.groups_relowered;
                stats.scheduler_events_removed += retired.events_removed;
                stats.scheduler_events_inserted += retired.events_inserted;
            }
        }
        let index = index as usize;
        self.frame.objects[index].effect = next;
        // Halo coverage is renderer-derived. Semantic geometry, spatial queries,
        // object motion, painter order and all unrelated rows stay untouched.
        self.mark_nonspatial_changed(index);
        self.last_stats = EvaluationStats::default();
        self.last_patch_stats = stats;
        Ok(())
    }
}
