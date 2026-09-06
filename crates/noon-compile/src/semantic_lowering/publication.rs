//! Local lowering for an exclusively prepared authored transaction.
use std::collections::{HashMap, HashSet};

use noon_core::{
    ObjectId, PreparedSemanticMutationTransaction, SemanticMutation, SemanticMutationTransaction,
    SemanticNodeId, SemanticNodeKind, SemanticObjectContent, SemanticObjectProperty,
    SemanticPresentation, SemanticTransactionNodeRef, SemanticTransactionReadError,
};

use super::{
    lower_content, lower_semantic_geometry_value, lower_semantic_style, lower_semantic_style_value,
    lower_semantic_transform, lower_semantic_transform_value, semantic_execution_object_id,
    SemanticCompiledSceneError, SemanticExecutionIndex, SemanticExecutionReachability,
    SemanticExecutionReachabilityUpdate, SemanticExecutionValueError, SemanticGeometryValueError,
    SemanticLoweringError,
};
use crate::{CompiledObject, CompiledResources, ExecutionMutationTransaction, ExecutionPatch};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticPublicationLoweringError {
    UnsupportedMutation {
        index: usize,
    },
    UnsupportedReactiveMembership {
        object: SemanticTransactionNodeRef,
    },
    UnsupportedTextMembership {
        object: SemanticTransactionNodeRef,
    },
    UnsupportedCameraMembership {
        object: SemanticTransactionNodeRef,
    },
    UnsupportedNodeRemoval {
        node: SemanticNodeId,
    },
    PreparedValue {
        object: SemanticTransactionNodeRef,
        error: SemanticExecutionValueError,
    },
    PreparedGeometry {
        object: SemanticTransactionNodeRef,
        error: SemanticGeometryValueError,
    },
    PreparedContent {
        object: SemanticTransactionNodeRef,
        error: SemanticCompiledSceneError,
    },
    PainterOrderInterleaving {
        object: SemanticTransactionNodeRef,
        order: (i32, u64),
        live_tail: (i32, u64),
    },
    Read(SemanticTransactionReadError),
    Value(SemanticLoweringError),
}

impl std::fmt::Display for SemanticPublicationLoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedMutation { index } => write!(
                f,
                "semantic mutation {index} has no incremental live publication contract"
            ),
            Self::UnsupportedReactiveMembership { object } => write!(
                f,
                "semantic object {object:?} has reactive bindings that require incremental reactive lowering"
            ),
            Self::UnsupportedTextMembership { object } => write!(
                f,
                "semantic text object {object:?} requires incremental compiled-resource publication"
            ),
            Self::UnsupportedCameraMembership { object } => write!(
                f,
                "semantic camera object {object:?} requires canonical camera publication"
            ),
            Self::UnsupportedNodeRemoval { node } => write!(
                f,
                "semantic node {}:{} is not a scene object or family and requires non-structural dependency publication",
                node.slot(),
                node.generation()
            ),
            Self::PreparedValue { object, error } => {
                write!(f, "semantic object {object:?} cannot lower for publication: {error}")
            }
            Self::PreparedGeometry { object, error } => write!(
                f,
                "semantic object {object:?} geometry cannot lower for publication: {error}"
            ),
            Self::PreparedContent { object, error } => {
                write!(f, "semantic object {object:?} content cannot lower for publication: {error}")
            }
            Self::PainterOrderInterleaving {
                object,
                order,
                live_tail,
            } => write!(
                f,
                "semantic object {object:?} painter order {order:?} precedes live tail {live_tail:?}; incremental insertion is append-only"
            ),
            Self::Read(error) => error.fmt(f),
            Self::Value(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for SemanticPublicationLoweringError {}
impl From<SemanticLoweringError> for SemanticPublicationLoweringError {
    fn from(error: SemanticLoweringError) -> Self {
        Self::Value(error)
    }
}
impl From<SemanticTransactionReadError> for SemanticPublicationLoweringError {
    fn from(error: SemanticTransactionReadError) -> Self {
        Self::Read(error)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SemanticPublicationPreparationStats {
    pub object_states_lowered: usize,
    pub possible_entries: usize,
    pub possible_exits: usize,
}

#[derive(Clone, Debug)]
struct PreparedEntry {
    object: SemanticTransactionNodeRef,
    compiled: CompiledObject,
    presentation: SemanticPresentation,
}

/// Fully fallible compiler work retained until transaction-local names become IDs.
#[derive(Debug)]
pub struct PreparedSemanticPublication {
    values: ExecutionMutationTransaction,
    resource_additions: CompiledResources,
    entries: Vec<PreparedEntry>,
    possible_exits: Vec<ObjectId>,
    stats: SemanticPublicationPreparationStats,
}

impl PreparedSemanticPublication {
    pub fn value_transaction(&self) -> &ExecutionMutationTransaction {
        &self.values
    }

    pub fn resource_additions(&self) -> &CompiledResources {
        &self.resource_additions
    }

    pub fn possible_exits(&self) -> &[ObjectId] {
        &self.possible_exits
    }

    pub fn possible_entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Conservative create patches for existing detached identities.
    ///
    /// Prepared animation activation uses these only for fallible runtime shape validation before
    /// semantic commit. Exact net entry remains bound from the committed membership update.
    pub fn conservative_existing_entry_patches(&self) -> Vec<ExecutionPatch> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let semantic = entry.object.existing()?;
                let mut compiled = entry.compiled.clone();
                compiled.id = semantic_execution_object_id(semantic);
                Some(ExecutionPatch::CreateObject(compiled))
            })
            .collect()
    }

    pub const fn stats(&self) -> SemanticPublicationPreparationStats {
        self.stats
    }

    /// Bind local names after semantic commit and retain exact net membership only.
    pub fn bind(
        self,
        result: &noon_core::SemanticMutationTransactionResult,
        membership: &SemanticExecutionReachabilityUpdate,
    ) -> BoundSemanticPublication {
        let entered = membership
            .entered_objects()
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        let mut patches = self.values.mutations().to_vec();
        patches.extend(
            membership
                .exited_execution_objects()
                .map(ExecutionPatch::RemoveObject),
        );
        for mut entry in self.entries {
            let semantic = match entry.object {
                SemanticTransactionNodeRef::Existing(node) => node,
                SemanticTransactionNodeRef::Pending(token) => result
                    .resolve(token)
                    .expect("prepared live entry token must commit to a semantic identity"),
            };
            if !entered.contains(&semantic) {
                continue;
            }
            entry.compiled.id = semantic_execution_object_id(semantic);
            patches.push(ExecutionPatch::CreateObject(entry.compiled));
        }
        BoundSemanticPublication {
            transaction: ExecutionMutationTransaction::from_mutations(patches),
            resource_additions: self.resource_additions,
        }
    }
}

