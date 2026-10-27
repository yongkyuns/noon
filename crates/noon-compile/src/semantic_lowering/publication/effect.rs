//! Attachment projection at the existing prepared semantic publication boundary.
use std::{collections::BTreeSet, sync::Arc};

use noon_core::{
    PreparedSemanticMutationTransaction, SemanticMutation, SemanticNodeCreation, SemanticNodeId,
    SemanticTransactionNodeRef,
};

use super::{
    SemanticExecutionIndex, SemanticExecutionReachability, SemanticPublicationLoweringError,
};
use crate::{CompiledEffect, CompiledEffectStack, ExecutionPatch, SemanticLoweringError};

/// Read final staged state, including allocator-proven pending attachment IDs.
/// Detached targets remain inert; callers invoke this only for execution rows.
pub(super) fn lower_prepared_attachment(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    owner: SemanticTransactionNodeRef,
) -> Result<Option<Arc<CompiledEffectStack>>, SemanticPublicationLoweringError> {
    let semantic = prepared
        .planned_node_id(owner)
        .ok_or(SemanticPublicationLoweringError::PlannedObjectIdUnavailable { object: owner })?;
    let effects = prepared.animation_effect_snapshot(owner)?;
    if effects.is_empty() {
        return Ok(None);
    }
    match prepared.proposed_object_state(owner) {
        Ok(state) => super::super::projection::validate_effect_source_profile(
            semantic,
            &state,
            prepared.store(),
        )?,
        Err(error) => {
            // An admitted non-drawable family already owns its immutable input
            // plan. Parameter-only publication must not traverse that family.
            if prepared.family_style(owner).is_err() {
                return Err(error.into());
            }
        }
    }
    let stack = CompiledEffectStack::new(effects.into_iter().map(|effect| CompiledEffect {
        attachment: effect.attachment,
        definition: effect.definition,
    }))
    .map_err(|_| SemanticLoweringError::UnsupportedEffectProfile { owner: semantic })?;
    Ok(Some(Arc::new(stack)))
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
        // First family attachment is an ordinary prepared execution entry.
        // Removing its last attachment is an ordinary execution membership exit.
        if !reachability.is_object_reachable(owner)
            || (super::composed::unrendered_family(prepared, owner.into())?
                && prepared.animation_effect_snapshot(owner)?.is_empty())
        {
            continue;
        }
        let Some(object) = index.execution_object_id(owner) else {
            // Every already-live execution row must have its existing identity.
            // Newly admitted scope rows are prepared separately above.
            return Err(SemanticLoweringError::UnsupportedEffectProfile { owner }.into());
        };
        let original = prepared
            .store()
            .animation_effect_snapshot(owner)
            .map_err(SemanticLoweringError::from)?;
        let previous = if original.is_empty() {
            None
        } else {
            Some(
                CompiledEffectStack::new(original.into_iter().map(|effect| CompiledEffect {
                    attachment: effect.attachment,
                    definition: effect.definition,
                }))
                .map_err(|_| SemanticLoweringError::UnsupportedEffectProfile { owner })?,
            )
        };
        let next = lower_prepared_attachment(prepared, owner.into())?;
        if previous.as_ref() == next.as_deref() {
            continue;
        }
        let expected = previous
            .as_ref()
            .map_or(&[][..], CompiledEffectStack::attachments);
        let next_ids = next.as_ref().map_or(&[][..], |stack| stack.attachments());
        if expected == next_ids {
            mutations.push(ExecutionPatch::SetEffects {
                object,
                effects: next.expect("changed same-topology stack"),
            });
        } else {
            mutations.push(ExecutionPatch::SetEffectStack {
                object,
                expected: expected.into(),
                effects: next,
            });
        }
    }
    Ok(())
}
