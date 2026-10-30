use std::collections::{HashMap, HashSet};

use noon_core::{
    SemanticMutationImpact, SemanticMutationTransactionResult, SemanticNodeId, SemanticNodeKind,
    SemanticStore, SemanticStoreError,
};

use crate::SemanticLoweringError;

fn has_execution_row(store: &SemanticStore, node: &noon_core::SemanticNode) -> bool {
    node.semantic_object_state().is_some()
        || (matches!(node.kind(), SemanticNodeKind::Family(_))
            && (!node.effect_ids().is_empty()
                || node.family_style().expect("family style").object_opacity != 1.0
                || store.has_family_appearance_target(node.id())))
}

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
    object: bool,
    members: HashSet<SemanticNodeId>,
    scene_root: bool,
    reachable_parents: HashSet<SemanticNodeId>,
    graph_root: bool,
}

impl ReachabilityNode {
    fn is_reachable(&self) -> bool {
        self.scene_root || !self.reachable_parents.is_empty()
    }

    fn is_reachable_object(&self) -> bool {
        self.object && self.is_reachable()
    }
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
    entered_graph_roots: Vec<SemanticNodeId>,
    exited_graph_roots: Vec<SemanticNodeId>,
    updated_graph_roots: Vec<SemanticNodeId>,
}

impl SemanticExecutionReachabilityUpdate {
    pub fn entered_objects(&self) -> &[SemanticNodeId] {
        &self.entered_objects
    }

    pub fn exited_objects(&self) -> &[SemanticNodeId] {
        &self.exited_objects
    }

    pub fn is_empty(&self) -> bool {
        self.entered_objects.is_empty()
            && self.exited_objects.is_empty()
            && self.entered_graph_roots.is_empty()
            && self.exited_graph_roots.is_empty()
            && self.updated_graph_roots.is_empty()
    }

    pub fn entered_graph_roots(&self) -> &[SemanticNodeId] {
        &self.entered_graph_roots
    }

    pub fn exited_graph_roots(&self) -> &[SemanticNodeId] {
        &self.exited_graph_roots
    }

    pub fn updated_graph_roots(&self) -> &[SemanticNodeId] {
        &self.updated_graph_roots
    }
}

#[derive(Default)]
struct ReachabilityJournal {
    originals: HashMap<SemanticNodeId, Option<ReachabilityNode>>,
    // Content capability changes journal one bit without copying membership/alias sets.
    object_flags: HashMap<SemanticNodeId, bool>,
    order: Vec<SemanticNodeId>,
}

impl ReachabilityJournal {
    fn record_object(
        &mut self,
        nodes: &HashMap<SemanticNodeId, ReachabilityNode>,
        id: SemanticNodeId,
    ) {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.object_flags.entry(id) {
            entry.insert(nodes[&id].object);
            if !self.originals.contains_key(&id) {
                self.order.push(id);
            }
        }
    }

