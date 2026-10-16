//! Bounded graph declaration replacement and author-time placement.
//!
//! The wrapper maps below are only used to translate public keys. Every edit
//! starts from the semantic declaration and publishes one final replacement
//! declaration with ordinary family membership changes in the same transaction.

use super::{
    construction::{arrow_options, line_options, vertex_options, GraphPublication},
    GraphAuthoringError, GraphEdgeEntry, GraphEdgeMobject, GraphOptions, GraphVertexEntry,
    RetainedGraph,
};
use crate::{
    arrow_authoring::{resolve_staged_arrow, stage_prepared_arrow, PreparedArrow, StagedArrow},
    AuthoringError, ManimArrow, ManimGeometryOptions, Mobject, MobjectFamily,
};
use noon_core::{
    GraphEdge, GraphEdgeId, GraphTopology, GraphVertexId, SemanticGraphEdgeBinding,
    SemanticGraphEdgeDependency, SemanticLocalNodeToken, SemanticMutationTransaction,
    SemanticNodeCreation, SemanticObjectProperty, SemanticTransactionGraphDeclaration,
    SemanticTransactionGraphEdgeBinding, SemanticTransactionNodeRef,
};
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    rc::Rc,
};

/// A deterministic author-time layout. Layout calculations never run on the
/// playback path; the resulting positions are persistent object translations.
///
/// `Random` uses Noon's seeded stream unless prepared NetworkX positions are
/// supplied. `Spring` accepts prepared initial positions and performs
/// NetworkX-compatible force integration here; `Tree` ports Manim's ordered
/// depth-first placement. Layouts are author-time, not playback operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphLayout {
    Circular,
    Shell,
    Partite,
    Spiral,
    Random,
    Spring,
    Spectral,
    Tree,
}

#[derive(Clone, Debug)]
pub struct GraphLayoutOptions {
    pub layout: GraphLayout,
    pub scale: f64,
    /// Optional independent vertical scale for 2D layouts that support it.
    /// `None` preserves the scalar `scale` behavior.
    pub scale_y: Option<f64>,
    pub center: (f64, f64),
    pub seed: u32,
    pub iterations: usize,
    pub threshold: f64,
    /// Optional NetworkX shell membership, expressed as vertex indices.
    /// Empty means one shell containing every vertex.
    pub shells: Vec<Vec<usize>>,
    /// Angle increment between shells; `None` uses `PI / shell_count`.
    pub rotate: Option<f64>,
    /// Angle step used by the NetworkX 2D spiral layout.
    pub resolution: f64,
    /// Space spiral points by chord distance instead of angle.
    pub equidistant: bool,
    /// Ordered, non-empty multipartite layers expressed as vertex indices.
    pub partitions: Vec<Vec<usize>>,
    /// Swap the two rescaled axes for NetworkX's horizontal multipartite view.
    pub align_horizontal: bool,
    /// Optional Fruchterman-Reingold optimal distance; `None` uses `1 / sqrt(n)`.
    pub spring_k: Option<f64>,
    /// Prepared NumPy-compatible initial positions for NetworkX spring layout.
    pub spring_positions: Option<Vec<(f64, f64)>>,
    /// Fixed spring vertices; `Some([])` still disables final rescaling.
    pub spring_fixed: Option<Vec<usize>>,
    /// Whether spring positions are rescaled and shifted after integration.
    pub spring_rescale: bool,
    /// Preserve directed adjacency in the force calculation.
    pub directed: bool,
    /// Prepared NetworkX random-layout positions, including its center offset.
    pub random_positions: Option<Vec<(f64, f64)>>,
    /// Root vertex index for Manim's deterministic tree layout.
    pub tree_root: Option<usize>,
    /// Place deeper tree levels above the root instead of below it.
    pub tree_orientation_up: bool,
    /// Optional horizontal and vertical spacing for the tree layout.
    pub tree_vertex_spacing: Option<(f64, f64)>,
}

impl Default for GraphLayoutOptions {
    fn default() -> Self {
        Self {
            layout: GraphLayout::Spring,
            scale: 2.0,
            scale_y: None,
            center: (0.0, 0.0),
            seed: 0,
            iterations: 50,
            threshold: 1e-4,
            shells: Vec::new(),
            rotate: None,
            resolution: 0.35,
            equidistant: false,
            partitions: Vec::new(),
            align_horizontal: false,
            spring_k: None,
            spring_positions: None,
            spring_fixed: None,
            spring_rescale: true,
            directed: false,
            random_positions: None,
            tree_root: None,
            tree_orientation_up: false,
            tree_vertex_spacing: None,
        }
    }
}

impl GraphLayoutOptions {
    /// Prepare positions without allocating semantic objects. Edge endpoints are
    /// indices in the supplied vertex order, allowing constructors and retained
    /// layout changes to share the same deterministic layout implementation.
    pub fn positions(
        &self,
        vertex_count: usize,
        edges: &[(usize, usize)],
    ) -> Result<Vec<(f64, f64)>, GraphAuthoringError> {
        if edges
            .iter()
            .any(|&(start, end)| start >= vertex_count || end >= vertex_count)
        {
            return Err(GraphAuthoringError::InvalidLayout(
                "layout edge endpoints must reference existing vertices",
            ));
        }
        graph_layout(vertex_count, edges, self)
    }
}

/// Handles affected by one committed topology mutation.
#[derive(Clone, Debug, Default)]
pub struct GraphMutationResult {
    pub added_vertices: Vec<Mobject>,
    pub added_edges: Vec<GraphEdgeMobject>,
    pub removed_vertices: Vec<Mobject>,
    pub removed_edges: Vec<GraphEdgeMobject>,
}

// Short-lived owned batch preparations stay inline to avoid an extra allocation
// per edge when constructing or extending a large graph.
#[allow(clippy::large_enum_variant)]
enum PreparedNewEdge {
    Line(ManimGeometryOptions),
    Arrow(PreparedArrow),
}

enum StagedNewEdge {
    Line {
        edge: GraphEdge,
        family: SemanticLocalNodeToken,
        line: SemanticLocalNodeToken,
    },
    Arrow {
        edge: GraphEdge,
        arrow: StagedArrow,
    },
}

fn require_publication<K, P: GraphPublication>(
    publication: &P,
    graph: &RetainedGraph<K>,
) -> Result<(), GraphAuthoringError> {
    if !Rc::ptr_eq(&publication.graph_store(), graph.family.integration_store()) {
        return Err(GraphAuthoringError::Authoring(AuthoringError::ForeignStore));
    }
    graph.family.validate()?;
    Ok(())
}

fn staged_binding(binding: SemanticGraphEdgeBinding) -> SemanticTransactionGraphEdgeBinding {
    match binding.dependency() {
        SemanticGraphEdgeDependency::Line => SemanticTransactionGraphEdgeBinding::new(
            binding.id(),
            binding.family().into(),
            binding.line().into(),
        ),
        SemanticGraphEdgeDependency::Arrow {
            end_tip,
            start_tip,
            policy,
        } => SemanticTransactionGraphEdgeBinding::new_arrow(
            binding.id(),
            binding.family().into(),
            binding.line().into(),
            end_tip.into(),
            start_tip.map(Into::into),
            policy,
        ),
    }
}

fn replace_declaration(
    transaction: &mut SemanticMutationTransaction,
    root: noon_core::SemanticNodeId,
    topology: GraphTopology,
    vertices: impl IntoIterator<Item = (GraphVertexId, SemanticTransactionNodeRef)>,
    edges: impl IntoIterator<Item = SemanticTransactionGraphEdgeBinding>,
) {
    transaction.set_graph_declaration(
        root,
        SemanticTransactionGraphDeclaration::new(topology, vertices, edges),
    );
}

