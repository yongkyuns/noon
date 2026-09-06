use std::collections::{HashMap, HashSet};

use noon_core::{
    SemanticMutationImpact, SemanticMutationTransactionResult, SemanticNodeId, SemanticNodeKind,
    SemanticStore, SemanticStoreError,
};

use crate::SemanticLoweringError;

/// Compiler-owned reachability metadata for the semantic -> execution boundary.
///
/// This is not a second identity or slot domain. It records only the graph facts
/// needed to decide whether one semantic object currently participates in execution:
/// explicit scene-root residency, reachable parent families, and cached direct
/// family membership. Stable/tombstoned object storage remains owned by the existing
/// compiled/runtime slot machinery.
///
/// A reachable family contributes exactly one live-parent edge to each direct
/// member, regardless of how many rooted paths reach that family. This preserves
/// alias semantics without path-count growth: adding a second alias cannot activate
/// the same subtree twice, and losing one alias cannot retire it while another
/// reachable parent remains.
#[derive(Clone, Debug, Default)]
pub struct SemanticExecutionReachability {
    nodes: HashMap<SemanticNodeId, ReachabilityNode>,
}

#[derive(Clone, Debug)]
struct ReachabilityNode {
    kind: ReachabilityKind,
    scene_root: bool,
    reachable_parents: HashSet<SemanticNodeId>,
}

impl ReachabilityNode {
    fn is_reachable(&self) -> bool {
        self.scene_root || !self.reachable_parents.is_empty()
    }

    fn is_reachable_object(&self) -> bool {
        matches!(self.kind, ReachabilityKind::Object) && self.is_reachable()
    }
}

#[derive(Clone, Debug)]
enum ReachabilityKind {
    Object,
    Family { members: HashSet<SemanticNodeId> },
}

/// Local execution-membership result produced after one committed semantic update.
///
/// This is derived compiler metadata, not an authored mutation vocabulary. Only net
/// object membership transitions are reported; transient changes inside one atomic
/// semantic transaction are intentionally suppressed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticExecutionReachabilityUpdate {
    entered_objects: Vec<SemanticNodeId>,
    exited_objects: Vec<SemanticNodeId>,
}

impl SemanticExecutionReachabilityUpdate {
    pub fn entered_objects(&self) -> &[SemanticNodeId] {
        &self.entered_objects
    }

    pub fn exited_objects(&self) -> &[SemanticNodeId] {
        &self.exited_objects
    }

    pub fn is_empty(&self) -> bool {
        self.entered_objects.is_empty() && self.exited_objects.is_empty()
    }
}

#[derive(Default)]
struct ReachabilityJournal {
    originals: HashMap<SemanticNodeId, Option<ReachabilityNode>>,
    order: Vec<SemanticNodeId>,
}

impl ReachabilityJournal {
    fn record(&mut self, nodes: &HashMap<SemanticNodeId, ReachabilityNode>, id: SemanticNodeId) {
        if self.originals.contains_key(&id) {
            return;
        }
        self.originals.insert(id, nodes.get(&id).cloned());
        self.order.push(id);
    }
}

impl SemanticExecutionReachability {
    /// Build reachability from the authoritative current scene roots.
    ///
    /// Only rooted/reachable graph state is retained. Detached subgraphs are loaded
    /// lazily if a later root or family transition makes them reachable, so the
    /// execution index does not mirror the entire semantic store.
    pub fn from_store(store: &SemanticStore) -> Result<Self, SemanticLoweringError> {
        let mut reachability = Self::default();
        let mut journal = ReachabilityJournal::default();

        for root in store.scene_roots() {
            reachability.set_scene_root(store, root, true, &mut journal)?;
        }

        if let Err(error) = reachability.validate_new_visible_objects(store, &journal) {
            reachability.rollback(journal);
            return Err(error);
        }
        Ok(reachability)
    }

    /// Build reachability for one explicitly selected execution root without
    /// changing authored scene residency.
    pub fn from_root(
        store: &SemanticStore,
        root: SemanticNodeId,
    ) -> Result<Self, SemanticLoweringError> {
        let mut reachability = Self::default();
        let mut journal = ReachabilityJournal::default();
        reachability.set_scene_root(store, root, true, &mut journal)?;
        if let Err(error) = reachability.validate_new_visible_objects(store, &journal) {
            reachability.rollback(journal);
            return Err(error);
        }
        Ok(reachability)
    }

