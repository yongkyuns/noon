use crate::{GraphEdgeId, GraphTopology, GraphTopologyError, GraphVertexId, Mobject, MobjectFamily};
use noon_core::SemanticNodeId;
use std::collections::HashMap;

/// Stable semantic bindings for one graph topology.
///
/// Topology owns graph identity and adjacency. This table only associates those
/// identities with ordinary semantic objects/families; it is not a second scene
/// model and carries no geometry, layout, or runtime state.
#[derive(Clone, Debug, Default)]
pub struct GraphSemanticBindings {
    vertices: HashMap<GraphVertexId, SemanticNodeId>,
    edges: HashMap<GraphEdgeId, SemanticNodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphBindingError {
    Topology(GraphTopologyError),
    VertexAlreadyBound(GraphVertexId),
    EdgeAlreadyBound(GraphEdgeId),
    VertexNotBound(GraphVertexId),
    EdgeNotBound(GraphEdgeId),
    SemanticIdentityAlreadyBound(SemanticNodeId),
}

impl From<GraphTopologyError> for GraphBindingError {
    fn from(value: GraphTopologyError) -> Self {
        Self::Topology(value)
    }
}

impl std::fmt::Display for GraphBindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Topology(error) => error.fmt(formatter),
            Self::VertexAlreadyBound(id) => write!(formatter, "graph vertex {} is already bound", id.get()),
            Self::EdgeAlreadyBound(id) => write!(formatter, "graph edge {} is already bound", id.get()),
            Self::VertexNotBound(id) => write!(formatter, "graph vertex {} is not bound", id.get()),
            Self::EdgeNotBound(id) => write!(formatter, "graph edge {} is not bound", id.get()),
            Self::SemanticIdentityAlreadyBound(id) => {
                write!(formatter, "semantic identity {id:?} is already bound in this graph")
            }
        }
    }
}

impl std::error::Error for GraphBindingError {}

