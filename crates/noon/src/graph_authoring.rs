//! Public retained Graph/DiGraph authoring over shared topology and ordinary semantics.
//!
//! This layer owns host/user-key adaptation and the lifetime coupling between one
//! the shared stable `GraphTopology` and ordinary semantic
//! mobjects/families used to render vertices and edges. It deliberately does not
//! add a graph renderer, layout engine, or per-frame edge updater.
//!
//! Initial construction accepts explicit authored positions only. The complete
//! graph is published through one semantic transaction. Persistent topology
//! mutation and effective endpoint following are later #697 slices.

use crate::{
    arrow_authoring::{
        resolve_staged_arrow, stage_prepared_arrow, CommittedArrow, PreparedArrow, StagedArrow,
    },
    AuthoringError, GraphEdge, GraphEdgeId, GraphTopology, GraphTopologyError, GraphVertexId,
    ManimArrow, ManimArrowOptions, ManimGeometryOptions, Mobject, MobjectFamily, Scene,
};
use noon_core::{
    Color, GeometryResourceHandle, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticNodeId, SemanticObjectState, SemanticStore, SemanticTransactionGraphDeclaration,
    SemanticTransactionGraphEdgeBinding, BLUE, WHITE,
};
use std::{collections::HashMap, hash::Hash, rc::Rc};

pub const DEFAULT_GRAPH_VERTEX_RADIUS: f64 = 0.15;
pub const DEFAULT_GRAPH_VERTEX_STROKE_WIDTH: f64 = 0.02;
pub const DEFAULT_GRAPH_EDGE_STROKE_WIDTH: f64 = 0.04;

/// Shared appearance/configuration for the first explicit-layout Graph/DiGraph slice.
///
/// The semantic leaves remain ordinary circles, Lines, and Arrows. A directed
/// edge buff of `None` follows `vertex_radius`, keeping the default arrow tip
/// outside the filled vertex without embedding vertex geometry in topology.
#[derive(Clone, Debug)]
pub struct GraphOptions {
    pub vertex_radius: f64,
    pub vertex_fill: Color,
    pub vertex_stroke: Color,
    pub vertex_stroke_width: f64,
    pub edge_color: Color,
    pub edge_stroke_width: f64,
    pub directed_edge_buff: Option<f64>,
}

impl Default for GraphOptions {
    fn default() -> Self {
        Self {
            vertex_radius: DEFAULT_GRAPH_VERTEX_RADIUS,
            vertex_fill: BLUE,
            vertex_stroke: WHITE,
            vertex_stroke_width: DEFAULT_GRAPH_VERTEX_STROKE_WIDTH,
            edge_color: WHITE,
            edge_stroke_width: DEFAULT_GRAPH_EDGE_STROKE_WIDTH,
            directed_edge_buff: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphEndpoint {
    Start,
    End,
}

#[derive(Debug)]
pub enum GraphAuthoringError {
    Authoring(AuthoringError),
    Topology(GraphTopologyError),
    DuplicateVertexKey {
        vertex_index: usize,
    },
    UnknownEdgeEndpoint {
        edge_index: usize,
        endpoint: GraphEndpoint,
    },
    SelfEdgeUnsupported {
        edge_index: usize,
    },
}

impl From<AuthoringError> for GraphAuthoringError {
    fn from(value: AuthoringError) -> Self {
        Self::Authoring(value)
    }
}

impl From<GraphTopologyError> for GraphAuthoringError {
    fn from(value: GraphTopologyError) -> Self {
        Self::Topology(value)
    }
}

impl std::fmt::Display for GraphAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(error) => error.fmt(formatter),
            Self::Topology(error) => error.fmt(formatter),
            Self::DuplicateVertexKey { vertex_index } => write!(
                formatter,
                "graph vertex at input index {vertex_index} repeats an existing user key"
            ),
            Self::UnknownEdgeEndpoint {
                edge_index,
                endpoint,
            } => write!(
                formatter,
                "graph edge at input index {edge_index} references an unknown {} vertex key",
                match endpoint {
                    GraphEndpoint::Start => "start",
                    GraphEndpoint::End => "end",
                }
            ),
            Self::SelfEdgeUnsupported { edge_index } => write!(
                formatter,
                "graph edge at input index {edge_index} is a self-edge; public Graph self-loop geometry is not implemented yet"
            ),
        }
    }
}

impl std::error::Error for GraphAuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Authoring(error) => Some(error),
            Self::Topology(error) => Some(error),
            Self::DuplicateVertexKey { .. }
            | Self::UnknownEdgeEndpoint { .. }
            | Self::SelfEdgeUnsupported { .. } => None,
        }
    }
}

