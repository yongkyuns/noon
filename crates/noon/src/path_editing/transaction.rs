//! Validated, ephemeral multi-path replacement prepared before resource admission.
use super::*;
use noon_core::{SemanticNodeId, StoredGeometry};

pub(crate) struct PreparedPathEdits {
    states: Vec<(SemanticNodeId, SemanticObjectState, SemanticObjectState)>,
    paths: Vec<VectorPath>,
}

impl PreparedPathEdits {
    pub(crate) fn prepare(
        store: &SemanticStore,
        replacements: impl IntoIterator<Item = (SemanticNodeId, SemanticObjectState, VectorPath)>,
    ) -> Result<Self, AuthoringError> {
        let mut result = Self {
            states: Vec::new(),
            paths: Vec::new(),
        };
        for (node, captured, path) in replacements {
            let before = store.semantic_object_state_checked(node)?;
            let after = path_replacement_state(captured)?;
            if path_is_unchanged(store, before, &after, &path) {
                continue;
            }
            result.states.push((node, before.clone(), after));
            result.paths.push(path);
        }
        Ok(result)
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
            let mut transaction = SemanticMutationTransaction::new();
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