pub(super) fn add_vertices<K, V, P>(
    publication: &mut P,
    graph: &mut RetainedGraph<K>,
    vertices: V,
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    V: IntoIterator<Item = (K, (f64, f64))>,
    P: GraphPublication,
{
    let options = graph.options.clone();
    add_vertices_with_options(
        publication,
        graph,
        vertices
            .into_iter()
            .map(|(key, position)| (key, position, options.clone())),
    )
}

pub(super) fn add_vertices_with_options<K, V, P>(
    publication: &mut P,
    graph: &mut RetainedGraph<K>,
    vertices: V,
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    V: IntoIterator<Item = (K, (f64, f64), GraphOptions)>,
    P: GraphPublication,
{
    require_publication(publication, graph)?;
    let vertices = vertices.into_iter().collect::<Vec<_>>();
    let mut seen = HashSet::new();
    for (key, position, options) in &vertices {
        if graph.vertex_lookup.contains_key(key) || !seen.insert(key.clone()) {
            return Err(GraphAuthoringError::DuplicateMutationVertexKey);
        }
        vertex_options(*position, options)?;
    }
    if vertices.is_empty() {
        return Ok(GraphMutationResult::default());
    }
    let root = graph.family.node_id();
    let declaration = graph.semantic_declaration()?.clone();
    let mut topology = declaration.topology().clone();
    let planned = vertices
        .iter()
        .map(|(key, position, options)| {
            let id = topology.add_vertex();
            Ok((key.clone(), id, *position, options.clone()))
        })
        .collect::<Result<Vec<_>, GraphAuthoringError>>()?;
    let (result, staged) = publication.with_graph_publication(|store, publish| {
        let prepared = planned
            .iter()
            .map(|(key, id, position, options)| {
                Ok((
                    key.clone(),
                    *id,
                    *position,
                    vertex_options(*position, options)?.into_state(store)?,
                ))
            })
            .collect::<Result<Vec<_>, AuthoringError>>()?;
        let mut transaction = SemanticMutationTransaction::new();
        let staged = prepared
            .into_iter()
            .map(|(key, id, _position, state)| {
                let node = transaction.create_node(SemanticNodeCreation::object(state));
                transaction.add_member(root, node);
                (key, id, node)
            })
            .collect::<Vec<_>>();
        let existing_vertices = declaration.vertices().map(|(id, node)| (id, node.into()));
        let all_vertices =
            existing_vertices.chain(staged.iter().map(|(_, id, node)| (*id, (*node).into())));
        let existing_edges = declaration
            .edges()
            .map(|(_, binding)| staged_binding(binding));
        replace_declaration(
            &mut transaction,
            root,
            topology,
            all_vertices,
            existing_edges,
        );
        Ok((publish(store, transaction)?, staged))
    })?;
    let mut mutation = GraphMutationResult::default();
    for (key, id, token) in staged {
        let node = result
            .resolve(token)
            .expect("committed graph vertex resolves");
        let object = Mobject::from_node(Rc::clone(graph.family.integration_store()), node)?;
        graph
            .vertex_lookup
            .insert(key.clone(), graph.vertices.len());
        graph.vertices.push(GraphVertexEntry {
            key,
            id,
            object: object.clone(),
        });
        mutation.added_vertices.push(object);
    }
    Ok(mutation)
}

pub(super) fn add_edges<K, E, P>(
    publication: &mut P,
    graph: &mut RetainedGraph<K>,
    edges: E,
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    E: IntoIterator<Item = (K, K)>,
    P: GraphPublication,
{
    let options = graph.options.clone();
    add_edges_with_options(
        publication,
        graph,
        edges
            .into_iter()
            .map(|(start, end)| (start, end, options.clone())),
    )
}

pub(super) fn add_edges_with_options<K, E, P>(
    publication: &mut P,
    graph: &mut RetainedGraph<K>,
    edges: E,
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    E: IntoIterator<Item = (K, K, GraphOptions)>,
    P: GraphPublication,
{
    require_publication(publication, graph)?;
    let requested = edges.into_iter().collect::<Vec<_>>();
    if requested.is_empty() {
        return Ok(GraphMutationResult::default());
    }
    let root = graph.family.node_id();
    let directed = graph.directed;
    let declaration = graph.semantic_declaration()?.clone();
    let mut topology = declaration.topology().clone();
    let mut seen = HashSet::new();
    let mut planned = Vec::with_capacity(requested.len());
    for (edge_index, (start_key, end_key, options)) in requested.into_iter().enumerate() {
        let start = graph
            .vertex_id(&start_key)
            .ok_or(GraphAuthoringError::UnknownVertexKey)?;
        let end = graph
            .vertex_id(&end_key)
            .ok_or(GraphAuthoringError::UnknownVertexKey)?;
        if start == end {
            return Err(GraphAuthoringError::SelfEdgeUnsupported { edge_index });
        }
        let canonical = if directed || start <= end {
            (start, end)
        } else {
            (end, start)
        };
        if !seen.insert(canonical) {
            return Err(GraphAuthoringError::DuplicateMutationEdgeKey);
        }
        let edge_id = topology.add_edge(start, end, directed)?;
        let edge = topology.edge(edge_id).expect("new graph edge is present");
        let start_position = graph
            .vertex(&start_key)
            .expect("validated vertex key")
            .state()?
            .transform
            .translation;
        let end_position = graph
            .vertex(&end_key)
            .expect("validated vertex key")
            .state()?
            .transform
            .translation;
        planned.push((
            edge,
            (start_position.x, start_position.y),
            (end_position.x, end_position.y),
            options,
        ));
    }
    let (result, staged_edges) = publication.with_graph_publication(|store, publish| {
        let mut prepared = Vec::with_capacity(planned.len());
        for (edge, start_position, end_position, options) in planned {
            let geometry = if directed {
                PreparedNewEdge::Arrow(
                    arrow_options(start_position, end_position, &options)?.prepare(store)?,
                )
            } else {
                PreparedNewEdge::Line(line_options(start_position, end_position, &options)?)
            };
            prepared.push((edge, geometry));
        }
        let paths = prepared
            .iter()
            .flat_map(|(_, geometry)| match geometry {
                PreparedNewEdge::Line(_) => Vec::new(),
                PreparedNewEdge::Arrow(arrow) => {
                    let mut paths = vec![arrow.end_tip.clone()];
                    if let Some(start_tip) = &arrow.start_tip {
                        paths.push(start_tip.clone());
                    }
                    paths
                }
            })
            .collect::<Vec<_>>();
        store.with_geometry_paths(paths, |store, handles| {
            let mut transaction = SemanticMutationTransaction::new();
            let mut handle_index = 0;
            let mut staged = Vec::with_capacity(prepared.len());
            for (edge, geometry) in prepared {
                let staged_edge = match geometry {
                    PreparedNewEdge::Line(options) => {
                        let line = transaction
                            .create_node(SemanticNodeCreation::object(options.into_state(store)?));
                        let family = transaction.create_node(SemanticNodeCreation::family());
                        transaction
                            .add_member(family, line)
                            .add_member(root, family);
                        StagedNewEdge::Line { edge, family, line }
                    }
                    PreparedNewEdge::Arrow(arrow) => {
                        let end_tip = handles[handle_index];
                        handle_index += 1;
                        let start_tip = arrow.start_tip.is_some().then(|| {
                            let value = handles[handle_index];
                            handle_index += 1;
                            value
                        });
                        let arrow =
                            stage_prepared_arrow(&mut transaction, &arrow, end_tip, start_tip);
                        transaction.add_member(root, arrow.family);
                        StagedNewEdge::Arrow { edge, arrow }
                    }
                };
                // Graph edge families paint before all direct vertex members.
                // Construction establishes the same edge-then-vertex ordering;
                // keep it when an edit appends a new edge later.
                let first_vertex = declaration
                    .vertices()
                    .next()
                    .map(|(_, node)| node)
                    .expect("validated edge endpoints require a graph vertex");
                let family: SemanticTransactionNodeRef = match &staged_edge {
                    StagedNewEdge::Line { family, .. } => (*family).into(),
                    StagedNewEdge::Arrow { arrow, .. } => arrow.family.into(),
                };
                transaction.reorder_member_ref(root, family, Some(first_vertex.into()));
                staged.push(staged_edge);
            }
            debug_assert_eq!(handle_index, handles.len());
            let vertices = declaration.vertices().map(|(id, node)| (id, node.into()));
            let existing = declaration
                .edges()
                .map(|(_, binding)| staged_binding(binding));
            let added = staged.iter().map(|edge| match edge {
                StagedNewEdge::Line { edge, family, line } => {
                    SemanticTransactionGraphEdgeBinding::new(
                        edge.id,
                        (*family).into(),
                        (*line).into(),
                    )
                }
                StagedNewEdge::Arrow { edge, arrow } => {
                    SemanticTransactionGraphEdgeBinding::new_arrow(
                        edge.id,
                        arrow.family.into(),
                        arrow.shaft.into(),
                        arrow.end_tip.into(),
                        arrow.start_tip.map(Into::into),
                        arrow.endpoint_policy,
                    )
                }
            });
            replace_declaration(
                &mut transaction,
                root,
                topology,
                vertices,
                existing.chain(added),
            );
            Ok((publish(store, transaction)?, staged))
        })
    })?;
    let mut mutation = GraphMutationResult::default();
    for staged in staged_edges {
        let (edge, object) = match staged {
            StagedNewEdge::Line { edge, family, line } => {
                let family = MobjectFamily::from_node(
                    Rc::clone(graph.family.integration_store()),
                    result
                        .resolve(family)
                        .expect("committed edge family resolves"),
                )?;
                let line = Mobject::from_node(
                    Rc::clone(graph.family.integration_store()),
                    result.resolve(line).expect("committed edge line resolves"),
                )?;
                (edge, GraphEdgeMobject::Line { family, line })
            }
            StagedNewEdge::Arrow { edge, arrow } => {
                let committed = resolve_staged_arrow(arrow, |token| result.resolve(token))
                    .expect("committed Arrow resolves");
                (
                    edge,
                    GraphEdgeMobject::Arrow(ManimArrow::from_committed(
                        Rc::clone(graph.family.integration_store()),
                        committed,
                    )?),
                )
            }
        };
        graph.edge_lookup.insert(edge.id, graph.edges.len());
        graph.edges.push(GraphEdgeEntry {
            edge,
            object: object.clone(),
        });
        mutation.added_edges.push(object);
    }
    Ok(mutation)
}

pub(super) fn add_vertices_and_edges_with_options<K, V, E, P>(
    publication: &mut P,
    graph: &mut RetainedGraph<K>,
    vertices: V,
    edges: E,
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    V: IntoIterator<Item = (K, (f64, f64), GraphOptions)>,
    E: IntoIterator<Item = (K, K, GraphOptions)>,
    P: GraphPublication,
{
    require_publication(publication, graph)?;
    let vertices = vertices.into_iter().collect::<Vec<_>>();
    let requested_edges = edges.into_iter().collect::<Vec<_>>();
    if vertices.is_empty() && requested_edges.is_empty() {
        return Ok(GraphMutationResult::default());
    }

    let root = graph.family.node_id();
    let directed = graph.directed;
    let declaration = graph.semantic_declaration()?.clone();
    let mut topology = declaration.topology().clone();
    let mut vertex_ids = HashMap::with_capacity(graph.vertices.len() + vertices.len());
    let mut positions = HashMap::with_capacity(graph.vertices.len() + vertices.len());
    for entry in &graph.vertices {
        let center = entry.object.state()?.transform.translation;
        vertex_ids.insert(entry.key.clone(), entry.id);
        positions.insert(entry.key.clone(), (center.x, center.y));
    }

    let mut seen_vertices = HashSet::new();
    let mut planned_vertices = Vec::with_capacity(vertices.len());
    for (key, position, options) in vertices {
        if vertex_ids.contains_key(&key) || !seen_vertices.insert(key.clone()) {
            return Err(GraphAuthoringError::DuplicateMutationVertexKey);
        }
        vertex_options(position, &options)?;
        let id = topology.add_vertex();
        vertex_ids.insert(key.clone(), id);
        positions.insert(key.clone(), position);
        planned_vertices.push((key, id, position, options));
    }

    let mut seen_edges = HashSet::new();
    let mut planned_edges = Vec::with_capacity(requested_edges.len());
    for (edge_index, (start_key, end_key, options)) in requested_edges.into_iter().enumerate() {
        let start = *vertex_ids
            .get(&start_key)
            .ok_or(GraphAuthoringError::UnknownVertexKey)?;
        let end = *vertex_ids
            .get(&end_key)
            .ok_or(GraphAuthoringError::UnknownVertexKey)?;
        if start == end {
            return Err(GraphAuthoringError::SelfEdgeUnsupported { edge_index });
        }
        let canonical = if directed || start <= end {
            (start, end)
        } else {
            (end, start)
        };
        if !seen_edges.insert(canonical) {
            return Err(GraphAuthoringError::DuplicateMutationEdgeKey);
        }
        let edge_id = topology.add_edge(start, end, directed)?;
        let edge = topology.edge(edge_id).expect("new graph edge is present");
        let start_position = *positions
            .get(&start_key)
            .expect("resolved vertex has a position");
        let end_position = *positions
            .get(&end_key)
            .expect("resolved vertex has a position");
        planned_edges.push((edge, start_position, end_position, options));
    }

    let (result, staged_vertices, staged_edges) =
        publication.with_graph_publication(|store, publish| {
            let prepared_vertices = planned_vertices
                .into_iter()
                .map(|(key, id, position, options)| {
                    Ok((
                        key,
                        id,
                        position,
                        vertex_options(position, &options)?.into_state(store)?,
                    ))
                })
                .collect::<Result<Vec<_>, AuthoringError>>()?;
            let prepared_edges = planned_edges
                .into_iter()
                .map(|(edge, start, end, options)| {
                    let geometry = if directed {
                        PreparedNewEdge::Arrow(arrow_options(start, end, &options)?.prepare(store)?)
                    } else {
                        PreparedNewEdge::Line(line_options(start, end, &options)?)
                    };
                    Ok((edge, geometry))
                })
                .collect::<Result<Vec<_>, AuthoringError>>()?;
            let paths = prepared_edges
                .iter()
                .flat_map(|(_, geometry)| match geometry {
                    PreparedNewEdge::Line(_) => Vec::new(),
                    PreparedNewEdge::Arrow(arrow) => {
                        let mut paths = vec![arrow.end_tip.clone()];
                        if let Some(start_tip) = &arrow.start_tip {
                            paths.push(start_tip.clone());
                        }
                        paths
                    }
                })
                .collect::<Vec<_>>();
            store.with_geometry_paths(paths, |store, handles| {
                let mut transaction = SemanticMutationTransaction::new();
                let staged_vertices = prepared_vertices
                    .into_iter()
                    .map(|(key, id, _position, state)| {
                        let node = transaction.create_node(SemanticNodeCreation::object(state));
                        transaction.add_member(root, node);
                        (key, id, node)
                    })
                    .collect::<Vec<_>>();
                let first_vertex = declaration
                    .vertices()
                    .next()
                    .map(|(_, node)| SemanticTransactionNodeRef::from(node))
                    .or_else(|| {
                        staged_vertices
                            .first()
                            .map(|(_, _, node)| SemanticTransactionNodeRef::from(*node))
                    })
                    .expect("new graph edges require an existing or added vertex");

                let mut handle_index = 0;
                let mut staged_edges = Vec::with_capacity(prepared_edges.len());
                for (edge, geometry) in prepared_edges {
                    let staged_edge = match geometry {
                        PreparedNewEdge::Line(options) => {
                            let line = transaction.create_node(SemanticNodeCreation::object(
                                options.into_state(store)?,
                            ));
                            let family = transaction.create_node(SemanticNodeCreation::family());
                            transaction
                                .add_member(family, line)
                                .add_member(root, family);
                            StagedNewEdge::Line { edge, family, line }
                        }
                        PreparedNewEdge::Arrow(arrow) => {
                            let end_tip = handles[handle_index];
                            handle_index += 1;
                            let start_tip = arrow.start_tip.is_some().then(|| {
                                let value = handles[handle_index];
                                handle_index += 1;
                                value
                            });
                            let arrow =
                                stage_prepared_arrow(&mut transaction, &arrow, end_tip, start_tip);
                            transaction.add_member(root, arrow.family);
                            StagedNewEdge::Arrow { edge, arrow }
                        }
                    };
                    let family: SemanticTransactionNodeRef = match &staged_edge {
                        StagedNewEdge::Line { family, .. } => (*family).into(),
                        StagedNewEdge::Arrow { arrow, .. } => arrow.family.into(),
                    };
                    transaction.reorder_member_ref(root, family, Some(first_vertex));
                    staged_edges.push(staged_edge);
                }
                debug_assert_eq!(handle_index, handles.len());

                let existing_vertices = declaration.vertices().map(|(id, node)| (id, node.into()));
                let all_vertices = existing_vertices.chain(
                    staged_vertices
                        .iter()
                        .map(|(_, id, node)| (*id, (*node).into())),
                );
                let existing_edges = declaration
                    .edges()
                    .map(|(_, binding)| staged_binding(binding));
                let added_edges = staged_edges.iter().map(|edge| match edge {
                    StagedNewEdge::Line { edge, family, line } => {
                        SemanticTransactionGraphEdgeBinding::new(
                            edge.id,
                            (*family).into(),
                            (*line).into(),
                        )
                    }
                    StagedNewEdge::Arrow { edge, arrow } => {
                        SemanticTransactionGraphEdgeBinding::new_arrow(
                            edge.id,
                            arrow.family.into(),
                            arrow.shaft.into(),
                            arrow.end_tip.into(),
                            arrow.start_tip.map(Into::into),
                            arrow.endpoint_policy,
                        )
                    }
                });
                replace_declaration(
                    &mut transaction,
                    root,
                    topology,
                    all_vertices,
                    existing_edges.chain(added_edges),
                );
                Ok((publish(store, transaction)?, staged_vertices, staged_edges))
            })
        })?;

    let mut mutation = GraphMutationResult::default();
    for (key, id, token) in staged_vertices {
        let node = result
            .resolve(token)
            .expect("committed graph vertex resolves");
        let object = Mobject::from_node(Rc::clone(graph.family.integration_store()), node)?;
        graph
            .vertex_lookup
            .insert(key.clone(), graph.vertices.len());
        graph.vertices.push(GraphVertexEntry {
            key,
            id,
            object: object.clone(),
        });
        mutation.added_vertices.push(object);
    }
    for staged in staged_edges {
        let (edge, object) = match staged {
            StagedNewEdge::Line { edge, family, line } => {
                let family = MobjectFamily::from_node(
                    Rc::clone(graph.family.integration_store()),
                    result
                        .resolve(family)
                        .expect("committed edge family resolves"),
                )?;
                let line = Mobject::from_node(
                    Rc::clone(graph.family.integration_store()),
                    result.resolve(line).expect("committed edge line resolves"),
                )?;
                (edge, GraphEdgeMobject::Line { family, line })
            }
            StagedNewEdge::Arrow { edge, arrow } => {
                let committed = resolve_staged_arrow(arrow, |token| result.resolve(token))
                    .expect("committed edge arrow resolves");
                (
                    edge,
                    GraphEdgeMobject::Arrow(ManimArrow::from_committed(
                        Rc::clone(graph.family.integration_store()),
                        committed,
                    )?),
                )
            }
        };
        graph.edge_lookup.insert(edge.id, graph.edges.len());
        graph.edges.push(GraphEdgeEntry {
            edge,
            object: object.clone(),
        });
        mutation.added_edges.push(object);
    }
    Ok(mutation)
}

pub(super) fn remove_vertices<K, V, P>(
    publication: &mut P,
    graph: &mut RetainedGraph<K>,
    vertices: V,
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    V: IntoIterator<Item = K>,
    P: GraphPublication,
{
    require_publication(publication, graph)?;
    let keys = vertices.into_iter().collect::<Vec<_>>();
    let mut seen = HashSet::new();
    let ids = keys
        .iter()
        .map(|key| {
            if !seen.insert(key.clone()) {
                return Err(GraphAuthoringError::DuplicateMutationVertexKey);
            }
            graph
                .vertex_id(key)
                .ok_or(GraphAuthoringError::UnknownVertexKey)
        })
        .collect::<Result<Vec<_>, _>>()?;
    remove(graph, publication, &ids, &[])
}

pub(super) fn remove_edges<K, E, P>(
    publication: &mut P,
    graph: &mut RetainedGraph<K>,
    edges: E,
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    E: IntoIterator<Item = (K, K)>,
    P: GraphPublication,
{
    require_publication(publication, graph)?;
    let mut seen = HashSet::new();
    let ids = edges
        .into_iter()
        .map(|(start, end)| {
            let edge = graph
                .edge_id(&start, &end)
                .ok_or(GraphAuthoringError::UnknownEdgeKey)?;
            if !seen.insert(edge) {
                return Err(GraphAuthoringError::DuplicateMutationEdgeKey);
            }
            Ok(edge)
        })
        .collect::<Result<Vec<_>, GraphAuthoringError>>()?;
    remove(graph, publication, &[], &ids)
}

fn remove<K, P>(
    graph: &mut RetainedGraph<K>,
    publication: &mut P,
    vertices: &[GraphVertexId],
    requested_edges: &[GraphEdgeId],
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    P: GraphPublication,
{
    if vertices.is_empty() && requested_edges.is_empty() {
        return Ok(GraphMutationResult::default());
    }
    let root = graph.family.node_id();
    let declaration = graph.semantic_declaration()?.clone();
    let mut topology = declaration.topology().clone();
    let mut removed_edges = HashSet::new();
    for &vertex in vertices {
        for edge in topology.remove_vertex(vertex)? {
            removed_edges.insert(edge.id);
        }
    }
    for &edge in requested_edges {
        if !removed_edges.contains(&edge) {
            topology.remove_edge(edge)?;
            removed_edges.insert(edge);
        }
    }
    let removed_vertices = vertices.iter().copied().collect::<HashSet<_>>();
    let (result, removed_vertices, removed_edges) =
        publication.with_graph_publication(|store, publish| {
            let mut transaction = SemanticMutationTransaction::new();
            for &vertex in vertices {
                transaction.remove_member(
                    root,
                    declaration
                        .vertex_node(vertex)
                        .expect("authoritative vertex binding"),
                );
            }
            for &edge in &removed_edges {
                transaction.remove_member(
                    root,
                    declaration
                        .edge_binding(edge)
                        .expect("authoritative edge binding")
                        .family(),
                );
            }
            let remaining_vertices = declaration
                .vertices()
                .filter(|(id, _)| !removed_vertices.contains(id))
                .map(|(id, node)| (id, node.into()));
            let remaining_edges = declaration
                .edges()
                .filter(|(edge, _)| !removed_edges.contains(&edge.id))
                .map(|(_, binding)| staged_binding(binding));
            replace_declaration(
                &mut transaction,
                root,
                topology,
                remaining_vertices,
                remaining_edges,
            );
            Ok((
                publish(store, transaction)?,
                removed_vertices.clone(),
                removed_edges.clone(),
            ))
        })?;
    let mut mutation = GraphMutationResult::default();
    let mut kept_edges = Vec::with_capacity(graph.edges.len());
    for entry in graph.edges.drain(..) {
        if removed_edges.contains(&entry.edge.id) {
            mutation.removed_edges.push(entry.object);
        } else {
            kept_edges.push(entry);
        }
    }
    graph.edges = kept_edges;
    graph.edge_lookup = graph
        .edges
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.edge.id, index))
        .collect();
    let mut kept_vertices = Vec::with_capacity(graph.vertices.len());
    for entry in graph.vertices.drain(..) {
        if removed_vertices.contains(&entry.id) {
            mutation.removed_vertices.push(entry.object);
        } else {
            kept_vertices.push(entry);
        }
    }
    graph.vertices = kept_vertices;
    graph.vertex_lookup = graph
        .vertices
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.key.clone(), index))
        .collect();
    let _ = result;
    Ok(mutation)
}

pub(super) fn change_layout<K, P>(
    publication: &mut P,
    graph: &mut RetainedGraph<K>,
    mut options: GraphLayoutOptions,
) -> Result<(), GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    P: GraphPublication,
{
    require_publication(publication, graph)?;
    options.directed = graph.directed;
    let declaration = graph.semantic_declaration()?.clone();
    let vertices = declaration.vertices().collect::<Vec<_>>();
    let edges = if matches!(
        options.layout,
        GraphLayout::Spring | GraphLayout::Spectral | GraphLayout::Tree
    ) {
        let indices = declaration
            .topology()
            .vertices()
            .enumerate()
            .map(|(index, vertex)| (vertex, index))
            .collect::<HashMap<_, _>>();
        declaration
            .edges()
            .map(|(edge, _)| (indices[&edge.start], indices[&edge.end]))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let positions = options.positions(vertices.len(), &edges)?;
    publish_layout_positions(publication, vertices, &positions)
}

pub(super) fn change_layout_positions<K, P>(
    publication: &mut P,
    graph: &mut RetainedGraph<K>,
    positions: &[(f64, f64)],
) -> Result<(), GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    P: GraphPublication,
{
    require_publication(publication, graph)?;
    let declaration = graph.semantic_declaration()?.clone();
    let vertices = declaration.vertices().collect::<Vec<_>>();
    if positions.len() != vertices.len() {
        return Err(GraphAuthoringError::InvalidLayout(
            "explicit positions must cover every graph vertex exactly once",
        ));
    }
    if positions
        .iter()
        .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(GraphAuthoringError::InvalidLayout(
            "explicit positions must be finite",
        ));
    }
    publish_layout_positions(publication, vertices, positions)
}

fn publish_layout_positions<P: GraphPublication>(
    publication: &mut P,
    vertices: Vec<(GraphVertexId, noon_core::SemanticNodeId)>,
    positions: &[(f64, f64)],
) -> Result<(), GraphAuthoringError> {
    publication.with_graph_publication(|store, publish| {
        let mut transaction = SemanticMutationTransaction::new();
        for ((_, node), &(x, y)) in vertices.iter().zip(positions) {
            transaction.set_property(
                *node,
                SemanticObjectProperty::Translation,
                noon_core::SemanticVec3::new(x, y, 0.0),
            );
        }
        publish(store, transaction).map(|_| ())
    })?;
    Ok(())
}

fn graph_layout(
    count: usize,
    edges: &[(usize, usize)],
    options: &GraphLayoutOptions,
) -> Result<Vec<(f64, f64)>, GraphAuthoringError> {
    if !options.scale.is_finite()
        || options.scale_y.is_some_and(|scale| !scale.is_finite())
        || !options.center.0.is_finite()
        || !options.center.1.is_finite()
        || !options.threshold.is_finite()
        || options.threshold < 0.0
    {
        return Err(GraphAuthoringError::InvalidLayout(
            "scale, center, and threshold must be finite",
        ));
    }
    if options.layout == GraphLayout::Shell {
        return shell_layout(count, options);
    }
    if options.layout == GraphLayout::Spiral {
        return spiral_layout(count, options);
    }
    if options.layout == GraphLayout::Partite {
        return partite_layout(count, options);
    }
    if options.layout == GraphLayout::Spring {
        return spring_layout(count, edges, options);
    }
    if options.layout == GraphLayout::Spectral {
        return spectral_layout(count, edges, options);
    }
    if options.layout == GraphLayout::Random && options.random_positions.is_some() {
        return networkx_random_layout(count, options);
    }
    if options.layout == GraphLayout::Tree {
        return tree_layout(count, edges, options);
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    if count == 1 {
        return Ok(vec![options.center]);
    }
    let mut random = DeterministicRandom::new(options.seed);
    let mut positions: Vec<(f64, f64)> = match options.layout {
        GraphLayout::Circular => (0..count)
            .map(|index| {
                let angle = std::f64::consts::TAU * index as f64 / count as f64;
                (angle.cos(), angle.sin())
            })
            .collect::<Vec<_>>(),
        GraphLayout::Shell => unreachable!("shell layout returned above"),
        GraphLayout::Partite => unreachable!("partite layout returned above"),
        GraphLayout::Spiral => unreachable!("spiral layout returned above"),
        GraphLayout::Spring => unreachable!("spring layout returned above"),
        GraphLayout::Spectral => unreachable!("spectral layout returned above"),
        GraphLayout::Random => (0..count)
            .map(|_| (random.unit() - 0.5, random.unit() - 0.5))
            .collect::<Vec<_>>(),
        GraphLayout::Tree => unreachable!("tree layout returned above"),
    };
    rescale(&mut positions, layout_scales(options), options.center);
    if positions
        .iter()
        .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(GraphAuthoringError::InvalidLayout(
            "computed coordinates are not finite",
        ));
    }
    Ok(positions)
}

fn spectral_layout(
    count: usize,
    edges: &[(usize, usize)],
    options: &GraphLayoutOptions,
) -> Result<Vec<(f64, f64)>, GraphAuthoringError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if count == 1 {
        return Ok(vec![options.center]);
    }
    if count == 2 {
        // NetworkX 3.4.2 special-cases two nodes before its scale/center pass.
        return Ok(vec![
            (0.0, 0.0),
            (2.0 * options.center.0, 2.0 * options.center.1),
        ]);
    }

    let mut adjacency = vec![0.0; count * count];
    for &(start, end) in edges {
        if options.directed {
            adjacency[start * count + end] += 1.0;
        } else {
            adjacency[start * count + end] += 1.0;
            adjacency[end * count + start] += 1.0;
        }
    }
    if options.directed {
        let directed_adjacency = adjacency.clone();
        for row in 0..count {
            for column in 0..count {
                let forward = directed_adjacency[row * count + column];
                let reverse = directed_adjacency[column * count + row];
                adjacency[row * count + column] = forward + reverse;
            }
        }
    }

    let mut laplacian = vec![vec![0.0; count]; count];
    for row in 0..count {
        let degree = (0..count)
            .map(|column| adjacency[row * count + column])
            .sum::<f64>();
        for column in 0..count {
            laplacian[row][column] = if row == column {
                degree - adjacency[row * count + column]
            } else {
                -adjacency[row * count + column]
            };
        }
    }

    let (eigenvalues, eigenvectors) = symmetric_eigenvectors(laplacian)?;
    let mut order = (0..count).collect::<Vec<_>>();
    order.sort_by(|&left, &right| {
        eigenvalues[left]
            .total_cmp(&eigenvalues[right])
            .then_with(|| left.cmp(&right))
    });
    let x_axis = eigenvectors[order[1]].as_slice();
    let y_axis = eigenvectors[order[2]].as_slice();
    let mut positions = (0..count)
        .map(|index| (x_axis[index], y_axis[index]))
        .collect::<Vec<_>>();

    // Eigenvector signs are mathematically arbitrary. Canonicalizing each axis
    // makes Rust and browser hosts deterministic across eigensolver builds.
    for axis in 0..2 {
        let value = positions.iter().find_map(|position| {
            let coordinate = if axis == 0 { position.0 } else { position.1 };
            (coordinate.abs() > 1e-12).then_some(coordinate)
        });
        let Some(value) = value else { continue };
        if value < 0.0 {
            for position in &mut positions {
                if axis == 0 {
                    position.0 = -position.0;
                } else {
                    position.1 = -position.1;
                }
            }
        }
    }

    rescale(&mut positions, layout_scales(options), options.center);
    if positions
        .iter()
        .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(GraphAuthoringError::InvalidLayout(
            "computed spectral coordinates are not finite",
        ));
    }
    Ok(positions)
}

/// Cyclic Jacobi decomposition for the symmetric, unnormalized graph Laplacian.
/// It is deterministic and dependency-free, which keeps layout meaning in the
/// shared Rust graph implementation on native and browser targets.
// Jacobi rotations update both rows and columns using the same pivot indices.
#[allow(clippy::needless_range_loop)]
fn symmetric_eigenvectors(
    mut matrix: Vec<Vec<f64>>,
) -> Result<(Vec<f64>, Vec<Vec<f64>>), GraphAuthoringError> {
    let count = matrix.len();
    let mut vectors = vec![vec![0.0; count]; count];
    for (index, row) in vectors.iter_mut().enumerate() {
        row[index] = 1.0;
    }

    let mut matrix_extent: f64 = 1.0;
    for row in &matrix {
        for &value in row {
            matrix_extent = matrix_extent.max(value.abs());
        }
    }
    let tolerance = 1e-14 * matrix_extent;
    let mut converged = false;
    for _ in 0..64 {
        for left in 0..count {
            for right in left + 1..count {
                let cross = matrix[left][right];
                if cross.abs() <= tolerance {
                    continue;
                }
                let left_value = matrix[left][left];
                let right_value = matrix[right][right];
                let tau = (right_value - left_value) / (2.0 * cross);
                let tangent = if tau >= 0.0 {
                    1.0 / (tau + tau.hypot(1.0))
                } else {
                    -1.0 / (-tau + tau.hypot(1.0))
                };
                let cosine = 1.0 / (1.0 + tangent * tangent).sqrt();
                let sine = tangent * cosine;

                for index in 0..count {
                    if index == left || index == right {
                        continue;
                    }
                    let left_entry = matrix[index][left];
                    let right_entry = matrix[index][right];
                    let new_left = cosine * left_entry - sine * right_entry;
                    let new_right = sine * left_entry + cosine * right_entry;
                    matrix[index][left] = new_left;
                    matrix[left][index] = new_left;
                    matrix[index][right] = new_right;
                    matrix[right][index] = new_right;
                }
                matrix[left][left] = left_value - tangent * cross;
                matrix[right][right] = right_value + tangent * cross;
                matrix[left][right] = 0.0;
                matrix[right][left] = 0.0;

                for row in &mut vectors {
                    let left_entry = row[left];
                    let right_entry = row[right];
                    row[left] = cosine * left_entry - sine * right_entry;
                    row[right] = sine * left_entry + cosine * right_entry;
                }
            }
        }
        let mut largest_off_diagonal: f64 = 0.0;
        for (left, row) in matrix.iter().enumerate() {
            for &value in row.iter().skip(left + 1) {
                largest_off_diagonal = largest_off_diagonal.max(value.abs());
            }
        }
        if largest_off_diagonal <= tolerance {
            converged = true;
            break;
        }
    }
    if !converged {
        return Err(GraphAuthoringError::InvalidLayout(
            "spectral eigensolver did not converge",
        ));
    }

    let eigenvalues = (0..count).map(|index| matrix[index][index]).collect();
    let eigenvectors = (0..count)
        .map(|column| vectors.iter().map(|row| row[column]).collect())
        .collect();
    Ok((eigenvalues, eigenvectors))
}

fn partite_layout(
    count: usize,
    options: &GraphLayoutOptions,
) -> Result<Vec<(f64, f64)>, GraphAuthoringError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if options.partitions.is_empty() || options.partitions.iter().any(Vec::is_empty) {
        return Err(GraphAuthoringError::InvalidLayout(
            "partite layout requires non-empty partition layers",
        ));
    }
    let mut seen = vec![false; count];
    for &vertex in options.partitions.iter().flatten() {
        let Some(assigned) = seen.get_mut(vertex) else {
            return Err(GraphAuthoringError::InvalidLayout(
                "partite partition references an unknown vertex",
            ));
        };
        if std::mem::replace(assigned, true) {
            return Err(GraphAuthoringError::InvalidLayout(
                "partite layers must assign every vertex exactly once",
            ));
        }
    }
    if seen.iter().any(|assigned| !assigned) {
        return Err(GraphAuthoringError::InvalidLayout(
            "partite layers must assign every vertex exactly once",
        ));
    }

    let width = options.partitions.len();
    let mut positions = vec![(0.0, 0.0); count];
    for (layer_index, layer) in options.partitions.iter().enumerate() {
        let layer_height = layer.len();
        for (row, &vertex) in layer.iter().enumerate() {
            positions[vertex] = (
                layer_index as f64 - (width - 1) as f64 / 2.0,
                row as f64 - (layer_height - 1) as f64 / 2.0,
            );
        }
    }
    rescale(&mut positions, layout_scales(options), options.center);
    if options.align_horizontal {
        for (x, y) in &mut positions {
            std::mem::swap(x, y);
        }
    }
    if positions
        .iter()
        .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(GraphAuthoringError::InvalidLayout(
            "computed partite coordinates are not finite",
        ));
    }
    Ok(positions)
}