impl GraphSemanticBindings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vertex_node(&self, id: GraphVertexId) -> Option<SemanticNodeId> {
        self.vertices.get(&id).copied()
    }

    pub fn edge_node(&self, id: GraphEdgeId) -> Option<SemanticNodeId> {
        self.edges.get(&id).copied()
    }

    pub fn bind_vertex(
        &mut self,
        topology: &GraphTopology,
        id: GraphVertexId,
        object: &Mobject,
    ) -> Result<(), GraphBindingError> {
        if !topology.contains_vertex(id) {
            return Err(GraphTopologyError::UnknownVertex(id).into());
        }
        if self.vertices.contains_key(&id) {
            return Err(GraphBindingError::VertexAlreadyBound(id));
        }
        self.require_unbound_semantic_identity(object.node_id())?;
        self.vertices.insert(id, object.node_id());
        Ok(())
    }

    pub fn bind_edge(
        &mut self,
        topology: &GraphTopology,
        id: GraphEdgeId,
        family: &MobjectFamily,
    ) -> Result<(), GraphBindingError> {
        if topology.edge(id).is_none() {
            return Err(GraphTopologyError::UnknownEdge(id).into());
        }
        if self.edges.contains_key(&id) {
            return Err(GraphBindingError::EdgeAlreadyBound(id));
        }
        self.require_unbound_semantic_identity(family.node_id())?;
        self.edges.insert(id, family.node_id());
        Ok(())
    }

    pub fn remove_vertex(
        &mut self,
        topology: &GraphTopology,
        id: GraphVertexId,
    ) -> Result<SemanticNodeId, GraphBindingError> {
        if !topology.contains_vertex(id) {
            return Err(GraphTopologyError::UnknownVertex(id).into());
        }
        self.vertices
            .remove(&id)
            .ok_or(GraphBindingError::VertexNotBound(id))
    }

    pub fn remove_edge(
        &mut self,
        topology: &GraphTopology,
        id: GraphEdgeId,
    ) -> Result<SemanticNodeId, GraphBindingError> {
        if topology.edge(id).is_none() {
            return Err(GraphTopologyError::UnknownEdge(id).into());
        }
        self.edges
            .remove(&id)
            .ok_or(GraphBindingError::EdgeNotBound(id))
    }

    /// Resolve exactly the semantic edge identities affected by one vertex.
    ///
    /// Complexity is O(degree) because adjacency remains owned by GraphTopology.
    pub fn incident_edge_nodes(
        &self,
        topology: &GraphTopology,
        vertex: GraphVertexId,
    ) -> Result<Vec<(GraphEdgeId, SemanticNodeId)>, GraphBindingError> {
        topology
            .incident_edges(vertex)?
            .iter()
            .map(|&edge| {
                self.edge_node(edge)
                    .map(|node| (edge, node))
                    .ok_or(GraphBindingError::EdgeNotBound(edge))
            })
            .collect()
    }

    fn require_unbound_semantic_identity(
        &self,
        node: SemanticNodeId,
    ) -> Result<(), GraphBindingError> {
        if self.vertices.values().chain(self.edges.values()).any(|&bound| bound == node) {
            Err(GraphBindingError::SemanticIdentityAlreadyBound(node))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ManimArrow, ManimArrowOptions, ManimGeometryOptions, Scene};
    use std::rc::Rc;

    #[test]
    fn semantic_bindings_preserve_topology_identity_and_incident_locality() {
        let mut topology = GraphTopology::new();
        let a = topology.add_vertex().unwrap();
        let b = topology.add_vertex().unwrap();
        let c = topology.add_vertex().unwrap();
        let ab = topology.add_edge(a, b, false).unwrap();
        let ac = topology.add_edge(a, c, true).unwrap();

        let mut scene = Scene::new();
        let va = scene.geometry(ManimGeometryOptions::circle(0.2).unwrap()).unwrap();
        let vb = scene.geometry(ManimGeometryOptions::circle(0.2).unwrap()).unwrap();
        let vc = scene.geometry(ManimGeometryOptions::circle(0.2).unwrap()).unwrap();
        let eab = ManimArrow::create(
            Rc::clone(scene.integration_store()),
            ManimArrowOptions::arrow(0.0, 0.0, 1.0, 0.0).unwrap(),
        )
        .unwrap();
        let eac = ManimArrow::create(
            Rc::clone(scene.integration_store()),
            ManimArrowOptions::arrow(0.0, 0.0, 0.0, 1.0).unwrap(),
        )
        .unwrap();

        let mut bindings = GraphSemanticBindings::new();
        bindings.bind_vertex(&topology, a, &va).unwrap();
        bindings.bind_vertex(&topology, b, &vb).unwrap();
        bindings.bind_vertex(&topology, c, &vc).unwrap();
        bindings.bind_edge(&topology, ab, eab.family()).unwrap();
        bindings.bind_edge(&topology, ac, eac.family()).unwrap();

        assert_eq!(bindings.vertex_node(a), Some(va.node_id()));
        assert_eq!(
            bindings.incident_edge_nodes(&topology, a).unwrap(),
            vec![(ab, eab.family().node_id()), (ac, eac.family().node_id())]
        );
        assert_eq!(
            bindings.incident_edge_nodes(&topology, b).unwrap(),
            vec![(ab, eab.family().node_id())]
        );
    }

    #[test]
    fn duplicate_semantic_identity_fails_without_partial_binding() {
        let mut topology = GraphTopology::new();
        let a = topology.add_vertex().unwrap();
        let b = topology.add_vertex().unwrap();
        let mut scene = Scene::new();
        let vertex = scene.geometry(ManimGeometryOptions::circle(0.2).unwrap()).unwrap();

        let mut bindings = GraphSemanticBindings::new();
        bindings.bind_vertex(&topology, a, &vertex).unwrap();
        assert!(matches!(
            bindings.bind_vertex(&topology, b, &vertex),
            Err(GraphBindingError::SemanticIdentityAlreadyBound(_))
        ));
        assert_eq!(bindings.vertex_node(b), None);
    }
}
