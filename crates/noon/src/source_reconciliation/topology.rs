//! Inert source topology and its translation into the shared mutation vocabulary.
use super::*;
use noon_core::{SemanticNodeKind, SemanticStore};
use std::collections::HashMap;

/// A keyed family declaration. Members name other declarations in the same
/// candidate, including shared objects or nested families; they are not handles
/// into a second semantic store.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceFamilyDeclaration {
    source: SourceIdentity,
    members: Vec<SourceIdentity>,
    z_index: f64,
}
impl SourceFamilyDeclaration {
    pub fn new(source: SourceIdentity, members: impl IntoIterator<Item = SourceIdentity>) -> Self {
        Self {
            source,
            members: members.into_iter().collect(),
            z_index: 0.0,
        }
    }
    pub fn with_z_index(mut self, z_index: f64) -> Self {
        self.z_index = z_index;
        self
    }
    pub fn source(&self) -> &SourceIdentity {
        &self.source
    }
    pub fn members(&self) -> &[SourceIdentity] {
        &self.members
    }
}

impl SourceCandidate {
    pub fn declare_family(
        &mut self,
        declaration: SourceFamilyDeclaration,
    ) -> Result<&mut Self, SourceCandidateError> {
        if !self.sources.insert(declaration.source.clone()) {
            return Err(SourceCandidateError::DuplicateSourceIdentity(
                declaration.source,
            ));
        }
        self.families.push(declaration);
        Ok(self)
    }
    /// Set explicit painter roots for a grouped candidate. Without this call,
    /// declarations remain direct roots (objects first, then families).
    /// References and cycles are validated together when reconciliation begins,
    /// so declarations may be supplied in any dependency order.
    pub fn set_root_members(
        &mut self,
        roots: impl IntoIterator<Item = SourceIdentity>,
    ) -> &mut Self {
        self.roots = Some(roots.into_iter().collect());
        self
    }
    pub fn families(&self) -> &[SourceFamilyDeclaration] {
        &self.families
    }
    pub(super) fn source_order(&self) -> impl Iterator<Item = &SourceIdentity> {
        self.declarations
            .iter()
            .map(|d| &d.source)
            .chain(self.families.iter().map(|d| &d.source))
    }
    fn root_sources(&self) -> Vec<SourceIdentity> {
        self.roots
            .clone()
            .unwrap_or_else(|| self.source_order().cloned().collect())
    }
    fn validate_topology(&self, roots: &[SourceIdentity]) -> Result<(), SourceCandidateError> {
        let invalid = |message| SourceCandidateError::InvalidTopology(message);
        let families: HashMap<_, _> = self.families.iter().map(|d| (&d.source, d)).collect();
        for members in
            std::iter::once(roots).chain(self.families.iter().map(|d| d.members.as_slice()))
        {
            let mut seen = HashSet::new();
            for source in members {
                if !self.sources.contains(source) {
                    return Err(invalid(format!("undeclared source member: {source:?}")));
                }
                if !seen.insert(source) {
                    return Err(invalid(format!("duplicate source member: {source:?}")));
                }
            }
        }
        for family in &self.families {
            if !family.z_index.is_finite() {
                return Err(invalid("source family z-index must be finite".into()));
            }
        }
        // Iterative DFS avoids consuming the host stack for deeply nested source.
        let mut visited = HashSet::new();
        let mut active = HashSet::new();
        let mut stack: Vec<_> = roots.iter().rev().map(|key| (key, false)).collect();
        while let Some((key, leaving)) = stack.pop() {
            if leaving {
                active.remove(key);
                visited.insert(key);
                continue;
            }
            if active.contains(key) {
                return Err(invalid(format!("cyclic source family: {key:?}")));
            }
            if visited.contains(key) {
                continue;
            }
            active.insert(key);
            stack.push((key, true));
            if let Some(family) = families.get(key) {
                stack.extend(family.members.iter().rev().map(|child| (child, false)));
            }
        }
        if visited.len() != self.sources.len() {
            return Err(invalid(
                "every source declaration must be reachable from a candidate root".into(),
            ));
        }
        Ok(())
    }
}

fn scope_nodes(
    store: &SemanticStore,
    root: SemanticNodeId,
) -> Result<Vec<SemanticNodeId>, SourceReconciliationError> {
    let mut seen = HashSet::new();
    let mut nodes = Vec::new();
    let mut stack = store
        .semantic_family_members_checked(root)
        .expect("Scene root remains a family");
    while let Some(node) = stack.pop() {
        if !seen.insert(node) {
            continue;
        }
        let record = store.node(node).expect("live family membership");
        if record.source_identity().is_none() {
            return Err(SourceReconciliationError::UnmanagedScopeMember { root, member: node });
        }
        nodes.push(node);
        stack.extend(record.members_iter());
    }
    // Source reconciliation owns this complete declaration scope, not other
    // roots that happen to reference one of its nodes. Reject that ambiguity
    // before a property edit or terminal deletion could affect the other root.
    for &node in &nodes {
        if store
            .node(node)
            .unwrap()
            .parents()
            .iter()
            .any(|parent| *parent != root && !seen.contains(parent))
        {
            return Err(SourceReconciliationError::SharedOutsideScope { node });
        }
    }
    Ok(nodes)
}