fn spiral_layout(
    count: usize,
    options: &GraphLayoutOptions,
) -> Result<Vec<(f64, f64)>, GraphAuthoringError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if count == 1 {
        return Ok(vec![options.center]);
    }
    if !options.resolution.is_finite() {
        return Err(GraphAuthoringError::InvalidLayout(
            "spiral resolution must be finite",
        ));
    }
    if options.equidistant && options.resolution == 0.0 {
        return Err(GraphAuthoringError::InvalidLayout(
            "equidistant spiral resolution must be nonzero",
        ));
    }
    let mut positions = Vec::with_capacity(count);
    if options.equidistant {
        let mut theta = options.resolution;
        theta += 1.0 / (0.5 * theta);
        for _ in 0..count {
            let radius = 0.5 * theta;
            theta += 1.0 / radius;
            positions.push((theta.cos() * radius, theta.sin() * radius));
        }
    } else {
        for index in 0..count {
            let distance = index as f64;
            let angle = options.resolution * distance;
            positions.push((distance * angle.cos(), distance * angle.sin()));
        }
    }
    rescale(&mut positions, layout_scales(options), options.center);
    if positions
        .iter()
        .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(GraphAuthoringError::InvalidLayout(
            "computed spiral coordinates are not finite",
        ));
    }
    Ok(positions)
}

