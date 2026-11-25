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
        // Ordinary motion must not capture visibility owned by a timed gesture.
        // An explicitly edited persistent RadialMask target is an ordinary
        // appearance animation and retains its exact source/copy correspondence.
        let radial_visibility = source
            .iter()
            .find(|effect| effect.name.is_visibility())
            .filter(|effect| effect.definition.as_radial_mask().is_some())
            .and_then(|source| {
                target
                    .iter()
                    .find(|effect| effect.name.is_visibility())
                    .filter(|effect| effect.definition.as_radial_mask().is_some())
                    .filter(|target| match target.copy_provenance.as_ref() {
                        Some(copy) if copy.source_attachment() == source.attachment => {
                            copy.has_parameter_writes()
                        }
                        _ => target.definition != source.definition,
                    })
                    .map(|target| (source.attachment, target.attachment))
            });
        let source: Vec<_> = source
            .into_iter()
            .filter(|effect| {
                !effect.name.is_visibility()
                    || radial_visibility.is_some_and(|(source, _)| source == effect.attachment)
            })
            .collect();
        let target: Vec<_> = target
            .into_iter()
            .filter(|effect| {
                !effect.name.is_visibility()
                    || radial_visibility.is_some_and(|(_, target)| target == effect.attachment)
            })
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
