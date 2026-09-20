use std::collections::HashMap;

/// Stable identity for one retained graph vertex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GraphVertexId(u64);

impl GraphVertexId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Stable identity for one retained graph edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GraphEdgeId(u64);

impl GraphEdgeId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One retained topology edge. Geometry/rendering is intentionally owned elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphEdge {
    pub id: GraphEdgeId,
    pub start: GraphVertexId,
    pub end: GraphVertexId,
    pub directed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphTopologyError {
    UnknownVertex(GraphVertexId),
    UnknownEdge(GraphEdgeId),
    DuplicateEdge {
        start: GraphVertexId,
        end: GraphVertexId,
        directed: bool,
    },
}

impl std::fmt::Display for GraphTopologyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownVertex(id) => write!(formatter, "unknown graph vertex {}", id.get()),
            Self::UnknownEdge(id) => write!(formatter, "unknown graph edge {}", id.get()),
            Self::DuplicateEdge {
                start,
                end,
                directed,
            } => write!(
                formatter,
                "duplicate {} graph edge {} -> {}",
                if *directed { "directed" } else { "undirected" },
                start.get(),
                end.get()
            ),
        }
    }
}

impl std::error::Error for GraphTopologyError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct EdgeKey {
    start: GraphVertexId,
    end: GraphVertexId,
    directed: bool,
}

impl EdgeKey {
    fn new(start: GraphVertexId, end: GraphVertexId, directed: bool) -> Self {
        if directed || start <= end {
            Self {
                start,
                end,
                directed,
            }
        } else {
            Self {
                start: end,
                end: start,
                directed,
            }
        }
    }
}

/// Renderer-independent retained graph topology.
///
/// Vertices and edges receive monotonic identities that are never renumbered after local
/// mutation. Incident-edge indexes make a moved vertex resolve exactly its dependency-local
/// edge set in O(degree) time without scanning the full graph. Insertion order is retained in
/// the public vertex/edge vectors for deterministic family/painter lowering by later layers.
#[derive(Clone, Debug, Default)]
pub struct RetainedGraphTopology {
    next_vertex_id: u64,
    next_edge_id: u64,
    vertices: Vec<GraphVertexId>,
    edges: Vec<GraphEdge>,
    vertex_positions: HashMap<GraphVertexId, usize>,
    edge_positions: HashMap<GraphEdgeId, usize>,
    edge_keys: HashMap<EdgeKey, GraphEdgeId>,
    incident_edges: HashMap<GraphVertexId, Vec<GraphEdgeId>>,
}

impl RetainedGraphTopology {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vertices(&self) -> &[GraphVertexId] {
        &self.vertices
    }

    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    pub fn add_vertex(&mut self) -> GraphVertexId {
        let id = GraphVertexId(self.next_vertex_id);
        self.next_vertex_id += 1;
        self.vertex_positions.insert(id, self.vertices.len());
        self.vertices.push(id);
        self.incident_edges.insert(id, Vec::new());
        id
    }

    pub fn contains_vertex(&self, id: GraphVertexId) -> bool {
        self.vertex_positions.contains_key(&id)
    }

    pub fn edge(&self, id: GraphEdgeId) -> Option<GraphEdge> {
        self.edge_positions.get(&id).map(|&index| self.edges[index])
    }

    pub fn add_edge(
        &mut self,
        start: GraphVertexId,
        end: GraphVertexId,
        directed: bool,
    ) -> Result<GraphEdgeId, GraphTopologyError> {
        self.require_vertex(start)?;
        self.require_vertex(end)?;

        let key = EdgeKey::new(start, end, directed);
        if self.edge_keys.contains_key(&key) {
            return Err(GraphTopologyError::DuplicateEdge {
                start,
                end,
                directed,
            });
        }

        let id = GraphEdgeId(self.next_edge_id);
        self.next_edge_id += 1;
        let edge = GraphEdge {
            id,
            start,
            end,
            directed,
        };
        self.edge_positions.insert(id, self.edges.len());
        self.edges.push(edge);
        self.edge_keys.insert(key, id);
        self.incident_edges.get_mut(&start).unwrap().push(id);
        if end != start {
            self.incident_edges.get_mut(&end).unwrap().push(id);
        }
        Ok(id)
    }

    pub fn incident_edges(
        &self,
        vertex: GraphVertexId,
    ) -> Result<&[GraphEdgeId], GraphTopologyError> {
        self.incident_edges
            .get(&vertex)
            .map(Vec::as_slice)
            .ok_or(GraphTopologyError::UnknownVertex(vertex))
    }

    pub fn remove_edge(&mut self, id: GraphEdgeId) -> Result<GraphEdge, GraphTopologyError> {
        let index = self
            .edge_positions
            .remove(&id)
            .ok_or(GraphTopologyError::UnknownEdge(id))?;
        let removed = self.edges.remove(index);
        self.edge_keys
            .remove(&EdgeKey::new(removed.start, removed.end, removed.directed));
        self.remove_incident_reference(removed.start, id);
        if removed.end != removed.start {
            self.remove_incident_reference(removed.end, id);
        }
        self.reindex_edges_from(index);
        Ok(removed)
    }

