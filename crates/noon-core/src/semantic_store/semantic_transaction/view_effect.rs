//! View-input ownership checked against the final local transaction overlay.
use super::*;

pub(super) fn validate(
    transaction: &SemanticMutationTransaction,
    preflight: &SemanticTransactionPreflight,
    store: &SemanticStore,
) -> Result<(), SemanticMutationTransactionError> {
    let mut seen = HashSet::new();
    let mut owners = Vec::new();
    for &owner in &preflight.staged_object_order {
        if seen.insert(owner) {
            owners.push(owner);
        }
    }
    for mutation in &transaction.mutations {
        let owner = match mutation {
            SemanticMutation::AddNode {
                creation: SemanticNodeCreation::Effect { owner, .. },
                ..
            } => Some(*owner),
            SemanticMutation::UpdateEffect { effect, .. } => store
                .semantic_effect_state(*effect)
                .ok()
                .map(|effect| effect.owner().into()),
            _ => None,
        };
        if let Some(owner) = owner {
            if seen.insert(owner) {
                owners.push(owner);
            }
        }
    }
    for owner in owners {
        let removed = match owner {
            SemanticTransactionNodeRef::Existing(id) => preflight.removed_existing.contains(&id),
            SemanticTransactionNodeRef::Pending(token) => {
                preflight.removed_pending.contains(&token)
            }
        };
        if removed {
            continue;
        }
        let existing = owner
            .existing()
            .and_then(|id| store.node(id))
            .map_or(&[][..], crate::SemanticNode::effect_ids)
            .iter()
            .filter_map(|id| {
                if preflight.removed_existing.contains(id) {
                    return None;
                }
                preflight
                    .staged_effects
                    .get(id)
                    .or_else(|| store.semantic_effect_state(*id).ok())
                    .map(|effect| effect.definition())
            });
        let pending = preflight
            .pending_effect_order
            .get(&owner)
            .into_iter()
            .flatten()
            .filter_map(|token| {
                if preflight.removed_pending.contains(token) {
                    return None;
                }
                match preflight.pending_creations.get(token) {
                    Some(SemanticNodeCreation::Effect { definition, .. }) => {
                        Some(definition.clone())
                    }
                    _ => None,
                }
            });
        let definitions = existing.chain(pending);
        if !definitions
            .clone()
            .any(|definition| definition.input_role() == crate::EffectInputRole::AuthoredView)
        {
            continue;
        }
        let state = preflight.staged_objects.get(&owner).or_else(|| {
            owner
                .existing()
                .and_then(|id| store.semantic_object_state_checked(id).ok())
        });
        if definitions.count() != 1
            || state.is_none_or(|state| {
                state.role() != SemanticObjectRole::Camera2D
                    || state.view_color_profile() != crate::ViewColorProfile::LinearHdr
            })
        {
            let index = transaction
                .mutations
                .iter()
                .position(|mutation| mutation.node_references().contains(&owner))
                .unwrap_or(0);
            return Err(SemanticMutationTransactionError::InvalidEffectAttachment {
                index,
                reason: "Bloom requires one view attachment on a LinearHdr Camera2D controller",
            });
        }
    }
    Ok(())
}
