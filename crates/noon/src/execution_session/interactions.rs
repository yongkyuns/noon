//! Source-declared click actions projected into the existing session input lane.
use super::{ExecutionSession, ExecutionSessionInputError, PointerSelectionClick};
use noon_core::{
    PlaybackControl, SemanticClickAction, SemanticMutationImpact, SemanticNodeId, SemanticStore,
};
use noon_runtime::PreparedTransientAnimation;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub(super) struct InteractionBindings {
    bindings: BTreeMap<SemanticNodeId, SemanticClickAction>,
    playback_count: usize,
}

impl InteractionBindings {
    pub(super) fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    fn insert(&mut self, node: SemanticNodeId, binding: SemanticClickAction) {
        let previous = self.bindings.insert(node, binding);
        self.playback_count -=
            usize::from(matches!(previous, Some(SemanticClickAction::Playback(_))));
        self.playback_count += usize::from(matches!(binding, SemanticClickAction::Playback(_)));
    }

    fn remove(&mut self, node: SemanticNodeId) {
        let previous = self.bindings.remove(&node);
        self.playback_count -=
            usize::from(matches!(previous, Some(SemanticClickAction::Playback(_))));
    }
}

pub(super) enum PreparedClickAction {
    Animation(PreparedTransientAnimation),
    GlowPulse {
        target: SemanticNodeId,
        binding: noon_core::SemanticClickGlowPulse,
    },
    Playback(PlaybackControl),
}

impl ExecutionSession {
    /// Whether this immutable initial plan declares scene playback controls.
    /// The count is maintained with the existing local binding projection.
    pub fn has_playback_controls(&self) -> bool {
        self.interaction_bindings.playback_count != 0
    }

    pub fn interactions_active(&self) -> bool {
        self.runtime.interactions_active()
    }

    /// Advance only input-triggered effective animation using platform monotonic seconds.
    pub fn advance_interactions(
        &mut self,
        store: &mut SemanticStore,
        wall_time_seconds: f64,
    ) -> Result<(), ExecutionSessionInputError> {
        let prepared = self
            .runtime
            .prepare_interactions(wall_time_seconds)
            .map_err(ExecutionSessionInputError::Interaction)?;
        self.publish_interaction_advance(store, prepared)
    }

    pub(super) fn retire_interactions(
        &mut self,
        store: &mut SemanticStore,
    ) -> Result<(), ExecutionSessionInputError> {
        let prepared = self
            .runtime
            .prepare_interaction_retirement()
            .map_err(ExecutionSessionInputError::Interaction)?;
        self.publish_interaction_advance(store, prepared)
    }

    /// Retire input-owned state before a platform replaces this source owner.
    /// This does not advance authored time or cancel authored animations. A
    /// recovery handoff that retains this same execution must not call it.
    pub fn retire_interactions_for_source_replacement(
        &mut self,
        store: &mut SemanticStore,
    ) -> Result<(), ExecutionSessionInputError> {
        if store.identity() != self.store_identity {
            return Err(super::ExecutionSessionPublicationError::ForeignSemanticStore.into());
        }
        if self.pending_callback_token().is_some() {
            return Err(ExecutionSessionInputError::RequiredCallbackPending);
        }
        // A dormant source has no input-owned semantic state to retire. Direct
        // authoring may already have made that returned execution obsolete.
        if !self.interactions_active() {
            return Ok(());
        }
        self.retire_interactions(store)
    }

    fn publish_interaction_advance(
        &mut self,
        store: &mut SemanticStore,
        prepared: noon_runtime::PreparedInteractionAdvance,
    ) -> Result<(), ExecutionSessionInputError> {
        self.require_published_store(store)?;
        if self.interactions_active() && self.pending_callback_token().is_some() {
            return Err(ExecutionSessionInputError::RequiredCallbackPending);
        }
        self.runtime
            .preflight_interaction_advance(&prepared)
            .map_err(ExecutionSessionInputError::Interaction)?;
        if prepared.retirements().is_empty() {
            return self
                .runtime
                .commit_prepared_interactions(prepared)
                .map_err(ExecutionSessionInputError::Interaction);
        }
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        for retirement in prepared.retirements() {
            // Deletion or replacement wins. Never resolve by name and remove a
            // newer generation that inherited the same canonical name.
            let Ok(effect) = store.semantic_effect_state(retirement.attachment) else {
                continue;
            };
            if self.execution_object_id(effect.owner()) == Some(retirement.object)
                && effect.name() == "glow"
                && store.effect_by_name(effect.owner(), "glow").ok().flatten()
                    == Some(retirement.attachment)
            {
                transaction.remove_node(retirement.attachment);
            }
        }
        let semantic = transaction
            .prepare(store)
            .map_err(super::ExecutionSessionPublicationError::Semantic)?;
        let (effective, metadata) = prepared.into_publication_parts();
        self.apply_prepared_semantic_transaction_with_execution_contract(
            semantic,
            Vec::new(),
            Some(effective).into(),
            super::publication::SemanticPublicationPurpose::TransientGlowLifecycle,
            None,
            None,
        )?;
        if let Some(pending) = self.pending_segment_completion.as_mut() {
            pending.activation_scene_revision = store.scene_revision();
        }
        self.runtime
            .finish_interaction_advance_after_publication(metadata);
        Ok(())
    }