    pub fn remove_vertex(
        &mut self,
        id: GraphVertexId,
    ) -> Result<Vec<GraphEdge>, GraphTopologyError> {
        let index = *self
            .vertex_positions
            .get(&id)
            .ok_or(GraphTopologyError::UnknownVertex(id))?;
        let incident = self.incident_edges.get(&id).cloned().unwrap_or_default();
        let mut removed_edges = Vec::with_capacity(incident.len());
        for edge in incident {
            removed_edges.push(self.remove_edge(edge)?);
        }

        self.incident_edges.remove(&id);
        self.vertex_positions.remove(&id);
        self.vertices.remove(index);
        for (position, vertex) in self.vertices.iter().copied().enumerate().skip(index) {
            self.vertex_positions.insert(vertex, position);
        }
        Ok(removed_edges)
    }

    fn require_vertex(&self, id: GraphVertexId) -> Result<(), GraphTopologyError> {
        if self.contains_vertex(id) {
            Ok(())
        } else {
            Err(GraphTopologyError::UnknownVertex(id))
        }
    }

    fn remove_incident_reference(&mut self, vertex: GraphVertexId, edge: GraphEdgeId) {
        if let Some(edges) = self.incident_edges.get_mut(&vertex) {
            if let Some(index) = edges.iter().position(|candidate| *candidate == edge) {
                edges.remove(index);
            }
        }
    }

    fn reindex_edges_from(&mut self, start: usize) {
        for (index, edge) in self.edges.iter().copied().enumerate().skip(start) {
            self.edge_positions.insert(edge.id, index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_vertex_can_resolve_only_incident_edges() {
        let mut topology = RetainedGraphTopology::new();
        let a = topology.add_vertex();
        let b = topology.add_vertex();
        let c = topology.add_vertex();
        let d = topology.add_vertex();
        let ab = topology.add_edge(a, b, false).unwrap();
        let ac = topology.add_edge(a, c, false).unwrap();
        let cd = topology.add_edge(c, d, false).unwrap();

        assert_eq!(topology.incident_edges(a).unwrap(), &[ab, ac]);
        assert_eq!(topology.incident_edges(b).unwrap(), &[ab]);
        assert_eq!(topology.incident_edges(c).unwrap(), &[ac, cd]);
    }

    #[test]
    fn local_mutation_preserves_unrelated_vertex_and_edge_identity() {
        let mut topology = RetainedGraphTopology::new();
        let a = topology.add_vertex();
        let b = topology.add_vertex();
        let c = topology.add_vertex();
        let ab = topology.add_edge(a, b, false).unwrap();
        let bc = topology.add_edge(b, c, false).unwrap();

        let removed = topology.remove_vertex(a).unwrap();
        assert_eq!(
            removed.iter().map(|edge| edge.id).collect::<Vec<_>>(),
            vec![ab]
        );
        assert_eq!(topology.vertices(), &[b, c]);
        assert_eq!(
            topology.edges(),
            &[GraphEdge {
                id: bc,
                start: b,
                end: c,
                directed: false,
            }]
        );
        assert_eq!(topology.incident_edges(b).unwrap(), &[bc]);
        assert_eq!(topology.incident_edges(c).unwrap(), &[bc]);

        let d = topology.add_vertex();
        assert!(d.get() > c.get());
        let cd = topology.add_edge(c, d, false).unwrap();
        assert!(cd.get() > bc.get());
    }

    #[test]
    fn undirected_duplicate_is_endpoint_order_independent() {
        let mut topology = RetainedGraphTopology::new();
        let a = topology.add_vertex();
        let b = topology.add_vertex();
        topology.add_edge(a, b, false).unwrap();

        assert!(matches!(
            topology.add_edge(b, a, false),
            Err(GraphTopologyError::DuplicateEdge {
                directed: false,
                ..
            })
        ));
        assert!(topology.add_edge(b, a, true).is_ok());
    }

    #[test]
    fn self_edge_is_indexed_once() {
        let mut topology = RetainedGraphTopology::new();
        let a = topology.add_vertex();
        let aa = topology.add_edge(a, a, false).unwrap();
        assert_eq!(topology.incident_edges(a).unwrap(), &[aa]);
        topology.remove_edge(aa).unwrap();
        assert!(topology.incident_edges(a).unwrap().is_empty());
    }

    #[test]
    fn unknown_endpoints_fail_before_topology_changes() {
        let mut topology = RetainedGraphTopology::new();
        let a = topology.add_vertex();
        let unknown = GraphVertexId(99);
        assert_eq!(
            topology.add_edge(a, unknown, false),
            Err(GraphTopologyError::UnknownVertex(unknown))
        );
        assert!(topology.edges().is_empty());
        assert!(topology.incident_edges(a).unwrap().is_empty());
    }
}
