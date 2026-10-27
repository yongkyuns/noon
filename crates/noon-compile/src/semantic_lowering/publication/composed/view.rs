//! Checked local reads of the held semantic transaction, never a staged scene.
use super::*;

pub(super) struct View<'a, 'store> {
    pub(super) prepared: &'a PreparedSemanticMutationTransaction<'store>,
    reachability: &'a SemanticExecutionReachability,
    reachable: HashMap<SemanticTransactionNodeRef, bool>,
}

pub(super) fn old_scope(store: &noon_core::SemanticStore, node: SemanticNodeId) -> bool {
    store.node(node).is_some_and(|node| {
        matches!(node.kind(), SemanticNodeKind::Family(_))
            && node.semantic_object_state().is_none()
            && !node.effect_ids().is_empty()
    })
}

pub(in crate::semantic_lowering::publication) fn unrendered_family(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    node: SemanticTransactionNodeRef,
) -> Result<bool, SemanticPublicationLoweringError> {
    match prepared.object_state(node) {
        Ok(_) => Ok(false),
        Err(
            SemanticTransactionReadError::NotObject(_)
            | SemanticTransactionReadError::Existing(
                noon_core::SemanticSceneOperationError::NotSemanticObject(_),
            ),
        ) => {
            prepared.family_style(node)?;
            Ok(true)
        }
        Err(error) => Err(error.into()),
    }
}

impl<'a, 'store> View<'a, 'store> {
    pub(super) fn new(
        prepared: &'a PreparedSemanticMutationTransaction<'store>,
        reachability: &'a SemanticExecutionReachability,
    ) -> Self {
        Self {
            prepared,
            reachability,
            reachable: HashMap::new(),
        }
    }

    pub(super) fn id(
        &self,
        node: SemanticTransactionNodeRef,
    ) -> Result<SemanticNodeId, SemanticPublicationLoweringError> {
        node.existing()
            .or_else(|| self.prepared.planned_node_id(node))
            .ok_or(SemanticPublicationLoweringError::PlannedObjectIdUnavailable { object: node })
    }

    pub(super) fn error(
        &self,
        owner: SemanticTransactionNodeRef,
        reason: &'static str,
    ) -> SemanticPublicationLoweringError {
        match self.id(owner) {
            Ok(owner) => SemanticLoweringError::InvalidEffectIsolation { owner, reason }.into(),
            Err(error) => error,
        }
    }

    pub(super) fn scope(
        &self,
        node: SemanticTransactionNodeRef,
    ) -> Result<bool, SemanticPublicationLoweringError> {
        if self.prepared.node_is_removed(node) {
            return Ok(false);
        }
        Ok(!self.prepared.animation_effect_snapshot(node)?.is_empty()
            && unrendered_family(self.prepared, node)?)
    }

    pub(super) fn reachable(
        &mut self,
        node: SemanticTransactionNodeRef,
    ) -> Result<bool, SemanticPublicationLoweringError> {
        if let Some(&value) = self.reachable.get(&node) {
            return Ok(value);
        }
        let mut pending = vec![(node, false)];
        let mut visiting = HashSet::new();
        while let Some((current, finish)) = pending.pop() {
            if self.reachable.contains_key(&current) {
                continue;
            }
            let removed = self.prepared.node_is_removed(current);
            let root = current
                .existing()
                .is_some_and(|id| self.reachability.is_execution_root(id));
            if removed || root {
                self.reachable.insert(current, !removed && root);
                continue;
            }
            let parents = self.prepared.family_parents(current)?;
            if finish {
                let value = parents
                    .iter()
                    .any(|parent| self.reachable.get(parent) == Some(&true));
                self.reachable.insert(current, value);
                visiting.remove(&current);
            } else {
                if !visiting.insert(current) {
                    return Err(self.error(node, "cyclic family membership"));
                }
                pending.push((current, true));
                pending.extend(parents.into_iter().map(|parent| (parent, false)));
            }
        }
        Ok(self.reachable[&node])
    }

    /// Isolation requires one rooted path, while detached aliases are harmless.
    pub(super) fn parent(
        &mut self,
        node: SemanticTransactionNodeRef,
        owner: SemanticTransactionNodeRef,
    ) -> Result<Option<SemanticTransactionNodeRef>, SemanticPublicationLoweringError> {
        let mut parents = Vec::new();
        for parent in self.prepared.family_parents(node)? {
            if self.reachable(parent)? {
                parents.push(parent);
            }
        }
        let root = node
            .existing()
            .is_some_and(|id| self.reachability.is_execution_root(id));
        if parents.len() > 1 || (root && !parents.is_empty()) {
            return Err(self.error(owner, "aliased family membership"));
        }
        Ok(parents.first().copied())
    }

    pub(super) fn parent_scope(
        &mut self,
        owner: SemanticTransactionNodeRef,
    ) -> Result<Option<ObjectId>, SemanticPublicationLoweringError> {
        let mut current = owner;
        while let Some(parent) = self.parent(current, owner)? {
            if self.scope(parent)? {
                return Ok(Some(semantic_execution_object_id(self.id(parent)?)));
            }
            current = parent;
        }
        Ok(None)
    }

    pub(super) fn leaves(
        &mut self,
        owner: SemanticTransactionNodeRef,
    ) -> Result<Vec<SemanticTransactionNodeRef>, SemanticPublicationLoweringError> {
        let mut pending = vec![owner];
        let mut seen = HashSet::new();
        let mut leaves = Vec::new();
        while let Some(node) = pending.pop() {
            if !seen.insert(node) {
                return Err(self.error(owner, "aliased family membership"));
            }
            self.parent(node, owner)?;
            if !unrendered_family(self.prepared, node)? {
                leaves.push(node);
            }
            pending.extend(self.prepared.family_members(node)?.into_iter().rev());
        }
        if leaves.is_empty() {
            return Err(self.error(owner, "empty source"));
        }
        Ok(leaves)
    }
}
