//! Appearance attachments owned by the existing Semantic Scene and allocator.
//! No playback, GPU resources, frontend state, or additional identity space.

use std::sync::Arc;

use super::{SemanticNode, SemanticNodeId, SemanticNodeKind, SemanticStore, SemanticStoreError};
use crate::{
    Blur, EffectParameter, EffectParameterError, EffectRadius, EffectUpdate, EffectValue, Glow,
    NoiseDissolve, Shadow,
};

/// Maximum attachment count in the qualified finite planar execution profile.
/// Semantic storage can retain declarations outside that execution profile.
pub const MAX_EFFECT_STACK_LEN: usize = 8;

/// Finite built-in definitions; custom program/schema resources remain #1897 M4.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectDefinition {
    Glow(Glow),
    Blur(Blur),
    Shadow(Shadow),
    NoiseDissolve(NoiseDissolve),
}

impl From<NoiseDissolve> for EffectDefinition {
    fn from(value: NoiseDissolve) -> Self {
        Self::NoiseDissolve(value)
    }
}

impl From<Shadow> for EffectDefinition {
    fn from(value: Shadow) -> Self {
        Self::Shadow(value)
    }
}

impl From<Blur> for EffectDefinition {
    fn from(value: Blur) -> Self {
        Self::Blur(value)
    }
}

impl From<Glow> for EffectDefinition {
    fn from(value: Glow) -> Self {
        Self::Glow(value)
    }
}

