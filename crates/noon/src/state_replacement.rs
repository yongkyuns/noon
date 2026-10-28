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

mod effects;
use effects::EffectReplacement;

/// Pair through the same topology/alias contract as ordinary family Transform.
/// All captures and fitting succeed before a caller can publish any edit.
pub(crate) fn prepare_family_become<E: From<AuthoringError>>(
    source: &MobjectFamily,
    target: &MobjectFamily,
    options: ManimBecomeOptions,
    execution: Option<&crate::ExecutionSession>,
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
            let store = source.integration_store().borrow();
            let mut transaction = SemanticMutationTransaction::new();
            stage_paired_family_colors(
                &store,
                &mut transaction,
                source.node_id(),
                target.node_id(),
            )?;
            stage_paired_family_styles(
                &store,
                &mut transaction,
                source.node_id(),
                target.node_id(),
                execution,
            )?;
            let mut effects =
                EffectReplacement::new(&store, execution, options == ManimBecomeOptions::default());
            for (source, target) in store
                .ordered_family_root_pairs(source.node_id(), target.node_id())
                .map_err(AuthoringError::from)?
            {
                effects.stage(source, target, &mut transaction)?;
            }
            return crate::path_editing::PreparedPathEdits::prepare(&store, Vec::new())
                .map(|prepared| {
                    prepared
                        .with_node_retirements(effects.into_retirements())
                        .with_transaction(transaction)
                })
                .map_err(E::from);
        }
        Err(
            noon_core::SemanticFamilyPairingError::TopologyMismatch { .. }
            | noon_core::SemanticFamilyPairingError::AliasMismatch { .. },
        ) => {
            return prepare_persistent_family_reconcile(
                source, target, options, execution, capture,
            );
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
    stage_paired_family_colors(&store, &mut transaction, source.node_id(), target.node_id())?;
    stage_paired_family_styles(
        &store,
        &mut transaction,
        source.node_id(),
        target.node_id(),
        execution,
    )?;
    let mut effects =
        EffectReplacement::new(&store, execution, options == ManimBecomeOptions::default());
    for (source, target) in store
        .ordered_family_root_pairs(source.node_id(), target.node_id())
        .map_err(AuthoringError::from)?
    {
        effects.stage(source, target, &mut transaction)?;
    }
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
        effects.stage(source, target_id, &mut transaction)?;
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
            .with_node_retirements(effects.into_retirements())
            .with_transaction(transaction),
    )
}

