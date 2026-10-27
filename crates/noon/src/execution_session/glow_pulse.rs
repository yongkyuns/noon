//! Authored pulse attachment ownership at the ordinary activation/completion barrier.
//!
//! The Runtime's existing glow track owns interpolation and child-end restoration.
//! This module only stages neutral semantic attachments and records which exact
//! attachment generations the enclosing segment may remove at completion.

use super::{ExecutionSession, ExecutionSessionAnimationError, ExecutionSessionPublicationError};
use crate::execution_segment::{ExecutionSegmentToken, GlowPulseAttachmentCompletion};
use noon_compile::{
    PreparedSemanticAnimationScheduleProjection, PreparedSemanticScheduledAnimationPayload,
};
use noon_core::{
    AnimationOptions, EffectParameter, EffectTrackEndBehavior, PreparedSemanticMutationTransaction,
    SemanticMutation, SemanticMutationTransaction, SemanticNodeId, SemanticTransactionNodeRef,
    TrackDefinition, TrackId, TrackValues,
};
use std::collections::{BTreeMap, BTreeSet};

/// Preflighted changes to existing segment ownership, committed only after the
/// corresponding ordinary semantic/execution publication has succeeded.
pub(super) struct PreparedGlowPulseEdit {
    pub token: ExecutionSegmentToken,
    pub removed_tracks: BTreeSet<TrackId>,
    adopted: BTreeSet<SemanticNodeId>,
}

impl ExecutionSession {
    pub(super) fn stage_glow_pulse_leaf(
        &self,
        target: SemanticNodeId,
        intensity: f64,
        options: AnimationOptions,
        declaration: &mut SemanticMutationTransaction,
    ) -> Result<noon_core::SemanticLocalNodeToken, ExecutionSessionAnimationError> {
        if !self.reachability.is_object_reachable(target) {
            return Err(ExecutionSessionAnimationError::InvalidComposition(
                "GlowPulse requires an object already present in the execution domain".into(),
            ));
        }
        Ok(declaration.create_glow_pulse_animation(target, intensity, options))
    }

    pub(super) fn missing_glow_pulse_attachments(
        &self,
        prepared: &PreparedSemanticMutationTransaction<'_>,
        schedule: &PreparedSemanticAnimationScheduleProjection,
    ) -> Result<BTreeSet<SemanticNodeId>, ExecutionSessionAnimationError> {
        let mut missing = BTreeSet::new();
        for leaf in schedule.leaves() {
            if !matches!(
                leaf.payload,
                PreparedSemanticScheduledAnimationPayload::GlowPulse { .. }
            ) {
                continue;
            }
            let owner = leaf
                .target
                .existing()
                .filter(|owner| self.reachability.is_object_reachable(*owner))
                .ok_or_else(|| {
                    ExecutionSessionAnimationError::InvalidComposition(
                        "GlowPulse requires an object already present in the execution domain"
                            .into(),
                    )
                })?;
            if prepared
                .animation_effect_snapshot(owner)
                .map_err(|error| {
                    ExecutionSessionAnimationError::InvalidComposition(error.to_string())
                })?
                .is_empty()
            {
                missing.insert(owner);
            }
        }
        Ok(missing)
    }

    /// A preliminary shape check keeps all ordinary pending-segment guards in
    /// place before semantic staging. The prepared proof below remains the
    /// authority for ownership, including explicit value-equal writes.
    pub(super) fn glow_pulse_edit_token(
        &self,
        mutations: &[SemanticMutation],
    ) -> Option<ExecutionSegmentToken> {
        let pending = self.pending_segment_completion.as_ref()?;
        (!mutations.is_empty()
            && mutations.iter().all(|mutation| {
                matches!(mutation, SemanticMutation::UpdateEffect { effect, .. }
                if pending.glow_pulse_attachments.contains_key(effect))
            }))
        .then_some(pending.token)
    }

