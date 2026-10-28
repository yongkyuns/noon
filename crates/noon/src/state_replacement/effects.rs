//! Preparation-local ordered appearance reads for one state replacement.
use std::collections::HashMap;

use noon_core::{
    SemanticAnimationEffectSnapshot, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticNodeId, SemanticStore, SemanticTransactionNodeRef,
};

use crate::{AuthoringError, ExecutionSession};

pub(super) struct EffectReplacement<'a> {
    store: &'a SemanticStore,
    execution: Option<&'a ExecutionSession>,
    ordered_reads: bool,
    staged: HashMap<SemanticNodeId, Vec<SemanticAnimationEffectSnapshot>>,
    retirements: Vec<SemanticNodeId>,
}

impl<'a> EffectReplacement<'a> {
    pub(super) fn new(
        store: &'a SemanticStore,
        execution: Option<&'a ExecutionSession>,
        ordered_reads: bool,
    ) -> Self {
        Self {
            store,
            execution,
            ordered_reads,
            staged: HashMap::new(),
            retirements: Vec::new(),
        }
    }

    pub(super) fn stage(
        &mut self,
        receiver: impl Into<SemanticTransactionNodeRef>,
        target: SemanticNodeId,
        transaction: &mut SemanticMutationTransaction,
    ) -> Result<(), AuthoringError> {
        let receiver = receiver.into();
        if receiver.existing() == Some(target) {
            return Ok(());
        }
        if !self.store.has_effect_attachments() {
            return Ok(());
        }
        let target_node = self
            .store
            .node(target)
            .ok_or(noon_core::SemanticStoreError::UnknownNode(target))?;
        let receiver_node = receiver
            .existing()
            .map(|owner| {
                self.store
                    .node(owner)
                    .ok_or(noon_core::SemanticStoreError::UnknownNode(owner))
            })
            .transpose()?;
        let staged_target = self.staged.get(&target).filter(|_| self.ordered_reads);
        // Ordinary effect-free operands allocate no attachment snapshots or
        // overlay entries, even when an unrelated object elsewhere has effects.
        if receiver_node.is_none_or(|node| node.effect_ids().is_empty())
            && staged_target.map_or(target_node.effect_ids().is_empty(), Vec::is_empty)
        {
            return Ok(());
        }
        let captured = match staged_target {
            Some(snapshot) => snapshot.clone(),
            None => match self.execution {
                Some(execution) => {
                    crate::effective_capture::capture_effects(self.store, execution, target)?
                }
                None => self.store.animation_effect_snapshot(target)?,
            },
        };
        if let Some(owner) = receiver.existing() {
            let node = receiver_node.expect("existing receiver was checked");
            self.retirements.extend_from_slice(node.effect_ids());
            if self.ordered_reads {
                self.staged.insert(owner, captured.clone());
            }
        }
        for effect in captured {
            // A persistent replacement creates receiver-owned bindings. It is
            // not another target-editor copy carrying weak driver correspondence.
            transaction.create_node(SemanticNodeCreation::Effect {
                owner: receiver,
                name: effect.name,
                definition: effect.definition,
                copy_provenance: None,
            });
        }
        Ok(())
    }
    pub(super) fn into_retirements(self) -> Vec<SemanticNodeId> {
        self.retirements
    }
}
