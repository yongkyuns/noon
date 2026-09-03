use super::{
    HostCallbackId, SemanticNodeId, SemanticNodeKind, SemanticSceneOperationError, SemanticStore,
};

impl SemanticStore {
    /// Return authored host-updater registrations for one target semantic node.
    ///
    /// Registration order is semantic authoring state. The host owns executable
    /// callable code keyed by [`HostCallbackId`]; Runtime scheduling is a lowering
    /// concern and is deliberately absent here.
    pub fn semantic_updater_callbacks(
        &self,
        target: SemanticNodeId,
    ) -> Result<&[HostCallbackId], SemanticSceneOperationError> {
        target_node_checked(self, target)?;
        Ok(self
            .node(target)
            .expect("semantic updater target validated above")
            .host_updaters())
    }

    /// Append one authored updater declaration to a semantic object or family.
    ///
    /// Reusing the same callback identity is intentional: a host callable may be
    /// registered more than once, and order remains observable authoring state.
    pub fn add_semantic_updater(
        &mut self,
        target: SemanticNodeId,
        callback: HostCallbackId,
    ) -> Result<(), SemanticSceneOperationError> {
        target_node_checked(self, target)?;
        self.node_mut(target)
            .expect("semantic updater target validated above")
            .host_updaters_mut()
            .push(callback);
        self.set_last_mutation_writes(1);
        Ok(())
    }

    /// Remove every authored registration of `callback` from one semantic target.
    ///
    /// This mirrors updater-list semantics without consulting Runtime or a host
    /// callable table. Work is proportional only to this target's updater degree.
    pub fn remove_semantic_updater(
        &mut self,
        target: SemanticNodeId,
        callback: HostCallbackId,
    ) -> Result<bool, SemanticSceneOperationError> {
        target_node_checked(self, target)?;
        let callbacks = self
            .node_mut(target)
            .expect("semantic updater target validated above")
            .host_updaters_mut();
        let before = callbacks.len();
        callbacks.retain(|candidate| *candidate != callback);
        let removed = callbacks.len() != before;
        self.set_last_mutation_writes(removed as usize);
        Ok(removed)
    }
}

fn target_node_checked(
    store: &SemanticStore,
    id: SemanticNodeId,
) -> Result<(), SemanticSceneOperationError> {
    let node = store
        .node(id)
        .ok_or(SemanticSceneOperationError::UnknownNode(id))?;
    let is_target = match node.kind() {
        SemanticNodeKind::Family => true,
        SemanticNodeKind::AuthoringObject => node.semantic_object_state().is_some(),
        SemanticNodeKind::Object(_) | SemanticNodeKind::Signal(_) => false,
    };
    if !is_target {
        return Err(SemanticSceneOperationError::NotSemanticAuthoringNode(id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticMutationStats, SemanticObjectState, StoredGeometry};

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    #[test]
    fn updater_declarations_are_ordered_scene_owned_authoring_state() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let first = HostCallbackId::new(7);
        let second = HostCallbackId::new(3);

        store.add_semantic_updater(target, first).unwrap();
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        store.add_semantic_updater(target, second).unwrap();
        store.add_semantic_updater(target, first).unwrap();

        assert_eq!(
            store.semantic_updater_callbacks(target).unwrap(),
            &[first, second, first]
        );
        assert_eq!(store.last_mutation_stats().slots_written, 1);
    }

    #[test]
    fn updater_declarations_survive_detach_and_readd() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let callback = HostCallbackId::new(11);
        store.add_semantic_updater(target, callback).unwrap();

        store.attach_semantic_object(target).unwrap();
        store.detach_semantic_object(target).unwrap();
        assert_eq!(
            store.semantic_updater_callbacks(target).unwrap(),
            &[callback]
        );
        store.attach_semantic_object(target).unwrap();
        assert_eq!(
            store.semantic_updater_callbacks(target).unwrap(),
            &[callback]
        );
    }

    #[test]
    fn semantic_families_can_own_updater_declarations() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let callback = HostCallbackId::new(4);

        store.add_semantic_updater(family, callback).unwrap();
        assert_eq!(
            store.semantic_updater_callbacks(family).unwrap(),
            &[callback]
        );
    }

    #[test]
    fn state_less_and_stale_targets_are_rejected_before_mutation() {
        let mut store = SemanticStore::new();
        let identity_only = store.insert_authoring_object();
        assert_eq!(
            store.add_semantic_updater(identity_only, HostCallbackId::new(1)),
            Err(SemanticSceneOperationError::NotSemanticAuthoringNode(
                identity_only
            ))
        );

        let stale = object(&mut store, 2.0);
        store.remove_node(stale).unwrap();
        assert_eq!(
            store.semantic_updater_callbacks(stale),
            Err(SemanticSceneOperationError::UnknownNode(stale))
        );
    }

    #[test]
    fn removing_updater_is_local_order_preserving_and_idempotent() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let first = HostCallbackId::new(1);
        let removed = HostCallbackId::new(2);
        let last = HostCallbackId::new(3);
        for callback in [first, removed, last, removed] {
            store.add_semantic_updater(target, callback).unwrap();
        }

        assert!(store.remove_semantic_updater(target, removed).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(
            store.semantic_updater_callbacks(target).unwrap(),
            &[first, last]
        );

        assert!(!store.remove_semantic_updater(target, removed).unwrap());
        assert_eq!(
            store.last_mutation_stats(),
            SemanticMutationStats::default()
        );
    }

    #[test]
    fn updater_mutation_does_not_touch_unrelated_semantic_nodes() {
        let mut store = SemanticStore::new();
        let targets = (0..10_000)
            .map(|index| object(&mut store, index as f32 + 1.0))
            .collect::<Vec<_>>();
        let target = targets[5_000];

        store
            .add_semantic_updater(target, HostCallbackId::new(99))
            .unwrap();
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert!(targets.iter().enumerate().all(
            |(index, id)| index == 5_000 || store.node(*id).unwrap().host_updaters().is_empty()
        ));
    }
}
