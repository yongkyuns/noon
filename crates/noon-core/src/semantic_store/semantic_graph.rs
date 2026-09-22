//! Authored graph topology on ordinary semantic family roots.
//!
//! Only graph declarations allocate topology/binding payloads. The public
//! declaration values are pointer-sized so their optional presence does not
//! embed graph maps in every semantic node or inflate every mutation value.

use std::collections::HashMap;

use crate::{
    GraphEdge, GraphEdgeId, GraphTopology, GraphVertexId, SemanticNodeId,
    SemanticTransactionNodeRef,
};

/// Geometry-independent policy required to reconstruct one Graph-owned Arrow
/// from effective vertex centers.
///
/// The values are authored semantics rather than compiler constants. Graph
/// lowering can therefore recompute buff shortening and tip sizing without
/// depending on frontend defaults or reverse-engineering an already-capped tip.
#[derive(Clone, Copy, Debug)]
pub struct SemanticGraphArrowPolicy {
    buff: f64,
    tip_length: f64,
    max_tip_length_to_length_ratio: f64,
}

impl SemanticGraphArrowPolicy {
    pub const fn new(buff: f64, tip_length: f64, max_tip_length_to_length_ratio: f64) -> Self {
        Self {
            buff,
            tip_length,
            max_tip_length_to_length_ratio,
        }
    }

    pub const fn buff(self) -> f64 {
        self.buff
    }

    pub const fn tip_length(self) -> f64 {
        self.tip_length
    }

    pub const fn max_tip_length_to_length_ratio(self) -> f64 {
        self.max_tip_length_to_length_ratio
    }

    pub fn is_valid(self) -> bool {
        self.buff.is_finite()
            && self.buff >= 0.0
            && self.tip_length.is_finite()
            && self.tip_length >= 0.0
            && self.max_tip_length_to_length_ratio.is_finite()
            && self.max_tip_length_to_length_ratio >= 0.0
    }

    fn canonical_bits(value: f64) -> u64 {
        if value == 0.0 {
            0
        } else {
            value.to_bits()
        }
    }
}

impl PartialEq for SemanticGraphArrowPolicy {
    fn eq(&self, other: &Self) -> bool {
        Self::canonical_bits(self.buff) == Self::canonical_bits(other.buff)
            && Self::canonical_bits(self.tip_length) == Self::canonical_bits(other.tip_length)
            && Self::canonical_bits(self.max_tip_length_to_length_ratio)
                == Self::canonical_bits(other.max_tip_length_to_length_ratio)
    }
}

impl Eq for SemanticGraphArrowPolicy {}

impl std::hash::Hash for SemanticGraphArrowPolicy {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Self::canonical_bits(self.buff).hash(state);
        Self::canonical_bits(self.tip_length).hash(state);
        Self::canonical_bits(self.max_tip_length_to_length_ratio).hash(state);
    }
}

/// Effective endpoint dependency carried by one Graph edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticGraphEdgeDependency {
    Line,
    Arrow {
        end_tip: SemanticNodeId,
        start_tip: Option<SemanticNodeId>,
        policy: SemanticGraphArrowPolicy,
    },
}

impl SemanticGraphEdgeDependency {
    fn referenced_nodes(self) -> [Option<SemanticNodeId>; 2] {
        match self {
            Self::Line => [None, None],
            Self::Arrow {
                end_tip, start_tip, ..
            } => [Some(end_tip), start_tip],
        }
    }
}

/// One authored semantic binding for a stable graph edge identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticGraphEdgeBinding {
    id: GraphEdgeId,
    family: SemanticNodeId,
    line: SemanticNodeId,
    dependency: SemanticGraphEdgeDependency,
}

impl SemanticGraphEdgeBinding {
    pub(crate) const fn from_resolved(
        id: GraphEdgeId,
        family: SemanticNodeId,
        line: SemanticNodeId,
        dependency: SemanticGraphEdgeDependency,
    ) -> Self {
        Self {
            id,
            family,
            line,
            dependency,
        }
    }

    pub const fn id(self) -> GraphEdgeId {
        self.id
    }

