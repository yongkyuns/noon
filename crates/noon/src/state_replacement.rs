//! Shared object/family become preparation and atomic semantic state replacement.
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::{
    semantic_mobject::{
        layout_for_content, scale_state_about_center, stage_state_changes, state_center,
        validate_content,
    },
    AuthoringError, Mobject, MobjectFamily,
};
use noon_core::{
    Bounds2D64, SemanticMutationTransaction, SemanticNodeCreation, SemanticNodeId,
    SemanticNodeKind, SemanticObjectState, SemanticStore, SemanticTransactionNodeRef, VectorPath,
};

/// Pair through the same topology/alias contract as ordinary family Transform.
/// All captures and fitting succeed before a caller can publish any edit.
pub(crate) fn prepare_family_become<E: From<AuthoringError>>(
    source: &MobjectFamily,
    target: &MobjectFamily,
    options: ManimBecomeOptions,
    mut capture: impl FnMut(&Mobject) -> Result<SemanticObjectState, E>,
) -> Result<crate::path_editing::PreparedPathEdits, E> {
    if !Rc::ptr_eq(source.integration_store(), target.integration_store()) {
        return Err(AuthoringError::ForeignStore.into());
    }
    source.validate()?;
    target.validate()?;
    let pairing = source
        .integration_store()
        .borrow()
        .ordered_family_leaf_pairs(source.node_id(), target.node_id());
    let pairs = match pairing {
        Ok(pairs) => pairs,
        Err(noon_core::SemanticFamilyPairingError::Empty) => {
            return crate::path_editing::PreparedPathEdits::prepare(
                &source.integration_store().borrow(),
                Vec::new(),
            )
            .map_err(E::from)
        }
        Err(
            noon_core::SemanticFamilyPairingError::TopologyMismatch { .. }
            | noon_core::SemanticFamilyPairingError::AliasMismatch { .. },
        ) => {
            return prepare_persistent_family_reconcile(source, target, options, capture);
        }
        Err(error) => return Err(AuthoringError::from(error).into()),
    };
    let mut sources = Vec::with_capacity(pairs.len());
    let mut targets = Vec::with_capacity(pairs.len());
    for &(source_id, target_id) in &pairs {
        sources.push(capture(&Mobject::from_node(
            Rc::clone(source.integration_store()),
            source_id,
        )?)?);
        targets.push(capture(&Mobject::from_node(
            Rc::clone(source.integration_store()),
            target_id,
        )?)?);
    }
    let store = source.integration_store().borrow();
    let targets = prepare_become_states(&store, &sources, targets, options)?;
    let mut transaction = SemanticMutationTransaction::new();
    let mut replacements = Vec::new();
    let mut staged = HashMap::<SemanticNodeId, SemanticObjectState>::new();
    for ((source, target_id), (mut target, path)) in pairs.into_iter().zip(targets) {
        // Plain Manim become reads a shared target after preceding leaf writes.
        // Matching options copy the target first, so those reads stay captured.
        if options == ManimBecomeOptions::default() {
            if let Some(previous_write) = staged.get(&target_id) {
                target = previous_write.clone();
            }
            staged.insert(source, target.clone());
        }
        let previous = store
            .semantic_object_state_checked(source)
            .map_err(AuthoringError::from)?;
        if let Some(path) = path {
            replacements.push((source, target, path));
        } else {
            stage_state_changes(&mut transaction, source, previous, &target);
        }
    }
    Ok(
        crate::path_editing::PreparedPathEdits::prepare(&store, replacements)?
            .with_transaction(transaction),
    )
}

#[derive(Clone)]
enum PersistentTargetNode {
    Object,
    Family {
        members: Vec<SemanticNodeId>,
        z_index: f64,
    },
}

/// Persistent `become()` reconciliation is intentionally separate from Transform
/// pairing. The target graph is a preflight template: target semantic identities
/// are never imported into the receiver. Compatible receiver identities are reused
/// once, while additional topology is allocated only through transaction-local
/// pending references.
struct PersistentFamilyReconcile<'a> {
    store: &'a SemanticStore,
    target_states: &'a HashMap<SemanticNodeId, (SemanticObjectState, Option<VectorPath>)>,
    transaction: SemanticMutationTransaction,
    target_to_receiver: HashMap<SemanticNodeId, SemanticTransactionNodeRef>,
    source_to_target: HashMap<SemanticNodeId, SemanticNodeId>,
    existing_leaf_pairs: Vec<(SemanticNodeId, SemanticNodeId)>,
}

