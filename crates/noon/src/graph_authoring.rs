//! Public retained Graph/DiGraph authoring over shared topology and ordinary semantics.
//!
//! This layer owns host/user-key adaptation and the lifetime coupling between one
//! `GraphTopology`, its `GraphSemanticBindings`, and the ordinary semantic
//! mobjects/families used to render vertices and edges. It deliberately does not
//! add a graph renderer, layout engine, or per-frame edge updater.
//!
//! Initial construction accepts explicit authored positions only. Persistent
//! topology mutation and effective endpoint following are later #697 slices.

use crate::{
    AuthoringError, GraphBindingError, GraphEdge, GraphEdgeId, GraphSemanticBindings,
    GraphTopology, GraphTopologyError, GraphVertexId, ManimArrow, ManimArrowOptions,
    ManimGeometryOptions, Mobject, MobjectFamily, MobjectTarget, Scene,
};
use noon_core::{Color, BLUE, WHITE};
use std::{collections::HashMap, hash::Hash};

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
    Binding(GraphBindingError),
    DuplicateVertexKey {
        vertex_index: usize,
    },
    UnknownEdgeEndpoint {
        edge_index: usize,
        endpoint: GraphEndpoint,
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

impl From<GraphBindingError> for GraphAuthoringError {
    fn from(value: GraphBindingError) -> Self {
        Self::Binding(value)
    }
}

impl std::fmt::Display for GraphAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(error) => error.fmt(formatter),
            Self::Topology(error) => error.fmt(formatter),
            Self::Binding(error) => error.fmt(formatter),
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
        }
    }
}

impl std::error::Error for GraphAuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Authoring(error) => Some(error),
            Self::Topology(error) => Some(error),
            Self::Binding(error) => Some(error),
            Self::DuplicateVertexKey { .. } | Self::UnknownEdgeEndpoint { .. } => None,
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
    topology: GraphTopology,
    bindings: GraphSemanticBindings,
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

    fn topology(&self) -> &GraphTopology {
        &self.topology
    }

    fn bindings(&self) -> &GraphSemanticBindings {
        &self.bindings
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
        self.topology.edge_between(start, end, self.directed)
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
/// User keys are authoring/front-end identity only. Stable graph IDs and semantic
/// handles remain the engine-owned identities used by later mutation/dependency
/// work. Moving a vertex does not yet update its edge endpoints automatically.
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

    pub fn topology(&self) -> &GraphTopology {
        self.inner.topology()
    }

    pub fn bindings(&self) -> &GraphSemanticBindings {
        self.inner.bindings()
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
/// is explicitly registered with `GraphSemanticBindings` for later endpoint
/// dependency lowering.
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

    pub fn topology(&self) -> &GraphTopology {
        self.inner.topology()
    }

    pub fn bindings(&self) -> &GraphSemanticBindings {
        self.inner.bindings()
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
    // Finish all deterministic input/topology/constructor validation before the
    // Scene receives a semantic mutation. This keeps duplicate/missing endpoints
    // and invalid authored geometry from leaving partial graph state.
    let mut topology = GraphTopology::new();
    let mut vertex_lookup = HashMap::new();
    let mut planned_vertices = Vec::new();
    let mut positions = Vec::new();

    for (vertex_index, (key, position)) in vertices.into_iter().enumerate() {
        if vertex_lookup.contains_key(&key) {
            return Err(GraphAuthoringError::DuplicateVertexKey { vertex_index });
        }
        let id = topology.add_vertex()?;
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

    // Edge families are authored before vertices so the graph root's painter
    // order keeps vertex disks above their connecting Lines/Arrows.
    let mut semantic_edges = Vec::with_capacity(planned_edges.len());
    for planned in planned_edges {
        let object = match planned.geometry {
            PlannedEdgeGeometry::Line(options) => {
                let line = scene.geometry(options)?;
                let family = scene.family(&[MobjectTarget::Object(&line)])?;
                GraphEdgeMobject::Line { family, line }
            }
            PlannedEdgeGeometry::Arrow(options) => {
                GraphEdgeMobject::Arrow(scene.manim_arrow(options)?)
            }
        };
        semantic_edges.push(GraphEdgeEntry {
            edge: planned.edge,
            object,
        });
    }

    let mut semantic_vertices = Vec::with_capacity(planned_vertices.len());
    for planned in planned_vertices {
        let object = scene.geometry(planned.options)?;
        semantic_vertices.push(GraphVertexEntry {
            key: planned.key,
            id: planned.id,
            object,
        });
    }

    let mut root_members = Vec::with_capacity(semantic_edges.len() + semantic_vertices.len());
    for edge in &semantic_edges {
        root_members.push(MobjectTarget::Family(edge.object.family()));
    }
    for vertex in &semantic_vertices {
        root_members.push(MobjectTarget::Object(&vertex.object));
    }
    let family = scene.family(&root_members)?;

    let mut bindings = GraphSemanticBindings::new();
    for vertex in &semantic_vertices {
        bindings.bind_vertex(&topology, vertex.id, &vertex.object)?;
    }
    for edge in &semantic_edges {
        bindings.bind_edge(&topology, edge.edge.id, edge.object.family())?;
        bindings.bind_edge_line(&topology, edge.edge.id, edge.object.line())?;
    }

    let edge_lookup = semantic_edges
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.edge.id, index))
        .collect();

    Ok(RetainedGraph {
        directed,
        topology,
        bindings,
        family,
        vertices: semantic_vertices,
        vertex_lookup,
        edges: semantic_edges,
        edge_lookup,
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
        let graph = scene
            .graph(
                [("a", (-2.0, 0.0)), ("b", (0.0, 1.0)), ("c", (2.0, 0.0))],
                [("a", "b"), ("b", "c")],
            )
            .unwrap();

        let a = graph.vertex_id(&"a").unwrap();
        let b = graph.vertex_id(&"b").unwrap();
        let c = graph.vertex_id(&"c").unwrap();
        assert_eq!(graph.topology().vertices(), &[a, b, c]);
        assert_eq!(
            graph.bindings().vertex_node(a),
            Some(graph.vertex(&"a").unwrap().node_id())
        );

        let ab = graph.edge_id(&"a", &"b").unwrap();
        assert_eq!(graph.edge_id(&"b", &"a"), Some(ab));
        assert_eq!(
            graph.bindings().edge_node(ab),
            Some(graph.edge(&"a", &"b").unwrap().family().node_id())
        );
        assert_eq!(
            graph.bindings().edge_line_node(ab),
            Some(graph.edge(&"a", &"b").unwrap().line().node_id())
        );

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
    }

    #[test]
    fn digraph_reuses_arrow_family_and_preserves_direction() {
        let mut scene = Scene::new();
        let graph = scene
            .digraph(
                [("a", (-1.0, 0.0)), ("b", (1.0, 0.0))],
                [("a", "b"), ("b", "a")],
            )
            .unwrap();

        let ab = graph.edge_id(&"a", &"b").unwrap();
        let ba = graph.edge_id(&"b", &"a").unwrap();
        assert_ne!(ab, ba);
        assert!(graph.edge(&"a", &"b").unwrap().arrow().is_some());
        assert!(graph.edge(&"b", &"a").unwrap().arrow().is_some());
        assert_eq!(
            graph.bindings().edge_line_node(ab),
            Some(graph.edge(&"a", &"b").unwrap().line().node_id())
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
                .iter()
                .map(|edge| (edge.start, edge.end))
                .collect::<Vec<_>>()
        );
    }
}