#[derive(Debug)]
pub struct BoundSemanticPublication {
    transaction: ExecutionMutationTransaction,
    resource_additions: CompiledResources,
}

impl BoundSemanticPublication {
    pub fn transaction(&self) -> &ExecutionMutationTransaction {
        &self.transaction
    }

    pub fn resource_additions(&self) -> &CompiledResources {
        &self.resource_additions
    }

    pub fn into_parts(self) -> (ExecutionMutationTransaction, CompiledResources) {
        (self.transaction, self.resource_additions)
    }
}

pub fn validate_semantic_publication(
    transaction: &SemanticMutationTransaction,
) -> Result<(), SemanticPublicationLoweringError> {
    validate_mutations(transaction.mutations())
}

fn validate_mutations(
    mutations: &[SemanticMutation],
) -> Result<(), SemanticPublicationLoweringError> {
    for (position, mutation) in mutations.iter().enumerate() {
        if !matches!(
            mutation,
            SemanticMutation::SetProperty { .. }
                | SemanticMutation::ReplaceContent { .. }
                | SemanticMutation::ReplaceStyle { .. }
                | SemanticMutation::AddMember { .. }
                | SemanticMutation::RemoveMember { .. }
                | SemanticMutation::AddNode { .. }
                | SemanticMutation::AddAnimation { .. }
                | SemanticMutation::RemoveNode { .. }
        ) {
            return Err(SemanticPublicationLoweringError::UnsupportedMutation { index: position });
        }
    }
    Ok(())
}