/// Ordinary retained semantic representation of one graph edge.
#[derive(Clone, Debug)]
pub enum GraphEdgeMobject {
    /// Undirected edge: one analytic Line inside a one-member family.
    Line {
        family: MobjectFamily,
        line: Mobject,
    },
    /// Directed edge: the existing shared Arrow family.
    Arrow(ManimArrow),
}

impl GraphEdgeMobject {
    pub fn family(&self) -> &MobjectFamily {
        match self {
            Self::Line { family, .. } => family,
            Self::Arrow(arrow) => arrow.family(),
        }
    }

    /// Analytic Line component used by endpoint dependency work.
    pub fn line(&self) -> &Mobject {
        match self {
            Self::Line { line, .. } => line,
            Self::Arrow(arrow) => arrow.shaft(),
        }
    }

    pub fn arrow(&self) -> Option<&ManimArrow> {
        match self {
            Self::Line { .. } => None,
            Self::Arrow(arrow) => Some(arrow),
        }
    }
}

struct GraphVertexEntry<K> {
    key: K,
    id: GraphVertexId,
    object: Mobject,
}

struct GraphEdgeEntry {
    edge: GraphEdge,
    object: GraphEdgeMobject,
}

struct RetainedGraph<K> {
    directed: bool,
    family: MobjectFamily,
    vertices: Vec<GraphVertexEntry<K>>,
    vertex_lookup: HashMap<K, usize>,
    edges: Vec<GraphEdgeEntry>,
    edge_lookup: HashMap<GraphEdgeId, usize>,
}

impl<K: Eq + Hash> RetainedGraph<K> {
    fn family(&self) -> &MobjectFamily {
        &self.family
    }

    fn topology(&self) -> GraphTopology {
        self.semantic_declaration().topology().clone()
    }

    fn semantic_declaration(&self) -> noon_core::SemanticGraphDeclaration {
        self.family
            .integration_store()
            .borrow()
            .semantic_graph_declaration(self.family.node_id())
            .expect("graph root remains a family")
            .expect("public Graph root retains authored graph declaration")
            .clone()
    }

    fn vertex_id(&self, key: &K) -> Option<GraphVertexId> {
        self.vertex_lookup
            .get(key)
            .map(|&index| self.vertices[index].id)
    }

    fn vertex(&self, key: &K) -> Option<&Mobject> {
        self.vertex_lookup
            .get(key)
            .map(|&index| &self.vertices[index].object)
    }

    fn edge_id(&self, start: &K, end: &K) -> Option<GraphEdgeId> {
        let start = self.vertex_id(start)?;
        let end = self.vertex_id(end)?;
        self.family
            .integration_store()
            .borrow()
            .semantic_graph_declaration(self.family.node_id())
            .ok()
            .flatten()?
            .edge_between(start, end, self.directed)
    }

    fn edge(&self, start: &K, end: &K) -> Option<&GraphEdgeMobject> {
        let id = self.edge_id(start, end)?;
        self.edge_lookup
            .get(&id)
            .map(|&index| &self.edges[index].object)
    }

    fn vertex_keys(&self) -> impl Iterator<Item = &K> {
        self.vertices.iter().map(|entry| &entry.key)
    }

    fn edge_mobjects(&self) -> impl Iterator<Item = (GraphEdge, &GraphEdgeMobject)> {
        self.edges.iter().map(|entry| (entry.edge, &entry.object))
    }
}

/// Explicit-position undirected retained graph.
///
/// User keys are frontend lookup convenience only. Stable GraphVertexId/GraphEdgeId
/// identities and the topology/dependency declaration live on the semantic graph root
/// and survives this wrapper. Moving a vertex does not yet update its edge
/// endpoints automatically.
pub struct Graph<K> {
    inner: RetainedGraph<K>,
}

impl<K: Clone + Eq + Hash> Graph<K> {
    pub fn new<V, E>(
        scene: &mut Scene,
        vertices: V,
        edges: E,
    ) -> Result<Self, GraphAuthoringError>
    where
        V: IntoIterator<Item = (K, (f64, f64))>,
        E: IntoIterator<Item = (K, K)>,
    {
        Self::with_options(scene, vertices, edges, GraphOptions::default())
    }

    pub fn with_options<V, E>(
        scene: &mut Scene,
        vertices: V,
        edges: E,
        options: GraphOptions,
    ) -> Result<Self, GraphAuthoringError>
    where
        V: IntoIterator<Item = (K, (f64, f64))>,
        E: IntoIterator<Item = (K, K)>,
    {
        Ok(Self {
            inner: build_graph(scene, vertices, edges, false, options)?,
        })
    }

