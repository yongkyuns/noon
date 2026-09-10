//! Validated, ephemeral multi-path replacement prepared before resource admission.
use super::*;
use noon_core::{SemanticNodeId, StoredGeometry};

pub(crate) struct PreparedPathEdits {
    states: Vec<(SemanticNodeId, SemanticObjectState, SemanticObjectState)>,
    paths: Vec<VectorPath>,
    property_edits: Vec<(SemanticNodeId, SemanticObjectState, SemanticObjectState)>,
    transaction: SemanticMutationTransaction,
}

impl PreparedPathEdits {
    pub(crate) fn prepare(
        store: &SemanticStore,
        replacements: impl IntoIterator<Item = (SemanticNodeId, SemanticObjectState, VectorPath)>,
    ) -> Result<Self, AuthoringError> {
        let mut result = Self {
            states: Vec::new(),
            paths: Vec::new(),
            property_edits: Vec::new(),
            transaction: SemanticMutationTransaction::new(),
        };
        for (node, captured, path) in replacements {
            let before = store.semantic_object_state_checked(node)?;
            let after = path_replacement_state(captured)?;
            if path_is_unchanged(store, before, &after, &path) {
                // The geometry can already match while become changes paint or
                // other presentation. Preserve the shared resource but publish
                // those state changes in the same atomic transaction.
                let mut after = after;
                after.content = before.content;
                result.property_edits.push((node, before.clone(), after));
                continue;
            }
            result.states.push((node, before.clone(), after));
            result.paths.push(path);
        }
        Ok(result)
    }

    pub(crate) fn with_transaction(mut self, transaction: SemanticMutationTransaction) -> Self {
        self.transaction = transaction;
        self
    }

    pub(crate) fn creates_resources(&self) -> bool {
        !self.paths.is_empty()
    }

    pub(crate) fn publish<T, E>(
        self,
        store: &mut SemanticStore,
        publish: impl FnOnce(&mut SemanticStore, SemanticMutationTransaction) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<noon_core::GeometryResourceError>,
    {
        store.with_geometry_paths(self.paths, |store, handles| {
            let mut transaction = self.transaction;
            for (node, before, after) in self.property_edits {
                crate::semantic_mobject::stage_state_changes(
                    &mut transaction,
                    node,
                    &before,
                    &after,
                );
            }
            for ((node, before, mut after), handle) in self.states.into_iter().zip(handles) {
                after.content = StoredGeometry::Resource(*handle).into();
                crate::semantic_mobject::stage_state_changes(
                    &mut transaction,
                    node,
                    &before,
                    &after,
                );
            }
            publish(store, transaction)
        })
    }
}
