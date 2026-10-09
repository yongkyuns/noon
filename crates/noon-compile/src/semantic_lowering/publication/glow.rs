//! Attachment projection at the existing prepared semantic publication boundary.
use std::{collections::BTreeSet, sync::Arc};

use noon_core::{
    EffectDefinition, PreparedSemanticMutationTransaction, SemanticMutation, SemanticNodeCreation,
    SemanticNodeId, SemanticTransactionNodeRef,
};

use super::{
    SemanticExecutionIndex, SemanticExecutionReachability, SemanticPublicationLoweringError,
};
use crate::{CompiledGlow, ExecutionPatch, SemanticLoweringError};

/// Read final staged state, including allocator-proven pending attachment IDs.
/// Detached targets remain inert; callers invoke this only for execution rows.
pub(super) fn lower_prepared_attachment(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    owner: SemanticTransactionNodeRef,
) -> Result<Option<Arc<CompiledGlow>>, SemanticPublicationLoweringError> {
    let semantic = prepared
        .planned_node_id(owner)
        .ok_or(SemanticPublicationLoweringError::PlannedObjectIdUnavailable { object: owner })?;
    let effects = prepared.animation_effect_snapshot(owner)?;
    if effects.is_empty() {
        return Ok(None);
    }
    let [effect] = effects.as_slice() else {
        return Err(SemanticLoweringError::UnsupportedGlowProfile { owner: semantic }.into());
    };
    let state = prepared.proposed_object_state(owner)?;
    super::super::projection::validate_glow_source_profile(semantic, &state, prepared.store())?;
    let EffectDefinition::Glow(definition) = effect.definition;
    Ok(Some(Arc::new(CompiledGlow {
        attachment: effect.attachment,
        definition,
    })))
}

/// Visit only owners touched by this batch. Never scan the store, root family,
/// or unrelated attachments to discover a local topology or parameter change.
pub(super) fn lower_changes(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    index: &SemanticExecutionIndex,
    reachability: &SemanticExecutionReachability,
    mutations: &mut Vec<ExecutionPatch>,
) -> Result<(), SemanticPublicationLoweringError> {
    let mut owners = BTreeSet::<SemanticNodeId>::new();
    for mutation in prepared.candidate_mutations() {
        match mutation {
            SemanticMutation::AddNode {
                creation: SemanticNodeCreation::Effect { owner, .. },
                ..
            } => {
                // Pending/newly reachable owners get the final column through
                // their ordinary CreateObject entry, never a premature patch.
                if let Some(owner) = owner.existing() {
                    owners.insert(owner);
                }
            }
            SemanticMutation::RemoveNode { node } => {
                if let Some(effect) = node
                    .existing()
                    .and_then(|node| prepared.store().node(node))
                    .and_then(noon_core::SemanticNode::semantic_effect_state)
                {
                    owners.insert(effect.owner());
                }
            }
            _ => {}
        }
    }
    owners.extend(prepared.effect_updates().map(|(_, owner, _)| owner));
    for owner in owners {
        if !reachability.is_reachable(owner) || prepared.node_is_removed(owner) {
            continue;
        }
        let Some(object) = index.execution_object_id(owner) else {
            // A reachable family cannot quietly accept a per-leaf treatment
            // in place of an unimplemented composed-group effect.
            return Err(SemanticLoweringError::UnsupportedGlowProfile { owner }.into());
        };
        let original = prepared
            .store()
            .animation_effect_snapshot(owner)
            .map_err(SemanticLoweringError::from)?;
        let previous = match original.as_slice() {
            [] => None,
            [effect] => {
                let EffectDefinition::Glow(definition) = effect.definition;
                Some(CompiledGlow {
                    attachment: effect.attachment,
                    definition,
                })
            }
            _ => return Err(SemanticLoweringError::UnsupportedGlowProfile { owner }.into()),
        };
        let next = lower_prepared_attachment(prepared, owner.into())?;
        if previous.as_ref() == next.as_deref() {
            continue;
        }
        let expected = previous.as_ref().map(|glow| glow.attachment);
        if expected == next.as_ref().map(|glow| glow.attachment) {
            mutations.push(ExecutionPatch::SetGlow {
                object,
                glow: next.expect("a changed same-generation column is present"),
            });
        } else {
            mutations.push(ExecutionPatch::SetGlowAttachment {
                object,
                expected,
                glow: next,
            });
        }
    }
    Ok(())
}