/// Pre-lower every possible entry and conservatively preflight current live exits.
pub fn prepare_semantic_publication(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    index: &SemanticExecutionIndex,
    reachability: &SemanticExecutionReachability,
    live_painter_tail: Option<(i32, u64)>,
) -> Result<PreparedSemanticPublication, SemanticPublicationLoweringError> {
    validate_mutations(prepared.mutations())?;
    let (values, resource_additions) = lower_semantic_publication(prepared, index, reachability)?;
    let mut possible_entry_refs = Vec::new();
    let mut seen_entries = HashSet::new();
    let mut possible_exit_nodes = Vec::new();
    let mut seen_exits = HashSet::new();

    for mutation in prepared.candidate_mutations() {
        match mutation {
            SemanticMutation::AddMember { family, member } => {
                let Some(family) = family.existing() else {
                    continue;
                };
                if reachability.is_reachable(family) && !prepared.node_is_removed(family) {
                    collect_prepared_entry_leaves(
                        prepared,
                        *member,
                        reachability,
                        &mut seen_entries,
                        &mut possible_entry_refs,
                    )?;
                }
            }
            SemanticMutation::RemoveMember { family, member } => {
                if family
                    .existing()
                    .is_some_and(|id| reachability.is_reachable(id))
                {
                    if let Some(member) = member.existing() {
                        collect_existing_exit_leaves(
                            prepared.store(),
                            member,
                            reachability,
                            &mut seen_exits,
                            &mut possible_exit_nodes,
                        )?;
                    }
                }
            }
            SemanticMutation::RemoveNode { node } => {
                if let Some(node) = node.existing() {
                    let kind = prepared.store().node(node).expect(
                        "prepared existing removal must retain a valid pre-commit semantic node",
                    );
                    if !matches!(
                        kind.kind(),
                        SemanticNodeKind::Object(_)
                            | SemanticNodeKind::AuthoringObject
                            | SemanticNodeKind::Family
                    ) {
                        return Err(SemanticPublicationLoweringError::UnsupportedNodeRemoval {
                            node,
                        });
                    }
                    collect_existing_exit_leaves(
                        prepared.store(),
                        node,
                        reachability,
                        &mut seen_exits,
                        &mut possible_exit_nodes,
                    )?;
                }
            }
            _ => {}
        }
    }

    let mut entries = possible_entry_refs
        .into_iter()
        .map(|object| lower_prepared_entry(prepared, object))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.presentation.order_key());
    if let (Some(tail), Some(entry)) = (live_painter_tail, entries.first()) {
        if entry.presentation.order_key() <= tail {
            return Err(SemanticPublicationLoweringError::PainterOrderInterleaving {
                object: entry.object,
                order: entry.presentation.order_key(),
                live_tail: tail,
            });
        }
    }

    let possible_exits = possible_exit_nodes
        .into_iter()
        .map(semantic_execution_object_id)
        .collect::<Vec<_>>();
    let stats = SemanticPublicationPreparationStats {
        object_states_lowered: entries.len(),
        possible_entries: entries.len(),
        possible_exits: possible_exits.len(),
    };
    Ok(PreparedSemanticPublication {
        values,
        resource_additions,
        entries,
        possible_exits,
        stats,
    })
}

