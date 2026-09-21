//! Explicit-position Graph/DiGraph authoring over ordinary retained semantics.
//!
//! Topology and semantic bindings live on the semantic family root. These
//! wrappers retain only user-key lookup and ordinary object/family handles.
//! Construction publishes one transaction; reads borrow the authoritative
//! declaration instead of cloning it. Endpoint following and topology editing
//! remain separate #697 work.

mod construction;
#[cfg(test)]
mod tests;

use construction::build_graph;
use crate::{
    AuthoringError, GraphEdge, GraphEdgeId, GraphTopology, GraphTopologyError, GraphVertexId,
    ManimArrow, Mobject, MobjectFamily, Scene,
};
use noon_core::{Color, SemanticGraphDeclaration, SemanticNodeId, SemanticStoreError, BLUE, WHITE};
use std::{cell::Ref, collections::HashMap, hash::Hash};

pub const DEFAULT_GRAPH_VERTEX_RADIUS: f64 = 0.15;
pub const DEFAULT_GRAPH_VERTEX_STROKE_WIDTH: f64 = 0.02;
pub const DEFAULT_GRAPH_EDGE_STROKE_WIDTH: f64 = 0.04;

/// Shared appearance for explicit-position Graph/DiGraph construction.
///
/// All options are validated, including for an empty graph. Directed edges use
/// `vertex_radius` as their default buff. Explicit positions are authored once;
/// moving a vertex does not yet update the incident edges automatically.
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
    /// The root is missing, stale, or not a semantic family.
    Store(SemanticStoreError),
    /// An explicit integration borrow currently holds the store exclusively.
    StoreBorrowed,
    /// The live root has no authored graph declaration.
    MissingDeclaration(SemanticNodeId),
    DuplicateVertexKey { vertex_index: usize },
    UnknownEdgeEndpoint { edge_index: usize, endpoint: GraphEndpoint },
    SelfEdgeUnsupported { edge_index: usize },
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
            Self::Store(error) => error.fmt(formatter),
            Self::StoreBorrowed => formatter.write_str("graph store is exclusively borrowed"),
            Self::MissingDeclaration(root) => write!(
                formatter,
                "semantic graph root {root:?} has no graph declaration"
            ),
            Self::DuplicateVertexKey { vertex_index } => write!(
                formatter,
                "graph vertex at input index {vertex_index} repeats an existing user key"
            ),
            Self::UnknownEdgeEndpoint { edge_index, endpoint } => write!(
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
            Self::Store(error) => Some(error),
            _ => None,
        }
    }
}

/// Ordinary retained semantic representation of one graph edge.
#[derive(Clone, Debug)]
pub enum GraphEdgeMobject {
    /// An analytic Line inside a one-member family.
    Line { family: MobjectFamily, line: Mobject },
    /// The existing shared retained Arrow family.
    Arrow(ManimArrow),
}

impl GraphEdgeMobject {
    pub fn family(&self) -> &MobjectFamily {
        match self {
            Self::Line { family, .. } => family,
            Self::Arrow(arrow) => arrow.family(),
        }
    }

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
    fn semantic_declaration(&self) -> Result<Ref<'_, SemanticGraphDeclaration>, GraphAuthoringError> {
        let root = self.family.node_id();
        let store = self.family.integration_store().try_borrow()
            .map_err(|_| GraphAuthoringError::StoreBorrowed)?;
        // Preserve the typed stale-root/wrong-kind error before projecting the
        // borrow. The exclusive mutation boundary cannot change this proof while
        // the returned Ref is alive.
        store.semantic_graph_declaration(root).map_err(GraphAuthoringError::Store)?;
        Ref::filter_map(store, |store| {
            store.node(root).and_then(|node| node.graph_declaration())
        }).map_err(|_| GraphAuthoringError::MissingDeclaration(root))
    }

    fn topology(&self) -> Result<Ref<'_, GraphTopology>, GraphAuthoringError> {
        Ok(Ref::map(self.semantic_declaration()?, SemanticGraphDeclaration::topology))
    }

    fn vertex_id(&self, key: &K) -> Option<GraphVertexId> {
        self.vertex_lookup.get(key).map(|&index| self.vertices[index].id)
    }

    fn vertex(&self, key: &K) -> Option<&Mobject> {
        self.vertex_lookup.get(key).map(|&index| &self.vertices[index].object)
    }

    fn edge_id(&self, start: &K, end: &K) -> Option<GraphEdgeId> {
        let start = self.vertex_id(start)?;
        let end = self.vertex_id(end)?;
        self.semantic_declaration().ok()?.edge_between(start, end, self.directed)
    }

    fn edge(&self, start: &K, end: &K) -> Option<&GraphEdgeMobject> {
        let id = self.edge_id(start, end)?;
        self.edge_lookup.get(&id).map(|&index| &self.edges[index].object)
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
/// Topology survives this wrapper on its semantic family root. Key lookups return
/// ordinary handles, which can become stale after explicit semantic deletion.
/// Use `semantic_declaration` for a fallible authoritative read. Self-loop
/// geometry, endpoint following, and persistent topology edits are not supported.
pub struct Graph<K> {
    inner: RetainedGraph<K>,
}

impl<K: Clone + Eq + Hash> Graph<K> {
    pub fn new<V, E>(scene: &mut Scene, vertices: V, edges: E) -> Result<Self, GraphAuthoringError>
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
        Ok(Self { inner: build_graph(scene, vertices, edges, false, options)? })
    }
}

