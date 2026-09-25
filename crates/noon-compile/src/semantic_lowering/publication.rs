//! Local lowering for an exclusively prepared authored transaction.
use std::collections::{HashMap, HashSet};

use noon_core::{
    GraphEdgeId, ObjectId, PreparedSemanticMutationTransaction, SemanticGraphEdgeDependency,
    SemanticMutation, SemanticMutationTransaction, SemanticNodeId, SemanticNodeKind,
    SemanticObjectContent, SemanticObjectProperty, SemanticObjectRole,
    SemanticTransactionGraphDeclaration, SemanticTransactionGraphEdgeDependency,
    SemanticTransactionNodeRef, SemanticTransactionReadError,
};

use super::{
    lower_content, lower_scalar_f32, lower_semantic_geometry_value, lower_semantic_style,
    lower_semantic_style_value, lower_semantic_transform, lower_semantic_transform_value,
    semantic_execution_object_id, SemanticCompiledSceneError, SemanticExecutionField,
    SemanticExecutionIndex, SemanticExecutionReachability, SemanticExecutionReachabilityUpdate,
    SemanticExecutionValueError, SemanticGeometryValueError, SemanticLoweringError,
};
use crate::{
    CompiledGraphArrowPolicy, CompiledGraphDependencyDefinition, CompiledGraphDependencyKind,
    CompiledNumericTextDriver, CompiledObject, CompiledResources, ExecutionMutationTransaction,
    ExecutionPatch,
};

#[derive(Clone, Debug)]
pub struct CompiledNumericTextDriverRevisionEntry {
    pub object: ObjectId,
    pub declaration: Option<CompiledNumericTextDriver>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticPublicationLoweringError {
    Resource(crate::CompiledResourceError),
    UnsupportedMutation {
        index: usize,
    },
    UpdaterTargetNotIndexed {
        target: SemanticNodeId,
    },
    RetroactiveUpdaterMutation {
        index: usize,
    },
    UnsupportedReactiveMembership {
        object: SemanticTransactionNodeRef,
    },
    /// A transaction-local text object has no stable semantic identity through
    /// which its pre-owned resource dependencies can be attributed.
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
    PainterOrderRootRequired {
        family: SemanticTransactionNodeRef,
    },
    Read(SemanticTransactionReadError),
    Value(SemanticLoweringError),
}

impl std::fmt::Display for SemanticPublicationLoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resource(error) => error.fmt(f),
            Self::UnsupportedMutation { index } => write!(
                f,
                "semantic mutation {index} has no incremental live publication contract"
            ),
            Self::UpdaterTargetNotIndexed { target } => write!(
                f,
                "live updater target {target:?} requires callback preorder enrollment before execution"
            ),
            Self::RetroactiveUpdaterMutation { index } => write!(
                f,
                "updater mutation {index} precedes the current live frame"
            ),
            Self::UnsupportedReactiveMembership { object } => write!(
                f,
                "semantic object {object:?} has reactive bindings that require incremental reactive lowering"
            ),
            Self::UnsupportedTextMembership { object } => write!(
                f,
                "transaction-local text object {object:?} requires a pre-owned semantic resource scope"
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
                write!(
                    f,
                    "semantic object {object:?} cannot lower for publication: {error}"
                )
            }
            Self::PreparedGeometry { object, error } => write!(
                f,
                "semantic object {object:?} geometry cannot lower for publication: {error}"
            ),
            Self::PreparedContent { object, error } => {
                write!(
                    f,
                    "semantic object {object:?} content cannot lower for publication: {error}"
                )
            }
            Self::PainterOrderRootRequired { family } => write!(
                f,
                "semantic family {family:?} reorder requires an explicit execution root"
            ),
            Self::Read(error) => error.fmt(f),
            Self::Value(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for SemanticPublicationLoweringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Resource(error) => Some(error),
            Self::PreparedValue { error, .. } => Some(error),
            Self::PreparedGeometry { error, .. } => Some(error),
            Self::PreparedContent { error, .. } => Some(error),
            Self::Read(error) => Some(error),
            Self::Value(error) => Some(error),
            _ => None,
        }
    }
}
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
    numeric_text: Option<CompiledNumericTextDriver>,
}

#[derive(Clone, Debug)]
struct PreparedGraphUpdate {
    scope: SemanticNodeId,
    dependencies: Vec<CompiledGraphDependencyDefinition>,
    preflight_dependencies: bool,
}