impl<'a> PersistentFamilyReconcile<'a> {
    fn new(
        store: &'a SemanticStore,
        target_states: &'a HashMap<SemanticNodeId, (SemanticObjectState, Option<VectorPath>)>,
    ) -> Self {
        Self {
            store,
            target_states,
            transaction: SemanticMutationTransaction::new(),
            target_to_receiver: HashMap::new(),
            source_to_target: HashMap::new(),
            existing_leaf_pairs: Vec::new(),
        }
    }

    fn target_node(&self, target: SemanticNodeId) -> Result<PersistentTargetNode, AuthoringError> {
        let node = self.store.node(target).ok_or_else(|| {
            AuthoringError::from(noon_core::SemanticSceneOperationError::UnknownNode(target))
        })?;
        match node.kind() {
            SemanticNodeKind::AuthoringObject => Ok(PersistentTargetNode::Object),
            SemanticNodeKind::Family(presentation) => Ok(PersistentTargetNode::Family {
                members: node.members_iter().collect(),
                z_index: presentation.z_index,
            }),
            _ => Err(AuthoringError::from(
                noon_core::SemanticSceneOperationError::NotSemanticAuthoringNode(target),
            )),
        }
    }

    fn reusable_candidate(
        &self,
        candidate: Option<SemanticNodeId>,
        target: &PersistentTargetNode,
    ) -> Option<SemanticNodeId> {
        let candidate = candidate?;
        if self.source_to_target.contains_key(&candidate) {
            return None;
        }
        let node = self.store.node(candidate)?;
        let compatible = matches!(
            (node.kind(), target),
            (
                SemanticNodeKind::AuthoringObject,
                PersistentTargetNode::Object
            ) | (
                SemanticNodeKind::Family(_),
                PersistentTargetNode::Family { .. }
            )
        );
        compatible.then_some(candidate)
    }

    fn reconcile_node(
        &mut self,
        candidate: Option<SemanticNodeId>,
        target: SemanticNodeId,
    ) -> Result<SemanticTransactionNodeRef, AuthoringError> {
        if let Some(mapped) = self.target_to_receiver.get(&target) {
            return Ok(*mapped);
        }
        let target_node = self.target_node(target)?;
        let reusable = self.reusable_candidate(candidate, &target_node);
        match target_node {
            PersistentTargetNode::Object => {
                let receiver = if let Some(source) = reusable {
                    self.source_to_target.insert(source, target);
                    self.existing_leaf_pairs.push((source, target));
                    SemanticTransactionNodeRef::Existing(source)
                } else {
                    let (state, path) = self.target_states.get(&target).ok_or_else(|| {
                        AuthoringError::from(
                            noon_core::SemanticSceneOperationError::NotSemanticAuthoringNode(
                                target,
                            ),
                        )
                    })?;
                    // Existing path replacement preparation addresses committed
                    // semantic IDs. Do not publish a half-correct pending object if
                    // a rotated non-uniform fit needs a freshly admitted path.
                    if path.is_some() {
                        return Err(AuthoringError::Unsupported(
                            crate::UnsupportedAuthoringOperation::RotatedDimensionStretch,
                        ));
                    }
                    SemanticTransactionNodeRef::Pending(
                        self.transaction
                            .create_node(SemanticNodeCreation::object(state.clone())),
                    )
                };
                self.target_to_receiver.insert(target, receiver);
                Ok(receiver)
            }
            PersistentTargetNode::Family { members, z_index } => {
                let (receiver, reused) = if let Some(source) = reusable {
                    self.source_to_target.insert(source, target);
                    (SemanticTransactionNodeRef::Existing(source), Some(source))
                } else {
                    let pending = self.transaction.create_node(SemanticNodeCreation::family());
                    if z_index != 0.0 {
                        self.transaction.set_z_index(pending, z_index);
                    }
                    (SemanticTransactionNodeRef::Pending(pending), None)
                };
                // Publish the mapping before descending so aliases in the target
                // DAG converge on exactly one receiver-owned identity.
                self.target_to_receiver.insert(target, receiver);
                if let Some(source) = reused {
                    self.reconcile_existing_family(source, &members)?;
                } else {
                    for target_member in members {
                        let member = self.reconcile_node(None, target_member)?;
                        self.transaction.add_member(receiver, member);
                    }
                }
                Ok(receiver)
            }
        }
    }

