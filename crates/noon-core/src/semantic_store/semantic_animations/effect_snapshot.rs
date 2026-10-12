//! Transform target correspondence captured once at declaration preparation.
//! Generational IDs here are weak activation expectations, not owning graph
//! edges: removing/replacing an attachment invalidates a declaration's capture
//! rather than causing it to drive a new same-name attachment.
use crate::{EffectDefinition, SemanticNodeId, SemanticStore};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticAnimationEffectSnapshot {
    pub attachment: SemanticNodeId,
    pub name: Arc<str>,
    pub definition: EffectDefinition,
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
                    name: Arc::clone(effect.shared_name()),
                    definition: effect.definition(),
                })
            })
            .collect()
    }
}