/// Fully fallible compiler work retained until transaction-local names become IDs.
#[derive(Debug)]
pub struct PreparedSemanticPublication {
    values: ExecutionMutationTransaction,
    resource_additions: CompiledResources,
    entries: Vec<PreparedEntry>,
    possible_exits: Vec<ObjectId>,
    graph_updates: Vec<PreparedGraphUpdate>,
    numeric_text: Vec<CompiledNumericTextDriverRevisionEntry>,
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

    pub fn conservative_graph_patches(&self) -> impl Iterator<Item = ExecutionPatch> + '_ {
        self.graph_updates
            .iter()
            .filter(|update| update.preflight_dependencies)
            .map(|update| ExecutionPatch::SetGraphDependencies {
                owner: semantic_execution_object_id(update.scope),
                dependencies: update.dependencies.clone(),
            })
    }

    /// Conservative create patches using the held transaction's allocator identities.
    ///
    /// Prepared animation activation uses these only for fallible runtime shape validation before
    /// semantic commit. Exact net entry remains bound from the committed membership update.
    pub fn conservative_entry_patches(
        &self,
        prepared: &PreparedSemanticMutationTransaction<'_>,
    ) -> Vec<ExecutionPatch> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let semantic = prepared.planned_node_id(entry.object)?;
                let mut compiled = entry.compiled.clone();
                compiled.id = semantic_execution_object_id(semantic);
                Some(ExecutionPatch::CreateObject(compiled))
            })
            .collect()
    }

    /// Candidate-local numeric driver changes for runtime preflight. Exact
    /// membership is selected after semantic commit from this already-validated
    /// set, so aliases never trigger a second lowering pass.
    pub fn conservative_numeric_text(
        &self,
        prepared: &PreparedSemanticMutationTransaction<'_>,
    ) -> Vec<CompiledNumericTextDriverRevisionEntry> {
        let mut revisions = self.numeric_text.clone();
        revisions.extend(self.entries.iter().filter_map(|entry| {
            let semantic = prepared.planned_node_id(entry.object)?;
            Some(CompiledNumericTextDriverRevisionEntry {
                object: semantic_execution_object_id(semantic),
                declaration: entry.numeric_text.clone(),
            })
        }));
        revisions.extend(self.possible_exits.iter().copied().map(|object| {
            CompiledNumericTextDriverRevisionEntry {
                object,
                declaration: None,
            }
        }));
        revisions
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
        let mut numeric_text = self.numeric_text;
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
            numeric_text.push(CompiledNumericTextDriverRevisionEntry {
                object: entry.compiled.id,
                declaration: entry.numeric_text,
            });
            patches.push(ExecutionPatch::CreateObject(entry.compiled));
        }
        let mut active_graphs = membership
            .entered_graph_roots()
            .iter()
            .chain(membership.updated_graph_roots())
            .copied()
            .collect::<HashSet<_>>();
        for scope in membership.exited_graph_roots() {
            active_graphs.remove(scope);
        }
        for update in self.graph_updates {
            if active_graphs.contains(&update.scope) {
                patches.push(ExecutionPatch::SetGraphDependencies {
                    owner: semantic_execution_object_id(update.scope),
                    dependencies: update.dependencies,
                });
            }
        }
        patches.extend(
            membership
                .exited_graph_roots()
                .iter()
                .copied()
                .map(|scope| ExecutionPatch::SetGraphDependencies {
                    owner: semantic_execution_object_id(scope),
                    dependencies: Vec::new(),
                }),
        );
        numeric_text.extend(membership.exited_execution_objects().map(|object| {
            CompiledNumericTextDriverRevisionEntry {
                object,
                declaration: None,
            }
        }));
        BoundSemanticPublication {
            transaction: ExecutionMutationTransaction::from_mutations(patches),
            resource_additions: self.resource_additions,
            numeric_text,
        }
    }
}

#[derive(Debug)]
pub struct BoundSemanticPublication {
    transaction: ExecutionMutationTransaction,
    resource_additions: CompiledResources,
    numeric_text: Vec<CompiledNumericTextDriverRevisionEntry>,
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

    pub fn numeric_text(&self) -> &[CompiledNumericTextDriverRevisionEntry] {
        &self.numeric_text
    }