    pub const fn family(self) -> SemanticNodeId {
        self.family
    }

    pub const fn line(self) -> SemanticNodeId {
        self.line
    }

    pub const fn dependency(self) -> SemanticGraphEdgeDependency {
        self.dependency
    }

    fn referenced_nodes(self) -> [Option<SemanticNodeId>; 4] {
        let dependency = self.dependency.referenced_nodes();
        [
            Some(self.family),
            Some(self.line),
            dependency[0],
            dependency[1],
        ]
    }
}

/// Authoritative Graph/DiGraph declaration on one semantic family root.
///
/// Stable graph IDs and adjacency remain in GraphTopology; bindings attach them
/// to ordinary semantic identities. Boxing is storage layout, not another owner
/// or authority. Cloning explicitly creates an independent read snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct SemanticGraphDeclaration {
    data: Box<GraphDeclarationData>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GraphDeclarationData {
    topology: GraphTopology,
    vertices: HashMap<GraphVertexId, SemanticNodeId>,
    edges: HashMap<GraphEdgeId, SemanticGraphEdgeBinding>,
    edge_lines: HashMap<SemanticNodeId, GraphEdgeId>,
}

impl SemanticGraphDeclaration {
    pub(crate) fn from_resolved(
        topology: GraphTopology,
        vertices: Vec<(GraphVertexId, SemanticNodeId)>,
        edges: Vec<SemanticGraphEdgeBinding>,
    ) -> Self {
        let vertices = vertices.into_iter().collect::<HashMap<_, _>>();
        let edges = edges
            .into_iter()
            .map(|edge| (edge.id(), edge))
            .collect::<HashMap<_, _>>();
        let edge_lines = edges
            .values()
            .map(|binding| (binding.line(), binding.id()))
            .collect::<HashMap<_, _>>();
        debug_assert_eq!(edge_lines.len(), edges.len());
        debug_assert_eq!(vertices.len(), topology.vertices().count());
        debug_assert_eq!(edges.len(), topology.edges().count());
        debug_assert!(topology
            .vertices()
            .all(|vertex| vertices.contains_key(&vertex)));
        debug_assert!(topology.edges().all(|edge| edges.contains_key(&edge.id)));
        Self {
            data: Box::new(GraphDeclarationData {
                topology,
                vertices,
                edges,
                edge_lines,
            }),
        }
    }

    pub fn topology(&self) -> &GraphTopology {
        &self.data.topology
    }

    pub fn vertex_node(&self, vertex: GraphVertexId) -> Option<SemanticNodeId> {
        self.data.vertices.get(&vertex).copied()
    }

    pub fn edge_binding(&self, edge: GraphEdgeId) -> Option<SemanticGraphEdgeBinding> {
        self.data.edges.get(&edge).copied()
    }

    /// Resolve the stable graph edge whose designated dependency Line is `node`.
    /// This reverse index keeps local content validation O(1).
    pub(crate) fn edge_for_line_node(&self, node: SemanticNodeId) -> Option<GraphEdgeId> {
        self.data.edge_lines.get(&node).copied()
    }

