//! Source-declared click actions projected into the existing session input lane.
use super::{ExecutionSession, ExecutionSessionInputError, PointerSelectionClick};
use noon_core::{SemanticClickIndicate, SemanticMutationImpact, SemanticNodeId, SemanticStore};
use noon_runtime::PreparedTransientAnimation;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub(super) struct InteractionBindings(BTreeMap<SemanticNodeId, SemanticClickIndicate>);
impl InteractionBindings {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl ExecutionSession {
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
            .and_then(|state| state.click_indicate());
        if let Some(binding) = binding.filter(|_| self.semantic_object_is_reachable(node)) {
            self.interaction_bindings.0.insert(node, binding);
        } else {
            self.interaction_bindings.0.remove(&node);
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
                self.interaction_bindings.0.remove(&node);
            }
        }
        for &object in entered {
            if let Some(node) = self.execution_index.semantic_object_id(object) {
                self.refresh_interaction_binding(store, node);
            }
        }
        for impact in result.impacts() {
            match *impact {
                SemanticMutationImpact::ClickIndicate { object }
                | SemanticMutationImpact::ObjectContent { object }
                | SemanticMutationImpact::Subscription { object, .. } => {
                    self.refresh_interaction_binding(store, object)
                }
                SemanticMutationImpact::NodeRemoved { node } => {
                    self.interaction_bindings.0.remove(&node);
                }
                _ => {}
            }
        }
    }

    pub(super) fn prepare_click_animation(
        &self,
        click: Option<PointerSelectionClick>,
    ) -> Result<Option<PreparedTransientAnimation>, ExecutionSessionInputError> {
        let Some(node) = click.and_then(|click| click.target()) else {
            return Ok(None);
        };
        let Some(binding) = self.interaction_bindings.0.get(&node) else {
            return Ok(None);
        };
        // Runtime-owned channel arbitration admits disjoint playback and defers
        // an effect when an authored/native driver owns one of its channels.
        if self.pending_callback.is_some() {
            return Ok(None);
        }
        let Some(object) = self.execution_object_id(node) else {
            return Ok(None);
        };
        if self.runtime.object_has_effective_driver(object) {
            return Ok(None);
        }
        self.runtime
            .prepare_click_indicate(object, *binding)
            .map_err(ExecutionSessionInputError::Interaction)
    }
}

#[cfg(test)]
mod tests;
