//! Sparse staged effect reads at the existing semantic transaction boundary.
use super::*;
use crate::{SemanticAnimationEffectSnapshot, SemanticTransformEffectSnapshot};

pub(super) fn effect_snapshot(
    store: &SemanticStore,
    preflight: &SemanticTransactionPreflight,
    planned_nodes: &HashMap<SemanticLocalNodeToken, SemanticNodeId>,
    owner: SemanticTransactionNodeRef,
) -> Vec<SemanticAnimationEffectSnapshot> {
    let mut result = match owner {
        SemanticTransactionNodeRef::Existing(id) => store
            .animation_effect_snapshot(id)
            .expect("preflighted effect owner")
            .into_iter()
            .filter_map(|mut effect| {
                if preflight.removed_existing.contains(&effect.attachment) {
                    return None;
                }
                if let Some(&definition) = preflight.staged_effects.get(&effect.attachment) {
                    effect.definition = definition;
                }
                Some(effect)
            })
            .collect(),
        SemanticTransactionNodeRef::Pending(_) => Vec::new(),
    };
    // Read only this owner's sparse staged list, preserving insertion order.
    for token in preflight
        .pending_effect_order
        .get(&owner)
        .into_iter()
        .flatten()
    {
        let SemanticNodeCreation::Effect {
            name, definition, ..
        } = &preflight.pending_creations[token]
        else {
            unreachable!("indexed effect creation");
        };
        result.push(SemanticAnimationEffectSnapshot {
            attachment: planned_nodes[token],
            name: name.clone(),
            definition: *definition,
        });
    }
    result
}

impl PreparedSemanticMutationTransaction<'_> {
    /// New target copies may retain inert attachments without enrolling them in
    /// execution. Existing owners and candidate membership edges do not qualify.
    pub fn is_detached_effect_target(&self, owner: SemanticTransactionNodeRef) -> bool {
        let SemanticTransactionNodeRef::Pending(token) = owner else {
            return false;
        };
        self.object_state(owner).is_ok() && self.preflight.family_edges.pending_is_detached(token)
    }

    /// Allocator-proven identities are preparation-local until commit; no
    /// mutable store/attachment or resource allocation is exposed to a caller.
    pub fn animation_effect_snapshot(
        &self,
        owner: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<Vec<SemanticAnimationEffectSnapshot>, SemanticTransactionReadError> {
        let owner = owner.into();
        self.object_state(owner)?;
        Ok(effect_snapshot(
            self.store,
            &self.preflight,
            &self.planned_nodes,
            owner,
        ))
    }

    /// Exact inert correspondence for an existing or candidate TransformTo.
    pub fn transform_effect_snapshot(
        &self,
        animation: impl Into<SemanticTransactionNodeRef>,
    ) -> Result<Option<&SemanticTransformEffectSnapshot>, SemanticTransactionReadError> {
        let animation = animation.into();
        match animation {
            SemanticTransactionNodeRef::Pending(token) => {
                self.pending_animation(token)?;
            }
            SemanticTransactionNodeRef::Existing(id) => {
                if self.preflight.removed_existing.contains(&id) {
                    return Err(SemanticTransactionReadError::RemovedExistingNode(id));
                }
                let node = self
                    .store
                    .node(id)
                    .ok_or(SemanticTransactionReadError::UnknownExistingNode(id))?;
                if !matches!(node.kind(), SemanticNodeKind::Animation(_)) {
                    return Err(SemanticTransactionReadError::NotAnimation(animation));
                }
            }
        }
        Ok(match animation {
            SemanticTransactionNodeRef::Existing(id) => self
                .store
                .semantic_animation_state(id)
                .expect("validated animation read")
                .transform_effect_snapshot(),
            SemanticTransactionNodeRef::Pending(token) => self
                .preflight
                .animation_effect_snapshots
                .get(&token)
                .map(Box::as_ref),
        })
    }
}
