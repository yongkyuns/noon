use super::{
    SemanticNode, SemanticNodeId, SemanticNodeKind, SemanticObjectState, SemanticStore,
    SemanticStoreError,
};

/// Failure from a shared Semantic Scene authoring operation.
///
/// Target operations deliberately reject migration-only legacy objects and the
/// temporary state-less frontend identity seam instead of allowing them to become
/// peer semantic authorities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticSceneOperationError {
    UnknownNode(SemanticNodeId),
    NotSemanticObject(SemanticNodeId),
    NotSemanticFamily(SemanticNodeId),
    NotSemanticAuthoringNode(SemanticNodeId),
    /// One local scene restructure would promote the same aliased node from
    /// multiple attached roots. Until the scene store exposes a local root-order
    /// comparison primitive, fail before commit rather than scan unrelated roots.
    AmbiguousCrossRootAlias(SemanticNodeId),
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
            Self::NotSemanticFamily(id) => write!(
                formatter,
                "semantic node {}:{} is not a target semantic family",
                id.slot(),
                id.generation()
            ),
            Self::NotSemanticAuthoringNode(id) => write!(
                formatter,
                "semantic node {}:{} is not a target semantic object or family",
                id.slot(),
                id.generation()
            ),
            Self::AmbiguousCrossRootAlias(id) => write!(
                formatter,
                "semantic node {}:{} would be promoted from multiple scene roots",
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

    fn semantic_family_checked(
        &self,
        id: SemanticNodeId,
    ) -> Result<&SemanticNode, SemanticSceneOperationError> {
        let node = self
            .node(id)
            .ok_or(SemanticSceneOperationError::UnknownNode(id))?;
        if !matches!(node.kind(), SemanticNodeKind::Family) {
            return Err(SemanticSceneOperationError::NotSemanticFamily(id));
        }
        Ok(node)
    }

    fn semantic_authoring_node_checked(
        &self,
        id: SemanticNodeId,
    ) -> Result<&SemanticNode, SemanticSceneOperationError> {
        let node = self
            .node(id)
            .ok_or(SemanticSceneOperationError::UnknownNode(id))?;
        let is_target = match node.kind() {
            SemanticNodeKind::Family => true,
            SemanticNodeKind::AuthoringObject => node.semantic_object_state().is_some(),
            SemanticNodeKind::Object(_)
            | SemanticNodeKind::Signal(_)
            | SemanticNodeKind::Animation(_) => false,
        };
        if !is_target {
            return Err(SemanticSceneOperationError::NotSemanticAuthoringNode(id));
        }
        Ok(node)
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
    /// Authored content/transform/style/z-index and updater declarations are copied.
    /// Scene membership, source identity, and family relationships are intentionally
    /// not copied. The store assigns both a fresh generational NodeId and a fresh
    /// stable painter insertion-order tie break.
    pub fn copy_semantic_object(
        &mut self,
        id: SemanticNodeId,
    ) -> Result<SemanticNodeId, SemanticSceneOperationError> {
        let state = self.semantic_object_state_checked(id)?.clone();
        let updaters = self
            .node(id)
            .expect("semantic object identity validated above")
            .host_updaters()
            .to_vec();
        let copy = self.insert_semantic_object(state);
        self.node_mut(copy)
            .expect("newly inserted semantic copy exists")
            .host_updaters_mut()
            .extend(updaters);
        Ok(copy)
    }

    /// Snapshot one target semantic family's direct members in authoritative order.
    ///
    /// This is a query allocation only; the mutable source of truth remains the
    /// store's ordered family links.
    pub fn semantic_family_members_checked(
        &self,
        family: SemanticNodeId,
    ) -> Result<Vec<SemanticNodeId>, SemanticSceneOperationError> {
        Ok(self.semantic_family_checked(family)?.members())
    }

    /// Append one target semantic object or family to a target family.
    ///
    /// Multi-parent aliasing remains valid. Duplicate membership is idempotent,
    /// and cycle detection/order/locality are delegated to the authoritative store
    /// primitive rather than repeated in a frontend facade.
    pub fn add_semantic_family_member(
        &mut self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<(), SemanticSceneOperationError> {
        self.semantic_family_checked(family)?;
        self.semantic_authoring_node_checked(member)?;
        self.add_member(family, member).map_err(Into::into)
    }

    /// Remove one direct target semantic family edge.
    ///
    /// Removing an alias from one family does not affect any other parent, scene
    /// residency, source identity, or node-owned object state.
    pub fn remove_semantic_family_member(
        &mut self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<bool, SemanticSceneOperationError> {
        self.semantic_family_checked(family)?;
        self.semantic_authoring_node_checked(member)?;
        self.remove_member(family, member).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HostCallbackId, SemanticMutationStats, SemanticMutationTransaction, SemanticNodeResidency,
        SourceIdentity, StoredGeometry,
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
        let callback = HostCallbackId::new(42);
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(source, callback, 0.0, None);
        transaction.apply(&mut store).unwrap();
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
        assert_ne!(
            copied_state.insertion_order(),
            source_state.insertion_order()
        );
        assert_eq!(
            store.semantic_updater_registrations(copy).unwrap(),
            store.semantic_updater_registrations(source).unwrap()
        );
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

    #[test]
    fn shared_family_membership_preserves_order_and_aliases() {
        let mut store = SemanticStore::new();
        let first = store.insert_semantic_object(state(1.0));
        let second = store.insert_semantic_object(state(2.0));
        let primary = store.insert_family();
        let alias = store.insert_family();

        store.add_semantic_family_member(primary, first).unwrap();
        store.add_semantic_family_member(primary, second).unwrap();
        store.add_semantic_family_member(alias, first).unwrap();

        assert_eq!(
            store.semantic_family_members_checked(primary).unwrap(),
            vec![first, second]
        );
        assert_eq!(
            store.semantic_family_members_checked(alias).unwrap(),
            vec![first]
        );
        assert_eq!(store.node(first).unwrap().parents(), &[primary, alias]);

        store.add_semantic_family_member(primary, first).unwrap();
        assert_eq!(
            store.last_mutation_stats(),
            SemanticMutationStats::default()
        );

        assert!(store.remove_semantic_family_member(primary, first).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 2);
        assert_eq!(
            store.semantic_family_members_checked(primary).unwrap(),
            vec![second]
        );
        assert_eq!(store.node(first).unwrap().parents(), &[alias]);
        assert!(!store.remove_semantic_family_member(primary, first).unwrap());
        assert_eq!(
            store.last_mutation_stats(),
            SemanticMutationStats::default()
        );
    }

    #[test]
    fn shared_family_membership_rejects_state_less_authoring_nodes() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let identity_only = store.insert_authoring_object();

        assert_eq!(
            store.add_semantic_family_member(family, identity_only),
            Err(SemanticSceneOperationError::NotSemanticAuthoringNode(
                identity_only
            ))
        );
        assert_eq!(
            store.remove_semantic_family_member(family, identity_only),
            Err(SemanticSceneOperationError::NotSemanticAuthoringNode(
                identity_only
            ))
        );
        assert!(store
            .semantic_family_members_checked(family)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn shared_family_operations_validate_family_identity_and_generation() {
        let mut store = SemanticStore::new();
        let object = store.insert_semantic_object(state(1.0));
        assert_eq!(
            store.semantic_family_members_checked(object),
            Err(SemanticSceneOperationError::NotSemanticFamily(object))
        );

        let stale_family = store.insert_family();
        store.remove_node(stale_family).unwrap();
        let replacement = store.insert_family();
        assert_eq!(stale_family.slot(), replacement.slot());
        assert_ne!(stale_family.generation(), replacement.generation());
        assert_eq!(
            store.add_semantic_family_member(stale_family, object),
            Err(SemanticSceneOperationError::UnknownNode(stale_family))
        );
    }

    #[test]
    fn shared_family_operations_preserve_cycle_rejection() {
        let mut store = SemanticStore::new();
        let outer = store.insert_family();
        let inner = store.insert_family();

        store.add_semantic_family_member(outer, inner).unwrap();
        assert_eq!(
            store.add_semantic_family_member(inner, outer),
            Err(SemanticSceneOperationError::Store(
                SemanticStoreError::FamilyCycle {
                    family: inner,
                    member: outer,
                }
            ))
        );
        assert_eq!(
            store.semantic_family_members_checked(outer).unwrap(),
            vec![inner]
        );
        assert!(store
            .semantic_family_members_checked(inner)
            .unwrap()
            .is_empty());
    }
}
