//! Authored retained graph topology attached to one ordinary semantic family root.
//!
//! Graph semantics stay in the authoritative Semantic Scene. Vertices are ordinary
//! semantic objects and edges are ordinary semantic families with one explicit
//! analytic Line dependency component. Renderer/runtime layers continue to see the
//! same ordinary leaves; this declaration only records topology/dependency meaning.

use std::collections::{HashMap, HashSet};

use crate::{SemanticNodeId, SemanticTransactionNodeRef};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SemanticGraphEdgeKey {
    start: SemanticNodeId,
    end: SemanticNodeId,
    directed: bool,
}

impl SemanticGraphEdgeKey {
    fn new(start: SemanticNodeId, end: SemanticNodeId, directed: bool) -> Self {
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

/// One authored graph edge over ordinary semantic identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticGraphEdge {
    family: SemanticNodeId,
    line: SemanticNodeId,
    start: SemanticNodeId,
    end: SemanticNodeId,
    directed: bool,
}

impl SemanticGraphEdge {
    pub(crate) const fn from_resolved(
        family: SemanticNodeId,
        line: SemanticNodeId,
        start: SemanticNodeId,
        end: SemanticNodeId,
        directed: bool,
    ) -> Self {
        Self {
            family,
            line,
            start,
            end,
            directed,
        }
    }

    pub const fn family(self) -> SemanticNodeId {
        self.family
    }

    pub const fn line(self) -> SemanticNodeId {
        self.line
    }

    pub const fn start(self) -> SemanticNodeId {
        self.start
    }

    pub const fn end(self) -> SemanticNodeId {
        self.end
    }

    pub const fn directed(self) -> bool {
        self.directed
    }
}

/// Authoritative graph declaration stored on one semantic family root.
///
/// Ordered vertex/edge vectors retain authored order. Hash indexes are derived
/// storage inside the same declaration, not a second identity model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticGraphDeclaration {
    vertices: Vec<SemanticNodeId>,
    edges: Vec<SemanticGraphEdge>,
    vertex_set: HashSet<SemanticNodeId>,
    edge_positions: HashMap<SemanticNodeId, usize>,
    edge_keys: HashMap<SemanticGraphEdgeKey, SemanticNodeId>,
    adjacency: HashMap<SemanticNodeId, Vec<SemanticNodeId>>,
}

impl SemanticGraphDeclaration {
    pub(crate) fn from_resolved(
        vertices: Vec<SemanticNodeId>,
        edges: Vec<SemanticGraphEdge>,
    ) -> Self {
        let vertex_set = vertices.iter().copied().collect::<HashSet<_>>();
        debug_assert_eq!(vertex_set.len(), vertices.len());

        let mut edge_positions = HashMap::with_capacity(edges.len());
        let mut edge_keys = HashMap::with_capacity(edges.len());
        let mut adjacency = vertices
            .iter()
            .copied()
            .map(|vertex| (vertex, Vec::new()))
            .collect::<HashMap<_, _>>();

        for (index, edge) in edges.iter().copied().enumerate() {
            debug_assert!(vertex_set.contains(&edge.start));
            debug_assert!(vertex_set.contains(&edge.end));
            debug_assert!(edge_positions.insert(edge.family, index).is_none());
            debug_assert!(edge_keys
                .insert(
                    SemanticGraphEdgeKey::new(edge.start, edge.end, edge.directed),
                    edge.family,
                )
                .is_none());
            adjacency
                .get_mut(&edge.start)
                .expect("validated graph start vertex")
                .push(edge.family);
            if edge.end != edge.start {
                adjacency
                    .get_mut(&edge.end)
                    .expect("validated graph end vertex")
                    .push(edge.family);
            }
        }

        Self {
            vertices,
            edges,
            vertex_set,
            edge_positions,
            edge_keys,
            adjacency,
        }
    }

    pub fn vertices(&self) -> &[SemanticNodeId] {
        &self.vertices
    }

    pub fn edges(&self) -> &[SemanticGraphEdge] {
        &self.edges
    }

    pub fn contains_vertex(&self, vertex: SemanticNodeId) -> bool {
        self.vertex_set.contains(&vertex)
    }

    pub fn edge(&self, family: SemanticNodeId) -> Option<SemanticGraphEdge> {
        self.edge_positions
            .get(&family)
            .map(|&index| self.edges[index])
    }

    /// Resolve one authored edge by endpoint semantic identity in O(1).
    pub fn edge_between(
        &self,
        start: SemanticNodeId,
        end: SemanticNodeId,
        directed: bool,
    ) -> Option<SemanticNodeId> {
        self.edge_keys
            .get(&SemanticGraphEdgeKey::new(start, end, directed))
            .copied()
    }