    fn previous_node<'a>(
        &'a self,
        nodes: &'a HashMap<SemanticNodeId, ReachabilityNode>,
        id: SemanticNodeId,
    ) -> Option<&'a ReachabilityNode> {
        match self.originals.get(&id) {
            Some(original) => original.as_ref(),
            None => nodes.get(&id),
        }
    }

    fn was_reachable_object(
        &self,
        nodes: &HashMap<SemanticNodeId, ReachabilityNode>,
        id: SemanticNodeId,
    ) -> bool {
        self.previous_node(nodes, id).is_some_and(|node| {
            node.is_reachable() && self.object_flags.get(&id).copied().unwrap_or(node.object)
        })
    }

    fn record(&mut self, nodes: &HashMap<SemanticNodeId, ReachabilityNode>, id: SemanticNodeId) {
        if self.originals.contains_key(&id) {
            return;
        }
        self.originals.insert(id, nodes.get(&id).cloned());
        if !self.object_flags.contains_key(&id) {
            self.order.push(id);
        }
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

    pub(super) fn execution_roots(&self) -> impl Iterator<Item = SemanticNodeId> + '_ {
        self.nodes
            .iter()
            .filter_map(|(&id, node)| node.scene_root.then_some(id))
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
        let mut updated_graph_roots = Vec::new();

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
                SemanticMutationImpact::ObjectContent { object }
                | SemanticMutationImpact::FamilyStyle { node: object }
                | SemanticMutationImpact::FamilyAppearanceScope { node: object }
                | SemanticMutationImpact::EffectAttachment { owner: object, .. } => {
                    let owns_content = store
                        .node(object)
                        .is_some_and(|node| has_execution_row(store, node));
                    if self
                        .nodes
                        .get(&object)
                        .is_some_and(|node| node.object != owns_content)
                    {
                        journal.record_object(&self.nodes, object);
                        self.nodes.get_mut(&object).expect("tracked object").object = owns_content;
                    }
                }
                SemanticMutationImpact::AudioClip { .. }
                | SemanticMutationImpact::EffectParameter { .. }
                | SemanticMutationImpact::SignalValue { .. }
                | SemanticMutationImpact::SignalTimeline { .. }
                | SemanticMutationImpact::ObjectProperty { .. }
                | SemanticMutationImpact::ObjectTransform { .. }
                | SemanticMutationImpact::SpatialCompositionDomain { .. }
                | SemanticMutationImpact::SpatialAnchorChanged { .. }
                | SemanticMutationImpact::CameraProfile { .. }
                | SemanticMutationImpact::CameraMotions { .. }
                | SemanticMutationImpact::BarMetadata { .. }
                | SemanticMutationImpact::ObjectRole { .. }
                | SemanticMutationImpact::ClickAction { .. }
                | SemanticMutationImpact::DecimalNumber { .. }
                | SemanticMutationImpact::TextPresentationBaseline { .. }
                | SemanticMutationImpact::ObjectStyle { .. }
                | SemanticMutationImpact::FamilyColor { .. }
                | SemanticMutationImpact::UpdaterSuspension { .. }
                | SemanticMutationImpact::ZIndex { .. }
                | SemanticMutationImpact::DrawEnabled { .. }
                | SemanticMutationImpact::Subscription { .. }
                | SemanticMutationImpact::UpdaterRegistrations { .. }
                | SemanticMutationImpact::SignalScoped { .. }
                | SemanticMutationImpact::ForegroundMembers { .. }
                | SemanticMutationImpact::FamilyMemberReordered { .. }
                | SemanticMutationImpact::NodeAdded { .. }
                | SemanticMutationImpact::AnimationAdded { .. } => {}
                SemanticMutationImpact::GraphDeclaration { scope } => {
                    let Some(reachable) =
                        self.nodes.get(&scope).map(ReachabilityNode::is_reachable)
                    else {
                        continue;
                    };
                    self.record_touch(&mut journal, scope);
                    let graph_root = store
                        .node(scope)
                        .and_then(|node| node.graph_declaration())
                        .is_some();
                    self.nodes
                        .get_mut(&scope)
                        .expect("tracked graph scope remains present")
                        .graph_root = graph_root;
                    if reachable && graph_root {
                        updated_graph_roots.push(scope);
                    }
                }
            }
        }

        let mut update = self.finish_update(store, journal)?;
        updated_graph_roots.sort_unstable();
        updated_graph_roots.dedup();
        update.updated_graph_roots = updated_graph_roots;
        Ok(update)
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
            let before = journal.was_reachable_object(&self.nodes, *id);
            let after = self
                .nodes
                .get(id)
                .is_some_and(ReachabilityNode::is_reachable_object);
            match (before, after) {
                (false, true) => update.entered_objects.push(*id),
                (true, false) => update.exited_objects.push(*id),
                _ => {}
            }
            let graph_before = journal
                .previous_node(&self.nodes, *id)
                .is_some_and(|node| node.graph_root && node.is_reachable());
            let graph_after = self
                .nodes
                .get(id)
                .is_some_and(|node| node.graph_root && node.is_reachable());
            match (graph_before, graph_after) {
                (false, true) => update.entered_graph_roots.push(*id),
                (true, false) => update.exited_graph_roots.push(*id),
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
            let before = journal.was_reachable_object(&self.nodes, *id);
            let after = self
                .nodes
                .get(id)
                .is_some_and(ReachabilityNode::is_reachable_object);
            if before || !after {
                continue;
            }
            if store
                .node(*id)
                .is_none_or(|node| !has_execution_row(store, node))
            {
                return Err(SemanticLoweringError::MissingSemanticObjectState(*id));
            }
        }
        Ok(())
    }

    fn rollback(&mut self, mut journal: ReachabilityJournal) {
        for id in journal.order.into_iter().rev() {
            if let Some(original) = journal.originals.remove(&id) {
                match original {
                    Some(node) => {
                        self.nodes.insert(id, node);
                    }
                    None => {
                        self.nodes.remove(&id);
                    }
                }
            }
            if let Some(object) = journal.object_flags.remove(&id) {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.object = object;
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
        let object = match node.kind() {
            SemanticNodeKind::AuthoringObject => true,
            SemanticNodeKind::Family(_) => has_execution_row(store, node),
            SemanticNodeKind::Signal(_)
            | SemanticNodeKind::Animation(_)
            | SemanticNodeKind::AudioClip(_)
            | SemanticNodeKind::Effect(_) => return Ok(false),
        };

        self.record_touch(journal, id);
        self.nodes.insert(
            id,
            ReachabilityNode {
                object,
                members: HashSet::new(),
                scene_root: false,
                reachable_parents: HashSet::new(),
                graph_root: node.graph_declaration().is_some(),
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
        self.record_touch(journal, family);
        self.nodes
            .get_mut(&family)
            .expect("tracked family remains present")
            .members
            .insert(member);
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
        self.record_touch(journal, family);
        let removed = self
            .nodes
            .get_mut(&family)
            .expect("tracked family remains present")
            .members
            .remove(&member);
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
        if !self.nodes.contains_key(&family) {
            return Ok(());
        }

        let members = store
            .node(family)
            .ok_or(SemanticLoweringError::Store(
                SemanticStoreError::UnknownNode(family),
            ))?
            .members();
        self.record_touch(journal, family);
        let cached_members = &mut self
            .nodes
            .get_mut(&family)
            .expect("tracked family remains present")
            .members;
        cached_members.clear();
        cached_members.extend(members.iter().copied());

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
        let mut members = match self.nodes.get(&family) {
            Some(node) => node.members.iter().copied().collect::<Vec<_>>(),
            None => return,
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
            if !self.nodes.contains_key(&parent) {
                continue;
            }
            self.record_touch(journal, parent);
            self.nodes
                .get_mut(&parent)
                .expect("tracked parent remains present")
                .members
                .remove(&id);
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
    fn drawing_root_content_journals_only_capability_and_preserves_membership_storage() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        for _ in 0..512 {
            let child = store.insert_semantic_object(circle(0.1));
            store.add_member(family, child).unwrap();
        }
        store.attach_to_scene(family).unwrap();
        let mut reachability = SemanticExecutionReachability::from_store(&store).unwrap();
        let members_capacity = reachability.nodes[&family].members.capacity();
        let mut journal = ReachabilityJournal::default();
        journal.record_object(&reachability.nodes, family);
        reachability.nodes.get_mut(&family).unwrap().object = true;
        assert!(journal.originals.is_empty());
        assert_eq!(journal.order, vec![family]);
        assert!(!journal.was_reachable_object(&reachability.nodes, family));
        // Upgrading to a structural journal later in the same batch still
        // restores the original capability on rollback, in either order.
        reachability.record_touch(&mut journal, family);
        reachability.rollback(journal);
        assert!(!reachability.nodes[&family].object);
        assert_eq!(
            reachability.nodes[&family].members.capacity(),
            members_capacity
        );
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_content(family, StoredGeometry::Circle { radius: 1.0 });
        let result = transaction.apply(&mut store).unwrap();
        let update = reachability
            .apply_transaction_result(&store, &result)
            .unwrap();
        assert_eq!(update.entered_objects(), &[family]);
        assert_eq!(
            reachability.nodes[&family].members.capacity(),
            members_capacity
        );
        assert_eq!(reachability.nodes[&family].members.len(), 512);
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_content(family, StoredGeometry::Circle { radius: 2.0 });
        let result = transaction.apply(&mut store).unwrap();
        assert!(reachability
            .apply_transaction_result(&store, &result)
            .unwrap()
            .is_empty());
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
    fn invalid_child_rolls_back_point_owning_reachability_without_losing_parent() {
        let mut store = SemanticStore::new();
        let root = store.insert_semantic_object(circle(2.0));
        store.attach_to_scene(root).unwrap();
        let invalid = store.insert_authoring_object();
        let mut reachability = SemanticExecutionReachability::from_store(&store).unwrap();
        store.add_member(root, invalid).unwrap();
        assert_eq!(
            reachability
                .apply_impacts(
                    &store,
                    &[SemanticMutationImpact::FamilyMemberAdded {
                        family: root,
                        member: invalid
                    }]
                )
                .unwrap_err(),
            SemanticLoweringError::MissingSemanticObjectState(invalid)
        );
        assert!(reachability.is_object_reachable(root));
        assert!(!reachability.is_reachable(invalid));
        assert_eq!(reachability.reachable_object_count(), 1);
        store.remove_member(root, invalid).unwrap();
        let valid = store.insert_semantic_object(circle(1.0));
        store.add_member(root, valid).unwrap();
        let update = reachability
            .apply_impacts(
                &store,
                &[SemanticMutationImpact::FamilyMemberAdded {
                    family: root,
                    member: valid,
                }],
            )
            .unwrap();
        assert_eq!(update.entered_objects(), &[valid]);
        assert_eq!(reachability.reachable_object_count(), 2);
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
