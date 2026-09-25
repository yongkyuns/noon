//! Bounded graph declaration replacement and author-time placement.
//!
//! The wrapper maps below are only used to translate public keys. Every edit
//! starts from the semantic declaration and publishes one final replacement
//! declaration with ordinary family membership changes in the same transaction.

use super::{
    construction::{arrow_options, line_options, vertex_options},
    GraphAuthoringError, GraphEdgeEntry, GraphEdgeMobject, GraphVertexEntry, RetainedGraph,
};
use crate::{
    arrow_authoring::{resolve_staged_arrow, stage_prepared_arrow, PreparedArrow, StagedArrow},
    AuthoringError, ManimArrow, ManimGeometryOptions, Mobject, MobjectFamily, Scene,
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
/// `Random` and `Spring` use Noon's seeded solver, not NetworkX-compatible
/// random streams or force integration. `Spring` is O(iterations * V²) and is
/// deliberately an author-time convenience rather than a runtime operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphLayout {
    Circular,
    Random,
    Spring,
}

#[derive(Clone, Debug)]
pub struct GraphLayoutOptions {
    pub layout: GraphLayout,
    pub scale: f64,
    pub center: (f64, f64),
    pub seed: u32,
    pub iterations: usize,
    pub threshold: f64,
}

impl Default for GraphLayoutOptions {
    fn default() -> Self {
        Self {
            layout: GraphLayout::Spring,
            scale: 2.0,
            center: (0.0, 0.0),
            seed: 0,
            iterations: 50,
            threshold: 1e-4,
        }
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

fn require_scene<K>(scene: &Scene, graph: &RetainedGraph<K>) -> Result<(), GraphAuthoringError> {
    if !Rc::ptr_eq(scene.integration_store(), graph.family.integration_store()) {
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

pub(super) fn add_vertices<K, V>(
    scene: &mut Scene,
    graph: &mut RetainedGraph<K>,
    vertices: V,
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    V: IntoIterator<Item = (K, (f64, f64))>,
{
    require_scene(scene, graph)?;
    let vertices = vertices.into_iter().collect::<Vec<_>>();
    let mut seen = HashSet::new();
    for (key, position) in &vertices {
        if graph.vertex_lookup.contains_key(key) || !seen.insert(key.clone()) {
            return Err(GraphAuthoringError::DuplicateMutationVertexKey);
        }
        vertex_options(*position, &graph.options)?;
    }
    if vertices.is_empty() {
        return Ok(GraphMutationResult::default());
    }
    let root = graph.family.node_id();
    let options = graph.options.clone();
    let declaration = graph.semantic_declaration()?.clone();
    let mut topology = declaration.topology().clone();
    let planned = vertices
        .iter()
        .map(|(key, position)| {
            let id = topology.add_vertex();
            Ok((key.clone(), id, *position))
        })
        .collect::<Result<Vec<_>, GraphAuthoringError>>()?;
    let (result, staged) = scene.with_semantic_publication(|store, publish| {
        let prepared = planned
            .iter()
            .map(|(key, id, position)| {
                Ok((
                    key.clone(),
                    *id,
                    *position,
                    vertex_options(*position, &options)?.into_state(store)?,
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

pub(super) fn add_edges<K, E>(
    scene: &mut Scene,
    graph: &mut RetainedGraph<K>,
    edges: E,
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    E: IntoIterator<Item = (K, K)>,
{
    require_scene(scene, graph)?;
    let requested = edges.into_iter().collect::<Vec<_>>();
    if requested.is_empty() {
        return Ok(GraphMutationResult::default());
    }
    let root = graph.family.node_id();
    let options = graph.options.clone();
    let directed = graph.directed;
    let declaration = graph.semantic_declaration()?.clone();
    let mut topology = declaration.topology().clone();
    let mut seen = HashSet::new();
    let mut planned = Vec::with_capacity(requested.len());
    for (start_key, end_key) in &requested {
        let start = graph
            .vertex_id(start_key)
            .ok_or(GraphAuthoringError::UnknownVertexKey)?;
        let end = graph
            .vertex_id(end_key)
            .ok_or(GraphAuthoringError::UnknownVertexKey)?;
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
            .vertex(start_key)
            .expect("validated vertex key")
            .state()?
            .transform
            .translation;
        let end_position = graph
            .vertex(end_key)
            .expect("validated vertex key")
            .state()?
            .transform
            .translation;
        planned.push((
            edge,
            (start_position.x, start_position.y),
            (end_position.x, end_position.y),
        ));
    }
    let (result, staged_edges) = scene.with_semantic_publication(|store, publish| {
        let mut prepared = Vec::with_capacity(planned.len());
        for (edge, start_position, end_position) in planned {
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

pub(super) fn remove_vertices<K, V>(
    scene: &mut Scene,
    graph: &mut RetainedGraph<K>,
    vertices: V,
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    V: IntoIterator<Item = K>,
{
    require_scene(scene, graph)?;
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
    remove(graph, scene, &ids, &[])
}

pub(super) fn remove_edges<K, E>(
    scene: &mut Scene,
    graph: &mut RetainedGraph<K>,
    edges: E,
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    E: IntoIterator<Item = (K, K)>,
{
    require_scene(scene, graph)?;
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
    remove(graph, scene, &[], &ids)
}

fn remove<K>(
    graph: &mut RetainedGraph<K>,
    scene: &mut Scene,
    vertices: &[GraphVertexId],
    requested_edges: &[GraphEdgeId],
) -> Result<GraphMutationResult, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
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
        scene.with_semantic_publication(|store, publish| {
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

pub(super) fn change_layout<K>(
    scene: &mut Scene,
    graph: &mut RetainedGraph<K>,
    options: GraphLayoutOptions,
) -> Result<(), GraphAuthoringError>
where
    K: Clone + Eq + Hash,
{
    require_scene(scene, graph)?;
    let declaration = graph.semantic_declaration()?.clone();
    let vertices = declaration.vertices().collect::<Vec<_>>();
    let edges = if options.layout == GraphLayout::Spring {
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
    let positions = graph_layout(vertices.len(), &edges, &options)?;
    publish_layout_positions(scene, vertices, &positions)
}

pub(super) fn change_layout_positions<K>(
    scene: &mut Scene,
    graph: &mut RetainedGraph<K>,
    positions: &[(f64, f64)],
) -> Result<(), GraphAuthoringError>
where
    K: Clone + Eq + Hash,
{
    require_scene(scene, graph)?;
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
    publish_layout_positions(scene, vertices, positions)
}

fn publish_layout_positions(
    scene: &mut Scene,
    vertices: Vec<(GraphVertexId, noon_core::SemanticNodeId)>,
    positions: &[(f64, f64)],
) -> Result<(), GraphAuthoringError> {
    scene.with_semantic_publication(|store, publish| {
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
        || !options.center.0.is_finite()
        || !options.center.1.is_finite()
        || !options.threshold.is_finite()
        || options.threshold < 0.0
    {
        return Err(GraphAuthoringError::InvalidLayout(
            "scale, center, and threshold must be finite",
        ));
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
        GraphLayout::Random | GraphLayout::Spring => (0..count)
            .map(|_| (random.unit() - 0.5, random.unit() - 0.5))
            .collect::<Vec<_>>(),
    };
    if options.layout == GraphLayout::Spring {
        spring(&mut positions, edges, options);
    }
    rescale(&mut positions, options.scale, options.center);
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

fn rescale(points: &mut [(f64, f64)], scale: f64, center: (f64, f64)) {
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
    let factor = if extent > 0.0 { scale / extent } else { 1.0 };
    for point in points {
        point.0 = point.0 * factor + center.0;
        point.1 = point.1 * factor + center.1;
    }
}

fn spring(points: &mut [(f64, f64)], edges: &[(usize, usize)], options: &GraphLayoutOptions) {
    let count = points.len();
    let k = (1.0 / count as f64).sqrt();
    let mut temperature = 0.1;
    let cooling = temperature / (options.iterations as f64 + 1.0);
    let mut displacement = vec![(0.0, 0.0); count];
    for _ in 0..options.iterations {
        displacement.fill((0.0, 0.0));
        for i in 0..count {
            for j in (i + 1)..count {
                let (dx, dy) = (points[i].0 - points[j].0, points[i].1 - points[j].1);
                let distance = dx.hypot(dy).max(0.01);
                let force = k * k / (distance * distance);
                displacement[i].0 += dx * force;
                displacement[i].1 += dy * force;
                displacement[j].0 -= dx * force;
                displacement[j].1 -= dy * force;
            }
        }
        for &(a, b) in edges {
            let (dx, dy) = (points[a].0 - points[b].0, points[a].1 - points[b].1);
            let force = dx.hypot(dy).max(0.01) / k;
            displacement[a].0 -= dx * force;
            displacement[a].1 -= dy * force;
            displacement[b].0 += dx * force;
            displacement[b].1 += dy * force;
        }
        let mut norm = 0.0;
        for (point, &(dx, dy)) in points.iter_mut().zip(&displacement) {
            let factor = temperature / dx.hypot(dy).max(0.01);
            let (x, y) = (dx * factor, dy * factor);
            point.0 += x;
            point.1 += y;
            norm += x * x + y * y;
        }
        temperature -= cooling;
        if norm.sqrt() / (count as f64) < options.threshold {
            break;
        }
    }
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