    pub fn into_parts_with_numeric_text(
        self,
    ) -> (
        ExecutionMutationTransaction,
        CompiledResources,
        Vec<CompiledNumericTextDriverRevisionEntry>,
    ) {
        (self.transaction, self.resource_additions, self.numeric_text)
    }
}

/// Registration-only batches use callback-plan lowering, not the property lane.
/// Mixed structural/registration transactions remain unsupported until their
/// proposed target preorder can be preflighted together.
pub fn is_semantic_updater_publication(mutations: &[SemanticMutation]) -> bool {
    !mutations.is_empty()
        && mutations.iter().all(|mutation| {
            matches!(
                mutation,
                SemanticMutation::AddUpdater { .. }
                    | SemanticMutation::RemoveUpdater { .. }
                    | SemanticMutation::ClearUpdaters { .. }
            )
        })
}

/// Prepare the callback-plan revision and the empty geometry projection together.
/// The caller must commit both after all semantic/runtime preflight succeeds.
pub fn prepare_semantic_updater_publication(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    callbacks: &super::SemanticHostCallbackPlan,
    current_time: f64,
) -> Result<
    (
        PreparedSemanticPublication,
        Option<super::SemanticHostCallbackRevision>,
    ),
    SemanticPublicationLoweringError,
> {
    let revised = callbacks.prepare_registration_revision(prepared, current_time)?;
    Ok((
        PreparedSemanticPublication {
            values: ExecutionMutationTransaction::from_mutations(Vec::new()),
            resource_additions: CompiledResources::default(),
            entries: Vec::new(),
            possible_exits: Vec::new(),
            graph_updates: Vec::new(),
            numeric_text: Vec::new(),
            stats: SemanticPublicationPreparationStats::default(),
        },
        revised,
    ))
}

pub fn validate_semantic_publication(
    transaction: &SemanticMutationTransaction,
) -> Result<(), SemanticPublicationLoweringError> {
    validate_mutations(transaction.mutations(), None, None)
}

fn validate_mutations(
    mutations: &[SemanticMutation],
    handled_scalar_signals: Option<&HashSet<SemanticNodeId>>,
    prepared: Option<&PreparedSemanticMutationTransaction<'_>>,
) -> Result<(), SemanticPublicationLoweringError> {
    for (position, mutation) in mutations.iter().enumerate() {
        let ordinary = matches!(
            mutation,
            SemanticMutation::SetProperty { .. }
                | SemanticMutation::ReplaceContent { .. }
                | SemanticMutation::SetBarMetadata { .. }
                | SemanticMutation::SetInset2DView { .. }
                | SemanticMutation::ReplaceDecimalNumber { .. }
                | SemanticMutation::ReplaceTextPresentationBaseline { .. }
                | SemanticMutation::ReplaceStyle { .. }
                | SemanticMutation::SetZIndex { .. }
                | SemanticMutation::SetForegroundMembers { .. }
                | SemanticMutation::SetGraphDeclaration { .. }
                | SemanticMutation::SetTableLayout { .. }
                | SemanticMutation::AddMember { .. }
                | SemanticMutation::RemoveMember { .. }
                | SemanticMutation::ReorderMember { .. }
                | SemanticMutation::AddNode { .. }
                | SemanticMutation::AddAnimation { .. }
                | SemanticMutation::RemoveNode { .. }
        );
        let handled_scalar = handled_scalar_signals.is_some_and(|signals| match mutation {
            SemanticMutation::AddScalarSignalTrack { signal, .. }
            | SemanticMutation::SetScalarSignalAt { signal, .. } => signals.contains(signal),
            SemanticMutation::ScopeSignal { signal, .. } => signal
                .existing()
                .or_else(|| prepared.and_then(|prepared| prepared.planned_node_id(*signal)))
                .is_some_and(|signal| signals.contains(&signal)),
            _ => false,
        });
        if !ordinary && !handled_scalar {
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
) -> Result<PreparedSemanticPublication, SemanticPublicationLoweringError> {
    prepare_semantic_publication_with_handled_scalar_signals(prepared, index, reachability, None)
}

/// Prepare ordinary publication while accepting only scalar mutations whose
/// signals were already lowered and preflighted by the caller's timeline lane.
pub fn prepare_semantic_publication_with_scalar_timeline(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    index: &SemanticExecutionIndex,
    reachability: &SemanticExecutionReachability,
    handled_scalar_signals: &HashSet<SemanticNodeId>,
) -> Result<PreparedSemanticPublication, SemanticPublicationLoweringError> {
    prepare_semantic_publication_with_handled_scalar_signals(
        prepared,
        index,
        reachability,
        Some(handled_scalar_signals),
    )
}

fn prepare_semantic_publication_with_handled_scalar_signals(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    index: &SemanticExecutionIndex,
    reachability: &SemanticExecutionReachability,
    handled_scalar_signals: Option<&HashSet<SemanticNodeId>>,
) -> Result<PreparedSemanticPublication, SemanticPublicationLoweringError> {
    validate_mutations(prepared.mutations(), handled_scalar_signals, Some(prepared))?;
    let (values, mut resource_additions, numeric_text) =
        lower_semantic_publication(prepared, index, reachability, handled_scalar_signals)?;
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
                        SemanticNodeKind::AuthoringObject | SemanticNodeKind::Family(_)
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

    let entries = possible_entry_refs
        .into_iter()
        .map(|object| lower_prepared_entry(prepared, object, &mut resource_additions))
        .collect::<Result<Vec<_>, _>>()?;

    let possible_exits = possible_exit_nodes
        .into_iter()
        .map(semantic_execution_object_id)
        .collect::<Vec<_>>();
    let graph_updates = prepare_graph_updates(prepared, reachability)?;
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
        graph_updates,
        numeric_text,
        stats,
    })
}