    fn reconcile_existing_family(
        &mut self,
        source: SemanticNodeId,
        target_members: &[SemanticNodeId],
    ) -> Result<(), AuthoringError> {
        let current = self
            .store
            .semantic_family_members_checked(source)
            .map_err(AuthoringError::from)?;
        let mut desired = Vec::with_capacity(target_members.len());
        for (index, &target_member) in target_members.iter().enumerate() {
            desired.push(self.reconcile_node(current.get(index).copied(), target_member)?);
        }

        let desired_existing: HashSet<_> = desired
            .iter()
            .filter_map(|member| member.existing())
            .collect();
        let current_set: HashSet<_> = current.iter().copied().collect();
        for member in current.iter().copied() {
            if !desired_existing.contains(&member) {
                self.transaction.remove_member(source, member);
            }
        }
        for member in desired
            .iter()
            .copied()
            .filter_map(|member| member.existing())
        {
            if !current_set.contains(&member) {
                self.transaction.add_member(source, member);
            }
        }

        // First establish the relative order of all committed identities. Pending
        // additions can then be inserted before the next committed target member;
        // multiple pending members sharing the same anchor retain target order.
        for member in desired
            .iter()
            .copied()
            .filter_map(|member| member.existing())
        {
            self.transaction.reorder_member(source, member, None);
        }
        for (index, member) in desired.iter().copied().enumerate() {
            if member.existing().is_some() {
                continue;
            }
            self.transaction.add_member(source, member);
            if let Some(before) = desired[index + 1..]
                .iter()
                .find_map(|candidate| candidate.existing())
            {
                self.transaction
                    .reorder_member(source, member, Some(before));
            }
        }
        Ok(())
    }
}

fn prepare_persistent_family_reconcile<E: From<AuthoringError>>(
    source: &MobjectFamily,
    target: &MobjectFamily,
    options: ManimBecomeOptions,
    mut capture: impl FnMut(&Mobject) -> Result<SemanticObjectState, E>,
) -> Result<crate::path_editing::PreparedPathEdits, E> {
    let (source_leaves, target_leaves) = {
        let store = source.integration_store().borrow();
        (
            store
                .ordered_leaf_nodes(source.node_id())
                .map_err(AuthoringError::from)?,
            store
                .ordered_leaf_nodes(target.node_id())
                .map_err(AuthoringError::from)?,
        )
    };
    let mut source_states = Vec::with_capacity(source_leaves.len());
    for source_id in source_leaves {
        source_states.push(capture(&Mobject::from_node(
            Rc::clone(source.integration_store()),
            source_id,
        )?)?);
    }
    let mut captured_targets = Vec::with_capacity(target_leaves.len());
    for &target_id in &target_leaves {
        captured_targets.push(capture(&Mobject::from_node(
            Rc::clone(source.integration_store()),
            target_id,
        )?)?);
    }

    let store = source.integration_store().borrow();
    let fitted_targets = prepare_become_states(&store, &source_states, captured_targets, options)?;
    let target_states: HashMap<_, _> = target_leaves.into_iter().zip(fitted_targets).collect();
    let mut reconcile = PersistentFamilyReconcile::new(&store, &target_states);
    let root = reconcile.reconcile_node(Some(source.node_id()), target.node_id())?;
    debug_assert_eq!(root.existing(), Some(source.node_id()));

    let mut transaction = reconcile.transaction;
    let mut replacements = Vec::new();
    for (source_id, target_id) in reconcile.existing_leaf_pairs {
        let (target_state, path) = target_states
            .get(&target_id)
            .expect("target leaf was captured during persistent become preflight")
            .clone();
        let previous = store
            .semantic_object_state_checked(source_id)
            .map_err(AuthoringError::from)?;
        if let Some(path) = path {
            replacements.push((source_id, target_state, path));
        } else {
            stage_state_changes(&mut transaction, source_id, previous, &target_state);
        }
    }
    Ok(
        crate::path_editing::PreparedPathEdits::prepare(&store, replacements)?
            .with_transaction(transaction),
    )
}

impl MobjectFamily {
    /// Persistently become another family while preserving receiver ownership.
    /// Uses authored state; live execution uses `LiveSession::become_family`.
    /// Matching topology preserves existing identities. Unequal topology is
    /// reconciled atomically: compatible receiver identities are reused, new
    /// receiver-owned nodes get fresh IDs at commit, and target IDs never transfer.
    pub fn become_family(
        &self,
        target: &Self,
        options: ManimBecomeOptions,
    ) -> Result<(), AuthoringError> {
        prepare_family_become(self, target, options, Mobject::state)?.publish(
            &mut self.integration_store().borrow_mut(),
            |store, transaction| {
                transaction
                    .apply(store)
                    .map(|_| ())
                    .map_err(AuthoringError::from)
            },
        )
    }
}

/// Dimension matching applies height then width; stretch overrides both.
/// Center matching runs last, after the target dimensions have been resolved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ManimBecomeOptions {
    pub match_height: bool,
    pub match_width: bool,
    pub match_center: bool,
    pub stretch: bool,
}