impl EffectDefinition {
    pub const fn radius(self) -> Option<EffectRadius> {
        match self {
            Self::Glow(v) => Some(v.radius()),
            Self::Blur(v) => Some(v.radius()),
            Self::Shadow(v) => Some(v.radius()),
            Self::NoiseDissolve(_) => None,
        }
    }
    pub fn is_neutral(self) -> bool {
        match self {
            Self::Glow(v) => v.is_neutral(),
            Self::Blur(v) => v.is_neutral(),
            Self::Shadow(v) => v.is_neutral(),
            Self::NoiseDissolve(v) => v.is_neutral(),
        }
    }
    pub const fn as_shadow(self) -> Option<Shadow> {
        if let Self::Shadow(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub const fn color(self) -> Option<crate::Color> {
        match self {
            Self::Glow(v) => Some(v.color()),
            Self::Shadow(v) => Some(v.color()),
            Self::Blur(_) | Self::NoiseDissolve(_) => None,
        }
    }
    pub const fn as_glow(self) -> Option<Glow> {
        match self {
            Self::Glow(v) => Some(v),
            _ => None,
        }
    }
    pub const fn as_blur(self) -> Option<Blur> {
        match self {
            Self::Blur(v) => Some(v),
            _ => None,
        }
    }
    pub const fn as_noise_dissolve(self) -> Option<NoiseDissolve> {
        if let Self::NoiseDissolve(value) = self {
            Some(value)
        } else {
            None
        }
    }
    /// Compare only the supplied write channels against the prepared result.
    /// Declaration identity and stack order are independent of parameter dirtiness.
    pub(crate) fn parameter_changed(self, next: Self, parameter: EffectParameter) -> bool {
        match (self, next) {
            (Self::Glow(previous), Self::Glow(next)) => match parameter {
                EffectParameter::Color => previous.color() != next.color(),
                EffectParameter::Offset => false,
                EffectParameter::Radius => previous.radius() != next.radius(),
                EffectParameter::Intensity => previous.intensity() != next.intensity(),
                EffectParameter::Source => previous.source() != next.source(),
                _ => false,
            },
            (Self::Shadow(previous), Self::Shadow(next)) => match parameter {
                EffectParameter::Color => previous.color() != next.color(),
                EffectParameter::Radius => previous.radius() != next.radius(),
                EffectParameter::Offset => previous.offset() != next.offset(),
                _ => false,
            },
            (Self::Blur(previous), Self::Blur(next)) => {
                parameter == EffectParameter::Radius && previous.radius() != next.radius()
            }
            (Self::NoiseDissolve(previous), Self::NoiseDissolve(next)) => match parameter {
                EffectParameter::Progress => previous.progress() != next.progress(),
                EffectParameter::Softness => previous.softness() != next.softness(),
                EffectParameter::Seed => previous.seed() != next.seed(),
                _ => false,
            },
            _ => true,
        }
    }

    pub fn update(self, update: EffectUpdate) -> Result<Self, EffectParameterError> {
        update.apply_to(self)
    }
    /// Authored fields with no continuous runtime channel. Persistent value
    /// publication carries these directly while preserving other driven fields.
    pub fn discrete_parameters(self) -> EffectUpdate {
        match self {
            Self::Glow(value) => EffectUpdate::default().source(value.source()),
            Self::NoiseDissolve(value) => EffectUpdate::default().seed(value.seed()),
            Self::Blur(_) | Self::Shadow(_) => EffectUpdate::default(),
        }
    }
}

impl crate::object_state::effect_value_sealed::Sealed for EffectDefinition {}
impl EffectValue for EffectDefinition {
    fn apply_effect_update(self, update: EffectUpdate) -> Result<Self, EffectParameterError> {
        match self {
            Self::Glow(v) => update.apply_to(v).map(Self::Glow),
            Self::Blur(v) => update.apply_to(v).map(Self::Blur),
            Self::Shadow(v) => update.apply_to(v).map(Self::Shadow),
            Self::NoiseDissolve(v) => update.apply_to(v).map(Self::NoiseDissolve),
        }
    }
    fn validate_effect_value_update(self, target: Self) -> Result<(), EffectParameterError> {
        match (self, target) {
            (Self::NoiseDissolve(a), Self::NoiseDissolve(b)) => a.validate_effect_value_update(b),
            _ => self.validate_effect_transition(target),
        }
    }
    fn validate_effect_transition(self, target: Self) -> Result<(), EffectParameterError> {
        match (self, target) {
            (Self::Glow(a), Self::Glow(b)) => a.validate_effect_transition(b),
            (Self::Blur(a), Self::Blur(b)) => a.validate_effect_transition(b),
            (Self::Shadow(a), Self::Shadow(b)) => a.validate_effect_transition(b),
            (Self::NoiseDissolve(a), Self::NoiseDissolve(b)) => a.validate_effect_transition(b),
            _ => Err(EffectParameterError::IncompatibleDefinition),
        }
    }
    fn sample_effect_parameters(
        self,
        target: Self,
        request: EffectUpdate,
        alpha: f64,
    ) -> Result<EffectUpdate, EffectParameterError> {
        match (self, target) {
            (Self::Glow(a), Self::Glow(b)) => a.sample_effect_parameters(b, request, alpha),
            (Self::Blur(a), Self::Blur(b)) => a.sample_effect_parameters(b, request, alpha),
            (Self::Shadow(a), Self::Shadow(b)) => a.sample_effect_parameters(b, request, alpha),
            (Self::NoiseDissolve(a), Self::NoiseDissolve(b)) => {
                a.sample_effect_parameters(b, request, alpha)
            }
            _ => Err(EffectParameterError::IncompatibleDefinition),
        }
    }
}

/// Weak source correspondence and authored parameter intent for an effect copy.
/// The source generation is not an owning reference: independent copies survive
/// source removal. A target animation may use this intent only for that exact
/// source generation, never by resolving its name again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticEffectCopyProvenance {
    source_attachment: SemanticNodeId,
    copied_definition: EffectDefinition,
    written_parameters: u8,
}

impl SemanticEffectCopyProvenance {
    pub(crate) const fn new(
        source_attachment: SemanticNodeId,
        copied_definition: EffectDefinition,
    ) -> Self {
        Self {
            source_attachment,
            copied_definition,
            written_parameters: 0,
        }
    }

    pub const fn source_attachment(self) -> SemanticNodeId {
        self.source_attachment
    }

    pub const fn copied_definition(self) -> EffectDefinition {
        self.copied_definition
    }

    /// Explicit same-valued writes retain their authored channel meaning.
    pub const fn writes_parameter(self, parameter: EffectParameter) -> bool {
        self.written_parameters & effect_parameter_bit(parameter) != 0
    }

