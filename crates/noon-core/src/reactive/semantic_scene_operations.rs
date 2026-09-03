use super::{SemanticNodeId, SemanticObjectState, SemanticStore, SemanticStoreError};

/// Failure from a shared Semantic Scene object operation.
///
/// These operations accept only target semantic objects: legacy compatibility
/// objects, families, and the temporary state-less frontend identity seam are not
/// valid substitutes for a node-owned [`SemanticObjectState`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticSceneOperationError {
    UnknownNode(SemanticNodeId),
    NotSemanticObject(SemanticNodeId),
    Store(SemanticStoreError),
}

impl std::fmt::Display for SemanticSceneOperationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownNode(id) => write!(
                formatter,
                "unknown semantic node {}:{}",
                id.slot(),
                id.generation()
            ),
            Self::NotSemanticObject(id) => write!(
                formatter,
                "semantic node {}:{} does not own target semantic object state",
                id.slot(),
                id.generation()
            ),
            Self::Store(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SemanticSceneOperationError {}

impl From<SemanticStoreError> for SemanticSceneOperationError {
    fn from(value: SemanticStoreError) -> Self {
        match value {
            SemanticStoreError::UnknownNode(id) => Self::UnknownNode(id),
            other => Self::Store(other),
        }
    }
}

impl SemanticStore {
    /// Return the node-owned authored state after validating target-object identity.
    pub fn semantic_object_state_checked(
        &self,
        id: SemanticNodeId,
    ) -> Result<&SemanticObjectState, SemanticSceneOperationError> {
        let node = self
            .node(id)
            .ok_or(SemanticSceneOperationError::UnknownNode(id))?;
        node.semantic_object_state()
            .ok_or(SemanticSceneOperationError::NotSemanticObject(id))
    }

    /// Add a target semantic object to authored top-level scene membership.
    ///
    /// This delegates ordering and locality to the authoritative store primitive:
    /// first attach appends to authored root order and a repeated attach is a no-op.
    pub fn attach_semantic_object(
        &mut self,
        id: SemanticNodeId,
    ) -> Result<bool, SemanticSceneOperationError> {
        self.semantic_object_state_checked(id)?;
        self.attach_to_scene(id).map_err(Into::into)
    }

    /// Remove a target semantic object from top-level scene membership without
    /// deleting it. Semantic identity, state, source identity, and family links
    /// remain valid; a repeated detach is a no-op.
    pub fn detach_semantic_object(
        &mut self,
        id: SemanticNodeId,
    ) -> Result<bool, SemanticSceneOperationError> {
        self.semantic_object_state_checked(id)?;
        self.detach_from_scene(id).map_err(Into::into)
    }

    /// Copy one target semantic object into a fresh detached semantic node.
    ///
    /// Authored content/transform/style/z-index are copied. Scene membership,
    /// source identity, and family relationships are intentionally not copied.
    /// The store assigns both a fresh generational NodeId and a fresh stable
    /// painter insertion-order tie break.
    pub fn copy_semantic_object(
        &mut self,
        id: SemanticNodeId,
    ) -> Result<SemanticNodeId, SemanticSceneOperationError> {
        let state = self.semantic_object_state_checked(id)?.clone();
        Ok(self.insert_semantic_object(state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SemanticMutationStats, SemanticNodeResidency, SourceIdentity, StoredGeometry,
    };

    fn state(radius: f32) -> SemanticObjectState {
        SemanticObjectState::new(StoredGeometry::Circle { radius })
    }

    #[test]
    fn semantic_object_lifecycle_delegates_to_authoritative_membership() {
        let mut store = SemanticStore::new();
        let object = store.insert_semantic_object(state(1.0));

        assert_eq!(
            store.node(object).unwrap().residency(),
            SemanticNodeResidency::Detached
        );
        assert!(store.attach_semantic_object(object).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(store.scene_roots().collect::<Vec<_>>(), vec![object]);

        assert!(!store.attach_semantic_object(object).unwrap());
        assert_eq!(
            store.last_mutation_stats(),
            SemanticMutationStats::default()
        );

        assert!(store.detach_semantic_object(object).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(
            store.node(object).unwrap().residency(),
            SemanticNodeResidency::Detached
        );
        assert!(store.scene_roots().next().is_none());

        assert!(!store.detach_semantic_object(object).unwrap());
        assert_eq!(
            store.last_mutation_stats(),
            SemanticMutationStats::default()
        );
        assert!(store.semantic_object_state_checked(object).is_ok());
    }

    #[test]
    fn lifecycle_rejects_nodes_without_target_semantic_state() {
        let mut store = SemanticStore::new();
        let identity_only = store.insert_authoring_object();
        let family = store.insert_family();

        for id in [identity_only, family] {
            assert_eq!(
                store.attach_semantic_object(id),
                Err(SemanticSceneOperationError::NotSemanticObject(id))
            );
            assert_eq!(
                store.detach_semantic_object(id),
                Err(SemanticSceneOperationError::NotSemanticObject(id))
            );
        }
        assert_eq!(store.scene_root_count(), 0);
    }

    #[test]
    fn stale_generation_fails_shared_object_operations() {
        let mut store = SemanticStore::new();
        let stale = store.insert_semantic_object(state(1.0));
        store.remove_node(stale).unwrap();
        let replacement = store.insert_semantic_object(state(2.0));

        assert_eq!(stale.slot(), replacement.slot());
        assert_ne!(stale.generation(), replacement.generation());
        assert_eq!(
            store.attach_semantic_object(stale),
            Err(SemanticSceneOperationError::UnknownNode(stale))
        );
        assert_eq!(
            store.copy_semantic_object(stale),
            Err(SemanticSceneOperationError::UnknownNode(stale))
        );
    }

    #[test]
    fn copy_gets_fresh_identity_and_order_without_copying_relationships() {
        let mut store = SemanticStore::new();
        let mut authored = state(2.0);
        authored.transform.translation.x = 4.5;
        authored.style.object_opacity = 0.4;
        authored.set_z_index(7);
        let source = store.insert_semantic_object(authored);
        let family = store.insert_family();
        store.add_member(family, source).unwrap();
        let source_identity = SourceIdentity::ExplicitKey("hero".into());
        store
            .set_source_identity(source, Some(source_identity.clone()))
            .unwrap();
        store.attach_semantic_object(source).unwrap();

        let source_state = store.semantic_object_state_checked(source).unwrap().clone();
        let copy = store.copy_semantic_object(source).unwrap();
        let copied_state = store.semantic_object_state_checked(copy).unwrap();

        assert_ne!(copy, source);
        assert_eq!(copied_state.content, source_state.content);
        assert_eq!(copied_state.transform, source_state.transform);
        assert_eq!(copied_state.style, source_state.style);
        assert_eq!(copied_state.z_index(), source_state.z_index());
        assert_ne!(copied_state.insertion_order(), source_state.insertion_order());
        assert_eq!(
            store.node(copy).unwrap().residency(),
            SemanticNodeResidency::Detached
        );
        assert!(store.node(copy).unwrap().parents().is_empty());
        assert!(store.node(copy).unwrap().source_identity().is_none());

        assert_eq!(
            store.node(source).unwrap().residency(),
            SemanticNodeResidency::SceneOwned
        );
        assert_eq!(store.node(source).unwrap().parents(), &[family]);
        assert_eq!(
            store.node(source).unwrap().source_identity(),
            Some(&source_identity)
        );
    }
}