impl<K: Eq + Hash> Graph<K> {
    pub fn family(&self) -> &MobjectFamily {
        &self.inner.family
    }

    /// Borrow the authoritative topology in O(1), without allocation or cloning.
    /// Drop this read guard before mutating the same semantic store.
    pub fn topology(&self) -> Result<Ref<'_, GraphTopology>, GraphAuthoringError> {
        self.inner.topology()
    }

    /// Borrow the authoritative declaration in O(1). Stale roots and conflicting
    /// integration borrows return typed errors rather than panicking.
    pub fn semantic_declaration(&self) -> Result<Ref<'_, SemanticGraphDeclaration>, GraphAuthoringError> {
        self.inner.semantic_declaration()
    }

    pub fn vertex_id(&self, key: &K) -> Option<GraphVertexId> {
        self.inner.vertex_id(key)
    }

    pub fn vertex(&self, key: &K) -> Option<&Mobject> {
        self.inner.vertex(key)
    }

    /// Missing keys, an unavailable declaration, or a missing edge return None.
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

/// Explicit-position directed retained graph using shared Arrow geometry.
/// The same read/lifetime and unsupported-feature boundaries as Graph apply.
pub struct DiGraph<K> {
    inner: RetainedGraph<K>,
}

impl<K: Clone + Eq + Hash> DiGraph<K> {
    pub fn new<V, E>(scene: &mut Scene, vertices: V, edges: E) -> Result<Self, GraphAuthoringError>
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
        Ok(Self { inner: build_graph(scene, vertices, edges, true, options)? })
    }
}

impl<K: Eq + Hash> DiGraph<K> {
    pub fn family(&self) -> &MobjectFamily {
        &self.inner.family
    }

    /// Borrow the authoritative topology in O(1). Drop before mutation.
    pub fn topology(&self) -> Result<Ref<'_, GraphTopology>, GraphAuthoringError> {
        self.inner.topology()
    }

    /// Borrow the authoritative declaration without cloning or stale-root panic.
    pub fn semantic_declaration(&self) -> Result<Ref<'_, SemanticGraphDeclaration>, GraphAuthoringError> {
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
    pub fn graph<K, V, E>(&mut self, vertices: V, edges: E) -> Result<Graph<K>, GraphAuthoringError>
    where
        K: Clone + Eq + Hash,
        V: IntoIterator<Item = (K, (f64, f64))>,
        E: IntoIterator<Item = (K, K)>,
    {
        Graph::new(self, vertices, edges)
    }

    pub fn graph_with_options<K, V, E>(
        &mut self, vertices: V, edges: E, options: GraphOptions,
    ) -> Result<Graph<K>, GraphAuthoringError>
    where
        K: Clone + Eq + Hash,
        V: IntoIterator<Item = (K, (f64, f64))>,
        E: IntoIterator<Item = (K, K)>,
    {
        Graph::with_options(self, vertices, edges, options)
    }

    pub fn digraph<K, V, E>(&mut self, vertices: V, edges: E) -> Result<DiGraph<K>, GraphAuthoringError>
    where
        K: Clone + Eq + Hash,
        V: IntoIterator<Item = (K, (f64, f64))>,
        E: IntoIterator<Item = (K, K)>,
    {
        DiGraph::new(self, vertices, edges)
    }

    pub fn digraph_with_options<K, V, E>(
        &mut self, vertices: V, edges: E, options: GraphOptions,
    ) -> Result<DiGraph<K>, GraphAuthoringError>
    where
        K: Clone + Eq + Hash,
        V: IntoIterator<Item = (K, (f64, f64))>,
        E: IntoIterator<Item = (K, K)>,
    {
        DiGraph::with_options(self, vertices, edges, options)
    }
}