pub(crate) fn prepare_become(
    store: &SemanticStore,
    node: SemanticNodeId,
    source: &SemanticObjectState,
    target: SemanticObjectState,
    options: ManimBecomeOptions,
) -> Result<crate::path_editing::PreparedPathEdits, AuthoringError> {
    let (target, path) =
        prepare_become_states(store, std::slice::from_ref(source), vec![target], options)?
            .pop()
            .expect("one target state");
    let mut transaction = SemanticMutationTransaction::new();
    let replacements = if let Some(path) = path {
        vec![(node, target, path)]
    } else {
        stage_state_changes(
            &mut transaction,
            node,
            store.semantic_object_state_checked(node)?,
            &target,
        );
        Vec::new()
    };
    Ok(
        crate::path_editing::PreparedPathEdits::prepare(store, replacements)?
            .with_transaction(transaction),
    )
}

/// Resolve matching once over aggregate family bounds, then transform each
/// captured target leaf exactly once. Captures are transient preparation data.
pub(crate) fn prepare_become_states(
    store: &SemanticStore,
    source: &[SemanticObjectState],
    targets: Vec<SemanticObjectState>,
    options: ManimBecomeOptions,
) -> Result<Vec<(SemanticObjectState, Option<VectorPath>)>, AuthoringError> {
    fn bounds(
        store: &SemanticStore,
        states: &[SemanticObjectState],
    ) -> Result<Option<Bounds2D64>, AuthoringError> {
        let mut total: Option<Bounds2D64> = None;
        for state in states {
            validate_content(store, state.content)?;
            if let Some(bounds) = layout_for_content(store, state.content, state.transform)? {
                if let Some(total) = &mut total {
                    total.include(bounds.min_x, bounds.min_y);
                    total.include(bounds.max_x, bounds.max_y);
                } else {
                    total = Some(bounds);
                }
            }
        }
        Ok(total)
    }
    fn center(
        store: &SemanticStore,
        states: &[SemanticObjectState],
    ) -> Result<(f64, f64), AuthoringError> {
        let mut boundary = None;
        for state in states {
            crate::family_layout::union_bounds(
                &mut boundary,
                crate::semantic_mobject::boundary_for_content(
                    store,
                    state.content,
                    state.transform,
                )?,
            );
        }
        if let Some(bounds) = boundary {
            return Ok((
                (bounds.min_x + bounds.max_x) * 0.5,
                (bounds.min_y + bounds.max_y) * 0.5,
            ));
        }
        if let [state] = states {
            return state_center(store, state);
        }
        Ok((0.0, 0.0))
    }
    let source_bounds = bounds(store, source)?;
    let target_bounds = bounds(store, &targets)?;
    let source_width = source_bounds.map_or(0.0, |b| b.width());
    let source_height = source_bounds.map_or(0.0, |b| b.height());
    let target_width = target_bounds.map_or(0.0, |b| b.width());
    let target_height = target_bounds.map_or(0.0, |b| b.height());
    let (mut x, mut y) = (1.0, 1.0);
    if options.stretch {
        if target_width == 0.0 || target_height == 0.0 {
            return Err(AuthoringError::ZeroStretchTarget);
        }
        x = source_width / target_width;
        y = source_height / target_height;
    } else {
        if options.match_height {
            if target_height == 0.0 {
                return Err(AuthoringError::ZeroMatchHeight);
            }
            x = source_height / target_height;
            y = x;
        }
        if options.match_width {
            let scaled_width = target_width * x;
            if scaled_width == 0.0 {
                return Err(AuthoringError::ZeroMatchWidth);
            }
            let factor = source_width / scaled_width;
            x *= factor;
            y *= factor;
        }
    }
    let target_center = center(store, &targets)?;
    let destination = if options.match_center {
        center(store, source)?
    } else {
        target_center
    };
    let mut result = Vec::with_capacity(targets.len());
    for mut target in targets {
        let Ok((local_x, local_y)) =
            crate::dimension_fit::world_scale_factors(target.transform.rotation_z, x, y)
        else {
            let path = crate::family_affine::world_scaled_path(
                store,
                &target,
                x,
                y,
                target_center,
                destination,
            )?;
            result.push((target, Some(path)));
            continue;
        };
        let old_center = state_center(store, &target)?;
        let next_center = (
            destination.0 + (old_center.0 - target_center.0) * x,
            destination.1 + (old_center.1 - target_center.1) * y,
        );
        scale_state_about_center(store, &mut target, local_x, local_y, next_center)?;
        result.push((target, None));
    }
    Ok(result)
}
