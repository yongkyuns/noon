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

/// One authored semantic binding for a stable graph edge identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticGraphEdgeBinding {
    id: GraphEdgeId,
    family: SemanticNodeId,
    line: SemanticNodeId,
}

impl SemanticGraphEdgeBinding {
    pub(crate) const fn from_resolved(
        id: GraphEdgeId,
        family: SemanticNodeId,
        line: SemanticNodeId,
    ) -> Self {
        Self { id, family, line }
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
                .flat_map(|edge| [edge.family(), edge.line()]),
        )
    }


}

/// Transaction-local semantic binding for one stable graph edge identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticTransactionGraphEdgeBinding {
    id: GraphEdgeId,
    family: SemanticTransactionNodeRef,
    line: SemanticTransactionNodeRef,
}

impl SemanticTransactionGraphEdgeBinding {
    pub const fn new(
        id: GraphEdgeId,
        family: SemanticTransactionNodeRef,
        line: SemanticTransactionNodeRef,
    ) -> Self {
        Self { id, family, line }
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
                .flat_map(|edge| [edge.family(), edge.line()]),
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
                SemanticGraphEdgeBinding::from_resolved(ab, id(10), id(11)),
                SemanticGraphEdgeBinding::from_resolved(bc, id(12), id(13)),
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
            )],
        );
        assert_eq!(graph.incident_edges(vertex).unwrap(), &[edge]);
    }
}