    pub fn family(&self) -> &MobjectFamily {
        self.inner.family()
    }

    /// Snapshot the authoritative semantic Graph topology.
    pub fn topology(&self) -> GraphTopology {
        self.inner.topology()
    }

    pub fn semantic_declaration(&self) -> noon_core::SemanticGraphDeclaration {
        self.inner.semantic_declaration()
    }

    pub fn vertex_id(&self, key: &K) -> Option<GraphVertexId> {
        self.inner.vertex_id(key)
    }

    pub fn vertex(&self, key: &K) -> Option<&Mobject> {
        self.inner.vertex(key)
    }

    pub fn edge_id(&self, start: &K, end: &K) -> Option<GraphEdgeId> {
        self.inner.edge_id(start, end)
    }

    pub fn edge(&self, start: &K, end: &K) -> Option<&GraphEdgeMobject> {
        self.inner.edge(start, end)
    }

    pub fn vertex_keys(&self) -> impl Iterator<Item = &K> {
        self.inner.vertex_keys()
    }

    pub fn edge_mobjects(&self) -> impl Iterator<Item = (GraphEdge, &GraphEdgeMobject)> {
        self.inner.edge_mobjects()
    }
}

/// Explicit-position directed retained graph.
///
/// Directed edges reuse the ordinary shared Arrow implementation; the shaft Line
/// and endpoint vertex identities are authored on the semantic graph root for
/// later endpoint dependency lowering.
pub struct DiGraph<K> {
    inner: RetainedGraph<K>,
}

impl<K: Clone + Eq + Hash> DiGraph<K> {
    pub fn new<V, E>(
        scene: &mut Scene,
        vertices: V,
        edges: E,
    ) -> Result<Self, GraphAuthoringError>
    where
        V: IntoIterator<Item = (K, (f64, f64))>,
        E: IntoIterator<Item = (K, K)>,
    {
        Self::with_options(scene, vertices, edges, GraphOptions::default())
    }

    pub fn with_options<V, E>(
        scene: &mut Scene,
        vertices: V,
        edges: E,
        options: GraphOptions,
    ) -> Result<Self, GraphAuthoringError>
    where
        V: IntoIterator<Item = (K, (f64, f64))>,
        E: IntoIterator<Item = (K, K)>,
    {
        Ok(Self {
            inner: build_graph(scene, vertices, edges, true, options)?,
        })
    }

    pub fn family(&self) -> &MobjectFamily {
        self.inner.family()
    }

    /// Snapshot the authoritative semantic Graph topology.
    pub fn topology(&self) -> GraphTopology {
        self.inner.topology()
    }

    pub fn semantic_declaration(&self) -> noon_core::SemanticGraphDeclaration {
        self.inner.semantic_declaration()
    }

    pub fn vertex_id(&self, key: &K) -> Option<GraphVertexId> {
        self.inner.vertex_id(key)
    }

    pub fn vertex(&self, key: &K) -> Option<&Mobject> {
        self.inner.vertex(key)
    }

    pub fn edge_id(&self, start: &K, end: &K) -> Option<GraphEdgeId> {
        self.inner.edge_id(start, end)
    }

    pub fn edge(&self, start: &K, end: &K) -> Option<&GraphEdgeMobject> {
        self.inner.edge(start, end)
    }

    pub fn vertex_keys(&self) -> impl Iterator<Item = &K> {
        self.inner.vertex_keys()
    }

    pub fn edge_mobjects(&self) -> impl Iterator<Item = (GraphEdge, &GraphEdgeMobject)> {
        self.inner.edge_mobjects()
    }
}

impl Scene {
    pub fn graph<K, V, E>(
        &mut self,
        vertices: V,
        edges: E,
    ) -> Result<Graph<K>, GraphAuthoringError>
    where
        K: Clone + Eq + Hash,
        V: IntoIterator<Item = (K, (f64, f64))>,
        E: IntoIterator<Item = (K, K)>,
    {
        Graph::new(self, vertices, edges)
    }

    pub fn graph_with_options<K, V, E>(
        &mut self,
        vertices: V,
        edges: E,
        options: GraphOptions,
    ) -> Result<Graph<K>, GraphAuthoringError>
    where
        K: Clone + Eq + Hash,
        V: IntoIterator<Item = (K, (f64, f64))>,
        E: IntoIterator<Item = (K, K)>,
    {
        Graph::with_options(self, vertices, edges, options)
    }

