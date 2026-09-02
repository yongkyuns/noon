use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Stable identity for one graph vertex.
///
/// IDs are allocated monotonically by a topology and are never reused after a
/// vertex is removed. This keeps references held by semantic-family lowering
/// from silently changing meaning after a local mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GraphVertexId(u64);

impl GraphVertexId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Stable identity for one graph edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GraphEdgeId(u64);

impl GraphEdgeId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One topology edge. Geometry and rendering remain owned by later semantic
/// lowering; this value only describes stable identity and endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
/// The public slices preserve insertion order for deterministic semantic-family
/// and painter-order lowering. Per-vertex incident indexes make dependency
/// lookup proportional to the selected vertex's degree rather than the total
/// graph size.
#[derive(Clone, Debug, Default)]
pub struct GraphTopology {
    next_vertex_id: u64,
    next_edge_id: u64,
    vertices: Vec<GraphVertexId>,
    edges: Vec<GraphEdge>,
    vertex_positions: HashMap<GraphVertexId, usize>,
    edge_positions: HashMap<GraphEdgeId, usize>,
    edge_keys: HashMap<EdgeKey, GraphEdgeId>,
    incident_edges: HashMap<GraphVertexId, Vec<GraphEdgeId>>,
}

/// The shared topology primitive used by the future `Graph` and `DiGraph`
/// semantic facades.
pub type RetainedGraphTopology = GraphTopology;

impl GraphTopology {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vertices(&self) -> &[GraphVertexId] {
        &self.vertices
    }

    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    pub fn contains_vertex(&self, id: GraphVertexId) -> bool {
        self.vertex_positions.contains_key(&id)
    }

    pub fn contains_edge(&self, id: GraphEdgeId) -> bool {
        self.edge_positions.contains_key(&id)
    }

    pub fn edge(&self, id: GraphEdgeId) -> Option<GraphEdge> {
        self.edge_positions.get(&id).map(|&index| self.edges[index])
    }

    pub fn add_vertex(&mut self) -> GraphVertexId {
        let id = GraphVertexId::new(self.next_vertex_id);
        self.next_vertex_id = self
            .next_vertex_id
            .checked_add(1)
            .expect("Noon graph vertex ID space exhausted");
        self.vertex_positions.insert(id, self.vertices.len());
        self.vertices.push(id);
        self.incident_edges.insert(id, Vec::new());
        id
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

        let id = GraphEdgeId::new(self.next_edge_id);
        self.next_edge_id = self
            .next_edge_id
            .checked_add(1)
            .expect("Noon graph edge ID space exhausted");
        let edge = GraphEdge {
            id,
            start,
            end,
            directed,
        };

        // All validation and fallible lookups happen before the first write.
        // The endpoint entries were validated above, so these inserts cannot
        // fail and preserve the operation's all-or-nothing contract.
        self.edge_positions.insert(id, self.edges.len());
        self.edges.push(edge);
        self.edge_keys.insert(key, id);
        self.incident_edges
            .get_mut(&start)
            .expect("validated graph endpoint exists")
            .push(id);
        if end != start {
            self.incident_edges
                .get_mut(&end)
                .expect("validated graph endpoint exists")
                .push(id);
        }
        Ok(id)
    }

    /// Return the edge IDs touching `vertex` in edge insertion order.
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
        let edge = self.preflight_edge_removal(id)?;
        let index = self
            .edge_positions
            .get(&id)
            .copied()
            .expect("preflighted graph edge remains indexed");