    fn with_update(mut self, update: EffectUpdate) -> Self {
        for parameter in update.parameters() {
            self.written_parameters |= effect_parameter_bit(parameter);
        }
        self
    }
}

const fn effect_parameter_bit(parameter: EffectParameter) -> u8 {
    match parameter {
        EffectParameter::Color => 1,
        EffectParameter::Radius => 2,
        EffectParameter::Intensity => 4,
        EffectParameter::Source => 8,
        EffectParameter::Offset => 16,
        EffectParameter::Progress => 32,
        EffectParameter::Softness => 64,
        EffectParameter::Seed => 128,
    }
}

/// One named attachment. Identity is the containing node's ordinary NodeId.
/// A family owns a composed treatment; it is never silently expanded into
/// independent child attachments. Execution validates source/isolation support.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticEffectState {
    owner: SemanticNodeId,
    name: Arc<str>,
    definition: EffectDefinition,
    copy_provenance: Option<SemanticEffectCopyProvenance>,
}

impl SemanticEffectState {
    pub const fn owner(&self) -> SemanticNodeId {
        self.owner
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn shared_name(&self) -> &Arc<str> {
        &self.name
    }

    pub const fn definition(&self) -> EffectDefinition {
        self.definition
    }

    pub const fn copy_provenance(&self) -> Option<SemanticEffectCopyProvenance> {
        self.copy_provenance
    }

    pub(crate) fn updated(&self, update: EffectUpdate) -> Result<Self, EffectParameterError> {
        let mut next = self.clone();
        next.definition = self.definition.update(update)?;
        next.copy_provenance = self.copy_provenance.map(|copy| copy.with_update(update));
        Ok(next)
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
        copy_provenance: Option<SemanticEffectCopyProvenance>,
    ) -> SemanticNodeId {
        let effect = self.insert_kind(SemanticNodeKind::Effect(SemanticEffectState {
            owner,
            name,
            definition,
            copy_provenance,
        }));
        self.node_mut(owner)
            .expect("preflighted effect owner")
            .effects
            .get_or_insert_with(|| Box::new(Vec::new()))
            .push(effect);
        self.register_semantic_references_for_owner(effect);
        effect
    }

    pub(crate) fn replace_effect_state(
        &mut self,
        effect: SemanticNodeId,
        next: SemanticEffectState,
    ) {
        let SemanticNodeKind::Effect(state) = &mut self
            .node_mut(effect)
            .expect("preflighted effect identity")
            .kind
        else {
            unreachable!("preflighted effect kind");
        };
        // Parameter/copy-intent updates do not change owning reference edges.
        debug_assert_eq!(state.owner, next.owner);
        debug_assert_eq!(state.name, next.name);
        *state = next;
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
            self.copy_effect_into_with_definition(id, target, effect.definition, transaction)?;
        }
        Ok(())
    }

    /// Freeze an authored animation target without changing the meaning of its
    /// existing parameter edits. This creates independent attachment identities
    /// while preserving the target's weak source correspondence and write mask.
    pub fn snapshot_effects_into(
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
            transaction.create_node(crate::SemanticNodeCreation::Effect {
                owner: target,
                name: effect.name.clone(),
                definition: effect.definition,
                copy_provenance: effect.copy_provenance,
            });
        }
        Ok(())
    }

    /// Stage one independent copy using a coherent effective definition supplied
    /// by the shared execution owner. The copied definition is its intent baseline;
    /// no effect parameter is considered explicitly authored by the copy itself.
    pub fn copy_effect_into_with_definition(
        &self,
        source: SemanticNodeId,
        target: impl Into<crate::SemanticTransactionNodeRef>,
        definition: impl Into<EffectDefinition>,
        transaction: &mut crate::SemanticMutationTransaction,
    ) -> Result<crate::SemanticLocalNodeToken, SemanticStoreError> {
        let effect = self.semantic_effect_state(source)?;
        let definition = definition.into();
        Ok(
            transaction.create_node(crate::SemanticNodeCreation::Effect {
                owner: target.into(),
                name: effect.name.clone(),
                definition,
                copy_provenance: Some(SemanticEffectCopyProvenance::new(source, definition)),
            }),
        )
    }
}

#[cfg(test)]
mod tests;