    pub fn digraph<K, V, E>(
        &mut self,
        vertices: V,
        edges: E,
    ) -> Result<DiGraph<K>, GraphAuthoringError>
    where
        K: Clone + Eq + Hash,
        V: IntoIterator<Item = (K, (f64, f64))>,
        E: IntoIterator<Item = (K, K)>,
    {
        DiGraph::new(self, vertices, edges)
    }

    pub fn digraph_with_options<K, V, E>(
        &mut self,
        vertices: V,
        edges: E,
        options: GraphOptions,
    ) -> Result<DiGraph<K>, GraphAuthoringError>
    where
        K: Clone + Eq + Hash,
        V: IntoIterator<Item = (K, (f64, f64))>,
        E: IntoIterator<Item = (K, K)>,
    {
        DiGraph::with_options(self, vertices, edges, options)
    }
}

struct PlannedVertex<K> {
    key: K,
    id: GraphVertexId,
    options: ManimGeometryOptions,
}

enum PlannedEdgeGeometry {
    Line(ManimGeometryOptions),
    Arrow(ManimArrowOptions),
}

struct PlannedEdge {
    edge: GraphEdge,
    geometry: PlannedEdgeGeometry,
}

struct PreparedVertex<K> {
    key: K,
    id: GraphVertexId,
    state: SemanticObjectState,
}

enum PreparedEdgeGeometry {
    Line(SemanticObjectState),
    Arrow(PreparedArrow),
}

struct PreparedEdge {
    edge: GraphEdge,
    geometry: PreparedEdgeGeometry,
}

struct StagedVertex<K> {
    key: K,
    id: GraphVertexId,
    node: noon_core::SemanticLocalNodeToken,
}

enum StagedEdgeGeometry {
    Line {
        family: noon_core::SemanticLocalNodeToken,
        line: noon_core::SemanticLocalNodeToken,
    },
    Arrow(StagedArrow),
}

struct StagedEdge {
    edge: GraphEdge,
    geometry: StagedEdgeGeometry,
}

struct StagedGraph<K> {
    root: noon_core::SemanticLocalNodeToken,
    vertices: Vec<StagedVertex<K>>,
    edges: Vec<StagedEdge>,
}

struct CommittedVertex<K> {
    key: K,
    id: GraphVertexId,
    node: SemanticNodeId,
}

enum CommittedEdgeGeometry {
    Line {
        family: SemanticNodeId,
        line: SemanticNodeId,
    },
    Arrow(CommittedArrow),
}

struct CommittedEdge {
    edge: GraphEdge,
    geometry: CommittedEdgeGeometry,
}

struct CommittedGraph<K> {
    root: SemanticNodeId,
    vertices: Vec<CommittedVertex<K>>,
    edges: Vec<CommittedEdge>,
}

