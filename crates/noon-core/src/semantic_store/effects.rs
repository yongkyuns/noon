//! Appearance attachments owned by the existing Semantic Scene and allocator.
//! No playback, GPU resources, frontend state, or additional identity space.

use std::sync::Arc;

use super::{SemanticNode, SemanticNodeId, SemanticNodeKind, SemanticStore, SemanticStoreError};
use crate::{Glow, GlowParameterError, GlowUpdate};

/// Finite built-in definitions; custom program/schema resources remain #1897 M4.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectDefinition {
    Glow(Glow),
}

impl From<Glow> for EffectDefinition {
    fn from(value: Glow) -> Self {
        Self::Glow(value)
    }
}

impl EffectDefinition {
    pub fn update(self, update: GlowUpdate) -> Result<Self, GlowParameterError> {
        match self {
            Self::Glow(value) => update.apply_to(value).map(Self::Glow),
        }
    }
}

/// One named attachment. Identity is the containing node's ordinary NodeId.
/// This first authored slice is leaf-local; composed scopes are not silently
/// expanded into independent children. The renderer support gate is separate.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticEffectState {
    owner: SemanticNodeId,
    name: Arc<str>,
    definition: EffectDefinition,
}

impl SemanticEffectState {
    pub const fn owner(&self) -> SemanticNodeId {
        self.owner
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn definition(&self) -> EffectDefinition {
        self.definition
    }
}

impl SemanticNode {
    /// Stable attachment order, separate from painter/family membership.
    pub fn effect_ids(&self) -> &[SemanticNodeId] {
        self.effects.as_deref().map_or(&[], Vec::as_slice)
    }

    pub fn semantic_effect_state(&self) -> Option<&SemanticEffectState> {
        match &self.kind {
            SemanticNodeKind::Effect(state) => Some(state),
            _ => None,
        }
    }
}

impl SemanticStore {
    /// Constant-time declaration/profile gate; this count is derived from the
    /// same node allocator, not a second attachment registry.
    pub const fn has_effect_attachments(&self) -> bool {
        self.effect_nodes != 0
    }

    pub fn semantic_effect_state(
        &self,
        effect: SemanticNodeId,
    ) -> Result<&SemanticEffectState, SemanticStoreError> {
        self.node(effect)
            .ok_or(SemanticStoreError::UnknownNode(effect))?
            .semantic_effect_state()
            .ok_or(SemanticStoreError::NotEffect(effect))
    }

    /// Owner-local lookup. Names are case-sensitive, nonempty, and not IDs.
    pub fn effect_by_name(
        &self,
        owner: SemanticNodeId,
        name: &str,
    ) -> Result<Option<SemanticNodeId>, SemanticStoreError> {
        let node = self
            .node(owner)
            .ok_or(SemanticStoreError::UnknownNode(owner))?;
        Ok(node.effect_ids().iter().copied().find(|effect| {
            self.node(*effect)
                .and_then(SemanticNode::semantic_effect_state)
                .is_some_and(|state| state.name() == name)
        }))
    }

    pub(crate) fn insert_semantic_effect(
        &mut self,
        owner: SemanticNodeId,
        name: Arc<str>,
        definition: EffectDefinition,
    ) -> SemanticNodeId {
        let effect = self.insert_kind(SemanticNodeKind::Effect(SemanticEffectState {
            owner,
            name,
            definition,
        }));
        self.node_mut(owner)
            .expect("preflighted effect owner")
            .effects
            .get_or_insert_with(|| Box::new(Vec::new()))
            .push(effect);
        self.register_semantic_references_for_owner(effect);
        effect
    }

    pub(crate) fn replace_effect_definition(
        &mut self,
        effect: SemanticNodeId,
        definition: EffectDefinition,
    ) {
        let SemanticNodeKind::Effect(state) = &mut self
            .node_mut(effect)
            .expect("preflighted effect identity")
            .kind
        else {
            unreachable!("preflighted effect kind");
        };
        // Definition values currently contain no node/resource references.
        state.definition = definition;
    }

    /// Remove only this owner's attachment edge. Called by ordinary node
    /// retirement; the reverse-reference machinery cascades owner deletion.
    pub(crate) fn unlink_effect(&mut self, effect: SemanticNodeId, owner: SemanticNodeId) {
        if let Some(node) = self.node_mut(owner) {
            if let Some(effects) = &mut node.effects {
                effects.retain(|candidate| *candidate != effect);
                if effects.is_empty() {
                    node.effects = None;
                }
            }
        }
    }
}

impl SemanticStore {
    /// Append independent copies to the caller's ordinary atomic transaction.
    /// Only this source's attachment list is read; identity allocation occurs at
    /// commit, never while preparing the copy. Immutable definitions are shared values.
    pub fn copy_effects_into(
        &self,
        source: SemanticNodeId,
        target: impl Into<crate::SemanticTransactionNodeRef>,
        transaction: &mut crate::SemanticMutationTransaction,
    ) -> Result<(), SemanticStoreError> {
        let node = self
            .node(source)
            .ok_or(SemanticStoreError::UnknownNode(source))?;
        let target = target.into();
        for &id in node.effect_ids() {
            let effect = self.semantic_effect_state(id)?;
            transaction.create_effect(target, effect.name.clone(), effect.definition);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
