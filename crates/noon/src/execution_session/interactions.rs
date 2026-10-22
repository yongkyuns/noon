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
        wall_time_seconds: f64,
    ) -> Result<(), ExecutionSessionInputError> {
        self.runtime
            .advance_interactions(wall_time_seconds)
            .map_err(ExecutionSessionInputError::Interaction)
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
            SemanticClickAction::Playback(control) => {
                Ok(Some(PreparedClickAction::Playback(control)))
            }
        }
    }
}

#[cfg(test)]
mod playback_tests;
#[cfg(test)]
mod tests;