    /// Resolve exactly the edge-family identities incident on one vertex.
    ///
    /// Complexity is O(degree); a self-edge appears once.
    pub fn incident_edges(&self, vertex: SemanticNodeId) -> Option<&[SemanticNodeId]> {
        self.adjacency.get(&vertex).map(Vec::as_slice)
    }

    pub(crate) fn referenced_nodes(&self) -> impl Iterator<Item = SemanticNodeId> + '_ {
        self.vertices.iter().copied().chain(self.edges.iter().flat_map(|edge| {
            [edge.family, edge.line, edge.start, edge.end]
        }))
    }
}

/// Transaction-local graph edge declaration. Every reference may name an
/// existing semantic identity or a node created by the same transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticTransactionGraphEdge {
    family: SemanticTransactionNodeRef,
    line: SemanticTransactionNodeRef,
    start: SemanticTransactionNodeRef,
    end: SemanticTransactionNodeRef,
    directed: bool,
}

impl SemanticTransactionGraphEdge {
    pub const fn new(
        family: SemanticTransactionNodeRef,
        line: SemanticTransactionNodeRef,
        start: SemanticTransactionNodeRef,
        end: SemanticTransactionNodeRef,
        directed: bool,
    ) -> Self {
        Self {
            family,
            line,
            start,
            end,
            directed,
        }
    }

    pub const fn family(self) -> SemanticTransactionNodeRef {
        self.family
    }

    pub const fn line(self) -> SemanticTransactionNodeRef {
        self.line
    }

    pub const fn start(self) -> SemanticTransactionNodeRef {
        self.start
    }

    pub const fn end(self) -> SemanticTransactionNodeRef {
        self.end
    }

    pub const fn directed(self) -> bool {
        self.directed
    }
}

/// Transaction-local complete graph declaration used for initial atomic
/// construction. Later persistent graph edits use local graph mutations rather
/// than replacing this entire declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticTransactionGraphDeclaration {
    vertices: Vec<SemanticTransactionNodeRef>,
    edges: Vec<SemanticTransactionGraphEdge>,
}

impl SemanticTransactionGraphDeclaration {
    pub fn new(
        vertices: impl IntoIterator<Item = impl Into<SemanticTransactionNodeRef>>,
        edges: impl IntoIterator<Item = SemanticTransactionGraphEdge>,
    ) -> Self {
        Self {
            vertices: vertices.into_iter().map(Into::into).collect(),
            edges: edges.into_iter().collect(),
        }
    }

    pub fn vertices(&self) -> &[SemanticTransactionNodeRef] {
        &self.vertices
    }

    pub fn edges(&self) -> &[SemanticTransactionGraphEdge] {
        &self.edges
    }

    pub(crate) fn node_references(
        &self,
    ) -> impl Iterator<Item = SemanticTransactionNodeRef> + '_ {
        self.vertices.iter().copied().chain(self.edges.iter().flat_map(|edge| {
            [edge.family, edge.line, edge.start, edge.end]
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(slot: u32) -> SemanticNodeId {
        SemanticNodeId::new(slot, 0)
    }

    #[test]
    fn resolved_graph_preserves_order_normalized_lookup_and_incident_locality() {
        let vertices = vec![id(1), id(2), id(3)];
        let edges = vec![
            SemanticGraphEdge {
                family: id(10),
                line: id(11),
                start: id(1),
                end: id(2),
                directed: false,
            },
            SemanticGraphEdge {
                family: id(12),
                line: id(13),
                start: id(2),
                end: id(3),
                directed: true,
            },
        ];
        let graph = SemanticGraphDeclaration::from_resolved(vertices.clone(), edges.clone());

        assert_eq!(graph.vertices(), vertices);
        assert_eq!(graph.edges(), edges);
        assert_eq!(graph.edge_between(id(2), id(1), false), Some(id(10)));
        assert_eq!(graph.edge_between(id(3), id(2), true), None);
        assert_eq!(graph.incident_edges(id(2)).unwrap(), &[id(10), id(12)]);
    }

    #[test]
    fn self_edge_is_indexed_once() {
        let graph = SemanticGraphDeclaration::from_resolved(
            vec![id(1)],
            vec![SemanticGraphEdge {
                family: id(10),
                line: id(11),
                start: id(1),
                end: id(1),
                directed: false,
            }],
        );
        assert_eq!(graph.incident_edges(id(1)).unwrap(), &[id(10)]);
    }
}