fn build_graph<K, V, E>(
    scene: &mut Scene,
    vertices: V,
    edges: E,
    directed: bool,
    options: GraphOptions,
) -> Result<RetainedGraph<K>, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    V: IntoIterator<Item = (K, (f64, f64))>,
    E: IntoIterator<Item = (K, K)>,
{
    // Finish deterministic input/topology/constructor validation before the
    // Scene receives any semantic mutation.
    let mut topology = GraphTopology::new();
    let mut vertex_lookup = HashMap::new();
    let mut planned_vertices = Vec::new();
    let mut positions = Vec::new();

    for (vertex_index, (key, position)) in vertices.into_iter().enumerate() {
        if vertex_lookup.contains_key(&key) {
            return Err(GraphAuthoringError::DuplicateVertexKey { vertex_index });
        }
        let id = topology.add_vertex();
        let object = vertex_options(position, &options)?;
        vertex_lookup.insert(key.clone(), planned_vertices.len());
        positions.push(position);
        planned_vertices.push(PlannedVertex {
            key,
            id,
            options: object,
        });
    }

    let mut planned_edges = Vec::new();
    for (edge_index, (start_key, end_key)) in edges.into_iter().enumerate() {
        let start_index = *vertex_lookup
            .get(&start_key)
            .ok_or(GraphAuthoringError::UnknownEdgeEndpoint {
                edge_index,
                endpoint: GraphEndpoint::Start,
            })?;
        let end_index = *vertex_lookup
            .get(&end_key)
            .ok_or(GraphAuthoringError::UnknownEdgeEndpoint {
                edge_index,
                endpoint: GraphEndpoint::End,
            })?;
        let start = planned_vertices[start_index].id;
        let end = planned_vertices[end_index].id;
        if start == end {
            return Err(GraphAuthoringError::SelfEdgeUnsupported { edge_index });
        }
        let id = topology.add_edge(start, end, directed)?;
        let edge = topology.edge(id).expect("new graph edge is present");
        let start_position = positions[start_index];
        let end_position = positions[end_index];
        let geometry = if directed {
            PlannedEdgeGeometry::Arrow(arrow_options(start_position, end_position, &options)?)
        } else {
            PlannedEdgeGeometry::Line(line_options(start_position, end_position, &options)?)
        };
        planned_edges.push(PlannedEdge { edge, geometry });
    }

    let store_rc = Rc::clone(scene.integration_store());
    let committed = scene.with_semantic_publication(|store, publish| {
        let vertices = planned_vertices
            .into_iter()
            .map(|vertex| {
                Ok(PreparedVertex {
                    key: vertex.key,
                    id: vertex.id,
                    state: vertex.options.into_state(store)?,
                })
            })
            .collect::<Result<Vec<_>, AuthoringError>>()?;
        let edges = planned_edges
            .into_iter()
            .map(|edge| {
                let geometry = match edge.geometry {
                    PlannedEdgeGeometry::Line(options) => {
                        PreparedEdgeGeometry::Line(options.into_state(store)?)
                    }
                    PlannedEdgeGeometry::Arrow(options) => {
                        PreparedEdgeGeometry::Arrow(options.prepare(store)?)
                    }
                };
                Ok(PreparedEdge {
                    edge: edge.edge,
                    geometry,
                })
            })
            .collect::<Result<Vec<_>, AuthoringError>>()?;

        let mut paths = Vec::new();
        for edge in &edges {
            if let PreparedEdgeGeometry::Arrow(arrow) = &edge.geometry {
                paths.push(arrow.end_tip.clone());
                if let Some(start_tip) = &arrow.start_tip {
                    paths.push(start_tip.clone());
                }
            }
        }

        if paths.is_empty() {
            return publish_prepared_graph(store, &topology, vertices, edges, &[], publish);
        }

        store.with_geometry_paths(paths, |store, handles| {
            publish_prepared_graph(store, &topology, vertices, edges, handles, publish)
        })
    })?;

    let family = MobjectFamily::from_node(Rc::clone(&store_rc), committed.root)
        .expect("committed graph root remains a family");
    let semantic_vertices = committed
        .vertices
        .into_iter()
        .map(|vertex| GraphVertexEntry {
            key: vertex.key,
            id: vertex.id,
            object: Mobject::from_node(Rc::clone(&store_rc), vertex.node)
                .expect("committed graph vertex remains an object"),
        })
        .collect::<Vec<_>>();
    let semantic_edges = committed
        .edges
        .into_iter()
        .map(|edge| {
            let object = match edge.geometry {
                CommittedEdgeGeometry::Line { family, line } => GraphEdgeMobject::Line {
                    family: MobjectFamily::from_node(Rc::clone(&store_rc), family)
                        .expect("committed graph edge root remains a family"),
                    line: Mobject::from_node(Rc::clone(&store_rc), line)
                        .expect("committed graph edge Line remains an object"),
                },
                CommittedEdgeGeometry::Arrow(arrow) => GraphEdgeMobject::Arrow(
                    ManimArrow::from_committed(Rc::clone(&store_rc), arrow)
                        .expect("committed graph Arrow remains structurally valid"),
                ),
            };
            GraphEdgeEntry {
                edge: edge.edge,
                object,
            }
        })
        .collect::<Vec<_>>();

    let edge_lookup = semantic_edges
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.edge.id, index))
        .collect();

    Ok(RetainedGraph {
        directed,
        family,
        vertices: semantic_vertices,
        vertex_lookup,
        edges: semantic_edges,
        edge_lookup,
    })
}