fn shell_layout(
    count: usize,
    options: &GraphLayoutOptions,
) -> Result<Vec<(f64, f64)>, GraphAuthoringError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let shells = if options.shells.is_empty() {
        vec![(0..count).collect::<Vec<_>>()]
    } else {
        options.shells.clone()
    };
    let mut seen = vec![false; count];
    for shell in &shells {
        for &vertex in shell {
            if vertex >= count || seen[vertex] {
                return Err(GraphAuthoringError::InvalidLayout(
                    "shell layout must assign each graph vertex exactly once",
                ));
            }
            seen[vertex] = true;
        }
    }
    if seen.iter().any(|assigned| !assigned) {
        return Err(GraphAuthoringError::InvalidLayout(
            "shell layout must assign each graph vertex exactly once",
        ));
    }
    if count == 1 {
        return Ok(vec![options.center]);
    }
    let rotate = options
        .rotate
        .unwrap_or(std::f64::consts::PI / shells.len() as f64);
    if !rotate.is_finite() {
        return Err(GraphAuthoringError::InvalidLayout(
            "shell rotation must be finite",
        ));
    }
    let radius_bump = options.scale / shells.len() as f64;
    let mut radius = if shells[0].len() == 1 {
        0.0
    } else {
        radius_bump
    };
    let mut first_theta = rotate;
    let mut positions = vec![(0.0, 0.0); count];
    for shell in &shells {
        for (index, &vertex) in shell.iter().enumerate() {
            // NetworkX 3.4.2 creates the linspace angles as float32 before
            // adding the (float64) starting rotation.
            let theta = ((index as f64 * std::f64::consts::TAU / shell.len() as f64) as f32) as f64
                + first_theta;
            positions[vertex] = (
                radius * theta.cos() + options.center.0,
                radius * theta.sin() + options.center.1,
            );
        }
        radius += radius_bump;
        first_theta += rotate;
    }
    if positions
        .iter()
        .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(GraphAuthoringError::InvalidLayout(
            "computed shell coordinates are not finite",
        ));
    }
    Ok(positions)
}

