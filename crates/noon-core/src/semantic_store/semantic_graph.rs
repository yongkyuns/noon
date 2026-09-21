//! Authored retained graph topology attached to one ordinary semantic family root.
//!
//! Graph semantics stay in the authoritative Semantic Scene. Stable graph IDs and
//! adjacency use the shared renderer-independent `GraphTopology`; vertices are
//! ordinary semantic objects and edges are ordinary semantic families with one
//! explicit analytic Line dependency component. Renderer/runtime layers continue
//! to see the same ordinary leaves.

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

/// Authoritative Graph/DiGraph declaration stored on one semantic family root.
///
/// Stable graph IDs, insertion order, directedness, adjacency and endpoint keys
/// remain in `GraphTopology`. The two binding maps attach those identities to
/// ordinary semantic objects/families. Hash indexes are derived storage inside
/// this one declaration, not a second graph model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticGraphDeclaration {
    topology: GraphTopology,
    vertices: HashMap<GraphVertexId, SemanticNodeId>,
    edges: HashMap<GraphEdgeId, SemanticGraphEdgeBinding>,
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
        debug_assert_eq!(vertices.len(), topology.vertices().count());
        debug_assert_eq!(edges.len(), topology.edges().count());
        debug_assert!(topology
            .vertices()
            .all(|vertex| vertices.contains_key(&vertex)));
        debug_assert!(topology.edges().all(|edge| edges.contains_key(&edge.id)));
        Self {
            topology,
            vertices,
            edges,
        }
    }

    pub fn topology(&self) -> &GraphTopology {
        &self.topology
    }

    pub fn vertex_node(&self, vertex: GraphVertexId) -> Option<SemanticNodeId> {
        self.vertices.get(&vertex).copied()
    }

    pub fn edge_binding(&self, edge: GraphEdgeId) -> Option<SemanticGraphEdgeBinding> {
        self.edges.get(&edge).copied()
    }

    /// Iterate semantic vertex bindings in stable topology insertion order.
    pub fn vertices(
        &self,
    ) -> impl Iterator<Item = (GraphVertexId, SemanticNodeId)> + '_ {
        self.topology
            .vertices()
            .map(|vertex| (vertex, self.vertices[&vertex]))
    }

    /// Iterate semantic edge bindings in stable topology insertion order.
    pub fn edges(
        &self,
    ) -> impl Iterator<Item = (GraphEdge, SemanticGraphEdgeBinding)> + '_ {
        self.topology.edges().map(|edge| (edge, self.edges[&edge.id]))
    }

    /// Resolve one stable edge identity by endpoint graph identity in O(1).
    pub fn edge_between(
        &self,
        start: GraphVertexId,
        end: GraphVertexId,
        directed: bool,
    ) -> Option<GraphEdgeId> {
        self.topology.edge_between(start, end, directed)
    }

    /// Resolve exactly the stable edge IDs incident on one vertex.
    ///
    /// Complexity is O(degree); a self-edge appears once.
    pub fn incident_edges(
        &self,
        vertex: GraphVertexId,
    ) -> Result<&[GraphEdgeId], crate::GraphTopologyError> {
        self.topology.incident_edges(vertex)
    }

    /// Resolve exactly the semantic edge-family identities incident on one vertex.
    ///
    /// Complexity is O(degree), with one binding lookup per touching edge.
    pub fn incident_edge_nodes(
        &self,
        vertex: GraphVertexId,
    ) -> Result<Vec<(GraphEdgeId, SemanticNodeId)>, crate::GraphTopologyError> {
        Ok(self
            .topology
            .incident_edges(vertex)?
            .iter()
            .map(|&edge| (edge, self.edges[&edge].family()))
            .collect())
    }

    pub(crate) fn referenced_nodes(&self) -> impl Iterator<Item = SemanticNodeId> + '_ {
        self.vertices
            .values()
            .copied()
            .chain(self.edges.values().flat_map(|edge| [edge.family(), edge.line()]))
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

/// Transaction-local complete graph declaration used for initial atomic
/// construction. Later persistent graph edits use local graph mutations rather
/// than replacing this entire declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticTransactionGraphDeclaration {
    topology: GraphTopology,
    vertices: Vec<(GraphVertexId, SemanticTransactionNodeRef)>,
    edges: Vec<SemanticTransactionGraphEdgeBinding>,
}

impl SemanticTransactionGraphDeclaration {
    pub fn new(
        topology: GraphTopology,
        vertices: impl IntoIterator<
            Item = (GraphVertexId, impl Into<SemanticTransactionNodeRef>),
        >,
        edges: impl IntoIterator<Item = SemanticTransactionGraphEdgeBinding>,
    ) -> Self {
        Self {
            topology,
            vertices: vertices
                .into_iter()
                .map(|(id, node)| (id, node.into()))
                .collect(),
            edges: edges.into_iter().collect(),
        }
    }

    pub fn topology(&self) -> &GraphTopology {
        &self.topology
    }

    pub fn vertices(&self) -> &[(GraphVertexId, SemanticTransactionNodeRef)] {
        &self.vertices
    }

    pub fn edges(&self) -> &[SemanticTransactionGraphEdgeBinding] {
        &self.edges
    }

    pub(crate) fn node_references(
        &self,
    ) -> impl Iterator<Item = SemanticTransactionNodeRef> + '_ {
        self.vertices
            .iter()
            .map(|(_, node)| *node)
            .chain(self.edges.iter().flat_map(|edge| [edge.family(), edge.line()]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(slot: u32) -> SemanticNodeId {
        SemanticNodeId::new(slot, 0)
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
            vec![SemanticGraphEdgeBinding::from_resolved(edge, id(10), id(11))],
        );
        assert_eq!(graph.incident_edges(vertex).unwrap(), &[edge]);
    }
}
