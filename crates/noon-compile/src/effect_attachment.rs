//! Sparse execution-plan projection of a semantic leaf attachment change.
use std::sync::Arc;

use noon_core::{ObjectId, SemanticNodeId};

use crate::{
    CompilePatchError, CompiledChannelKey, CompiledEffect, CompiledPatchStats, CompiledScene,
};

pub(crate) fn validate_change(
    object: ObjectId,
    previous: Option<&CompiledEffect>,
    expected: Option<SemanticNodeId>,
    next: Option<&CompiledEffect>,
) -> Result<(), CompilePatchError> {
    if previous.map(|effect| effect.attachment) != expected
        || next.is_some_and(|effect| Some(effect.attachment) == expected)
    {
        return Err(CompilePatchError::InvalidEffectAttachmentChange { object });
    }
    Ok(())
}

impl CompiledScene {
    pub(crate) fn apply_effect_attachment(
        &mut self,
        object: ObjectId,
        expected: Option<SemanticNodeId>,
        next: Option<Arc<CompiledEffect>>,
        stats: &mut CompiledPatchStats,
    ) -> Result<(), CompilePatchError> {
        let index = self
            .object_index(object)
            .ok_or(CompilePatchError::UnknownObject(object))?;
        validate_change(
            object,
            self.objects[index as usize].effect.as_deref(),
            expected,
            next.as_deref(),
        )?;
        for property in CompiledEffect::PARAMETER_PROPERTIES {
            let Some(attachment) = expected else { break };
            let channel = CompiledChannelKey::effect(index, property, attachment);
            if let Some(retired) = self.tracks.remove(&channel) {
                self.track_count -= retired.len();
                for track in retired {
                    self.track_locators.remove(&track.id);
                    stats.track_locators_removed += 1;
                }
            }
        }
        let row = &mut self.objects[index as usize];
        row.effect = next;
        // The retired attachment's channels are empty. Do not traverse unrelated
        // channels, recompute motion, or move any retained object/track slot.
        row.dynamic.effect = false;
        Ok(())
    }
}
