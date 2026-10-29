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
    visibility: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompiledEffectStackError {
    Empty,
    DuplicateAttachment(SemanticNodeId),
    ExceedsProfile,
    InvalidVisibilityRole,
    MixedInputRoles,
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
        Self::new_with_visibility(effects, None)
    }
    /// Preserve the semantic private Visibility role in the same immutable
    /// attachment column. The role must identify its trailing dissolve binding.
    pub fn new_with_visibility(
        effects: impl IntoIterator<Item = CompiledEffect>,
        visibility: Option<SemanticNodeId>,
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
        if visibility.is_some_and(|id| {
            attachments.last() != Some(&id)
                || definitions
                    .last()
                    .is_none_or(|definition| definition.as_noise_dissolve().is_none())
        }) {
            return Err(CompiledEffectStackError::InvalidVisibilityRole);
        }
        if definitions
            .iter()
            .any(|definition| definition.input_role() == noon_core::EffectInputRole::PainterPrefix)
            && (definitions[0].as_backdrop_blur().is_none()
                || definitions.len() != 1 + usize::from(visibility.is_some()))
        {
            return Err(CompiledEffectStackError::MixedInputRoles);
        }
        if definitions
            .iter()
            .any(|definition| definition.input_role() == noon_core::EffectInputRole::AuthoredView)
            && (definitions.len() != 1
                || definitions[0].as_bloom().is_none()
                || visibility.is_some())
        {
            return Err(CompiledEffectStackError::MixedInputRoles);
        }
        Ok(Self {
            visibility: visibility.is_some(),
            attachments: attachments.into_boxed_slice(),
            definitions: definitions.into_boxed_slice(),
        })
    }
    /// Input role follows the checked immutable attachment topology.
    pub fn input_role(&self) -> noon_core::EffectInputRole {
        self.definitions[0].input_role()
    }
    pub fn backdrop_blur(&self) -> Option<CompiledEffect> {
        self.definitions[0]
            .as_backdrop_blur()
            .map(|_| self.iter().next().expect("nonempty stack"))
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
    pub fn visibility(&self) -> Option<CompiledEffect> {
        self.visibility
            .then(|| self.iter().next_back().expect("nonempty visibility column"))
    }
    pub fn get(&self, attachment: SemanticNodeId) -> Option<CompiledEffect> {
        let index = self.attachments.iter().position(|&id| id == attachment)?;
        Some(CompiledEffect {
            attachment,
            definition: self.definitions[index].clone(),
        })
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = CompiledEffect> + DoubleEndedIterator + '_ {
        self.attachments
            .iter()
            .zip(&self.definitions)
            .map(|(&attachment, definition)| CompiledEffect {
                attachment,
                definition: definition.clone(),
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
        if self.definitions[index].input_role() != effect.definition.input_role() {
            return false;
        }
        if self.visibility
            && index + 1 == self.len()
            && effect.definition.as_noise_dissolve().is_none()
        {
            return false;
        }
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
                if (previous.visibility().map(|value| value.attachment) == Some(effect.attachment))
                    != (next.visibility().map(|value| value.attachment) == Some(effect.attachment))
                {
                    return Err(CompilePatchError::InvalidEffectAttachmentChange { object });
                }
                noon_core::EffectValue::validate_effect_value_update(
                    &old.definition,
                    &effect.definition,
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
            let retired = self.objects[index as usize]
                .effects
                .as_ref()
                .and_then(|stack| stack.get(attachment))
                .expect("validated previous attachment");
            for property in retired.parameter_properties() {
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
                stack
                    .get(attachment)
                    .expect("validated attachment")
                    .parameter_properties()
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

#[cfg(test)]
mod visibility_tests {
    use super::*;
    #[test]
    fn visibility_role_is_trailing_typed_generational_and_immutable() {
        let object = ObjectId::new(1);
        let id = SemanticNodeId::new(9, 3);
        let mask = CompiledEffect {
            attachment: id,
            definition: noon_core::NoiseDissolve::default().into(),
        };
        let named = CompiledEffect {
            attachment: SemanticNodeId::new(10, 4),
            ..mask.clone()
        };
        let mut private =
            CompiledEffectStack::new_with_visibility([mask.clone()], Some(id)).unwrap();
        let ordinary = CompiledEffectStack::new([mask.clone()]).unwrap();
        assert_eq!(private.visibility(), Some(mask.clone()));
        assert!(ordinary.visibility().is_none());
        for (effects, role) in [
            (vec![mask.clone()], SemanticNodeId::new(9, 4)),
            (vec![mask.clone(), named.clone()], id),
            (
                vec![CompiledEffect {
                    definition: noon_core::Glow::default().into(),
                    ..mask.clone()
                }],
                id,
            ),
        ] {
            assert_eq!(
                CompiledEffectStack::new_with_visibility(effects, Some(role)),
                Err(CompiledEffectStackError::InvalidVisibilityRole)
            );
        }
        assert!(
            crate::execution_patch::validate_effect_values(object, Some(&private), &ordinary)
                .is_err()
        );
        let forged = CompiledEffectStack::new([mask.clone(), named]).unwrap();
        assert!(validate_change(object, Some(&private), &[id], Some(&forged)).is_err());
        assert!(!private.replace(CompiledEffect {
            definition: noon_core::Glow::default().into(),
            ..mask.clone()
        }));
        let updated = CompiledEffect {
            definition: noon_core::NoiseDissolve::default()
                .with_progress(0.5)
                .unwrap()
                .into(),
            ..mask.clone()
        };
        assert!(private.replace(updated.clone()));
        assert_eq!(private.visibility(), Some(updated.clone()));
        let same_role = CompiledEffectStack::new_with_visibility([updated], Some(id)).unwrap();
        crate::execution_patch::validate_effect_values(object, Some(&private), &same_role).unwrap();
    }
}

#[cfg(test)]
mod backdrop_tests {
    use super::*;
    use noon_core::{
        BackdropBlur, Blur, EffectDefinition, EffectInputRole, EffectUpdate, NoiseDissolve, Pixels,
    };
    fn effect(id: u32, definition: impl Into<EffectDefinition>) -> CompiledEffect {
        CompiledEffect {
            attachment: SemanticNodeId::new(id, 2),
            definition: definition.into(),
        }
    }
    #[test]
    fn prefix_input_has_exact_binding_and_only_trailing_visibility_in_this_profile() {
        let blur = effect(1, BackdropBlur::default());
        let mask = effect(2, NoiseDissolve::default());
        let stack = CompiledEffectStack::new([blur.clone()]).unwrap();
        assert_eq!(stack.input_role(), EffectInputRole::PainterPrefix);
        assert_eq!(stack.backdrop_blur(), Some(blur.clone()));
        assert!(CompiledEffectStack::new_with_visibility(
            [blur.clone(), mask.clone()],
            Some(mask.attachment)
        )
        .is_ok());
        for definitions in [
            vec![blur.clone(), mask.clone()],
            vec![mask, blur.clone()],
            vec![blur.clone(), effect(3, Blur::default())],
            vec![effect(3, Blur::default()), blur.clone()],
            vec![blur.clone(), effect(3, BackdropBlur::default())],
        ] {
            assert_eq!(
                CompiledEffectStack::new(definitions),
                Err(CompiledEffectStackError::MixedInputRoles)
            );
        }
        let mut next = stack;
        assert!(!next.replace(CompiledEffect {
            definition: Blur::default().into(),
            ..blur
        }));
        assert_eq!(next.backdrop_blur(), Some(blur.clone()));
        let updated = CompiledEffect {
            definition: BackdropBlur::new(EffectUpdate::default().radius(Pixels(8.0)))
                .unwrap()
                .into(),
            ..blur
        };
        assert!(next.replace(updated.clone()));
        assert_eq!(next.backdrop_blur(), Some(updated));
        let channels = blur
            .parameter_channels(&EffectUpdate::default().radius(0.4))
            .unwrap();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].0, noon_core::Property::EffectRadius);
    }
}