fn prepare_graph_updates(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    reachability: &SemanticExecutionReachability,
) -> Result<Vec<PreparedGraphUpdate>, SemanticPublicationLoweringError> {
    let staged = prepared
        .candidate_mutations()
        .filter_map(|mutation| match mutation {
            SemanticMutation::SetGraphDeclaration { scope, graph } => {
                prepared.planned_node_id(*scope).map(|scope| (scope, graph))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut entering = HashSet::new();
    let mut exiting = HashSet::new();
    for mutation in prepared.candidate_mutations() {
        match mutation {
            SemanticMutation::AddMember { family, member }
                if family
                    .existing()
                    .is_some_and(|family| reachability.is_reachable(family)) =>
            {
                collect_prepared_graph_roots(
                    prepared,
                    *member,
                    &staged,
                    &mut HashSet::new(),
                    &mut entering,
                )?;
            }
            SemanticMutation::RemoveMember { family, member }
                if family
                    .existing()
                    .is_some_and(|family| reachability.is_reachable(family)) =>
            {
                if let Some(member) = member.existing() {
                    collect_existing_graph_roots(
                        prepared.store(),
                        member,
                        &mut HashSet::new(),
                        &mut exiting,
                    )?;
                }
            }
            _ => {}
        }
    }

    let mut roots = staged.keys().copied().collect::<HashSet<_>>();
    roots.extend(entering.iter().copied());
    roots.extend(exiting.iter().copied());
    let mut roots = roots.into_iter().collect::<Vec<_>>();
    roots.sort_unstable();
    roots
        .into_iter()
        .map(|scope| {
            let dependencies = if let Some(graph) = staged.get(&scope) {
                lower_transaction_graph(prepared, scope, graph)?
            } else if let Some(graph) = prepared
                .store()
                .semantic_graph_declaration(scope)
                .map_err(SemanticLoweringError::from)?
            {
                lower_existing_graph(prepared, scope, graph)?
            } else {
                Vec::new()
            };
            Ok(PreparedGraphUpdate {
                scope,
                dependencies,
                preflight_dependencies: entering.contains(&scope)
                    || (staged.contains_key(&scope) && reachability.is_reachable(scope)),
            })
        })
        .collect()
}

fn collect_prepared_graph_roots(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    node: SemanticTransactionNodeRef,
    staged: &HashMap<SemanticNodeId, &SemanticTransactionGraphDeclaration>,
    seen: &mut HashSet<SemanticNodeId>,
    roots: &mut HashSet<SemanticNodeId>,
) -> Result<(), SemanticPublicationLoweringError> {
    let Some(id) = prepared.planned_node_id(node) else {
        return Ok(());
    };
    if !seen.insert(id) {
        return Ok(());
    }
    if staged.contains_key(&id)
        || prepared
            .store()
            .semantic_graph_declaration(id)
            .ok()
            .flatten()
            .is_some()
    {
        roots.insert(id);
    }
    match prepared.family_members(node) {
        Ok(members) => {
            for member in members {
                collect_prepared_graph_roots(prepared, member, staged, seen, roots)?;
            }
        }
        Err(SemanticTransactionReadError::NotFamily(_)) => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn collect_existing_graph_roots(
    store: &noon_core::SemanticStore,
    node: SemanticNodeId,
    seen: &mut HashSet<SemanticNodeId>,
    roots: &mut HashSet<SemanticNodeId>,
) -> Result<(), SemanticPublicationLoweringError> {
    if !seen.insert(node) {
        return Ok(());
    }
    let semantic = store.node(node).ok_or_else(|| {
        SemanticLoweringError::Store(noon_core::SemanticStoreError::UnknownNode(node))
    })?;
    if semantic.graph_declaration().is_some() {
        roots.insert(node);
    }
    if matches!(semantic.kind(), SemanticNodeKind::Family(_)) {
        for member in semantic.members_iter() {
            collect_existing_graph_roots(store, member, seen, roots)?;
        }
    }
    Ok(())
}

fn lower_transaction_graph(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    scope: SemanticNodeId,
    graph: &SemanticTransactionGraphDeclaration,
) -> Result<Vec<CompiledGraphDependencyDefinition>, SemanticPublicationLoweringError> {
    let vertices = graph.vertices().iter().copied().collect::<HashMap<_, _>>();
    let edges = graph
        .edges()
        .iter()
        .copied()
        .map(|binding| (binding.id(), binding))
        .collect::<HashMap<_, _>>();
    graph
        .topology()
        .edges()
        .map(|edge| {
            let binding = edges[&edge.id];
            lower_graph_dependency(
                prepared,
                scope,
                edge.id,
                vertices[&edge.start],
                vertices[&edge.end],
                binding.line(),
                binding.dependency(),
            )
        })
        .collect()
}

fn lower_existing_graph(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    scope: SemanticNodeId,
    graph: &noon_core::SemanticGraphDeclaration,
) -> Result<Vec<CompiledGraphDependencyDefinition>, SemanticPublicationLoweringError> {
    graph
        .topology()
        .edges()
        .map(|edge| {
            let binding = graph
                .edge_binding(edge.id)
                .expect("validated graph edge binding");
            let dependency = match binding.dependency() {
                SemanticGraphEdgeDependency::Line => SemanticTransactionGraphEdgeDependency::Line,
                SemanticGraphEdgeDependency::Arrow {
                    end_tip,
                    start_tip,
                    policy,
                } => SemanticTransactionGraphEdgeDependency::Arrow {
                    end_tip: end_tip.into(),
                    start_tip: start_tip.map(Into::into),
                    policy,
                },
            };
            lower_graph_dependency(
                prepared,
                scope,
                edge.id,
                graph
                    .vertex_node(edge.start)
                    .expect("validated start binding")
                    .into(),
                graph
                    .vertex_node(edge.end)
                    .expect("validated end binding")
                    .into(),
                binding.line().into(),
                dependency,
            )
        })
        .collect()
}

fn lower_graph_dependency(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    scope: SemanticNodeId,
    edge: GraphEdgeId,
    start: SemanticTransactionNodeRef,
    end: SemanticTransactionNodeRef,
    line: SemanticTransactionNodeRef,
    dependency: SemanticTransactionGraphEdgeDependency,
) -> Result<CompiledGraphDependencyDefinition, SemanticPublicationLoweringError> {
    let resolve = |node| {
        prepared
            .planned_node_id(node)
            .map(semantic_execution_object_id)
            .expect("validated graph dependency survives the prepared transaction")
    };
    let kind = match dependency {
        SemanticTransactionGraphEdgeDependency::Line => CompiledGraphDependencyKind::Line,
        SemanticTransactionGraphEdgeDependency::Arrow {
            end_tip,
            start_tip,
            policy,
        } => {
            let line_id = prepared
                .planned_node_id(line)
                .expect("validated graph line survives preparation");
            let shaft = prepared.proposed_object_state(line)?;
            let SemanticObjectRole::ArrowShaft(shaft_policy) = shaft.role() else {
                return Err(SemanticLoweringError::InvalidGraphDependency {
                    root: scope,
                    edge,
                    reason: "Arrow shaft lost its authored shaft role",
                }
                .into());
            };
            let lower = |field, value| {
                lower_scalar_f32(field, value).map_err(|error| error.with_node(line_id))
            };
            CompiledGraphDependencyKind::Arrow {
                end_tip: resolve(end_tip),
                start_tip: start_tip.map(resolve),
                policy: CompiledGraphArrowPolicy::new(
                    lower(SemanticExecutionField::GraphArrowBuff, policy.buff())?,
                    lower(
                        SemanticExecutionField::GraphArrowTipLength,
                        policy.tip_length(),
                    )?,
                    lower(
                        SemanticExecutionField::GraphArrowTipLengthRatio,
                        policy.max_tip_length_to_length_ratio(),
                    )?,
                    lower(
                        SemanticExecutionField::GraphArrowInitialStrokeWidth,
                        shaft_policy.initial_stroke_width(),
                    )?,
                    lower(
                        SemanticExecutionField::GraphArrowStrokeWidthRatio,
                        shaft_policy.max_stroke_width_to_length_ratio(),
                    )?,
                ),
            }
        }
    };
    Ok(CompiledGraphDependencyDefinition {
        edge,
        start_vertex: resolve(start),
        end_vertex: resolve(end),
        line: resolve(line),
        kind,
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
        Err(
            SemanticTransactionReadError::NotObject(_)
            | SemanticTransactionReadError::Existing(
                noon_core::SemanticSceneOperationError::NotSemanticObject(_),
            ),
        ) => {
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
        SemanticNodeKind::AuthoringObject => {
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
        SemanticNodeKind::Family(_) => {
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
    resource_additions: &mut CompiledResources,
) -> Result<PreparedEntry, SemanticPublicationLoweringError> {
    let state = prepared.proposed_object_state(object)?;
    if matches!(state.role(), noon_core::SemanticObjectRole::Camera2D) {
        return Err(SemanticPublicationLoweringError::UnsupportedCameraMembership { object });
    }
    if !state.signal_bindings().is_empty() {
        return Err(SemanticPublicationLoweringError::UnsupportedReactiveMembership { object });
    }
    let (content, text_bounds) = match state.content {
        SemanticObjectContent::Geometry(content) => (
            lower_semantic_geometry_value(content, Some(prepared.store()))
                .map_err(|error| SemanticPublicationLoweringError::PreparedGeometry {
                    object,
                    error,
                })?
                .into(),
            None,
        ),
        SemanticObjectContent::Image(image) => {
            let content = resource_additions
                .capture_image(prepared.store(), image)
                .map_err(SemanticPublicationLoweringError::Resource)?;
            (content.into(), None)
        }
        SemanticObjectContent::Text(text) => {
            let node = object
                .existing()
                .ok_or(SemanticPublicationLoweringError::UnsupportedTextMembership { object })?;
            lower_content(
                node,
                SemanticObjectContent::Text(text),
                Some(prepared.store()),
                resource_additions,
            )
            .map_err(|error| SemanticPublicationLoweringError::PreparedContent { object, error })?
        }
    };
    let transform = lower_semantic_transform_value(&state)
        .map_err(|error| SemanticPublicationLoweringError::PreparedValue { object, error })?;
    let style = lower_semantic_style_value(&state)
        .map_err(|error| SemanticPublicationLoweringError::PreparedValue { object, error })?;
    let mut compiled = CompiledObject::new(ObjectId::new(0), content, transform, style);
    compiled.text_bounds = text_bounds;
    compiled.base_z_index = state.z_index();
    let numeric_text = lower_numeric_text_driver(&state, prepared.store(), resource_additions)?;
    Ok(PreparedEntry {
        object,
        compiled,
        numeric_text,
    })
}

/// Lower only changed content/transform/style values already in this execution domain.
fn lower_semantic_publication(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    index: &SemanticExecutionIndex,
    reachability: &SemanticExecutionReachability,
    handled_scalar_signals: Option<&HashSet<SemanticNodeId>>,
) -> Result<
    (
        ExecutionMutationTransaction,
        CompiledResources,
        Vec<CompiledNumericTextDriverRevisionEntry>,
    ),
    SemanticPublicationLoweringError,
> {
    validate_mutations(prepared.mutations(), handled_scalar_signals, Some(prepared))?;
    let mut domains: HashMap<SemanticNodeId, (bool, bool, bool, bool, bool)> = HashMap::new();
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
            SemanticMutation::ReplaceDecimalNumber { object, .. } => {
                if let Some(object) = object.existing() {
                    domains.entry(object).or_default().4 = true;
                }
            }
            SemanticMutation::ReplaceTextPresentationBaseline { .. } => {}
            SemanticMutation::SetZIndex { node, .. } => {
                if let Some(object) = node.existing() {
                    domains.entry(object).or_default().3 = true;
                }
            }
            SemanticMutation::ReplaceStyle { object, .. } => {
                if let Some(object) = object.existing() {
                    domains.entry(object).or_default().1 = true;
                }
            }
            SemanticMutation::SetBarMetadata { .. } => {}
            SemanticMutation::AddMember { .. }
            | SemanticMutation::RemoveMember { .. }
            | SemanticMutation::ReorderMember { .. }
            | SemanticMutation::AddNode { .. }
            | SemanticMutation::AddAnimation { .. }
            | SemanticMutation::RemoveNode { .. }
            | SemanticMutation::AddScalarSignalTrack { .. }
            | SemanticMutation::SetScalarSignalAt { .. }
            | SemanticMutation::ScopeSignal { .. }
            | SemanticMutation::SetForegroundMembers { .. }
            | SemanticMutation::SetGraphDeclaration { .. }
            | SemanticMutation::SetTableLayout { .. }
            | SemanticMutation::SetInset2DView { .. } => {}
            _ => unreachable!("supported vocabulary checked above"),
        }
    }
    let mut mutations = Vec::with_capacity(domains.len() * 3);
    let mut resource_additions = CompiledResources::default();
    let mut numeric_text = Vec::new();
    for (node, state) in prepared.object_updates() {
        if !reachability.is_reachable(node) {
            continue;
        }
        let Some(object) = index.execution_object_id(node) else {
            continue;
        };
        let (transform, style, content, z_index, numeric) = domains[&node];
        if numeric {
            numeric_text.push(CompiledNumericTextDriverRevisionEntry {
                object,
                declaration: lower_numeric_text_driver(
                    &state,
                    prepared.store(),
                    &mut resource_additions,
                )?,
            });
        }
        if z_index {
            mutations.push(ExecutionPatch::SetZIndex {
                object,
                value: state.z_index(),
            });
        }
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
        numeric_text,
    ))
}

fn lower_numeric_text_driver(
    state: &noon_core::SemanticObjectState,
    store: &noon_core::SemanticStore,
    resources: &mut CompiledResources,
) -> Result<Option<CompiledNumericTextDriver>, SemanticPublicationLoweringError> {
    let Some(number) = state.decimal_number() else {
        return Ok(None);
    };
    let Some(binding) = number.binding() else {
        return Ok(None);
    };
    for (_, handle) in binding.token_resources() {
        resources
            .capture_text(store, *handle)
            .map_err(SemanticPublicationLoweringError::Resource)?;
    }
    Ok(Some(CompiledNumericTextDriver {
        signal: super::semantic_execution_signal_id(binding.signal()),
        object_index: 0,
        format: noon_core::DecimalFormat {
            decimal_places: number.decimal_places(),
            include_sign: number.include_sign(),
            group_with_commas: number.group_with_commas(),
            show_ellipsis: number.show_ellipsis(),
            unit: number.unit().map(str::to_owned),
        },
        font_size: number.font_size(),
        point_to_scene_scale: binding.point_to_scene_scale(),
        token_resources: binding.token_resources().to_vec().into(),
    }))
}

#[cfg(test)]
mod tests {
    use noon_core::{RateFunction, SemanticStore, TrackTiming};

    use super::*;

    #[test]
    fn scalar_publication_contract_accepts_only_explicitly_preflighted_signals() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let other = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_scalar_signal_track(
            signal,
            0.0,
            2.0,
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        );

        assert!(matches!(
            validate_semantic_publication(&transaction),
            Err(SemanticPublicationLoweringError::UnsupportedMutation { index: 0 })
        ));
        assert!(matches!(
            validate_mutations(transaction.mutations(), Some(&HashSet::from([other])), None),
            Err(SemanticPublicationLoweringError::UnsupportedMutation { index: 0 })
        ));
        assert!(validate_mutations(
            transaction.mutations(),
            Some(&HashSet::from([signal])),
            None
        )
        .is_ok());
    }
}
