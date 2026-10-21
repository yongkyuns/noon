//! Retained read views of the canonical direct-member container.
//!
//! In-place edits continue to use the store's rank index. Replacing a container
//! freezes its previous order only while a reader retains it. This is a derived
//! read snapshot, never another membership graph or semantic identity space.
use super::{SemanticNodeId, SemanticStore, SemanticStoreError, SemanticStoreIdentity};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

#[derive(Debug, Default)]
pub(super) struct MemberContainer {
    readers: AtomicUsize,
    frozen: Mutex<Option<Vec<SemanticNodeId>>>,
}

impl MemberContainer {
    pub(super) fn is_observed(&self) -> bool {
        self.readers.load(Ordering::Acquire) != 0
    }

    fn retain(&self) {
        self.readers
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .expect("member view reader count fits address space");
    }
}

/// A view cannot be read through a different or independently cloned store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticMemberViewError {
    ForeignStore,
    Store(SemanticStoreError),
}
impl std::fmt::Display for SemanticMemberViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "semantic member view read failed: {self:?}")
    }
}
impl std::error::Error for SemanticMemberViewError {}
impl From<SemanticStoreError> for SemanticMemberViewError {
    fn from(error: SemanticStoreError) -> Self {
        Self::Store(error)
    }
}

/// Retain a direct-member list's lifetime without copying its members at capture.
/// Removal, insertion and reorder remain visible until explicit replacement.
/// A replaced view retains member identities; it does not freeze member values.
#[derive(Debug)]
pub struct SemanticMemberView {
    store: SemanticStoreIdentity,
    family: SemanticNodeId,
    container: Arc<MemberContainer>,
}
impl Clone for SemanticMemberView {
    fn clone(&self) -> Self {
        self.container.retain();
        Self {
            store: self.store.clone(),
            family: self.family,
            container: Arc::clone(&self.container),
        }
    }
}
impl Drop for SemanticMemberView {
    fn drop(&mut self) {
        if self.container.readers.fetch_sub(1, Ordering::AcqRel) == 1 {
            // Also release a speculative freeze retained by a rolled-back edit.
            *self.container.frozen.lock().expect("member view lock") = None;
        }
    }
}
impl SemanticMemberView {
    fn current<'a>(
        &self,
        store: &'a SemanticStore,
    ) -> Result<&'a super::SemanticNode, SemanticMemberViewError> {
        if self.store != store.identity() {
            return Err(SemanticMemberViewError::ForeignStore);
        }
        let node = store
            .node(self.family)
            .ok_or(SemanticStoreError::UnknownNode(self.family))?;
        if !node.can_own_members() {
            return Err(SemanticStoreError::NotFamily(self.family).into());
        }
        Ok(node)
    }

    /// Current cardinality of the captured list, not the root's replacement list.
    pub fn len(&self, store: &SemanticStore) -> Result<usize, SemanticMemberViewError> {
        let node = self.current(store)?;
        if node
            .members
            .observation
            .as_ref()
            .is_some_and(|value| Arc::ptr_eq(value, &self.container))
        {
            return Ok(node.member_count());
        }
        Ok(self
            .container
            .frozen
            .lock()
            .expect("member view lock")
            .as_ref()
            .expect("replaced observed container retains its order")
            .len())
    }
    pub fn is_empty(&self, store: &SemanticStore) -> Result<bool, SemanticMemberViewError> {
        self.len(store).map(|len| len == 0)
    }

    /// Read a normalized list rank. A host iterator may retain its own cursor.
    pub fn member_at(
        &self,
        store: &SemanticStore,
        index: usize,
    ) -> Result<Option<SemanticNodeId>, SemanticMemberViewError> {
        let node = self.current(store)?;
        if node
            .members
            .observation
            .as_ref()
            .is_some_and(|value| Arc::ptr_eq(value, &self.container))
        {
            return Ok(node.members.member_at_rank(index));
        }
        Ok(self
            .container
            .frozen
            .lock()
            .expect("member view lock")
            .as_ref()
            .expect("replaced observed container retains its order")
            .get(index)
            .copied())
    }
}

