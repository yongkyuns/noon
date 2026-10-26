//! Persistent handoff of exact parameter lanes currently owned by a click pulse.
//! Submitted intent, including value-equal writes, drives adoption. The receipt
//! changes invocation ownership only after the shared publication has committed.

use super::{
    ExecutionSession, ExecutionSessionPublicationError, PreparedRuntimePublication,
    SemanticPublicationPurpose,
};
use noon_compile::ExecutionPatch;
use noon_core::{
    ObjectId, PreparedSemanticMutationTransaction, Property, SemanticMutation, SemanticNodeId,
    TrackValues,
};
use noon_runtime::EffectivePropertyWrite;
use std::collections::{BTreeMap, HashSet};

#[derive(Default)]
struct GlowEditIntent {
    intensity: Option<f64>,
    authored_intensity_track: bool,
}

pub(super) struct PreparedTransientGlowEdits {
    edits: Vec<(ObjectId, SemanticNodeId, bool)>,
    transient_carry: Vec<ObjectId>,
}

impl ExecutionSession {
    pub(super) fn prepare_transient_glow_edits(
        &self,
        semantic: &PreparedSemanticMutationTransaction<'_>,
        execution: &[ExecutionPatch],
        runtime: &mut PreparedRuntimePublication,
        purpose: SemanticPublicationPurpose,
    ) -> Result<PreparedTransientGlowEdits, ExecutionSessionPublicationError> {
        let mut intents = BTreeMap::<(ObjectId, SemanticNodeId), GlowEditIntent>::new();
        if purpose != SemanticPublicationPurpose::TransientGlowLifecycle {
            for mutation in semantic.mutations() {
                let SemanticMutation::UpdateEffect { effect, update } = mutation else {
                    continue;
                };
                if update.parameters().next().is_none() {
                    continue;
                }
                let Ok(state) = semantic.store().semantic_effect_state(*effect) else {
                    continue;
                };
                let Some(object) = self.execution_object_id(state.owner()) else {
                    continue;
                };
                if !self
                    .runtime
                    .transient_glow_attachment(object)
                    .is_some_and(|(attachment, _)| attachment == *effect)
                {
                    continue;
                }
                let intent = intents.entry((object, *effect)).or_default();
                if let Some(intensity) = update.intensity {
                    intent.intensity = Some(intensity);
                }
            }
            for patch in execution {
                let ExecutionPatch::AddTrack(track) = patch else {
                    continue;
                };
                let TrackValues::Glow { attachment, .. } = track.values else {
                    continue;
                };
                if !self
                    .runtime
                    .transient_glow_attachment(track.object)
                    .is_some_and(|(active, _)| active == attachment)
                {
                    continue;
                }
                let intent = intents.entry((track.object, attachment)).or_default();
                intent.authored_intensity_track |= track.property == Property::GlowIntensity;
            }
        }
        if intents.is_empty() {
            return Ok(PreparedTransientGlowEdits {
                edits: Vec::new(),
                transient_carry: Vec::new(),
            });
        }
        self.runtime
            .preflight_transient_glow_edits(intents.len())
            .map_err(ExecutionSessionPublicationError::Interaction)?;
        let original_carry = runtime
            .effective
            .as_ref()
            .map(|batch| batch.objects().collect::<HashSet<_>>())
            .unwrap_or_default();
        let mut writes = Vec::with_capacity(intents.len());
        let mut edits = Vec::with_capacity(intents.len());
        let mut transient_carry = Vec::with_capacity(intents.len());
        for ((object, attachment), intent) in intents {
            edits.push((
                object,
                attachment,
                intent.intensity.is_some() || intent.authored_intensity_track,
            ));
            // A new ordinary intensity track supplies its own initial sample,
            // including explicit reversal. Never overwrite that with old pulse data.
            if intent.authored_intensity_track {
                continue;
            }
            let Some(glow) = self
                .runtime
                .effective_object(object)
                .and_then(|row| row.glow.as_deref())
                .filter(|glow| glow.attachment == attachment)
            else {
                continue;
            };
            writes.push(EffectivePropertyWrite::GlowIntensity {
                object,
                attachment,
                intensity: intent
                    .intensity
                    .unwrap_or_else(|| glow.definition.intensity()),
            });
            if !self.runtime.object_has_effective_driver(object)
                && !original_carry.contains(&object)
            {
                transient_carry.push(object);
            }
        }
        if !writes.is_empty() {
            runtime.effective = Some(
                self.runtime
                    .extend_effective_property_batch(runtime.effective.take(), &writes)
                    .map_err(ExecutionSessionPublicationError::Runtime)?,
            );
        }
        Ok(PreparedTransientGlowEdits {
            edits,
            transient_carry,
        })
    }

    pub(super) fn commit_transient_glow_edits(&mut self, prepared: PreparedTransientGlowEdits) {
        for (object, attachment, supersede) in prepared.edits {
            self.runtime
                .commit_transient_glow_edit(object, attachment, true, supersede);
        }
        self.runtime
            .release_transient_effective_carry(&prepared.transient_carry);
    }
}

#[cfg(test)]
mod tests;