pub(super) fn stage_candidate(
    scene: &Scene,
    candidate: &SourceCandidate,
) -> Result<(SemanticMutationTransaction, bool), SourceReconciliationError> {
    let roots = candidate.root_sources();
    candidate
        .validate_topology(&roots)
        .map_err(SourceReconciliationError::InvalidCandidate)?;
    let root = scene.root();
    let store = scene.integration_store().borrow();
    let current_nodes = scope_nodes(&store, root)?;
    let current_set: HashSet<_> = current_nodes.iter().copied().collect();
    let mut transaction = SemanticMutationTransaction::new();
    let mut refs = HashMap::new();
    let mut matched = HashSet::new();
    let mut changed = false;
    for source in candidate.source_order() {
        if let Some(node) = store.node_for_source(source) {
            if !current_set.contains(&node) {
                return Err(SourceReconciliationError::SourceOutsideScope {
                    source: source.clone(),
                    node,
                    root,
                });
            }
            matched.insert(node);
            refs.insert(source, SemanticTransactionNodeRef::Existing(node));
        }
    }
    for declaration in &candidate.declarations {
        if let Some(SemanticTransactionNodeRef::Existing(node)) =
            refs.get(&declaration.source).copied()
        {
            let current = store.semantic_object_state_checked(node).map_err(|_| {
                SourceReconciliationError::SourceIsNotObject {
                    source: declaration.source.clone(),
                    node,
                }
            })?;
            changed |= stage_object_delta(
                &mut transaction,
                node,
                current,
                store.node(node).unwrap().host_updaters().is_empty(),
                &declaration.state,
                &declaration.source,
            )?;
        } else {
            let token = transaction.create_node(
                SemanticNodeCreation::object(declaration.state.clone())
                    .with_source_identity(declaration.source.clone()),
            );
            refs.insert(&declaration.source, token.into());
            changed = true;
        }
    }
    for declaration in &candidate.families {
        if let Some(SemanticTransactionNodeRef::Existing(node)) =
            refs.get(&declaration.source).copied()
        {
            let SemanticNodeKind::Family(presentation) = store.node(node).unwrap().kind() else {
                return Err(SourceReconciliationError::UnsupportedDeclarationChange {
                    source: declaration.source.clone(),
                    field: "object/family kind",
                });
            };
            if presentation.z_index != declaration.z_index {
                transaction.set_z_index(node, declaration.z_index);
                changed = true;
            }
        } else {
            let token = transaction.create_node(
                SemanticNodeCreation::family().with_source_identity(declaration.source.clone()),
            );
            if declaration.z_index != 0.0 {
                transaction.set_z_index(token, declaration.z_index);
            }
            refs.insert(&declaration.source, token.into());
            changed = true;
        }
    }
    let mut edges = Vec::new();
    for (family, desired) in
        std::iter::once((SemanticTransactionNodeRef::Existing(root), roots.as_slice())).chain(
            candidate
                .families
                .iter()
                .map(|d| (refs[&d.source], d.members.as_slice())),
        )
    {
        let current = match family {
            SemanticTransactionNodeRef::Existing(node) => store
                .semantic_family_members_checked(node)
                .expect("validated family"),
            SemanticTransactionNodeRef::Pending(_) => Vec::new(),
        };
        let desired: Vec<_> = desired.iter().map(|key| refs[key]).collect();
        if current
            .iter()
            .copied()
            .map(SemanticTransactionNodeRef::Existing)
            .eq(desired.iter().copied())
        {
            continue;
        }
        changed = true;
        let desired_set: HashSet<_> = desired.iter().copied().collect();
        for member in &current {
            if !desired_set.contains(&SemanticTransactionNodeRef::Existing(*member)) {
                transaction.remove_member(family, *member);
            }
        }
        edges.push((family, current, desired));
    }
    // Remove obsolete edges in every changed family before adding new edges,
    // avoiding temporary cycles when one nesting relationship is reversed.
    for (family, current, desired) in edges {
        let desired_set: HashSet<_> = desired.iter().copied().collect();
        let mut order: Vec<_> = current
            .into_iter()
            .map(SemanticTransactionNodeRef::Existing)
            .filter(|member| desired_set.contains(member))
            .collect();
        let current_set: HashSet<_> = order.iter().copied().collect();
        for member in &desired {
            if !current_set.contains(member) {
                transaction.add_member(family, *member);
                order.push(*member);
            }
        }
        stage_reorder(&mut transaction, family, &order, &desired);
    }
    for node in current_nodes {
        if !matched.contains(&node) {
            transaction.remove_node(node);
            changed = true;
        }
    }
    Ok((transaction, changed))
}

/// Keep the longest already-ordered subsequence and move only the other edges.
/// Source comparison is O(n log n), but inserting or moving one keyed item emits
/// at most one reorder, rather than touching every member of a large family.
fn stage_reorder(
    transaction: &mut SemanticMutationTransaction,
    family: SemanticTransactionNodeRef,
    current: &[SemanticTransactionNodeRef],
    desired: &[SemanticTransactionNodeRef],
) {
    let positions: HashMap<_, _> = current
        .iter()
        .enumerate()
        .map(|(i, node)| (*node, i))
        .collect();
    let mut tails: Vec<usize> = Vec::new();
    let mut previous = vec![None; desired.len()];
    for (index, node) in desired.iter().enumerate() {
        let position = positions[node];
        let at = tails.partition_point(|&tail| positions[&desired[tail]] < position);
        if at > 0 {
            previous[index] = Some(tails[at - 1]);
        }
        if at == tails.len() {
            tails.push(index);
        } else {
            tails[at] = index;
        }
    }
    let mut retained = vec![false; desired.len()];
    let mut cursor = tails.last().copied();
    while let Some(index) = cursor {
        retained[index] = true;
        cursor = previous[index];
    }
    for index in (0..desired.len()).rev() {
        if !retained[index] {
            transaction.reorder_member_ref(family, desired[index], desired.get(index + 1).copied());
        }
    }
}