impl SemanticStore {
    /// Capture an opaque list lifetime. Only derived observation metadata is
    /// initialized: no semantic node, revision, ordered snapshot or mutation work.
    pub fn member_view(
        &mut self,
        family: SemanticNodeId,
    ) -> Result<SemanticMemberView, SemanticStoreError> {
        let store = self.identity();
        let node = self
            .node_mut(family)
            .ok_or(SemanticStoreError::UnknownNode(family))?;
        if !node.can_own_members() {
            return Err(SemanticStoreError::NotFamily(family));
        }
        let container = node
            .members
            .observation
            .get_or_insert_with(|| Arc::new(MemberContainer::default()));
        container.retain();
        Ok(SemanticMemberView {
            store,
            family,
            container: Arc::clone(container),
        })
    }

    pub(super) fn member_container_is_observed(&self, family: SemanticNodeId) -> bool {
        self.node(family)
            .and_then(|node| node.members.observation.as_ref())
            .is_some_and(|view| view.is_observed())
    }

    /// Replace the logical list, retaining old order only for active readers.
    /// Ordinary transactions expose this together with the corresponding edges.
    pub fn replace_member_container(
        &mut self,
        family: SemanticNodeId,
    ) -> Result<(), SemanticStoreError> {
        let node = self
            .node_mut(family)
            .ok_or(SemanticStoreError::UnknownNode(family))?;
        if !node.can_own_members() {
            return Err(SemanticStoreError::NotFamily(family));
        }
        if let Some(container) = node.members.observation.take() {
            let mut frozen = container.frozen.lock().expect("member view lock");
            if container.is_observed() {
                *frozen = Some(node.members());
            } else {
                *frozen = None;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SemanticMutationTransaction;

    fn fixture() -> (SemanticStore, SemanticNodeId, [SemanticNodeId; 3]) {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let members = [
            store.insert_family(),
            store.insert_family(),
            store.insert_family(),
        ];
        for id in &members[..2] {
            store.add_member(family, *id).unwrap();
        }
        (store, family, members)
    }
    fn ids(view: &SemanticMemberView, store: &SemanticStore) -> Vec<SemanticNodeId> {
        (0..view.len(store).unwrap())
            .map(|i| view.member_at(store, i).unwrap().unwrap())
            .collect()
    }

    #[test]
    fn in_place_edits_are_live_but_replacement_freezes_the_previous_list() {
        let (mut store, family, [a, b, c]) = fixture();
        let revision = store.scene_revision();
        let nodes = store.len();
        let view = store.member_view(family).unwrap();
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.len(), nodes);
        assert!(view.container.frozen.lock().unwrap().is_none());
        store.remove_member(family, a).unwrap();
        assert_eq!(ids(&view, &store), [b]);
        store.add_member(family, a).unwrap(); // Explicit in-place native insertion.
        store.reorder_member(family, a, Some(b)).unwrap();
        assert_eq!(ids(&view, &store), [a, b]);
        let mut tx = SemanticMutationTransaction::new();
        tx.replace_member_container(family);
        tx.add_member(family, c);
        tx.apply(&mut store).unwrap();
        assert_eq!(ids(&view, &store), [a, b]);
        let current = store.member_view(family).unwrap();
        assert_eq!(ids(&current, &store), [a, b, c]);
        store.remove_member(family, a).unwrap();
        assert_eq!(ids(&current, &store), [b, c]);
        assert_eq!(ids(&view, &store), [a, b]);
        assert_eq!(store.len(), nodes);
    }

    #[test]
    fn no_edge_change_still_replaces_an_observed_list_and_readers_release_storage() {
        let (mut store, family, [a, b, _]) = fixture();
        let view = store.member_view(family).unwrap();
        let clone = view.clone();
        let container = Arc::clone(&view.container);
        let revision = store.scene_revision();
        let mut tx = SemanticMutationTransaction::new();
        tx.replace_member_container(family);
        let result = tx.apply(&mut store).unwrap();
        assert!(result.impacts().is_empty());
        assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
        store.remove_member(family, a).unwrap();
        assert_eq!(ids(&clone, &store), [a, b]);
        drop(view);
        assert!(container.frozen.lock().unwrap().is_some());
        drop(clone);
        assert!(container.frozen.lock().unwrap().is_none());
        let revision = store.scene_revision();
        let mut tx = SemanticMutationTransaction::new();
        tx.replace_member_container(family);
        tx.apply(&mut store).unwrap();
        assert_eq!(store.scene_revision(), revision);
    }

    #[test]
    fn rejected_and_discarded_transactions_do_not_replace_a_captured_list() {
        let (mut store, family, [a, b, _]) = fixture();
        let view = store.member_view(family).unwrap();
        let mut tx = SemanticMutationTransaction::new();
        tx.replace_member_container(family);
        tx.add_member(family, family);
        assert!(tx.apply(&mut store).is_err());
        let mut tx = SemanticMutationTransaction::new();
        tx.replace_member_container(family);
        drop(tx.prepare(&mut store).unwrap());
        assert!(view.container.frozen.lock().unwrap().is_none());
        store.remove_member(family, a).unwrap();
        assert_eq!(ids(&view, &store), [b]);
    }

    #[test]
    fn restored_container_reads_current_members_and_can_be_replaced_again() {
        let (mut store, family, [a, b, _]) = fixture();
        let view = store.member_view(family).unwrap();
        let saved = store.node(family).unwrap().members.clone();
        store.replace_member_container(family).unwrap();
        // A rollback restores the original container identity, not a frozen read.
        store.node_mut(family).unwrap().members = saved;
        store.remove_member(family, a).unwrap();
        assert_eq!(ids(&view, &store), [b]);
        store.replace_member_container(family).unwrap();
        store.remove_member(family, b).unwrap();
        assert_eq!(ids(&view, &store), [b]);
    }

    #[test]
    fn independent_store_clones_never_share_observation_lifetimes() {
        let (mut store, family, [a, b, _]) = fixture();
        let view = store.member_view(family).unwrap();
        let mut fork = store.clone();
        assert_eq!(view.len(&fork), Err(SemanticMemberViewError::ForeignStore));
        let fork_view = fork.member_view(family).unwrap();
        fork.replace_member_container(family).unwrap();
        fork.remove_member(family, a).unwrap();
        store.remove_member(family, b).unwrap();
        assert_eq!(ids(&view, &store), [a]);
        assert_eq!(ids(&fork_view, &fork), [a, b]);
        fn send_sync<T: Send + Sync>() {}
        send_sync::<SemanticMemberView>();
        send_sync::<SemanticStore>();
    }

    #[test]
    fn deleted_root_generation_cannot_be_read_through_a_retained_view() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let view = store.member_view(root).unwrap();
        let mut tx = SemanticMutationTransaction::new();
        tx.remove_node(root);
        tx.apply(&mut store).unwrap();
        let reused = store.insert_family();
        assert_eq!(root.slot(), reused.slot());
        assert_ne!(root, reused);
        assert_eq!(
            view.len(&store),
            Err(SemanticMemberViewError::Store(
                SemanticStoreError::UnknownNode(root)
            ))
        );
    }

    #[test]
    fn capture_and_unobserved_replacement_do_not_copy_large_membership() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        for _ in 0..4096 {
            let member = store.insert_family();
            store.add_member(family, member).unwrap();
        }
        let view = store.member_view(family).unwrap();
        assert!(view.container.frozen.lock().unwrap().is_none());
        assert_eq!(view.len(&store).unwrap(), 4096);
        assert!(view.member_at(&store, 4095).unwrap().is_some());
        drop(view);
        let revision = store.scene_revision();
        let mut tx = SemanticMutationTransaction::new();
        tx.replace_member_container(family);
        tx.apply(&mut store).unwrap();
        assert_eq!(store.scene_revision(), revision);
    }
}
