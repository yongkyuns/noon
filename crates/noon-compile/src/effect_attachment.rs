//! Sparse execution-plan projection of a semantic leaf attachment change.
use std::sync::Arc;

use noon_core::{ObjectId, SemanticNodeId};

use crate::{
    CompilePatchError, CompiledChannelKey, CompiledEffect, CompiledPatchStats, CompiledScene,
};

/// One ordered derived column. Attachment identities and parameter values have
/// exactly one slot each; definition storage is directly borrowable by rendering.
/// Ordinary rows retain None and allocate no attachment storage.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledEffectStack {
    attachments: Box<[SemanticNodeId]>,
    definitions: Box<[noon_core::EffectDefinition]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompiledEffectStackError {
    Empty,
    DuplicateAttachment(SemanticNodeId),
    ExceedsProfile,
}
impl std::fmt::Display for CompiledEffectStackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid effect stack: {self:?}")
    }
}
impl std::error::Error for CompiledEffectStackError {}

impl CompiledEffectStack {
    pub fn new(
        effects: impl IntoIterator<Item = CompiledEffect>,
    ) -> Result<Self, CompiledEffectStackError> {
        let mut attachments = Vec::new();
        let mut definitions = Vec::new();
        for effect in effects {
            if attachments.len() == noon_core::MAX_EFFECT_STACK_LEN {
                return Err(CompiledEffectStackError::ExceedsProfile);
            }
            if attachments.contains(&effect.attachment) {
                return Err(CompiledEffectStackError::DuplicateAttachment(
                    effect.attachment,
                ));
            }
            attachments.push(effect.attachment);
            definitions.push(effect.definition);
        }
        if attachments.is_empty() {
            return Err(CompiledEffectStackError::Empty);
        }
        Ok(Self {
            attachments: attachments.into_boxed_slice(),
            definitions: definitions.into_boxed_slice(),
        })
    }
    pub fn len(&self) -> usize {
        self.attachments.len()
    }
    pub fn is_empty(&self) -> bool {
        self.attachments.is_empty()
    }
    pub fn attachments(&self) -> &[SemanticNodeId] {
        &self.attachments
    }
    pub fn definitions(&self) -> &[noon_core::EffectDefinition] {
        &self.definitions
    }
    pub fn get(&self, attachment: SemanticNodeId) -> Option<CompiledEffect> {
        let index = self.attachments.iter().position(|&id| id == attachment)?;
        Some(CompiledEffect {
            attachment,
            definition: self.definitions[index],
        })
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = CompiledEffect> + DoubleEndedIterator + '_ {
        self.attachments
            .iter()
            .zip(&self.definitions)
            .map(|(&attachment, &definition)| CompiledEffect {
                attachment,
                definition,
            })
    }
    /// Replace an already validated parameter value without changing topology.
    pub fn replace(&mut self, effect: CompiledEffect) -> bool {
        let Some(index) = self
            .attachments
            .iter()
            .position(|&id| id == effect.attachment)
        else {
            return false;
        };
        self.definitions[index] = effect.definition;
        true
    }
}

pub(crate) fn validate_change(
    object: ObjectId,
    previous: Option<&CompiledEffectStack>,
    expected: &[SemanticNodeId],
    next: Option<&CompiledEffectStack>,
) -> Result<(), CompilePatchError> {
    let previous_ids = previous.map_or(&[][..], CompiledEffectStack::attachments);
    let next_ids = next.map_or(&[][..], CompiledEffectStack::attachments);
    if previous_ids != expected || (next_ids == expected && !expected.is_empty()) {
        return Err(CompilePatchError::InvalidEffectAttachmentChange { object });
    }
    if let (Some(previous), Some(next)) = (previous, next) {
        for effect in next.iter() {
            if let Some(old) = previous.get(effect.attachment) {
                noon_core::EffectValue::validate_effect_value_update(
                    old.definition,
                    effect.definition,
                )
                .map_err(|_| CompilePatchError::InvalidEffectAttachmentChange { object })?;
            }
        }
    }
    Ok(())
}

impl CompiledScene {
    pub(crate) fn apply_effect_stack(
        &mut self,
        object: ObjectId,
        expected: &[SemanticNodeId],
        next: Option<Arc<CompiledEffectStack>>,
        stats: &mut CompiledPatchStats,
    ) -> Result<(), CompilePatchError> {
        let index = self
            .object_index(object)
            .ok_or(CompilePatchError::UnknownObject(object))?;
        validate_change(
            object,
            self.objects[index as usize].effects.as_deref(),
            expected,
            next.as_deref(),
        )?;
        for &attachment in expected {
            if next
                .as_ref()
                .is_some_and(|stack| stack.get(attachment).is_some())
            {
                continue;
            }
            for property in CompiledEffect::PARAMETER_PROPERTIES {
                let channel = CompiledChannelKey::effect(index, property, attachment);
                if let Some(retired) = self.tracks.remove(&channel) {
                    self.track_count -= retired.len();
                    for track in retired {
                        self.track_locators.remove(&track.id);
                        stats.track_locators_removed += 1;
                    }
                }
            }
        }
        let dynamic = next.as_ref().is_some_and(|stack| {
            stack.attachments().iter().any(|&attachment| {
                CompiledEffect::PARAMETER_PROPERTIES
                    .into_iter()
                    .any(|property| {
                        self.has_channel(CompiledChannelKey::effect(index, property, attachment))
                    })
            })
        });
        let row = &mut self.objects[index as usize];
        row.effects = next;
        // Surviving generations retain their exact channels. Do not traverse
        // motion channels or move retained object/track slots.
        row.dynamic.effect = dynamic;
        Ok(())
    }
}