fn publish_prepared_graph<K>(
    store: &mut SemanticStore,
    topology: &GraphTopology,
    vertices: Vec<PreparedVertex<K>>,
    edges: Vec<PreparedEdge>,
    handles: &[GeometryResourceHandle],
    publish: &mut dyn FnMut(
        &mut SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError>,
) -> Result<CommittedGraph<K>, AuthoringError> {
    let mut transaction = SemanticMutationTransaction::new();
    let root = transaction.create_node(SemanticNodeCreation::family());
    let mut staged_edges = Vec::with_capacity(edges.len());
    let mut handle_index = 0usize;

    // Edges precede vertices in the root family so equal-priority vertices paint
    // above their connectors without encoding graph knowledge in the renderer.
    for edge in edges {
        let geometry = match edge.geometry {
            PreparedEdgeGeometry::Line(state) => {
                let line = transaction.create_node(SemanticNodeCreation::object(state));
                let family = transaction.create_node(SemanticNodeCreation::family());
                transaction.add_member(family, line);
                transaction.add_member(root, family);
                StagedEdgeGeometry::Line { family, line }
            }
            PreparedEdgeGeometry::Arrow(arrow) => {
                let end_tip = handles
                    .get(handle_index)
                    .copied()
                    .expect("one resource handle per prepared Arrow end tip");
                handle_index += 1;
                let start_tip = if arrow.start_tip.is_some() {
                    let handle = handles
                        .get(handle_index)
                        .copied()
                        .expect("one resource handle per prepared Arrow start tip");
                    handle_index += 1;
                    Some(handle)
                } else {
                    None
                };
                let staged =
                    stage_prepared_arrow(&mut transaction, &arrow, end_tip, start_tip);
                transaction.add_member(root, staged.family);
                StagedEdgeGeometry::Arrow(staged)
            }
        };
        staged_edges.push(StagedEdge {
            edge: edge.edge,
            geometry,
        });
    }
    debug_assert_eq!(handle_index, handles.len());

    let mut staged_vertices = Vec::with_capacity(vertices.len());
    for vertex in vertices {
        let node = transaction.create_node(SemanticNodeCreation::object(vertex.state));
        transaction.add_member(root, node);
        staged_vertices.push(StagedVertex {
            key: vertex.key,
            id: vertex.id,
            node,
        });
    }

    // The graph declaration itself is authored Semantic Scene state. The public
    // Graph<K> key/index tables below are only frontend convenience and can be
    // dropped without losing topology or endpoint dependencies.
    let graph_vertices = staged_vertices
        .iter()
        .map(|vertex| (vertex.id, vertex.node));
    let graph_edges = staged_edges.iter().map(|edge| {
        let (family, line) = match &edge.geometry {
            StagedEdgeGeometry::Line { family, line } => (*family, *line),
            StagedEdgeGeometry::Arrow(arrow) => (arrow.family, arrow.shaft),
        };
        SemanticTransactionGraphEdgeBinding::new(edge.edge.id, family.into(), line.into())
    });
    transaction.set_graph_declaration(
        root,
        SemanticTransactionGraphDeclaration::new(topology.clone(), graph_vertices, graph_edges),
    );

    let result = publish(store, transaction)?;
    let root = result
        .resolve(root)
        .ok_or(AuthoringError::UnresolvedCreatedNode(root))?;
    let vertices = staged_vertices
        .into_iter()
        .map(|vertex| {
            Ok(CommittedVertex {
                key: vertex.key,
                id: vertex.id,
                node: result
                    .resolve(vertex.node)
                    .ok_or(AuthoringError::UnresolvedCreatedNode(vertex.node))?,
            })
        })
        .collect::<Result<Vec<_>, AuthoringError>>()?;
    let edges = staged_edges
        .into_iter()
        .map(|edge| {
            let geometry = match edge.geometry {
                StagedEdgeGeometry::Line { family, line } => {
                    CommittedEdgeGeometry::Line {
                        family: result
                            .resolve(family)
                            .ok_or(AuthoringError::UnresolvedCreatedNode(family))?,
                        line: result
                            .resolve(line)
                            .ok_or(AuthoringError::UnresolvedCreatedNode(line))?,
                    }
                }
                StagedEdgeGeometry::Arrow(arrow) => CommittedEdgeGeometry::Arrow(
                    resolve_staged_arrow(arrow, |token| result.resolve(token))?,
                ),
            };
            Ok(CommittedEdge {
                edge: edge.edge,
                geometry,
            })
        })
        .collect::<Result<Vec<_>, AuthoringError>>()?;

    Ok(CommittedGraph {
        root,
        vertices,
        edges,
    })
}

fn vertex_options(
    position: (f64, f64),
    options: &GraphOptions,
) -> Result<ManimGeometryOptions, AuthoringError> {
    let mut vertex = ManimGeometryOptions::circle(options.vertex_radius)?;
    vertex.set_translation(position.0, position.1)?;
    vertex.set_fill(
        f64::from(options.vertex_fill.red),
        f64::from(options.vertex_fill.green),
        f64::from(options.vertex_fill.blue),
        f64::from(options.vertex_fill.alpha),
    )?;
    vertex.set_stroke(
        f64::from(options.vertex_stroke.red),
        f64::from(options.vertex_stroke.green),
        f64::from(options.vertex_stroke.blue),
        f64::from(options.vertex_stroke.alpha),
    )?;
    vertex.set_stroke_width(options.vertex_stroke_width)?;
    Ok(vertex)
}

fn line_options(
    start: (f64, f64),
    end: (f64, f64),
    options: &GraphOptions,
) -> Result<ManimGeometryOptions, AuthoringError> {
    let mut line = ManimGeometryOptions::line(start.0, start.1, end.0, end.1)?;
    line.disable_fill();
    line.set_stroke(
        f64::from(options.edge_color.red),
        f64::from(options.edge_color.green),
        f64::from(options.edge_color.blue),
        f64::from(options.edge_color.alpha),
    )?;
    line.set_stroke_width(options.edge_stroke_width)?;
    Ok(line)
}

fn arrow_options(
    start: (f64, f64),
    end: (f64, f64),
    options: &GraphOptions,
) -> Result<ManimArrowOptions, AuthoringError> {
    let mut arrow = ManimArrowOptions::arrow(start.0, start.1, end.0, end.1)?;
    arrow.set_buff(options.directed_edge_buff.unwrap_or(options.vertex_radius))?;
    arrow.set_color(
        f64::from(options.edge_color.red),
        f64::from(options.edge_color.green),
        f64::from(options.edge_color.blue),
        f64::from(options.edge_color.alpha),
    )?;
    arrow.set_stroke_width(options.edge_stroke_width)?;
    Ok(arrow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::StoredGeometry;

    #[test]
    fn explicit_graph_binds_user_keys_to_stable_topology_and_semantic_identity() {
        let mut scene = Scene::new();
        let before = scene.revision();
        let graph = scene
            .graph(
                [("a", (-2.0, 0.0)), ("b", (0.0, 1.0)), ("c", (2.0, 0.0))],
                [("a", "b"), ("b", "c")],
            )
            .unwrap();
        assert_eq!(scene.revision(), before.checked_next().unwrap());

        let a = graph.vertex_id(&"a").unwrap();
        let b = graph.vertex_id(&"b").unwrap();
        let c = graph.vertex_id(&"c").unwrap();
        assert_eq!(
            graph.topology().vertices().collect::<Vec<_>>(),
            vec![a, b, c]
        );
        assert_eq!(
            graph.semantic_declaration().vertex_node(a),
            Some(graph.vertex(&"a").unwrap().node_id())
        );

        let ab = graph.edge_id(&"a", &"b").unwrap();
        assert_eq!(graph.edge_id(&"b", &"a"), Some(ab));
        let binding = graph.semantic_declaration().edge_binding(ab).unwrap();
        assert_eq!(
            binding.family(),
            graph.edge(&"a", &"b").unwrap().family().node_id()
        );
        assert_eq!(binding.line(), graph.edge(&"a", &"b").unwrap().line().node_id());

        let store = scene.integration_store().borrow();
        let root_members = store
            .semantic_family_members_checked(graph.family().node_id())
            .unwrap();
        assert_eq!(root_members.len(), 5);
        assert_eq!(root_members[0], graph.edge(&"a", &"b").unwrap().family().node_id());
        assert_eq!(root_members[2], graph.vertex(&"a").unwrap().node_id());
        assert!(matches!(
            graph.edge(&"a", &"b").unwrap().line().state().unwrap().content.geometry(),
            Some(StoredGeometry::Line { .. })
        ));

        let root = graph.family().node_id();
        let semantic_a = graph.vertex(&"a").unwrap().node_id();
        let semantic_b = graph.vertex(&"b").unwrap().node_id();
        let edge_family = graph.edge(&"a", &"b").unwrap().family().node_id();
        let declaration = graph.semantic_declaration();
        assert_eq!(declaration.vertex_node(a), Some(semantic_a));
        assert_eq!(declaration.vertex_node(b), Some(semantic_b));
        assert_eq!(declaration.edge_between(b, a, false), Some(ab));
        assert_eq!(
            declaration.incident_edge_nodes(b).unwrap().len(),
            2,
            "semantic adjacency is authored independently of the wrapper index"
        );
        drop(graph);
        let store = scene.integration_store().borrow();
        assert_eq!(
            store
                .semantic_graph_declaration(root)
                .unwrap()
                .unwrap()
                .edge_binding(ab)
                .map(|binding| binding.family()),
            Some(edge_family)
        );
    }

    #[test]
    fn digraph_reuses_arrow_family_and_publishes_once() {
        let mut scene = Scene::new();
        let before = scene.revision();
        let graph = scene
            .digraph(
                [("a", (-1.0, 0.0)), ("b", (1.0, 0.0))],
                [("a", "b"), ("b", "a")],
            )
            .unwrap();
        assert_eq!(scene.revision(), before.checked_next().unwrap());

        let ab = graph.edge_id(&"a", &"b").unwrap();
        let ba = graph.edge_id(&"b", &"a").unwrap();
        assert_ne!(ab, ba);
        assert!(graph.edge(&"a", &"b").unwrap().arrow().is_some());
        assert!(graph.edge(&"b", &"a").unwrap().arrow().is_some());
        assert_eq!(
            graph.semantic_declaration().edge_binding(ab).unwrap().line(),
            graph.edge(&"a", &"b").unwrap().line().node_id()
        );
    }

    #[test]
    fn invalid_key_or_endpoint_fails_before_semantic_mutation() {
        let mut scene = Scene::new();
        let revision = scene.revision();
        let nodes = scene.integration_store().borrow().len();

        assert!(matches!(
            scene.graph(
                [("a", (0.0, 0.0)), ("a", (1.0, 0.0))],
                std::iter::empty::<(&str, &str)>(),
            ),
            Err(GraphAuthoringError::DuplicateVertexKey { .. })
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.integration_store().borrow().len(), nodes);

        assert!(matches!(
            scene.graph([("a", (0.0, 0.0))], [("a", "missing")]),
            Err(GraphAuthoringError::UnknownEdgeEndpoint {
                endpoint: GraphEndpoint::End,
                ..
            })
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
    }

    #[test]
    fn self_edges_reject_before_semantic_mutation_until_loop_geometry_exists() {
        let mut scene = Scene::new();
        let revision = scene.revision();
        let nodes = scene.integration_store().borrow().len();
        assert!(matches!(
            scene.graph([("a", (0.0, 0.0))], [("a", "a")]),
            Err(GraphAuthoringError::SelfEdgeUnsupported { edge_index: 0 })
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
    }

    #[test]
    fn duplicate_edges_fail_before_semantic_mutation() {
        let mut scene = Scene::new();
        let revision = scene.revision();
        let nodes = scene.integration_store().borrow().len();

        assert!(matches!(
            scene.graph(
                [("a", (-1.0, 0.0)), ("b", (1.0, 0.0))],
                [("a", "b"), ("b", "a")],
            ),
            Err(GraphAuthoringError::Topology(
                GraphTopologyError::DuplicateEdge { directed: false, .. }
            ))
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
    }

    #[test]
    fn invalid_style_is_rejected_before_semantic_mutation() {
        let mut scene = Scene::new();
        let revision = scene.revision();
        let nodes = scene.integration_store().borrow().len();
        let options = GraphOptions {
            edge_stroke_width: f64::NAN,
            ..GraphOptions::default()
        };

        assert!(scene
            .graph_with_options(
                [("a", (-1.0, 0.0)), ("b", (1.0, 0.0))],
                [("a", "b")],
                options,
            )
            .is_err());
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
    }

    #[test]
    fn running_scene_graph_construction_publishes_once_and_stays_detached_until_added() {
        let mut scene = Scene::new();
        let sentinel = scene.circle(0.25).unwrap();
        scene.add(&sentinel).unwrap();
        let execution = scene.execution_session().unwrap();
        scene.install_execution(execution);
        let before = scene.revision();

        let graph = scene
            .graph(
                [("a", (-1.0, 0.0)), ("b", (1.0, 0.0))],
                [("a", "b")],
            )
            .unwrap();

        assert_eq!(scene.revision(), before.checked_next().unwrap());
        assert_eq!(
            scene.owned_execution().publication_context().scene_revision(),
            scene.revision()
        );
        assert!(scene
            .owned_execution()
            .execution_object_id(graph.vertex(&"a").unwrap().node_id())
            .is_none());

        scene
            .add_many(&[crate::MobjectTarget::Family(graph.family())])
            .unwrap();
        assert!(scene
            .owned_execution()
            .execution_object_id(graph.vertex(&"a").unwrap().node_id())
            .is_some());
        assert!(scene
            .owned_execution()
            .execution_object_id(graph.edge(&"a", &"b").unwrap().line().node_id())
            .is_some());
    }

    #[test]
    fn lookup_iteration_preserves_input_and_topology_order() {
        let mut scene = Scene::new();
        let graph = scene
            .graph(
                [(10, (-1.0, 0.0)), (20, (0.0, 0.0)), (30, (1.0, 0.0))],
                [(20, 30), (10, 20)],
            )
            .unwrap();
        assert_eq!(graph.vertex_keys().copied().collect::<Vec<_>>(), vec![10, 20, 30]);
        assert_eq!(
            graph
                .edge_mobjects()
                .map(|(edge, _)| (edge.start, edge.end))
                .collect::<Vec<_>>(),
            graph
                .topology()
                .edges()
                .map(|edge| (edge.start, edge.end))
                .collect::<Vec<_>>()
        );
    }
}