fn layout_scales(options: &GraphLayoutOptions) -> (f64, f64) {
    (options.scale, options.scale_y.unwrap_or(options.scale))
}

fn rescale(points: &mut [(f64, f64)], scale: (f64, f64), center: (f64, f64)) {
    let (sum_x, sum_y) = points
        .iter()
        .fold((0.0, 0.0), |(x, y), point| (x + point.0, y + point.1));
    let mean = (sum_x / points.len() as f64, sum_y / points.len() as f64);
    let mut extent: f64 = 0.0;
    for point in points.iter_mut() {
        point.0 -= mean.0;
        point.1 -= mean.1;
        extent = extent.max(point.0.abs()).max(point.1.abs());
    }
    let factor_x = if extent > 0.0 { scale.0 / extent } else { 1.0 };
    let factor_y = if extent > 0.0 { scale.1 / extent } else { 1.0 };
    for point in points {
        point.0 = point.0 * factor_x + center.0;
        point.1 = point.1 * factor_y + center.1;
    }
}

fn spring_layout(
    count: usize,
    edges: &[(usize, usize)],
    options: &GraphLayoutOptions,
) -> Result<Vec<(f64, f64)>, GraphAuthoringError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if count == 1 {
        return Ok(vec![options.center]);
    }
    let mut random = DeterministicRandom::new(options.seed);
    let mut points = options.spring_positions.clone().unwrap_or_else(|| {
        (0..count)
            .map(|_| (random.unit(), random.unit()))
            .collect::<Vec<_>>()
    });
    if points.len() != count || points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return Err(GraphAuthoringError::InvalidLayout(
            "spring positions must provide one finite 2D point per vertex",
        ));
    }
    if options.spring_fixed.is_some() && options.spring_positions.is_none() {
        return Err(GraphAuthoringError::InvalidLayout(
            "spring fixed vertices require initial positions",
        ));
    }
    let mut fixed = vec![false; count];
    if let Some(indices) = &options.spring_fixed {
        for &index in indices {
            let Some(value) = fixed.get_mut(index) else {
                return Err(GraphAuthoringError::InvalidLayout(
                    "spring fixed vertex references an unknown vertex",
                ));
            };
            *value = true;
        }
    }

    let k = options
        .spring_k
        .unwrap_or_else(|| (1.0 / count as f64).sqrt());
    if !k.is_finite() || k == 0.0 {
        return Err(GraphAuthoringError::InvalidLayout(
            "spring k must be finite and non-zero",
        ));
    }
    let mut adjacency = vec![0.0; count * count];
    for &(start, end) in edges {
        if start >= count || end >= count {
            return Err(GraphAuthoringError::InvalidLayout(
                "layout edge endpoints must reference existing vertices",
            ));
        }
        adjacency[start * count + end] = 1.0;
        if !options.directed {
            adjacency[end * count + start] = 1.0;
        }
    }

    let min_x = points
        .iter()
        .map(|point| point.0)
        .fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|point| point.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = points
        .iter()
        .map(|point| point.1)
        .fold(f64::INFINITY, f64::min);
    let max_y = points
        .iter()
        .map(|point| point.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut temperature = 0.1 * (max_x - min_x).max(max_y - min_y);
    let cooling = temperature / (options.iterations as f64 + 1.0);
    let mut displacement = vec![(0.0, 0.0); count];
    for _ in 0..options.iterations {
        displacement.fill((0.0, 0.0));
        for i in 0..count {
            for j in 0..count {
                if i == j {
                    continue;
                }
                let dx = points[i].0 - points[j].0;
                let dy = points[i].1 - points[j].1;
                let distance = (dx * dx + dy * dy).sqrt().max(0.01);
                let force = k * k / (distance * distance) - adjacency[i * count + j] * distance / k;
                displacement[i].0 += dx * force;
                displacement[i].1 += dy * force;
            }
        }
        let mut squared_delta_norm = 0.0;
        for index in 0..count {
            let (dx, dy) = displacement[index];
            let displacement_length = (dx * dx + dy * dy).sqrt();
            let displacement_length = if displacement_length < 0.01 {
                0.1
            } else {
                displacement_length
            };
            let factor = temperature / displacement_length;
            let (mut delta_x, mut delta_y) = (dx * factor, dy * factor);
            if fixed[index] {
                delta_x = 0.0;
                delta_y = 0.0;
            }
            points[index].0 += delta_x;
            points[index].1 += delta_y;
            squared_delta_norm += delta_x * delta_x + delta_y * delta_y;
        }
        temperature -= cooling;
        if squared_delta_norm.sqrt() / (count as f64) < options.threshold {
            break;
        }
    }
    if options.spring_rescale && options.spring_fixed.is_none() {
        rescale(&mut points, layout_scales(options), options.center);
    }
    if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return Err(GraphAuthoringError::InvalidLayout(
            "computed coordinates are not finite",
        ));
    }
    Ok(points)
}