    /// Iterate semantic vertex bindings in topology insertion order.
    pub fn vertices(&self) -> impl Iterator<Item = (GraphVertexId, SemanticNodeId)> + '_ {
        self.data
            .topology
            .vertices()
            .map(|vertex| (vertex, self.data.vertices[&vertex]))
    }

    /// Iterate semantic edge bindings in topology insertion order.
    pub fn edges(&self) -> impl Iterator<Item = (GraphEdge, SemanticGraphEdgeBinding)> + '_ {
        self.data
            .topology
            .edges()
            .map(|edge| (edge, self.data.edges[&edge.id]))
    }

    pub fn edge_between(
        &self,
        start: GraphVertexId,
        end: GraphVertexId,
        directed: bool,
    ) -> Option<GraphEdgeId> {
        self.data.topology.edge_between(start, end, directed)
    }

    /// Borrow the incident-edge index; enumeration is O(degree).
    pub fn incident_edges(
        &self,
        vertex: GraphVertexId,
    ) -> Result<&[GraphEdgeId], crate::GraphTopologyError> {
        self.data.topology.incident_edges(vertex)
    }

    /// Resolve the semantic edge-family identities incident on one vertex.
    pub fn incident_edge_nodes(
        &self,
        vertex: GraphVertexId,
    ) -> Result<Vec<(GraphEdgeId, SemanticNodeId)>, crate::GraphTopologyError> {
        Ok(self
            .data
            .topology
            .incident_edges(vertex)?
            .iter()
            .map(|&edge| (edge, self.data.edges[&edge].family()))
            .collect())
    }

    pub(crate) fn referenced_nodes(&self) -> impl Iterator<Item = SemanticNodeId> + '_ {
        self.data.vertices.values().copied().chain(
            self.data
                .edges
                .values()
                .copied()
                .flat_map(SemanticGraphEdgeBinding::referenced_nodes)
                .flatten(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticTransactionGraphEdgeDependency {
    Line,
    Arrow {
        end_tip: SemanticTransactionNodeRef,
        start_tip: Option<SemanticTransactionNodeRef>,
        policy: SemanticGraphArrowPolicy,
    },
}

impl SemanticTransactionGraphEdgeDependency {
    fn node_references(self) -> [Option<SemanticTransactionNodeRef>; 2] {
        match self {
            Self::Line => [None, None],
            Self::Arrow {
                end_tip, start_tip, ..
            } => [Some(end_tip), start_tip],
        }
    }
}

/// Transaction-local semantic binding for one stable graph edge identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticTransactionGraphEdgeBinding {
    id: GraphEdgeId,
    family: SemanticTransactionNodeRef,
    line: SemanticTransactionNodeRef,
    dependency: SemanticTransactionGraphEdgeDependency,
}

impl SemanticTransactionGraphEdgeBinding {
    /// Construct an undirected/plain Line edge binding.
    pub const fn new(
        id: GraphEdgeId,
        family: SemanticTransactionNodeRef,
        line: SemanticTransactionNodeRef,
    ) -> Self {
        Self {
            id,
            family,
            line,
            dependency: SemanticTransactionGraphEdgeDependency::Line,
        }
    }

    pub const fn new_arrow(
        id: GraphEdgeId,
        family: SemanticTransactionNodeRef,
        line: SemanticTransactionNodeRef,
        end_tip: SemanticTransactionNodeRef,
        start_tip: Option<SemanticTransactionNodeRef>,
        policy: SemanticGraphArrowPolicy,
    ) -> Self {
        Self {
            id,
            family,
            line,
            dependency: SemanticTransactionGraphEdgeDependency::Arrow {
                end_tip,
                start_tip,
                policy,
            },
        }
    }

    pub const fn id(self) -> GraphEdgeId {
        self.id
    }

    pub const fn family(self) -> SemanticTransactionNodeRef {
        self.family
    }

    pub const fn line(self) -> SemanticTransactionNodeRef {
        self.line
    }

    pub const fn dependency(self) -> SemanticTransactionGraphEdgeDependency {
        self.dependency
    }

    fn node_references(self) -> [Option<SemanticTransactionNodeRef>; 4] {
        let dependency = self.dependency.node_references();
        [
            Some(self.family),
            Some(self.line),
            dependency[0],
            dependency[1],
        ]
    }
}

/// Initial graph declaration staged in the shared semantic transaction.
/// Its graph-sized payload is allocated only for graph construction.
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct SemanticTransactionGraphDeclaration {
    data: Box<TransactionGraphData>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TransactionGraphData {
    topology: GraphTopology,
    vertices: Vec<(GraphVertexId, SemanticTransactionNodeRef)>,
    edges: Vec<SemanticTransactionGraphEdgeBinding>,
}

impl SemanticTransactionGraphDeclaration {
    pub fn new<V, I, E>(topology: GraphTopology, vertices: I, edges: E) -> Self
    where
        V: Into<SemanticTransactionNodeRef>,
        I: IntoIterator<Item = (GraphVertexId, V)>,
        E: IntoIterator<Item = SemanticTransactionGraphEdgeBinding>,
    {
        Self {
            data: Box::new(TransactionGraphData {
                topology,
                vertices: vertices
                    .into_iter()
                    .map(|(id, node)| (id, node.into()))
                    .collect(),
                edges: edges.into_iter().collect(),
            }),
        }
    }

    pub fn topology(&self) -> &GraphTopology {
        &self.data.topology
    }

    pub fn vertices(&self) -> &[(GraphVertexId, SemanticTransactionNodeRef)] {
        &self.data.vertices
    }

    pub fn edges(&self) -> &[SemanticTransactionGraphEdgeBinding] {
        &self.data.edges
    }

    pub(crate) fn node_references(&self) -> impl Iterator<Item = SemanticTransactionNodeRef> + '_ {
        self.data.vertices.iter().map(|(_, node)| *node).chain(
            self.data
                .edges
                .iter()
                .copied()
                .flat_map(SemanticTransactionGraphEdgeBinding::node_references)
                .flatten(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(slot: u32) -> SemanticNodeId {
        SemanticNodeId::new(slot, 0)
    }

    #[test]
    fn optional_graph_metadata_and_transaction_payload_are_pointer_sized() {
        use std::mem::size_of;
        assert_eq!(size_of::<SemanticGraphDeclaration>(), size_of::<usize>());
        assert_eq!(
            size_of::<Option<SemanticGraphDeclaration>>(),
            size_of::<usize>()
        );
        assert_eq!(
            size_of::<SemanticTransactionGraphDeclaration>(),
            size_of::<usize>()
        );
    }

    #[test]
    fn resolved_graph_preserves_stable_ids_normalized_lookup_and_incident_locality() {
        let mut topology = GraphTopology::new();
        let a = topology.add_vertex();
        let b = topology.add_vertex();
        let c = topology.add_vertex();
        let ab = topology.add_edge(a, b, false).unwrap();
        let bc = topology.add_edge(b, c, true).unwrap();
        let graph = SemanticGraphDeclaration::from_resolved(
            topology,
            vec![(a, id(1)), (b, id(2)), (c, id(3))],
            vec![
                SemanticGraphEdgeBinding::from_resolved(
                    ab,
                    id(10),
                    id(11),
                    SemanticGraphEdgeDependency::Line,
                ),
                SemanticGraphEdgeBinding::from_resolved(
                    bc,
                    id(12),
                    id(13),
                    SemanticGraphEdgeDependency::Line,
                ),
            ],
        );
        assert_eq!(graph.vertex_node(a), Some(id(1)));
        assert_eq!(graph.edge_between(b, a, false), Some(ab));
        assert_eq!(graph.edge_between(c, b, true), None);
        assert_eq!(graph.incident_edges(b).unwrap(), &[ab, bc]);
        assert_eq!(
            graph.incident_edge_nodes(b).unwrap(),
            vec![(ab, id(10)), (bc, id(12))]
        );
    }

    #[test]
    fn arrow_policy_preserves_constructor_values_and_component_references() {
        let policy = SemanticGraphArrowPolicy::new(0.25, 0.35, 0.25);
        assert!(policy.is_valid());
        assert_eq!(policy.buff(), 0.25);
        assert_eq!(policy.tip_length(), 0.35);
        assert_eq!(policy.max_tip_length_to_length_ratio(), 0.25);

        let dependency = SemanticGraphEdgeDependency::Arrow {
            end_tip: id(20),
            start_tip: Some(id(21)),
            policy,
        };
        assert_eq!(dependency.referenced_nodes(), [Some(id(20)), Some(id(21))]);
    }

    #[test]
    fn self_edge_is_indexed_once() {
        let mut topology = GraphTopology::new();
        let vertex = topology.add_vertex();
        let edge = topology.add_edge(vertex, vertex, false).unwrap();
        let graph = SemanticGraphDeclaration::from_resolved(
            topology,
            vec![(vertex, id(1))],
            vec![SemanticGraphEdgeBinding::from_resolved(
                edge,
                id(10),
                id(11),
                SemanticGraphEdgeDependency::Line,
            )],
        );
        assert_eq!(graph.incident_edges(vertex).unwrap(), &[edge]);
    }
}