    /// Prove the semantic half of the narrow lifecycle publication. The existing
    /// compiler and Runtime separately validate the resulting execution patches.
    pub(super) fn validate_transient_glow_lifecycle(
        &self,
        prepared: &noon_core::PreparedSemanticMutationTransaction<'_>,
    ) -> Result<(), super::ExecutionSessionPublicationError> {
        use noon_core::{EffectDefinition, SemanticMutation, SemanticNodeCreation};
        let neutral = noon_core::Glow::new(noon_core::GlowUpdate::default().intensity(0.0))
            .expect("neutral glow is valid");
        for mutation in prepared.mutations() {
            let valid = match mutation {
                SemanticMutation::AddNode {
                    creation:
                        SemanticNodeCreation::Effect {
                            owner,
                            name,
                            definition,
                            copy_provenance,
                        },
                    ..
                } => {
                    let owner = owner.existing();
                    name.as_ref() == "glow"
                        && *definition == EffectDefinition::Glow(neutral)
                        && copy_provenance.is_none()
                        && owner.is_some_and(|owner| {
                            prepared.store().effect_by_name(owner, "glow").ok() == Some(None)
                                && self.execution_object_id(owner).is_some_and(|object| {
                                    self.runtime.can_prepare_click_glow_pulse(object)
                                })
                        })
                        && self
                            .pending_segment_completion
                            .as_ref()
                            .is_none_or(|pending| {
                                pending.kind.lifecycle_root.is_none()
                                    && pending.kind.lifecycle_removals.is_empty()
                                    && pending.kind.family_transform.is_none()
                            })
                }
                SemanticMutation::RemoveNode { node } => {
                    node.existing().is_some_and(|attachment| {
                        prepared
                            .store()
                            .semantic_effect_state(attachment)
                            .ok()
                            .is_some_and(|effect| {
                                effect.name() == "glow"
                                    && self.execution_object_id(effect.owner()).is_some_and(
                                        |object| {
                                            self.runtime.transient_glow_attachment(object)
                                                == Some((attachment, true))
                                        },
                                    )
                            })
                    })
                }
                _ => false,
            };
            if !valid {
                return Err(super::ExecutionSessionPublicationError::Interaction(
                    "input glow lifecycle requires an exact neutral enrollment or owned temporary retirement".into(),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn initialize_interaction_bindings(&mut self, store: &SemanticStore) {
        let nodes: Vec<_> = self
            .runtime
            .painter_order()
            .iter()
            .filter_map(|&index| {
                self.execution_index
                    .semantic_object_id(self.frame().objects[index as usize].id)
            })
            .collect();
        for node in nodes {
            self.refresh_interaction_binding(store, node);
        }
    }

    fn refresh_interaction_binding(&mut self, store: &SemanticStore, node: SemanticNodeId) {
        let binding = store
            .semantic_object_state_checked(node)
            .ok()
            .filter(|state| state.signal_bindings().is_empty())
            .and_then(|state| state.click_action());
        if let Some(binding) = binding.filter(|_| self.semantic_object_is_reachable(node)) {
            self.interaction_bindings.insert(node, binding);
        } else {
            self.interaction_bindings.remove(node);
        }
    }

    pub(super) fn refresh_interaction_bindings(
        &mut self,
        store: &SemanticStore,
        result: &noon_core::SemanticMutationTransactionResult,
        entered: &[noon_core::ObjectId],
        exited: &[noon_core::ObjectId],
    ) {
        for &object in exited {
            if let Some(node) = self.execution_index.semantic_object_id(object) {
                self.interaction_bindings.remove(node);
            }
        }
        for &object in entered {
            if let Some(node) = self.execution_index.semantic_object_id(object) {
                self.refresh_interaction_binding(store, node);
            }
        }
        for impact in result.impacts() {
            match *impact {
                SemanticMutationImpact::ClickAction { object }
                | SemanticMutationImpact::ObjectContent { object }
                | SemanticMutationImpact::Subscription { object, .. } => {
                    self.refresh_interaction_binding(store, object)
                }
                SemanticMutationImpact::NodeRemoved { node } => {
                    self.interaction_bindings.remove(node);
                }
                _ => {}
            }
        }
    }

    pub(super) fn prepare_click_action(
        &self,
        click: Option<PointerSelectionClick>,
    ) -> Result<Option<PreparedClickAction>, ExecutionSessionInputError> {
        let Some(node) = click.and_then(|click| click.target()) else {
            return Ok(None);
        };
        let Some(binding) = self.interaction_bindings.bindings.get(&node) else {
            return Ok(None);
        };
        // Keep the current conservative callback/effective-driver admission.
        // Indicate also delegates channel arbitration to the ordinary Runtime.
        if self.pending_callback.is_some() {
            return Ok(None);
        }
        let Some(object) = self.execution_object_id(node) else {
            return Ok(None);
        };
        if self.runtime.object_has_effective_driver(object) {
            return Ok(None);
        }
        match *binding {
            SemanticClickAction::Indicate(indication) => self
                .runtime
                .prepare_click_indicate(object, indication)
                .map(|animation| animation.map(PreparedClickAction::Animation))
                .map_err(ExecutionSessionInputError::Interaction),
            SemanticClickAction::GlowPulse(binding) => {
                if !self.runtime.can_prepare_click_glow_pulse(object) {
                    return Ok(None);
                }
                Ok(Some(PreparedClickAction::GlowPulse {
                    target: node,
                    binding,
                }))
            }
            SemanticClickAction::Playback(control) => {
                Ok(Some(PreparedClickAction::Playback(control)))
            }
        }
    }
}

#[cfg(test)]
mod glow_pulse_tests;
#[cfg(test)]
mod playback_tests;
#[cfg(test)]
mod tests;