fn networkx_random_layout(
    count: usize,
    options: &GraphLayoutOptions,
) -> Result<Vec<(f64, f64)>, GraphAuthoringError> {
    let positions = options
        .random_positions
        .as_ref()
        .expect("NetworkX random positions are checked by the caller");
    if positions.len() != count
        || positions
            .iter()
            .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(GraphAuthoringError::InvalidLayout(
            "random positions must provide one finite 2D point per vertex",
        ));
    }
    // ManimCE's `_random_layout` wraps NetworkX's [0, 1) coordinates with
    // `2 * scale * (position - 0.5)`. Python provides the float32 NetworkX
    // samples (with its center already added); shared Rust owns this transform.
    let (scale_x, scale_y) = layout_scales(options);
    let positions = positions
        .iter()
        .map(|&(x, y)| (2.0 * scale_x * (x - 0.5), 2.0 * scale_y * (y - 0.5)))
        .collect::<Vec<_>>();
    if positions
        .iter()
        .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(GraphAuthoringError::InvalidLayout(
            "computed coordinates are not finite",
        ));
    }
    Ok(positions)
}

fn tree_layout(
    count: usize,
    edges: &[(usize, usize)],
    options: &GraphLayoutOptions,
) -> Result<Vec<(f64, f64)>, GraphAuthoringError> {
    if options.directed {
        return Err(GraphAuthoringError::InvalidLayout(
            "tree layout requires an undirected graph",
        ));
    }
    if count == 0 {
        return Err(GraphAuthoringError::InvalidLayout(
            "tree layout requires a non-empty graph",
        ));
    }
    let root = options.tree_root.ok_or(GraphAuthoringError::InvalidLayout(
        "tree layout requires a root vertex",
    ))?;
    if root >= count {
        return Err(GraphAuthoringError::InvalidLayout(
            "tree root references an unknown vertex",
        ));
    }
    if edges.len() != count - 1 || edges.iter().any(|&(start, end)| start == end) {
        return Err(GraphAuthoringError::InvalidLayout(
            "tree layout requires a connected acyclic graph",
        ));
    }

    let mut neighbors = vec![Vec::new(); count];
    for &(start, end) in edges {
        neighbors[start].push(end);
        neighbors[end].push(start);
    }
    let mut visited = vec![false; count];
    let mut pending = vec![root];
    visited[root] = true;
    while let Some(vertex) = pending.pop() {
        for &neighbor in &neighbors[vertex] {
            if !visited[neighbor] {
                visited[neighbor] = true;
                pending.push(neighbor);
            }
        }
    }
    if visited.iter().any(|&is_visited| !is_visited) {
        return Err(GraphAuthoringError::InvalidLayout(
            "tree layout requires a connected acyclic graph",
        ));
    }

    let mut children = HashMap::<usize, Vec<usize>>::with_capacity(count);
    children.insert(root, neighbors[root].clone());
    let mut levels = vec![neighbors[root].clone()];
    let mut path = vec![root];
    let mut positions: Vec<Option<(f64, f64)>> = vec![None; count];
    let mut obstruction = vec![0.0_f64; count];
    let orientation = if options.tree_orientation_up {
        1_isize
    } else {
        -1
    };

    while !levels.is_empty() {
        let next_vertex = levels.last_mut().and_then(Vec::pop);
        if let Some(vertex) = next_vertex {
            let parent = *path.last().expect("tree traversal always has a root");
            let descendants = neighbors[vertex]
                .iter()
                .copied()
                .filter(|&neighbor| neighbor != parent)
                .collect::<Vec<_>>();
            children.insert(vertex, descendants.clone());
            levels.push(descendants);
            path.push(vertex);
            continue;
        }

        levels.pop();
        let vertex = path.pop().expect("tree traversal always has a root");
        let depth = levels.len() as isize;
        let y = orientation * depth;
        // The pinned Manim implementation uses Python list indexing with
        // negative y coordinates, so map them from the end of the buffer.
        let row = if y < 0 {
            (count as isize + y) as usize
        } else {
            y as usize
        };
        let descendants = children.entry(vertex).or_default();
        let x = if descendants.is_empty() {
            obstruction[row]
        } else {
            descendants
                .iter()
                .map(|&child| {
                    positions[child]
                        .expect("tree children are laid out first")
                        .0
                })
                .sum::<f64>()
                / descendants.len() as f64
        };
        positions[vertex] = Some((x, y as f64));
        let mut x = x;
        let occupied = obstruction[row];
        if x < occupied {
            let shift = occupied - x;
            let mut subtree = vec![vertex];
            while let Some(current) = subtree.pop() {
                let (current_x, current_y) =
                    positions[current].expect("tree node position was prepared");
                let shifted_x = current_x + shift;
                positions[current] = Some((shifted_x, current_y));
                let current_row = if current_y < 0.0 {
                    (count as isize + current_y as isize) as usize
                } else {
                    current_y as usize
                };
                obstruction[current_row] = obstruction[current_row].max(shifted_x + 1.0);
                subtree.extend(children[&current].iter().copied());
            }
            x = occupied;
        }
        obstruction[row] = x + 1.0;
    }

    let mut positions = positions
        .into_iter()
        .map(|position| position.expect("tree traversal lays out every vertex"))
        .collect::<Vec<_>>();
    let x_min = positions
        .iter()
        .map(|&(x, _)| x)
        .fold(f64::INFINITY, f64::min);
    let x_max = positions
        .iter()
        .map(|&(x, _)| x)
        .fold(f64::NEG_INFINITY, f64::max);
    let y_min = positions
        .iter()
        .map(|&(_, y)| y)
        .fold(f64::INFINITY, f64::min);
    let y_max = positions
        .iter()
        .map(|&(_, y)| y)
        .fold(f64::NEG_INFINITY, f64::max);
    let width = x_max - x_min;
    let height = y_max - y_min;
    let center = ((x_min + x_max) / 2.0, (y_min + y_max) / 2.0);
    let (scale_x, scale_y) = match options.tree_vertex_spacing {
        Some((spacing_x, spacing_y)) => {
            if !spacing_x.is_finite() || !spacing_y.is_finite() {
                return Err(GraphAuthoringError::InvalidLayout(
                    "tree vertex spacing must be finite",
                ));
            }
            // Manim's vertex_spacing is a direct per-axis scale, not a target
            // half-extent. It also overrides layout_scale entirely.
            (spacing_x, spacing_y)
        }
        None if options.scale_y.is_some() => (
            if width > 0.0 {
                2.0 * options.scale / width
            } else {
                1.0
            },
            if height > 0.0 {
                2.0 * options.scale_y.expect("checked above") / height
            } else {
                1.0
            },
        ),
        None if width > 0.0 || height > 0.0 => {
            let scale = 2.0 * options.scale / width.max(height);
            (scale, scale)
        }
        None => (1.0, 1.0),
    };
    for (x, y) in &mut positions {
        *x = (*x - center.0) * scale_x;
        *y = (*y - center.1) * scale_y;
    }
    if positions
        .iter()
        .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(GraphAuthoringError::InvalidLayout(
            "computed coordinates are not finite",
        ));
    }
    Ok(positions)
}

