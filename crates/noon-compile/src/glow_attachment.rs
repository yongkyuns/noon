//! Sparse execution-plan projection of a semantic leaf attachment change.
use std::sync::Arc;

use noon_core::{ObjectId, SemanticNodeId};

use crate::{
    CompilePatchError, CompiledChannelKey, CompiledGlow, CompiledPatchStats, CompiledScene,
};

pub(crate) fn validate_change(
    object: ObjectId,
    previous: Option<&CompiledGlow>,
    expected: Option<SemanticNodeId>,
    next: Option<&CompiledGlow>,
) -> Result<(), CompilePatchError> {
    if previous.map(|glow| glow.attachment) != expected
        || next.is_some_and(|glow| Some(glow.attachment) == expected)
    {
        return Err(CompilePatchError::InvalidGlowAttachmentChange { object });
    }
    Ok(())
}

impl CompiledScene {
    pub(crate) fn apply_glow_attachment(
        &mut self,
        object: ObjectId,
        expected: Option<SemanticNodeId>,
        next: Option<Arc<CompiledGlow>>,
        stats: &mut CompiledPatchStats,
    ) -> Result<(), CompilePatchError> {
        let index = self
            .object_index(object)
            .ok_or(CompilePatchError::UnknownObject(object))?;
        validate_change(
            object,
            self.objects[index as usize].glow.as_deref(),
            expected,
            next.as_deref(),
        )?;
        for property in CompiledGlow::PARAMETER_PROPERTIES {
            let channel = CompiledChannelKey::new(index, property);
            if let Some(retired) = self.tracks.remove(&channel) {
                self.track_count -= retired.len();
                for track in retired {
                    self.track_locators.remove(&track.id);
                    stats.track_locators_removed += 1;
                }
            }
        }
        let row = &mut self.objects[index as usize];
        row.glow = next;
        // All three glow channels are now empty. Do not traverse unrelated
        // channels, recompute motion, or move any retained object/track slot.
        row.dynamic.glow = false;
        Ok(())
    }
}