    pub(super) fn prepare_glow_pulse_edit(
        &self,
        prepared: &PreparedSemanticMutationTransaction<'_>,
    ) -> Result<PreparedGlowPulseEdit, ExecutionSessionPublicationError> {
        let token = self
            .glow_pulse_edit_token(prepared.mutations())
            .ok_or(ExecutionSessionPublicationError::SegmentCompletionPending)?;
        let pending = self
            .pending_segment_completion
            .as_ref()
            .expect("validated pulse segment");
        let mut edit = PreparedGlowPulseEdit {
            token,
            removed_tracks: BTreeSet::new(),
            adopted: BTreeSet::new(),
        };
        // Deliberately inspect every explicit submitted parameter, not changed
        // candidates/impacts: a same-valued write still transfers ownership.
        for mutation in prepared.mutations() {
            let SemanticMutation::UpdateEffect { effect, update } = mutation else {
                unreachable!("pulse edit shape was validated above");
            };
            let ownership = &pending.glow_pulse_attachments[effect];
            if update
                .parameters()
                .any(|parameter| ownership.ordinary_parameters & parameter_bit(parameter) != 0)
            {
                return Err(ExecutionSessionPublicationError::SegmentCompletionPending);
            }
            let state = prepared
                .store()
                .semantic_effect_state(*effect)
                .expect("prepared effect update validated the exact generation");
            if state.owner() != ownership.owner {
                return Err(ExecutionSessionPublicationError::SegmentCompletionPending);
            }
            if update.parameters().next().is_some() {
                edit.adopted.insert(*effect);
            }
            if update.intensity.is_some() {
                edit.removed_tracks.extend(ownership.tracks.iter().copied());
            }
        }
        Ok(edit)
    }

    pub(super) fn commit_glow_pulse_edit(&mut self, edit: PreparedGlowPulseEdit) {
        let revision = self.publication_context().scene_revision();
        let pending = self
            .pending_segment_completion
            .as_mut()
            .expect("published pulse edit retains its segment");
        debug_assert_eq!(pending.token, edit.token);
        for attachment in edit.adopted {
            let ownership = pending
                .glow_pulse_attachments
                .get_mut(&attachment)
                .expect("preflighted attachment ownership remains present");
            ownership.remove_on_completion = false;
            ownership
                .tracks
                .retain(|track| !edit.removed_tracks.contains(track));
        }
        pending
            .superseded_glow_pulse_tracks
            .extend(edit.removed_tracks);
        // This proof advances only after both semantic and execution commits.
        // Keep the conservative affected-object index for other driver guards.
        pending.activation_scene_revision = revision;
    }
}

pub(super) fn record_attachment_completion(
    attachments: &mut BTreeMap<SemanticNodeId, GlowPulseAttachmentCompletion>,
    target: SemanticTransactionNodeRef,
    definition: &TrackDefinition,
    temporary: &BTreeSet<SemanticNodeId>,
) {
    let TrackValues::Effect {
        attachment,
        end_behavior: EffectTrackEndBehavior::RestoreSource,
        ..
    } = &definition.values
    else {
        return;
    };
    let owner = target
        .existing()
        .expect("pulse activation admitted an existing reachable object");
    let completion =
        attachments
            .entry(*attachment)
            .or_insert_with(|| GlowPulseAttachmentCompletion {
                owner,
                tracks: BTreeSet::new(),
                ordinary_parameters: 0,
                remove_on_completion: temporary.contains(attachment),
            });
    debug_assert_eq!(completion.owner, owner);
    completion.tracks.insert(definition.id);
}

pub(super) fn adopt_ordinary_parameter_tracks(
    attachments: &mut BTreeMap<SemanticNodeId, GlowPulseAttachmentCompletion>,
    definitions: &[TrackDefinition],
) {
    for definition in definitions {
        if let TrackValues::Effect {
            attachment,
            from,
            end_behavior: EffectTrackEndBehavior::Sample,
            ..
        } = &definition.values
        {
            if let Some(ownership) = attachments.get_mut(attachment) {
                ownership.remove_on_completion = false;
                for parameter in from.update().parameters() {
                    ownership.ordinary_parameters |= parameter_bit(parameter);
                }
            }
        }
    }
}

const fn parameter_bit(parameter: EffectParameter) -> u8 {
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

#[cfg(test)]
mod tests;
