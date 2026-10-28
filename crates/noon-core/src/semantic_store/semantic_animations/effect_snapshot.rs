//! Transform target correspondence captured once at declaration preparation.
//! Generational IDs here are weak activation expectations, not owning graph
//! edges: removing/replacing an attachment invalidates a declaration's capture
//! rather than causing it to drive a new same-name attachment.
use crate::{EffectDefinition, SemanticEffectName, SemanticNodeId, SemanticStore};

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticAnimationEffectSnapshot {
    pub attachment: SemanticNodeId,
    pub name: SemanticEffectName,
    pub definition: EffectDefinition,
    pub copy_provenance: Option<crate::SemanticEffectCopyProvenance>,
}

/// Frozen attachment generations for the two ordinary TransformTo endpoints.
/// Values remain in the authoritative endpoint objects; activation captures the
/// effective source and reads the target using the ordinary target-state rules.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticTransformEffectSnapshot {
    pub source: Box<[SemanticNodeId]>,
    pub target: Box<[SemanticNodeId]>,
}

impl SemanticTransformEffectSnapshot {
    pub(crate) fn new(
        source: Vec<SemanticAnimationEffectSnapshot>,
        target: Vec<SemanticAnimationEffectSnapshot>,
    ) -> Option<Box<Self>> {
        // Visibility is a lifecycle role owned by Dissolve, rather than an
        // authored appearance target. Ordinary motion/Transform must neither
        // capture its temporary generation nor copy its progress back later.
        let source: Vec<_> = source
            .into_iter()
            .filter(|effect| !effect.name.is_visibility())
            .collect();
        let target: Vec<_> = target
            .into_iter()
            .filter(|effect| !effect.name.is_visibility())
            .collect();
        if source.is_empty() && target.is_empty() {
            return None;
        }
        Some(Box::new(Self {
            source: source.into_iter().map(|effect| effect.attachment).collect(),
            target: target.into_iter().map(|effect| effect.attachment).collect(),
        }))
    }
}

impl SemanticStore {
    /// Owner-local immutable snapshot; copies values, not attachment ownership.
    pub fn animation_effect_snapshot(
        &self,
        owner: SemanticNodeId,
    ) -> Result<Vec<SemanticAnimationEffectSnapshot>, crate::SemanticStoreError> {
        let node = self
            .node(owner)
            .ok_or(crate::SemanticStoreError::UnknownNode(owner))?;
        node.effect_ids()
            .iter()
            .map(|&id| {
                let effect = self.semantic_effect_state(id)?;
                Ok(SemanticAnimationEffectSnapshot {
                    attachment: id,
                    name: effect.attachment_name().clone(),
                    definition: effect.definition(),
                    copy_provenance: effect.copy_provenance(),
                })
            })
            .collect()
    }
}