fn collect_prepared_entry_leaves(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    node: SemanticTransactionNodeRef,
    reachability: &SemanticExecutionReachability,
    seen: &mut HashSet<SemanticTransactionNodeRef>,
    leaves: &mut Vec<SemanticTransactionNodeRef>,
) -> Result<(), SemanticPublicationLoweringError> {
    if !seen.insert(node) || prepared.node_is_removed(node) {
        return Ok(());
    }
    match prepared.object_state(node) {
        Ok(_) => {
            if node
                .existing()
                .is_none_or(|id| !reachability.is_object_reachable(id))
            {
                leaves.push(node);
            }
            Ok(())
        }
        Err(SemanticTransactionReadError::NotObject(_)) => {
            for member in prepared.family_members(node)? {
                collect_prepared_entry_leaves(prepared, member, reachability, seen, leaves)?;
            }
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn collect_existing_exit_leaves(
    store: &noon_core::SemanticStore,
    node: SemanticNodeId,
    reachability: &SemanticExecutionReachability,
    seen: &mut HashSet<SemanticNodeId>,
    leaves: &mut Vec<SemanticNodeId>,
) -> Result<(), SemanticPublicationLoweringError> {
    if !seen.insert(node) || !reachability.is_reachable(node) {
        return Ok(());
    }
    let semantic = store.node(node).ok_or({
        SemanticLoweringError::Store(noon_core::SemanticStoreError::UnknownNode(node))
    })?;
    match semantic.kind() {
        SemanticNodeKind::Object(_) | SemanticNodeKind::AuthoringObject => {
            let state = semantic.semantic_object_state();
            if state.is_some_and(|state| {
                matches!(state.role(), noon_core::SemanticObjectRole::Camera2D)
            }) {
                return Err(
                    SemanticPublicationLoweringError::UnsupportedCameraMembership {
                        object: node.into(),
                    },
                );
            }
            if state.is_some_and(|state| !state.signal_bindings().is_empty()) {
                return Err(
                    SemanticPublicationLoweringError::UnsupportedReactiveMembership {
                        object: node.into(),
                    },
                );
            }
            if reachability.is_object_reachable(node) {
                leaves.push(node);
            }
        }
        SemanticNodeKind::Family => {
            for member in semantic.members() {
                collect_existing_exit_leaves(store, member, reachability, seen, leaves)?;
            }
        }
        SemanticNodeKind::Signal(_) | SemanticNodeKind::Animation(_) => {}
    }
    Ok(())
}

fn lower_prepared_entry(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    object: SemanticTransactionNodeRef,
) -> Result<PreparedEntry, SemanticPublicationLoweringError> {
    let state = prepared.proposed_object_state(object)?;
    if matches!(state.role(), noon_core::SemanticObjectRole::Camera2D) {
        return Err(SemanticPublicationLoweringError::UnsupportedCameraMembership { object });
    }
    if !state.signal_bindings().is_empty() {
        return Err(SemanticPublicationLoweringError::UnsupportedReactiveMembership { object });
    }
    let geometry = match state.content {
        SemanticObjectContent::Geometry(geometry) => {
            lower_semantic_geometry_value(geometry, Some(prepared.store())).map_err(|error| {
                SemanticPublicationLoweringError::PreparedGeometry { object, error }
            })?
        }
        SemanticObjectContent::Text(_) => {
            return Err(SemanticPublicationLoweringError::UnsupportedTextMembership { object });
        }
    };
    let transform = lower_semantic_transform_value(&state)
        .map_err(|error| SemanticPublicationLoweringError::PreparedValue { object, error })?;
    let style = lower_semantic_style_value(&state)
        .map_err(|error| SemanticPublicationLoweringError::PreparedValue { object, error })?;
    Ok(PreparedEntry {
        object,
        compiled: CompiledObject::new(ObjectId::new(0), geometry, transform, style),
        presentation: state.presentation(),
    })
}

/// Lower only changed content/transform/style values already in this execution domain.
fn lower_semantic_publication(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    index: &SemanticExecutionIndex,
    reachability: &SemanticExecutionReachability,
) -> Result<(ExecutionMutationTransaction, CompiledResources), SemanticPublicationLoweringError> {
    validate_mutations(prepared.mutations())?;
    let mut domains: HashMap<SemanticNodeId, (bool, bool, bool)> = HashMap::new();
    for mutation in prepared.candidate_mutations() {
        match mutation {
            SemanticMutation::SetProperty {
                object, property, ..
            } => {
                let Some(object) = object.existing() else {
                    continue;
                };
                let flags = domains.entry(object).or_default();
                match property {
                    SemanticObjectProperty::Translation
                    | SemanticObjectProperty::Scale
                    | SemanticObjectProperty::RotationZ => flags.0 = true,
                    _ => flags.1 = true,
                }
            }
            SemanticMutation::ReplaceContent { object, .. } => {
                if let Some(object) = object.existing() {
                    domains.entry(object).or_default().2 = true;
                }
            }
            SemanticMutation::ReplaceStyle { object, .. } => {
                if let Some(object) = object.existing() {
                    domains.entry(object).or_default().1 = true;
                }
            }
            SemanticMutation::AddMember { .. }
            | SemanticMutation::RemoveMember { .. }
            | SemanticMutation::AddNode { .. }
            | SemanticMutation::AddAnimation { .. }
            | SemanticMutation::RemoveNode { .. } => {}
            _ => unreachable!("supported vocabulary checked above"),
        }
    }
    let mut mutations = Vec::with_capacity(domains.len() * 3);
    let mut resource_additions = CompiledResources::default();
    for (node, state) in prepared.object_updates() {
        if !reachability.is_reachable(node) {
            continue;
        }
        let Some(object) = index.execution_object_id(node) else {
            continue;
        };
        let (transform, style, content) = domains[&node];
        if content {
            let (content, text_bounds) = lower_content(
                node,
                state.content,
                Some(prepared.store()),
                &mut resource_additions,
            )
            .map_err(|error| SemanticPublicationLoweringError::PreparedContent {
                object: node.into(),
                error,
            })?;
            mutations.push(ExecutionPatch::SetContent {
                object,
                content,
                text_bounds,
            });
        }
        if transform {
            mutations.push(ExecutionPatch::SetTransform {
                object,
                transform: lower_semantic_transform(node, &state)?,
            });
        }
        if style {
            mutations.push(ExecutionPatch::SetStyle {
                object,
                style: lower_semantic_style(node, &state)?,
            });
        }
    }
    Ok((
        ExecutionMutationTransaction::from_mutations(mutations),
        resource_additions,
    ))
}