        self.edge_positions.remove(&id);
        self.edges.remove(index);
        self.edge_keys
            .remove(&EdgeKey::new(edge.start, edge.end, edge.directed));
        self.remove_incident_reference(edge.start, id);
        if edge.end != edge.start {
            self.remove_incident_reference(edge.end, id);
        }
        self.reindex_edges_from(index);
        Ok(edge)
    }

    /// Remove a vertex and all touching edges as one topology transaction.
    ///
    /// The incident edge IDs and every edge/index entry are preflighted before
    /// any state is changed. Thus an invalid or inconsistent request returns
    /// without partially removing the vertex or one of its edges.
    pub fn remove_vertex(
        &mut self,
        id: GraphVertexId,
    ) -> Result<Vec<GraphEdge>, GraphTopologyError> {
        let vertex_index = *self
            .vertex_positions
            .get(&id)
            .ok_or(GraphTopologyError::UnknownVertex(id))?;
        let incident = self
            .incident_edges
            .get(&id)
            .ok_or(GraphTopologyError::UnknownVertex(id))?
            .clone();

        let mut removed_edges = Vec::with_capacity(incident.len());
        for edge_id in &incident {
            let edge = self.preflight_edge_removal(*edge_id)?;
            if edge.start != id && edge.end != id {
                return Err(GraphTopologyError::UnknownEdge(*edge_id));
            }
            removed_edges.push(edge);
        }

        // The preflight above guarantees each remove_edge call succeeds, so a
        // failure here cannot be caused by caller input and no public mutable
        // access can create the inconsistent state. Commit in stable order.
        for edge_id in incident {
            self.remove_edge(edge_id)
                .expect("preflighted graph edge remains removable");
        }
        self.incident_edges.remove(&id);
        self.vertex_positions.remove(&id);
        self.vertices.remove(vertex_index);
        for (position, vertex) in self.vertices.iter().copied().enumerate().skip(vertex_index) {
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

    fn preflight_edge_removal(&self, id: GraphEdgeId) -> Result<GraphEdge, GraphTopologyError> {
        let index = *self
            .edge_positions
            .get(&id)
            .ok_or(GraphTopologyError::UnknownEdge(id))?;
        let edge = self.edges[index];
        if self
            .edge_keys
            .get(&EdgeKey::new(edge.start, edge.end, edge.directed))
            != Some(&id)
            || !self
                .incident_edges
                .get(&edge.start)
                .is_some_and(|edges| edges.contains(&id))
            || (edge.end != edge.start
                && !self
                    .incident_edges
                    .get(&edge.end)
                    .is_some_and(|edges| edges.contains(&id)))
        {
            return Err(GraphTopologyError::UnknownEdge(id));
        }
        Ok(edge)
    }

    fn remove_incident_reference(&mut self, vertex: GraphVertexId, edge: GraphEdgeId) {
        let edges = self
            .incident_edges
            .get_mut(&vertex)
            .expect("edge endpoint remains in topology");
        let index = edges
            .iter()
            .position(|candidate| *candidate == edge)
            .expect("preflighted incident edge remains indexed");
        edges.remove(index);
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
    fn undirected_incident_lookup_is_degree_local_and_self_edge_is_once() {
        let mut topology = GraphTopology::new();
        let a = topology.add_vertex();
        let b = topology.add_vertex();
        let c = topology.add_vertex();
        let ab = topology.add_edge(a, b, false).unwrap();
        let ac = topology.add_edge(a, c, false).unwrap();
        let cc = topology.add_edge(c, c, false).unwrap();

        assert_eq!(topology.incident_edges(a).unwrap(), &[ab, ac]);
        assert_eq!(topology.incident_edges(b).unwrap(), &[ab]);
        assert_eq!(topology.incident_edges(c).unwrap(), &[ac, cc]);
    }

    #[test]
    fn directed_orientation_is_significant() {
        let mut topology = GraphTopology::new();
        let a = topology.add_vertex();
        let b = topology.add_vertex();
        let ab = topology.add_edge(a, b, true).unwrap();
        let ba = topology.add_edge(b, a, true).unwrap();

        assert_ne!(ab, ba);
        assert_eq!(topology.edge(ab).unwrap().start, a);
        assert_eq!(topology.edge(ab).unwrap().end, b);
        assert_eq!(topology.edge(ba).unwrap().start, b);
        assert_eq!(topology.edge(ba).unwrap().end, a);
    }

    #[test]
    fn duplicate_semantics_are_endpoint_order_independent_only_for_undirected_edges() {
        let mut topology = GraphTopology::new();
        let a = topology.add_vertex();
        let b = topology.add_vertex();
        topology.add_edge(a, b, false).unwrap();

        assert_eq!(
            topology.add_edge(b, a, false),
            Err(GraphTopologyError::DuplicateEdge {
                start: b,
                end: a,
                directed: false,
            })
        );
        assert!(topology.add_edge(b, a, true).is_ok());
    }

    #[test]
    fn invalid_endpoint_is_transactional() {
        let mut topology = GraphTopology::new();
        let a = topology.add_vertex();
        let unknown = GraphVertexId::new(99);
        assert_eq!(
            topology.add_edge(a, unknown, false),
            Err(GraphTopologyError::UnknownVertex(unknown))
        );
        assert_eq!(topology.vertices(), &[a]);
        assert!(topology.edges().is_empty());
        assert!(topology.incident_edges(a).unwrap().is_empty());
    }

    #[test]
    fn vertex_removal_is_transactional_and_ids_remain_monotonic() {
        let mut topology = GraphTopology::new();
        let a = topology.add_vertex();
        let b = topology.add_vertex();
        let c = topology.add_vertex();
        let ab = topology.add_edge(a, b, false).unwrap();
        let bc = topology.add_edge(b, c, false).unwrap();

        let before = topology.clone();
        assert_eq!(
            topology.remove_vertex(GraphVertexId::new(99)),
            Err(GraphTopologyError::UnknownVertex(GraphVertexId::new(99)))
        );
        assert_eq!(topology.vertices(), before.vertices());
        assert_eq!(topology.edges(), before.edges());

        let expected_removed = topology.edge(ab).unwrap();
        assert_eq!(topology.remove_vertex(a).unwrap(), vec![expected_removed]);
        assert_eq!(
            topology.edges(),
            &[GraphEdge {
                id: bc,
                start: b,
                end: c,
                directed: false
            }]
        );
        let d = topology.add_vertex();
        let cd = topology.add_edge(c, d, false).unwrap();
        assert!(d.get() > c.get());
        assert!(cd.get() > bc.get());
    }

    #[test]
    fn randomized_mutations_preserve_edge_and_incident_indexes() {
        let mut topology = GraphTopology::new();
        let mut vertices = Vec::new();
        let mut state = 0x4d595df4d0f33173_u64;

        for _ in 0..400 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            match (state % 3, vertices.as_slice()) {
                (0, _) | (_, []) => vertices.push(topology.add_vertex()),
                (1, current) => {
                    let vertex = current[(state as usize / 3) % current.len()];
                    let _ = topology.remove_vertex(vertex);
                    vertices.retain(|candidate| *candidate != vertex);
                }
                (2, current) if current.len() >= 2 => {
                    let start = current[(state as usize / 5) % current.len()];
                    let end = current[(state as usize / 7) % current.len()];
                    let _ = topology.add_edge(start, end, state & 1 == 0);
                }
                _ => {}
            }

            for edge in topology.edges() {
                assert!(topology.contains_vertex(edge.start));
                assert!(topology.contains_vertex(edge.end));
                assert_eq!(topology.edge(edge.id), Some(*edge));
                assert!(topology
                    .incident_edges(edge.start)
                    .unwrap()
                    .contains(&edge.id));
                if edge.end != edge.start {
                    assert!(topology
                        .incident_edges(edge.end)
                        .unwrap()
                        .contains(&edge.id));
                }
            }
            for vertex in topology.vertices() {
                for edge_id in topology.incident_edges(*vertex).unwrap() {
                    let edge = topology.edge(*edge_id).unwrap();
                    assert!(edge.start == *vertex || edge.end == *vertex);
                }
            }
        }
    }
}
