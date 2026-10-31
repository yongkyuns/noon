//! Appearance attachments owned by the existing Semantic Scene and allocator.
//! No playback, GPU resources, frontend state, or additional identity space.

use std::sync::Arc;

use super::{SemanticNode, SemanticNodeId, SemanticNodeKind, SemanticStore, SemanticStoreError};
use crate::{
    BackdropBlur, Blur, EffectParameter, EffectParameterError, EffectRadius, EffectUpdate,
    EffectValue, Glow, NoiseDissolve, Pixelate, Shadow,
};

/// Maximum attachment count in the qualified finite planar execution profile.
/// Semantic storage can retain declarations outside that execution profile.
pub const MAX_EFFECT_STACK_LEN: usize = 8;

/// Checked appearance values and immutable program-backed treatments.
#[derive(Clone, Debug, PartialEq)]
pub enum EffectDefinition {
    Bloom(crate::Bloom),
    Glow(Glow),
    Blur(Blur),
    BackdropBlur(BackdropBlur),
    Shadow(Shadow),
    NoiseDissolve(NoiseDissolve),
    Pixelate(Pixelate),
    Shader(crate::ShaderEffect),
}

/// Explicit high-level input role; an ordinary painted filter never implicitly
/// samples content behind its owner. These are source roles, not new scopes,
/// identities, clocks or renderer-owned scene state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectInputRole {
    Painted,
    PainterPrefix,
    AuthoredView,
}

impl From<crate::Bloom> for EffectDefinition {
    fn from(value: crate::Bloom) -> Self {
        Self::Bloom(value)
    }
}

impl From<BackdropBlur> for EffectDefinition {
    fn from(value: BackdropBlur) -> Self {
        Self::BackdropBlur(value)
    }
}

