//! Validated, ephemeral multi-path replacement prepared before resource admission.
use super::*;
use noon_core::{SemanticNodeId, StoredGeometry};

pub(crate) struct PreparedPathEdits {
    states: Vec<(SemanticNodeId, SemanticObjectState, SemanticObjectState)>,
    paths: Vec<VectorPath>,
    property_edits: Vec<(SemanticNodeId, SemanticObjectState, SemanticObjectState)>,
    initial_families: Vec<SemanticNodeId>,
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
            initial_families: Vec::new(),
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

    /// Prepare paths that were edited in local coordinates and keep their
    /// captured object transforms intact. Family `Mobject.reverse_points`
    /// reverses point order without changing the affine state of each member.
    pub(crate) fn prepare_preserving_transform(
        store: &SemanticStore,
        replacements: impl IntoIterator<Item = (SemanticNodeId, SemanticObjectState, VectorPath)>,
    ) -> Result<Self, AuthoringError> {
        let mut result = Self {
            states: Vec::new(),
            paths: Vec::new(),
            property_edits: Vec::new(),
            initial_families: Vec::new(),
            transaction: SemanticMutationTransaction::new(),
        };
        for (node, after, path) in replacements {
            let before = store.semantic_object_state_checked(node)?;
            if path_is_unchanged(store, before, &after, &path) {
                let mut after = after;
                after.content = before.content;
                result.property_edits.push((node, before.clone(), after));
            } else {
                result.states.push((node, before.clone(), after));
                result.paths.push(path);
            }
        }
        Ok(result)
    }

    pub(crate) fn with_transaction(mut self, transaction: SemanticMutationTransaction) -> Self {
        self.transaction = transaction;
        self
    }

    /// Admit first root paths in the same resource/publication transaction as
    /// existing-object edits. Core materialization retains root presentation.
    pub(crate) fn with_initial_family_paths(
        mut self,
        store: &SemanticStore,
        paths: impl IntoIterator<Item = (SemanticNodeId, VectorPath)>,
    ) -> Result<Self, AuthoringError> {
        for (node, path) in paths {
            let family = store.semantic_family_checked(node)?;
            if family.semantic_object_state().is_some() {
                return Err(crate::UnsupportedAuthoringOperation::PathEditContent.into());
            }
            if !path.is_finite() {
                return Err(AuthoringError::PathQuery(
                    noon_geometry::PathProportionError::InvalidMetric,
                ));
            }
            self.initial_families.push(node);
            self.paths.push(path);
        }
        Ok(self)
    }

    pub(crate) fn creates_resources(&self) -> bool {
        !self.paths.is_empty()
    }

    pub(crate) fn has_changes(&self) -> bool {
        !self.states.is_empty()
            || !self.property_edits.is_empty()
            || !self.initial_families.is_empty()
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
            let (existing_handles, initial_handles) = handles.split_at(self.states.len());
            for (node, before, after) in self.property_edits {
                crate::semantic_mobject::stage_state_changes(
                    &mut transaction,
                    node,
                    &before,
                    &after,
                );
            }
            for ((node, before, mut after), handle) in self.states.into_iter().zip(existing_handles)
            {
                after.content = StoredGeometry::Resource(*handle).into();
                crate::semantic_mobject::stage_state_changes(
                    &mut transaction,
                    node,
                    &before,
                    &after,
                );
            }
            for (node, handle) in self.initial_families.into_iter().zip(initial_handles) {
                transaction.replace_content(node, StoredGeometry::Resource(*handle));
            }
            publish(store, transaction)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_path_preserves_resource_and_still_publishes_target_paint() {
        let mut scene = crate::Scene::new();
        let object = scene
            .path(
                VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(Vec2::new(2., 0.)),
                Default::default(),
            )
            .unwrap();
        let before = object.state().unwrap();
        let mut captured = before.clone();
        captured.style.object_opacity = 0.25;
        let store = scene.integration_store();
        let path = crate::path_editing::world_path(&store.borrow(), &before).unwrap();
        let count = store.borrow().geometry_resources().len();
        let prepared =
            PreparedPathEdits::prepare(&store.borrow(), [(object.node_id(), captured, path)])
                .unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_z_index(object.node_id(), 2.0);
        prepared
            .with_transaction(transaction)
            .publish(&mut store.borrow_mut(), |store, transaction| {
                transaction.apply(store).map_err(AuthoringError::from)
            })
            .unwrap();
        let after = object.state().unwrap();
        assert_eq!(after.content, before.content);
        assert_eq!(after.style.object_opacity, 0.25);
        assert_eq!(after.z_index(), 2.0);
        assert_eq!(store.borrow().geometry_resources().len(), count);
    }
}