struct DeterministicRandom {
    state: u64,
}
impl DeterministicRandom {
    fn new(seed: u32) -> Self {
        Self {
            state: u64::from(seed) + 0x9e37_79b9_7f4a_7c15,
        }
    }
    fn unit(&mut self) -> f64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        (self.state >> 11) as f64 / ((1u64 << 53) as f64)
    }
}

#[cfg(test)]
mod layout_preparation_tests {
    use super::*;

    #[test]
    fn pure_layout_validates_edges_and_non_finite_inputs_before_construction() {
        let options = GraphLayoutOptions {
            layout: GraphLayout::Circular,
            ..Default::default()
        };
        assert!(options.positions(2, &[(0, 2)]).is_err());
        assert!(GraphLayoutOptions {
            scale: f64::NAN,
            ..options.clone()
        }
        .positions(0, &[])
        .is_err());
        assert!(GraphLayoutOptions {
            scale_y: Some(f64::INFINITY),
            ..options.clone()
        }
        .positions(0, &[])
        .is_err());
        let positions = options.positions(4, &[]).unwrap();
        assert_eq!(positions.len(), 4);
        assert!((positions[0].0 - 2.0).abs() < 1e-12);
        assert!(positions[0].1.abs() < 1e-12);
        assert_eq!(options.positions(1, &[]).unwrap(), vec![(0.0, 0.0)]);
    }
}