impl From<Pixelate> for EffectDefinition {
    fn from(value: Pixelate) -> Self {
        Self::Pixelate(value)
    }
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

impl From<crate::ShaderEffect> for EffectDefinition {
    fn from(value: crate::ShaderEffect) -> Self {
        Self::Shader(value)
    }
}

impl EffectDefinition {
    pub fn as_shader(&self) -> Option<&crate::ShaderEffect> {
        if let Self::Shader(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub const fn as_bloom(&self) -> Option<crate::Bloom> {
        if let Self::Bloom(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub const fn intensity(&self) -> Option<f64> {
        match self {
            Self::Bloom(v) => Some(v.intensity()),
            Self::Glow(v) => Some(v.intensity()),
            _ => None,
        }
    }
    pub const fn threshold(&self) -> Option<f64> {
        match self {
            Self::Bloom(v) => Some(v.threshold()),
            _ => None,
        }
    }
    pub const fn input_role(&self) -> EffectInputRole {
        match self {
            Self::BackdropBlur(_) => EffectInputRole::PainterPrefix,
            Self::Bloom(_) => EffectInputRole::AuthoredView,
            _ => EffectInputRole::Painted,
        }
    }
    pub const fn as_backdrop_blur(&self) -> Option<BackdropBlur> {
        if let Self::BackdropBlur(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub const fn radius(&self) -> Option<EffectRadius> {
        match self {
            Self::Bloom(v) => Some(v.radius()),
            Self::Glow(v) => Some(v.radius()),
            Self::Blur(v) => Some(v.radius()),
            Self::BackdropBlur(v) => Some(v.radius()),
            Self::Shadow(v) => Some(v.radius()),
            Self::NoiseDissolve(_) | Self::Pixelate(_) | Self::Shader(_) => None,
        }
    }
    pub fn is_neutral(&self) -> bool {
        match self {
            Self::Bloom(v) => v.is_neutral(),
            Self::Glow(v) => v.is_neutral(),
            Self::Blur(v) => v.is_neutral(),
            Self::BackdropBlur(v) => v.is_neutral(),
            Self::Shadow(v) => v.is_neutral(),
            Self::NoiseDissolve(v) => v.is_neutral(),
            Self::Pixelate(v) => v.is_neutral(),
            Self::Shader(_) => false,
        }
    }
    pub const fn as_shadow(&self) -> Option<Shadow> {
        if let Self::Shadow(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub const fn color(&self) -> Option<crate::Color> {
        match self {
            Self::Glow(v) => Some(v.color()),
            Self::Shadow(v) => Some(v.color()),
            Self::Bloom(_)
            | Self::Blur(_)
            | Self::BackdropBlur(_)
            | Self::NoiseDissolve(_)
            | Self::Pixelate(_)
            | Self::Shader(_) => None,
        }
    }
    pub const fn as_glow(&self) -> Option<Glow> {
        match self {
            Self::Glow(v) => Some(*v),
            _ => None,
        }
    }
    pub const fn as_blur(&self) -> Option<Blur> {
        match self {
            Self::Blur(v) => Some(*v),
            _ => None,
        }
    }
    pub const fn as_pixelate(&self) -> Option<Pixelate> {
        if let Self::Pixelate(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub const fn as_noise_dissolve(&self) -> Option<NoiseDissolve> {
        if let Self::NoiseDissolve(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    /// Compare only the supplied write channels against the prepared result.
    /// Declaration identity and stack order are independent of parameter dirtiness.
    pub(crate) fn parameter_changed(&self, next: &Self, parameter: EffectParameter) -> bool {
        match (self, next) {
            (Self::Bloom(a), Self::Bloom(b)) => match parameter {
                EffectParameter::Radius => a.radius() != b.radius(),
                EffectParameter::Threshold => a.threshold() != b.threshold(),
                EffectParameter::Intensity => a.intensity() != b.intensity(),
                _ => false,
            },
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
            (Self::BackdropBlur(previous), Self::BackdropBlur(next)) => {
                parameter == EffectParameter::Radius && previous.radius() != next.radius()
            }
            (Self::NoiseDissolve(previous), Self::NoiseDissolve(next)) => match parameter {
                EffectParameter::Progress => previous.progress() != next.progress(),
                EffectParameter::Softness => previous.softness() != next.softness(),
                EffectParameter::Seed => previous.seed() != next.seed(),
                _ => false,
            },
            (Self::Pixelate(a), Self::Pixelate(b)) => {
                parameter == EffectParameter::Size && a.size() != b.size()
            }
            (Self::Shader(a), Self::Shader(b)) => match parameter {
                EffectParameter::Seed => a.seed() != b.seed(),
                EffectParameter::Shader(index) => {
                    a.program() != b.program()
                        || a.values().get(usize::from(index)) != b.values().get(usize::from(index))
                }
                _ => false,
            },
            _ => true,
        }
    }

    pub fn update(self, update: &EffectUpdate) -> Result<Self, EffectParameterError> {
        update.apply_to(self)
    }
    /// Authored fields with no continuous runtime channel. Persistent value
    /// publication carries these directly while preserving other driven fields.
    pub fn discrete_parameters(&self) -> EffectUpdate {
        match self {
            Self::Glow(value) => EffectUpdate::default().source(value.source()),
            Self::NoiseDissolve(value) => EffectUpdate::default().seed(value.seed()),
            Self::Shader(value) => {
                let mut update = if value.program().schema().dependencies.seed {
                    EffectUpdate::default().seed(value.seed())
                } else {
                    EffectUpdate::default()
                };
                for (index, parameter) in value.program().schema().parameters.iter().enumerate() {
                    if !parameter.kind.is_continuous() {
                        let key = value
                            .program()
                            .parameter(&parameter.name)
                            .expect("checked schema");
                        update = update.shader_parameter(&key, value.values()[index]);
                    }
                }
                update
            }

            Self::Bloom(_)
            | Self::Blur(_)
            | Self::BackdropBlur(_)
            | Self::Shadow(_)
            | Self::Pixelate(_) => EffectUpdate::default(),
        }
    }
}

impl crate::object_state::effect_value_sealed::Sealed for EffectDefinition {}
impl EffectValue for EffectDefinition {
    fn apply_effect_update(self, update: &EffectUpdate) -> Result<Self, EffectParameterError> {
        match self {
            Self::Bloom(v) => update.apply_to(v).map(Self::Bloom),
            Self::Glow(v) => update.apply_to(v).map(Self::Glow),
            Self::Blur(v) => update.apply_to(v).map(Self::Blur),
            Self::BackdropBlur(v) => update.apply_to(v).map(Self::BackdropBlur),
            Self::Shadow(v) => update.apply_to(v).map(Self::Shadow),
            Self::NoiseDissolve(v) => update.apply_to(v).map(Self::NoiseDissolve),
            Self::Pixelate(v) => update.apply_to(v).map(Self::Pixelate),
            Self::Shader(v) => update.apply_to(v).map(Self::Shader),
        }
    }
    fn validate_effect_value_update(&self, target: &Self) -> Result<(), EffectParameterError> {
        match (self, target) {
            (Self::NoiseDissolve(a), Self::NoiseDissolve(b)) => a.validate_effect_value_update(b),
            (Self::Pixelate(a), Self::Pixelate(b)) => a.validate_effect_value_update(b),
            (Self::Shader(a), Self::Shader(b)) => a.validate_effect_value_update(b),
            (previous, next) => previous.validate_effect_transition(next),
        }
    }
    fn validate_effect_transition(&self, target: &Self) -> Result<(), EffectParameterError> {
        match (self, target) {
            (Self::Bloom(a), Self::Bloom(b)) => a.validate_effect_transition(b),
            (Self::Glow(a), Self::Glow(b)) => a.validate_effect_transition(b),
            (Self::Blur(a), Self::Blur(b)) => a.validate_effect_transition(b),
            (Self::BackdropBlur(a), Self::BackdropBlur(b)) => a.validate_effect_transition(b),
            (Self::Shadow(a), Self::Shadow(b)) => a.validate_effect_transition(b),
            (Self::NoiseDissolve(a), Self::NoiseDissolve(b)) => a.validate_effect_transition(b),
            (Self::Pixelate(a), Self::Pixelate(b)) => a.validate_effect_transition(b),
            (Self::Shader(a), Self::Shader(b)) => a.validate_effect_transition(b),
            _ => Err(EffectParameterError::IncompatibleDefinition),
        }
    }
    fn sample_effect_parameters(
        &self,
        target: &Self,
        request: &EffectUpdate,
        alpha: f64,
    ) -> Result<EffectUpdate, EffectParameterError> {
        match (self, target) {
            (Self::Bloom(a), Self::Bloom(b)) => a.sample_effect_parameters(b, request, alpha),
            (Self::Glow(a), Self::Glow(b)) => a.sample_effect_parameters(b, request, alpha),
            (Self::Blur(a), Self::Blur(b)) => a.sample_effect_parameters(b, request, alpha),
            (Self::BackdropBlur(a), Self::BackdropBlur(b)) => {
                a.sample_effect_parameters(b, request, alpha)
            }
            (Self::Shadow(a), Self::Shadow(b)) => a.sample_effect_parameters(b, request, alpha),
            (Self::NoiseDissolve(a), Self::NoiseDissolve(b)) => {
                a.sample_effect_parameters(b, request, alpha)
            }
            (Self::Pixelate(a), Self::Pixelate(b)) => a.sample_effect_parameters(b, request, alpha),
            (Self::Shader(a), Self::Shader(b)) => a.sample_effect_parameters(b, request, alpha),
            _ => Err(EffectParameterError::IncompatibleDefinition),
        }
    }
}

/// Weak source correspondence and authored parameter intent for an effect copy.
/// The source generation is not an owning reference: independent copies survive
/// source removal. A target animation may use this intent only for that exact
/// source generation, never by resolving its name again.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticEffectCopyProvenance {
    source_attachment: SemanticNodeId,
    copied_definition: EffectDefinition,
    written_parameters: u32,
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

    pub const fn source_attachment(&self) -> SemanticNodeId {
        self.source_attachment
    }

    pub fn copied_definition(&self) -> &EffectDefinition {
        &self.copied_definition
    }

    /// Explicit same-valued writes retain their authored channel meaning.
    pub const fn writes_parameter(&self, parameter: EffectParameter) -> bool {
        self.written_parameters & effect_parameter_bit(parameter) != 0
    }

    fn with_update(mut self, update: &EffectUpdate) -> Self {
        for parameter in update.parameters() {
            self.written_parameters |= effect_parameter_bit(parameter);
        }
        self
    }
}

const fn effect_parameter_bit(parameter: EffectParameter) -> u32 {
    match parameter {
        EffectParameter::Color => 1,
        EffectParameter::Radius => 2,
        EffectParameter::Intensity => 4,
        EffectParameter::Source => 8,
        EffectParameter::Offset => 16,
        EffectParameter::Progress => 32,
        EffectParameter::Softness => 64,
        EffectParameter::Seed => 128,
        EffectParameter::Size => 256,
        EffectParameter::Threshold => 512,
        EffectParameter::Shader(index) if index < 16 => 1 << (10 + index),
        EffectParameter::Shader(_) => 0,
    }
}

/// Attachment keys share one owner-local namespace. Visibility is a distinct
/// semantic role, so an animation cannot overwrite an authored name or be
/// followed by a filter that resurrects an otherwise hidden contribution.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SemanticEffectName {
    Authored(Arc<str>),
    Visibility,
}
impl SemanticEffectName {
    pub fn authored(&self) -> Option<&str> {
        match self {
            Self::Authored(name) => Some(name),
            Self::Visibility => None,
        }
    }
    pub const fn is_visibility(&self) -> bool {
        matches!(self, Self::Visibility)
    }
}
impl From<Arc<str>> for SemanticEffectName {
    fn from(value: Arc<str>) -> Self {
        Self::Authored(value)
    }
}
impl From<&str> for SemanticEffectName {
    fn from(value: &str) -> Self {
        Self::Authored(value.into())
    }
}
impl From<String> for SemanticEffectName {
    fn from(value: String) -> Self {
        Self::Authored(value.into())
    }
}

/// One appearance attachment. Identity is the containing node's ordinary NodeId.
/// A family owns a composed treatment; it is never silently expanded into
/// independent child attachments. Execution validates source/isolation support.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticEffectState {
    owner: SemanticNodeId,
    name: SemanticEffectName,
    definition: EffectDefinition,
    copy_provenance: Option<SemanticEffectCopyProvenance>,
}

impl SemanticEffectState {
    pub const fn owner(&self) -> SemanticNodeId {
        self.owner
    }

    pub fn name(&self) -> Option<&str> {
        self.name.authored()
    }

    pub fn attachment_name(&self) -> &SemanticEffectName {
        &self.name
    }

    pub fn definition(&self) -> EffectDefinition {
        self.definition.clone()
    }

    pub fn copy_provenance(&self) -> Option<SemanticEffectCopyProvenance> {
        self.copy_provenance.clone()
    }

    pub(crate) fn updated(&self, update: &EffectUpdate) -> Result<Self, EffectParameterError> {
        let mut next = self.clone();
        next.definition = next.definition.update(update)?;
        next.copy_provenance = next.copy_provenance.map(|copy| copy.with_update(update));
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

    /// Constant-time gate for declarations that can require an isolated family
    /// image. The cardinalities are maintained with the same semantic nodes.
    pub const fn has_composed_source_declarations(&self) -> bool {
        self.effect_nodes != 0
            || self.partial_opacity_families != 0
            || self.explicit_family_opacity_targets != 0
            || self.appearance_target_animations != 0
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
                .is_some_and(|state| state.name() == Some(name))
        }))
    }

    /// One bounded visibility binding, always last in this owner's stack.
    pub fn visibility_effect(
        &self,
        owner: SemanticNodeId,
    ) -> Result<Option<SemanticNodeId>, SemanticStoreError> {
        let node = self
            .node(owner)
            .ok_or(SemanticStoreError::UnknownNode(owner))?;
        Ok(node.effect_ids().last().copied().filter(|id| {
            self.semantic_effect_state(*id)
                .is_ok_and(|state| state.attachment_name().is_visibility())
        }))
    }

    /// Stage creation or replacement of one visibility value. Repeated
    /// operations reuse its exact generation until ordinary node retirement.
    /// No animation, clock or completion ownership is introduced by this helper.
    pub fn stage_visibility_mask(
        &self,
        owner: impl Into<crate::SemanticTransactionNodeRef>,
        definition: NoiseDissolve,
        transaction: &mut crate::SemanticMutationTransaction,
    ) -> Result<crate::SemanticTransactionNodeRef, SemanticStoreError> {
        let owner = owner.into();
        if let crate::SemanticTransactionNodeRef::Existing(id) = owner {
            if let Some(attachment) = self.visibility_effect(id)? {
                transaction.update_effect(
                    attachment,
                    EffectUpdate::default()
                        .progress(definition.progress())
                        .softness(definition.softness())
                        .seed(definition.seed()),
                );
                return Ok(attachment.into());
            }
        }
        Ok(transaction
            .create_visibility_effect(owner, definition)
            .into())
    }

    pub(crate) fn effect_by_attachment_name(
        &self,
        owner: SemanticNodeId,
        name: &SemanticEffectName,
    ) -> Result<Option<SemanticNodeId>, SemanticStoreError> {
        match name {
            SemanticEffectName::Authored(name) => self.effect_by_name(owner, name),
            SemanticEffectName::Visibility => self.visibility_effect(owner),
        }
    }

    pub(crate) fn insert_semantic_effect(
        &mut self,
        owner: SemanticNodeId,
        name: SemanticEffectName,
        definition: EffectDefinition,
        copy_provenance: Option<SemanticEffectCopyProvenance>,
    ) -> SemanticNodeId {
        let last_is_visibility = self
            .node(owner)
            .expect("preflighted effect owner")
            .effect_ids()
            .last()
            .is_some_and(|id| {
                self.semantic_effect_state(*id)
                    .expect("live attachment")
                    .attachment_name()
                    .is_visibility()
            });
        let insert_before_visibility = !name.is_visibility() && last_is_visibility;
        let effect = self.insert_kind(SemanticNodeKind::Effect(SemanticEffectState {
            owner,
            name,
            definition,
            copy_provenance,
        }));
        let effects = self
            .node_mut(owner)
            .expect("preflighted effect owner")
            .effects
            .get_or_insert_with(|| Box::new(Vec::new()));
        effects.insert(
            effects.len() - usize::from(insert_before_visibility),
            effect,
        );
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
            self.copy_effect_into_with_definition(
                id,
                target,
                effect.definition.clone(),
                transaction,
            )?;
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
                definition: effect.definition.clone(),
                copy_provenance: effect.copy_provenance.clone(),
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
                definition: definition.clone(),
                copy_provenance: Some(SemanticEffectCopyProvenance::new(source, definition)),
            }),
        )
    }
}

#[cfg(test)]
mod tests;