/// Shared metadata pairing for become and equal-topology animation completion.
pub(crate) fn paired_family_colors(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<Vec<(SemanticNodeId, crate::Color)>, AuthoringError> {
    store
        .ordered_family_root_pairs(source, target)
        .map_err(AuthoringError::from)?
        .into_iter()
        .map(|(source, target)| {
            Ok((
                source,
                store
                    .node(target)
                    .and_then(noon_core::SemanticNode::family_color)
                    .ok_or_else(|| {
                        AuthoringError::from(noon_core::SemanticFamilyPairingError::UnknownNode(
                            target,
                        ))
                    })?,
            ))
        })
        .collect()
}

fn stage_paired_family_colors(
    store: &SemanticStore,
    transaction: &mut SemanticMutationTransaction,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<(), AuthoringError> {
    for (source, color) in paired_family_colors(store, source, target)? {
        transaction.set_family_color(source, color);
    }
    Ok(())
}

fn stage_paired_family_styles(
    store: &SemanticStore,
    transaction: &mut SemanticMutationTransaction,
    source: SemanticNodeId,
    target: SemanticNodeId,
    execution: Option<&crate::ExecutionSession>,
) -> Result<(), AuthoringError> {
    for (
        source,
        style,
        background_stroke_color,
        background_stroke_width,
        background_stroke_opacity,
    ) in paired_family_style_states(store, source, target, execution)?
    {
        transaction.set_family_style_with_background_stroke_update(
            source,
            style,
            Some(background_stroke_color),
            Some(background_stroke_width),
            Some(background_stroke_opacity),
        );
    }
    for (source, target) in store
        .ordered_family_root_pairs(source, target)
        .map_err(AuthoringError::from)?
    {
        let direction = store
            .node(target)
            .and_then(noon_core::SemanticNode::sheen_direction)
            .ok_or_else(|| {
                AuthoringError::from(noon_core::SemanticFamilyPairingError::UnknownNode(target))
            })?;
        transaction.set_sheen_direction(source, direction);
    }
    Ok(())
}

pub(crate) fn paired_family_styles(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<Vec<(SemanticNodeId, noon_core::SemanticStyle)>, AuthoringError> {
    Ok(paired_family_style_states(store, source, target, None)?
        .into_iter()
        .map(|(source, style, _, _, _)| (source, style))
        .collect())
}

type PairedFamilyStyleState = (
    SemanticNodeId,
    noon_core::SemanticStyle,
    crate::Color,
    f64,
    f64,
);

fn paired_family_style_states(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
    execution: Option<&crate::ExecutionSession>,
) -> Result<Vec<PairedFamilyStyleState>, AuthoringError> {
    store
        .ordered_family_root_pairs(source, target)
        .map_err(AuthoringError::from)?
        .into_iter()
        .map(|(source, target)| {
            let node = store.node(target).ok_or_else(|| {
                AuthoringError::from(noon_core::SemanticFamilyPairingError::UnknownNode(target))
            })?;
            let mut style = node.family_style().cloned().ok_or_else(|| {
                AuthoringError::from(noon_core::SemanticFamilyPairingError::UnknownNode(target))
            })?;
            if let Some(execution) = execution {
                crate::effective_capture::capture_family_style(
                    store, execution, target, &mut style,
                )?;
            }
            let background_stroke_color =
                node.family_background_stroke_color().ok_or_else(|| {
                    AuthoringError::from(noon_core::SemanticFamilyPairingError::UnknownNode(target))
                })?;
            let background_stroke_width =
                node.family_background_stroke_width().ok_or_else(|| {
                    AuthoringError::from(noon_core::SemanticFamilyPairingError::UnknownNode(target))
                })?;
            let background_stroke_opacity =
                node.family_background_stroke_opacity().ok_or_else(|| {
                    AuthoringError::from(noon_core::SemanticFamilyPairingError::UnknownNode(target))
                })?;
            Ok((
                source,
                style,
                background_stroke_color,
                background_stroke_width,
                background_stroke_opacity,
            ))
        })
        .collect()
}

#[derive(Clone)]
enum PersistentTargetNode {
    Object,
    Family {
        members: Vec<SemanticNodeId>,
        own_color: crate::Color,
        own_style: Box<noon_core::SemanticStyle>,
        own_background_stroke_color: crate::Color,
        own_background_stroke_width: f64,
        own_background_stroke_opacity: f64,
        own_sheen_direction: noon_core::SemanticSheenDirection,
    },
}

/// Select receiver metadata for synthesized expansion nodes using the same repeat
/// index rule as Manim's `add_n_more_submobjects`. This does not transfer target
/// identity or painter metadata; it only chooses the receiver-side prototype.
fn aligned_source_prototype(
    source_members: &[SemanticNodeId],
    target_len: usize,
    target_index: usize,
) -> Option<SemanticNodeId> {
    if source_members.is_empty() {
        return None;
    }
    if source_members.len() >= target_len {
        return source_members.get(target_index).copied();
    }
    let source_index = target_index * source_members.len() / target_len;
    source_members.get(source_index).copied()
}

/// Persistent `become()` reconciliation is intentionally separate from Transform
/// pairing. The target graph is a preflight template: target semantic identities
/// are never imported into the receiver. Compatible receiver identities are reused
/// once, while additional topology is allocated only through transaction-local
/// pending references.
struct PersistentFamilyReconcile<'a> {
    store: &'a SemanticStore,
    target_states: &'a HashMap<SemanticNodeId, (SemanticObjectState, Option<VectorPath>)>,
    execution: Option<&'a crate::ExecutionSession>,
    effects: EffectReplacement<'a>,
    transaction: SemanticMutationTransaction,
    target_to_receiver: HashMap<SemanticNodeId, SemanticTransactionNodeRef>,
    source_to_target: HashMap<SemanticNodeId, SemanticNodeId>,
    existing_leaf_pairs: Vec<(SemanticNodeId, SemanticNodeId)>,
}

impl<'a> PersistentFamilyReconcile<'a> {
    fn new(
        store: &'a SemanticStore,
        target_states: &'a HashMap<SemanticNodeId, (SemanticObjectState, Option<VectorPath>)>,
        execution: Option<&'a crate::ExecutionSession>,
    ) -> Self {
        Self {
            store,
            target_states,
            execution,
            effects: EffectReplacement::new(store, execution, false),
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
            SemanticNodeKind::Family(_) => Ok(PersistentTargetNode::Family {
                members: node.members_iter().collect(),
                own_color: node.family_color().expect("family color"),
                own_style: {
                    let mut style = node.family_style().expect("family style").clone();
                    if let Some(execution) = self.execution {
                        crate::effective_capture::capture_family_style(
                            self.store, execution, target, &mut style,
                        )?;
                    }
                    Box::new(style)
                },
                own_background_stroke_color: node
                    .family_background_stroke_color()
                    .expect("family background stroke color"),
                own_background_stroke_width: node
                    .family_background_stroke_width()
                    .expect("family background stroke width"),
                own_background_stroke_opacity: node
                    .family_background_stroke_opacity()
                    .expect("family background stroke opacity"),
                own_sheen_direction: node.sheen_direction().expect("family sheen direction"),
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
        prototype: Option<SemanticNodeId>,
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
                    {
                        let receiver_state = prototype
                            .and_then(|prototype| {
                                self.store.semantic_object_state_checked(prototype).ok()
                            })
                            .cloned()
                            .unwrap_or_else(|| SemanticObjectState::new(state.content))
                            .with_visual_state_from(state);
                        SemanticTransactionNodeRef::Pending(
                            self.transaction
                                .create_node(SemanticNodeCreation::object(receiver_state)),
                        )
                    }
                };
                self.effects
                    .stage(receiver, target, &mut self.transaction)?;
                self.target_to_receiver.insert(target, receiver);
                Ok(receiver)
            }
            PersistentTargetNode::Family {
                members,
                own_color,
                own_style,
                own_background_stroke_color,
                own_background_stroke_width,
                own_background_stroke_opacity,
                own_sheen_direction,
            } => {
                let (receiver, reused, prototype_members) = if let Some(source) = reusable {
                    self.source_to_target.insert(source, target);
                    (
                        SemanticTransactionNodeRef::Existing(source),
                        Some(source),
                        Vec::new(),
                    )
                } else {
                    let pending = self.transaction.create_node(SemanticNodeCreation::family());
                    let (prototype_members, prototype_z_index) = prototype
                        .and_then(|prototype| self.store.node(prototype))
                        .and_then(|node| match node.kind() {
                            SemanticNodeKind::Family(_) => Some((
                                node.members_iter().collect::<Vec<_>>(),
                                node.presentation().expect("family presentation").z_index,
                            )),
                            _ => None,
                        })
                        .unwrap_or_else(|| (Vec::new(), 0.0));
                    if prototype_z_index != 0.0 {
                        self.transaction.set_z_index(pending, prototype_z_index);
                    }
                    (
                        SemanticTransactionNodeRef::Pending(pending),
                        None,
                        prototype_members,
                    )
                };
                self.effects
                    .stage(receiver, target, &mut self.transaction)?;
                self.transaction.set_family_color(receiver, own_color);
                self.transaction
                    .set_family_style_with_background_stroke_update(
                        receiver,
                        *own_style,
                        Some(own_background_stroke_color),
                        Some(own_background_stroke_width),
                        Some(own_background_stroke_opacity),
                    );
                self.transaction
                    .set_sheen_direction(receiver, own_sheen_direction);
                // Publish the mapping before descending so aliases in the target
                // DAG converge on exactly one receiver-owned identity.
                self.target_to_receiver.insert(target, receiver);
                if let Some(source) = reused {
                    self.reconcile_existing_family(source, &members)?;
                } else {
                    for (index, target_member) in members.iter().copied().enumerate() {
                        let member = self.reconcile_node(
                            None,
                            aligned_source_prototype(&prototype_members, members.len(), index),
                            target_member,
                        )?;
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
            desired.push(self.reconcile_node(
                current.get(index).copied(),
                aligned_source_prototype(&current, target_members.len(), index),
                target_member,
            )?);
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
    execution: Option<&crate::ExecutionSession>,
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
    let mut reconcile = PersistentFamilyReconcile::new(&store, &target_states, execution);
    let root = reconcile.reconcile_node(
        Some(source.node_id()),
        Some(source.node_id()),
        target.node_id(),
    )?;
    debug_assert_eq!(root.existing(), Some(source.node_id()));

    let retirements = reconcile.effects.into_retirements();
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
            .with_node_retirements(retirements)
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
        prepare_family_become(self, target, options, None, Mobject::state)?.publish(
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
    other_node: SemanticNodeId,
    source: &SemanticObjectState,
    target: SemanticObjectState,
    options: ManimBecomeOptions,
    execution: Option<&crate::ExecutionSession>,
) -> Result<crate::path_editing::PreparedPathEdits, AuthoringError> {
    let (target, path) =
        prepare_become_states(store, std::slice::from_ref(source), vec![target], options)?
            .pop()
            .expect("one target state");
    let mut transaction = SemanticMutationTransaction::new();
    let mut effects = EffectReplacement::new(store, execution, false);
    effects.stage(node, other_node, &mut transaction)?;
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
            .with_node_retirements(effects.into_retirements())
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
            if let Some(bounds) = layout_for_content(
                store,
                state.content,
                state
                    .transform
                    .as_planar()
                    .ok_or(AuthoringError::NonFiniteObjectState)?,
            )? {
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
                    state
                        .transform
                        .as_planar()
                        .ok_or(AuthoringError::NonFiniteObjectState)?,
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
        let Ok((local_x, local_y)) = crate::dimension_fit::world_scale_factors(
            target
                .transform
                .planar_rotation()
                .ok_or(AuthoringError::NonFiniteObjectState)?,
            x,
            y,
        ) else {
            let (target, path) = crate::family_affine::world_scaled_replacement(
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