    pub fn is_reachable(&self, id: SemanticNodeId) -> bool {
        self.nodes
            .get(&id)
            .is_some_and(ReachabilityNode::is_reachable)
    }

    /// Whether `id` is one of the roots that defines this execution domain.
    ///
    /// This is derived execution metadata. Lifecycle operations use it to reject
    /// a caller-supplied family outside the session without scanning authored
    /// scene roots or retaining another root list.
    pub fn is_execution_root(&self, id: SemanticNodeId) -> bool {
        self.nodes.get(&id).is_some_and(|node| node.scene_root)
    }

    pub fn is_object_reachable(&self, id: SemanticNodeId) -> bool {
        self.nodes
            .get(&id)
            .is_some_and(ReachabilityNode::is_reachable_object)
    }

    pub fn reachable_object_count(&self) -> usize {
        self.nodes
            .values()
            .filter(|node| node.is_reachable_object())
            .count()
    }

    pub fn reachable_objects(&self) -> impl Iterator<Item = SemanticNodeId> + '_ {
        self.nodes
            .iter()
            .filter_map(|(id, node)| node.is_reachable_object().then_some(*id))
    }

    /// Synchronize one explicitly changed top-level scene-root residency bit.
    ///
    /// Direct `attach_to_scene` / `detach_from_scene` callers already know the node
    /// whose residency changed and can keep this operation local. Higher-level scene
    /// restructuring still needs to surface its exact root replacements before it can
    /// use this same seam without a root-list scan.
    pub fn sync_scene_root(
        &mut self,
        store: &SemanticStore,
        id: SemanticNodeId,
    ) -> Result<SemanticExecutionReachabilityUpdate, SemanticLoweringError> {
        let node = store.node(id).ok_or(SemanticLoweringError::Store(
            SemanticStoreError::UnknownNode(id),
        ))?;
        let mut journal = ReachabilityJournal::default();
        self.set_scene_root(store, id, node.is_scene_owned(), &mut journal)?;
        self.finish_update(store, journal)
    }

    /// Apply one committed A1.5 semantic transaction as an atomic reachability edit.
    ///
    /// Family-edge impacts update only the affected reachable subgraph. `NodeRemoved`
    /// uses cached direct family membership because semantic deletion has already
    /// removed those edges from the store before impacts are consumed. Object
    /// property/content/subscription and family-order changes do not affect live
    /// execution membership.
    pub fn apply_transaction_result(
        &mut self,
        store: &SemanticStore,
        result: &SemanticMutationTransactionResult,
    ) -> Result<SemanticExecutionReachabilityUpdate, SemanticLoweringError> {
        self.apply_impacts(store, result.impacts())
    }

    pub fn apply_impacts(
        &mut self,
        store: &SemanticStore,
        impacts: &[SemanticMutationImpact],
    ) -> Result<SemanticExecutionReachabilityUpdate, SemanticLoweringError> {
        let mut journal = ReachabilityJournal::default();

        for impact in impacts {
            match *impact {
                SemanticMutationImpact::FamilyMemberAdded { family, member } => {
                    // A later structural deletion in the same atomic transaction may
                    // have removed this edge again. Consume only final live topology.
                    let edge_is_live = store
                        .node(member)
                        .is_some_and(|node| node.parents().contains(&family));
                    if edge_is_live {
                        self.add_family_member(store, family, member, &mut journal)?;
                    }
                }
                SemanticMutationImpact::FamilyMemberRemoved { family, member } => {
                    self.remove_family_member(family, member, &mut journal);
                }
                SemanticMutationImpact::NodeRemoved { node } => {
                    self.remove_node(node, &mut journal);
                }
                SemanticMutationImpact::SignalValue { .. }
                | SemanticMutationImpact::SignalTimeline { .. }
                | SemanticMutationImpact::ObjectProperty { .. }
                | SemanticMutationImpact::ObjectContent { .. }
                | SemanticMutationImpact::ObjectStyle { .. }
                | SemanticMutationImpact::Subscription { .. }
                | SemanticMutationImpact::UpdaterRegistrations { .. }
                | SemanticMutationImpact::SignalScoped { .. }
                | SemanticMutationImpact::FamilyMemberReordered { .. }
                | SemanticMutationImpact::NodeAdded { .. }
                | SemanticMutationImpact::AnimationAdded { .. } => {}
            }
        }

        self.finish_update(store, journal)
    }

    fn finish_update(
        &mut self,
        store: &SemanticStore,
        journal: ReachabilityJournal,
    ) -> Result<SemanticExecutionReachabilityUpdate, SemanticLoweringError> {
        if let Err(error) = self.validate_new_visible_objects(store, &journal) {
            self.rollback(journal);
            return Err(error);
        }

        let mut update = SemanticExecutionReachabilityUpdate::default();
        for id in &journal.order {
            let before = journal
                .originals
                .get(id)
                .and_then(Option::as_ref)
                .is_some_and(ReachabilityNode::is_reachable_object);
            let after = self
                .nodes
                .get(id)
                .is_some_and(ReachabilityNode::is_reachable_object);
            match (before, after) {
                (false, true) => update.entered_objects.push(*id),
                (true, false) => update.exited_objects.push(*id),
                _ => {}
            }
        }
        Ok(update)
    }

    fn validate_new_visible_objects(
        &self,
        store: &SemanticStore,
        journal: &ReachabilityJournal,
    ) -> Result<(), SemanticLoweringError> {
        for id in &journal.order {
            let before = journal
                .originals
                .get(id)
                .and_then(Option::as_ref)
                .is_some_and(ReachabilityNode::is_reachable_object);
            let after = self
                .nodes
                .get(id)
                .is_some_and(ReachabilityNode::is_reachable_object);
            if before || !after {
                continue;
            }
            if store
                .node(*id)
                .and_then(|node| node.semantic_object_state())
                .is_none()
            {
                return Err(SemanticLoweringError::MissingSemanticObjectState(*id));
            }
        }
        Ok(())
    }

    fn rollback(&mut self, mut journal: ReachabilityJournal) {
        for id in journal.order.into_iter().rev() {
            match journal
                .originals
                .remove(&id)
                .expect("journal order and original map stay coherent")
            {
                Some(node) => {
                    self.nodes.insert(id, node);
                }
                None => {
                    self.nodes.remove(&id);
                }
            }
        }
    }

    fn record_touch(&self, journal: &mut ReachabilityJournal, id: SemanticNodeId) {
        journal.record(&self.nodes, id);
    }

    fn ensure_node(
        &mut self,
        store: &SemanticStore,
        id: SemanticNodeId,
        journal: &mut ReachabilityJournal,
    ) -> Result<bool, SemanticLoweringError> {
        if self.nodes.contains_key(&id) {
            return Ok(true);
        }

        let node = store.node(id).ok_or(SemanticLoweringError::Store(
            SemanticStoreError::UnknownNode(id),
        ))?;
        let kind = match node.kind() {
            SemanticNodeKind::Object(_) | SemanticNodeKind::AuthoringObject => {
                ReachabilityKind::Object
            }
            SemanticNodeKind::Family => ReachabilityKind::Family {
                members: HashSet::new(),
            },
            SemanticNodeKind::Signal(_) | SemanticNodeKind::Animation(_) => return Ok(false),
        };

        self.record_touch(journal, id);
        self.nodes.insert(
            id,
            ReachabilityNode {
                kind,
                scene_root: false,
                reachable_parents: HashSet::new(),
            },
        );
        Ok(true)
    }

    fn set_scene_root(
        &mut self,
        store: &SemanticStore,
        id: SemanticNodeId,
        scene_root: bool,
        journal: &mut ReachabilityJournal,
    ) -> Result<(), SemanticLoweringError> {
        if scene_root && !self.ensure_node(store, id, journal)? {
            return Ok(());
        }
        let Some(node) = self.nodes.get(&id) else {
            return Ok(());
        };
        if node.scene_root == scene_root {
            return Ok(());
        }

        let was_reachable = node.is_reachable();
        self.record_touch(journal, id);
        self.nodes
            .get_mut(&id)
            .expect("tracked node remains present")
            .scene_root = scene_root;
        let is_reachable = self.nodes[&id].is_reachable();
        self.propagate_reachability_transition(store, id, was_reachable, is_reachable, journal)
    }

    fn add_family_member(
        &mut self,
        store: &SemanticStore,
        family: SemanticNodeId,
        member: SemanticNodeId,
        journal: &mut ReachabilityJournal,
    ) -> Result<(), SemanticLoweringError> {
        let Some(family_node) = self.nodes.get(&family) else {
            // An unreachable family not yet observed by execution does not need a
            // compiler mirror. Its current topology will be loaded if it later
            // becomes reachable.
            return Ok(());
        };
        let family_reachable = family_node.is_reachable();
        if !matches!(family_node.kind, ReachabilityKind::Family { .. }) {
            return Ok(());
        }

        self.record_touch(journal, family);
        if let ReachabilityKind::Family { members } = &mut self
            .nodes
            .get_mut(&family)
            .expect("tracked family remains present")
            .kind
        {
            members.insert(member);
        }
        if family_reachable {
            self.add_reachable_parent(store, member, family, journal)?;
        }
        Ok(())
    }

    fn remove_family_member(
        &mut self,
        family: SemanticNodeId,
        member: SemanticNodeId,
        journal: &mut ReachabilityJournal,
    ) {
        let Some(family_node) = self.nodes.get(&family) else {
            return;
        };
        let family_reachable = family_node.is_reachable();
        let is_family = matches!(family_node.kind, ReachabilityKind::Family { .. });
        if !is_family {
            return;
        }

        self.record_touch(journal, family);
        let removed = match &mut self
            .nodes
            .get_mut(&family)
            .expect("tracked family remains present")
            .kind
        {
            ReachabilityKind::Family { members } => members.remove(&member),
            ReachabilityKind::Object => false,
        };
        if removed && family_reachable {
            self.remove_reachable_parent(member, family, journal);
        }
    }

    fn add_reachable_parent(
        &mut self,
        store: &SemanticStore,
        id: SemanticNodeId,
        parent: SemanticNodeId,
        journal: &mut ReachabilityJournal,
    ) -> Result<(), SemanticLoweringError> {
        if !self.ensure_node(store, id, journal)? {
            return Ok(());
        }
        let was_reachable = self.nodes[&id].is_reachable();
        self.record_touch(journal, id);
        let inserted = self
            .nodes
            .get_mut(&id)
            .expect("tracked member remains present")
            .reachable_parents
            .insert(parent);
        if !inserted {
            return Ok(());
        }
        let is_reachable = self.nodes[&id].is_reachable();
        self.propagate_reachability_transition(store, id, was_reachable, is_reachable, journal)
    }

    fn remove_reachable_parent(
        &mut self,
        id: SemanticNodeId,
        parent: SemanticNodeId,
        journal: &mut ReachabilityJournal,
    ) {
        let Some(node) = self.nodes.get(&id) else {
            return;
        };
        let was_reachable = node.is_reachable();
        self.record_touch(journal, id);
        let removed = self
            .nodes
            .get_mut(&id)
            .expect("tracked member remains present")
            .reachable_parents
            .remove(&parent);
        if !removed {
            return;
        }
        let is_reachable = self.nodes[&id].is_reachable();
        if was_reachable && !is_reachable {
            self.deactivate_family_children(id, journal);
        }
    }

    fn propagate_reachability_transition(
        &mut self,
        store: &SemanticStore,
        id: SemanticNodeId,
        was_reachable: bool,
        is_reachable: bool,
        journal: &mut ReachabilityJournal,
    ) -> Result<(), SemanticLoweringError> {
        match (was_reachable, is_reachable) {
            (false, true) => self.activate_family_children(store, id, journal),
            (true, false) => {
                self.deactivate_family_children(id, journal);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn activate_family_children(
        &mut self,
        store: &SemanticStore,
        family: SemanticNodeId,
        journal: &mut ReachabilityJournal,
    ) -> Result<(), SemanticLoweringError> {
        if !matches!(
            self.nodes.get(&family).map(|node| &node.kind),
            Some(ReachabilityKind::Family { .. })
        ) {
            return Ok(());
        }

        let members = store
            .node(family)
            .ok_or(SemanticLoweringError::Store(
                SemanticStoreError::UnknownNode(family),
            ))?
            .members();
        self.record_touch(journal, family);
        if let ReachabilityKind::Family {
            members: cached_members,
        } = &mut self
            .nodes
            .get_mut(&family)
            .expect("tracked family remains present")
            .kind
        {
            cached_members.clear();
            cached_members.extend(members.iter().copied());
        }

        for member in members {
            self.add_reachable_parent(store, member, family, journal)?;
        }
        Ok(())
    }

    fn deactivate_family_children(
        &mut self,
        family: SemanticNodeId,
        journal: &mut ReachabilityJournal,
    ) {
        let mut members = match self.nodes.get(&family).map(|node| &node.kind) {
            Some(ReachabilityKind::Family { members }) => {
                members.iter().copied().collect::<Vec<_>>()
            }
            _ => return,
        };
        // Semantic identity order is stable and avoids hash-iteration-dependent
        // free-list behavior when a deleted family is no longer available in the
        // store to provide authored member order.
        members.sort_unstable();
        for member in members {
            self.remove_reachable_parent(member, family, journal);
        }
    }

    fn remove_node(&mut self, id: SemanticNodeId, journal: &mut ReachabilityJournal) {
        let Some(node) = self.nodes.get(&id).cloned() else {
            return;
        };

        if node.is_reachable() {
            self.deactivate_family_children(id, journal);
        }

        // Deletion has already removed `id` from semantic parent families. Clean the
        // corresponding cached edges locally using only the reachable-parent set.
        let mut parents = node.reachable_parents.iter().copied().collect::<Vec<_>>();
        parents.sort_unstable();
        for parent in parents {
            let Some(parent_node) = self.nodes.get(&parent) else {
                continue;
            };
            if !matches!(parent_node.kind, ReachabilityKind::Family { .. }) {
                continue;
            }
            self.record_touch(journal, parent);
            if let ReachabilityKind::Family { members } = &mut self
                .nodes
                .get_mut(&parent)
                .expect("tracked parent remains present")
                .kind
            {
                members.remove(&id);
            }
        }

        self.record_touch(journal, id);
        self.nodes.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        SemanticMutationImpact, SemanticMutationTransaction, SemanticNodeCreation,
        SemanticObjectState, SemanticStore, StoredGeometry,
    };

    use super::*;

    fn circle(radius: f32) -> SemanticObjectState {
        SemanticObjectState::new(StoredGeometry::Circle { radius })
    }

    #[test]
    fn initial_reachability_deduplicates_nested_cross_root_aliases() {
        let mut store = SemanticStore::new();
        let shared = store.insert_semantic_object(circle(1.0));
        let nested = store.insert_family();
        let first_root = store.insert_family();
        let second_root = store.insert_family();
        store.add_member(nested, shared).unwrap();
        store.add_member(first_root, nested).unwrap();
        store.add_member(second_root, shared).unwrap();
        store.attach_to_scene(first_root).unwrap();
        store.attach_to_scene(second_root).unwrap();

        let reachability = SemanticExecutionReachability::from_store(&store).unwrap();

        assert!(reachability.is_reachable(first_root));
        assert!(reachability.is_reachable(nested));
        assert!(reachability.is_object_reachable(shared));
        assert_eq!(reachability.reachable_object_count(), 1);
    }

    #[test]
    fn detached_node_addition_does_not_enter_execution_membership() {
        let mut store = SemanticStore::new();
        let mut reachability = SemanticExecutionReachability::from_store(&store).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_node(SemanticNodeCreation::object(circle(1.0)));
        let result = transaction.apply(&mut store).unwrap();
        let [SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
            panic!("expected one node-added impact");
        };

        let update = reachability
            .apply_transaction_result(&store, &result)
            .unwrap();

        assert!(update.is_empty());
        assert!(!reachability.is_reachable(*node));
        assert_eq!(reachability.reachable_object_count(), 0);
    }

    #[test]
    fn adding_member_to_reachable_family_activates_nested_subtree_once() {
        let mut store = SemanticStore::new();
        let object = store.insert_semantic_object(circle(1.0));
        let nested = store.insert_family();
        store.add_member(nested, object).unwrap();
        let root = store.insert_family();
        store.attach_to_scene(root).unwrap();
        let mut reachability = SemanticExecutionReachability::from_store(&store).unwrap();

        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_member(root, nested);
        let result = transaction.apply(&mut store).unwrap();
        let update = reachability
            .apply_transaction_result(&store, &result)
            .unwrap();

        assert_eq!(update.entered_objects(), &[object]);
        assert!(update.exited_objects().is_empty());
        assert!(reachability.is_reachable(nested));
        assert!(reachability.is_object_reachable(object));
    }

    #[test]
    fn alias_add_and_first_remove_do_not_churn_object_membership() {
        let mut store = SemanticStore::new();
        let shared = store.insert_semantic_object(circle(1.0));
        let first = store.insert_family();
        let second = store.insert_family();
        store.add_member(first, shared).unwrap();
        store.attach_to_scene(first).unwrap();
        store.attach_to_scene(second).unwrap();
        let mut reachability = SemanticExecutionReachability::from_store(&store).unwrap();

        let mut add_alias = SemanticMutationTransaction::new();
        add_alias.add_member(second, shared);
        let result = add_alias.apply(&mut store).unwrap();
        assert!(reachability
            .apply_transaction_result(&store, &result)
            .unwrap()
            .is_empty());

        let mut remove_first = SemanticMutationTransaction::new();
        remove_first.remove_member(first, shared);
        let result = remove_first.apply(&mut store).unwrap();
        assert!(reachability
            .apply_transaction_result(&store, &result)
            .unwrap()
            .is_empty());
        assert!(reachability.is_object_reachable(shared));

        let mut remove_last = SemanticMutationTransaction::new();
        remove_last.remove_member(second, shared);
        let result = remove_last.apply(&mut store).unwrap();
        let update = reachability
            .apply_transaction_result(&store, &result)
            .unwrap();
        assert_eq!(update.exited_objects(), &[shared]);
        assert!(!reachability.is_object_reachable(shared));
    }

    #[test]
    fn removing_reachable_family_uses_cached_members_after_store_cleanup() {
        let mut store = SemanticStore::new();
        let object = store.insert_semantic_object(circle(1.0));
        let nested = store.insert_family();
        let root = store.insert_family();
        store.add_member(nested, object).unwrap();
        store.add_member(root, nested).unwrap();
        store.attach_to_scene(root).unwrap();
        let mut reachability = SemanticExecutionReachability::from_store(&store).unwrap();

        let mut remove = SemanticMutationTransaction::new();
        remove.remove_node(nested);
        let result = remove.apply(&mut store).unwrap();
        assert!(store.node(nested).is_none());
        assert!(store.node(object).unwrap().parents().is_empty());

        let update = reachability
            .apply_transaction_result(&store, &result)
            .unwrap();

        assert_eq!(update.exited_objects(), &[object]);
        assert!(!reachability.is_reachable(nested));
        assert!(!reachability.is_object_reachable(object));
    }

    #[test]
    fn direct_root_sync_preserves_alias_until_last_reachable_source_is_removed() {
        let mut store = SemanticStore::new();
        let object = store.insert_semantic_object(circle(1.0));
        let family = store.insert_family();
        store.add_member(family, object).unwrap();
        store.attach_to_scene(object).unwrap();
        store.attach_to_scene(family).unwrap();
        let mut reachability = SemanticExecutionReachability::from_store(&store).unwrap();

        store.detach_from_scene(object).unwrap();
        assert!(reachability
            .sync_scene_root(&store, object)
            .unwrap()
            .is_empty());
        assert!(reachability.is_object_reachable(object));

        store.detach_from_scene(family).unwrap();
        let update = reachability.sync_scene_root(&store, family).unwrap();
        assert_eq!(update.exited_objects(), &[object]);
        assert!(!reachability.is_object_reachable(object));
    }

    #[test]
    fn invalid_newly_visible_object_rolls_back_only_touched_reachability() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        store.attach_to_scene(root).unwrap();
        let identity_only = store.insert_authoring_object();
        let mut reachability = SemanticExecutionReachability::from_store(&store).unwrap();

        store.add_member(root, identity_only).unwrap();
        let impacts = [SemanticMutationImpact::FamilyMemberAdded {
            family: root,
            member: identity_only,
        }];

        assert_eq!(
            reachability.apply_impacts(&store, &impacts).unwrap_err(),
            SemanticLoweringError::MissingSemanticObjectState(identity_only)
        );
        assert!(reachability.is_reachable(root));
        assert!(!reachability.is_reachable(identity_only));
        assert_eq!(reachability.reachable_object_count(), 0);
    }
}
