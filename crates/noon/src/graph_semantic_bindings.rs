use crate::{
    GraphEdgeId, GraphTopology, GraphTopologyError, GraphVertexId, Mobject, MobjectFamily,
};
use noon_core::SemanticNodeId;
use std::{collections::HashMap, rc::Rc};

/// Stable semantic bindings for one graph topology.
///
/// Topology owns graph identity and adjacency. This table only associates those
/// identities with ordinary semantic objects/families; it is not a second scene
/// model and carries no geometry, layout, or runtime state.
#[derive(Clone, Debug, Default)]
pub struct GraphSemanticBindings {
    vertices: HashMap<GraphVertexId, SemanticNodeId>,
    edges: HashMap<GraphEdgeId, SemanticNodeId>,
    edge_lines: HashMap<GraphEdgeId, SemanticNodeId>,
    store: Option<Rc<std::cell::RefCell<noon_core::SemanticStore>>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GraphBindingError {
    Topology(GraphTopologyError),
    Authoring(crate::AuthoringError),
    EdgeLineAlreadyBound(GraphEdgeId),
    EdgeLineNotAnalytic(SemanticNodeId),
    EdgeLineNotMember {
        edge: GraphEdgeId,
        node: SemanticNodeId,
    },
    VertexAlreadyBound(GraphVertexId),
    EdgeAlreadyBound(GraphEdgeId),
    VertexNotBound(GraphVertexId),
    EdgeNotBound(GraphEdgeId),
    SemanticIdentityAlreadyBound(SemanticNodeId),
    ForeignSemanticStore,
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
            Self::Authoring(error) => error.fmt(formatter),
            Self::EdgeLineAlreadyBound(id) => write!(
                formatter,
                "graph edge {} already has a Line component",
                id.get()
            ),
            Self::EdgeLineNotAnalytic(node) => write!(
                formatter,
                "graph Line component {node:?} is not an analytic Line"
            ),
            Self::EdgeLineNotMember { edge, node } => write!(
                formatter,
                "Line component {node:?} is not a direct member of graph edge {}",
                edge.get()
            ),
            Self::VertexAlreadyBound(id) => {
                write!(formatter, "graph vertex {} is already bound", id.get())
            }
            Self::EdgeAlreadyBound(id) => {
                write!(formatter, "graph edge {} is already bound", id.get())
            }
            Self::VertexNotBound(id) => write!(formatter, "graph vertex {} is not bound", id.get()),
            Self::EdgeNotBound(id) => write!(formatter, "graph edge {} is not bound", id.get()),
            Self::SemanticIdentityAlreadyBound(id) => {
                write!(
                    formatter,
                    "semantic identity {id:?} is already bound in this graph"
                )
            }
            Self::ForeignSemanticStore => {
                formatter.write_str("graph semantic bindings cannot span semantic stores")
            }
        }
    }
}

impl std::error::Error for GraphBindingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Topology(error) => Some(error),
            Self::Authoring(error) => Some(error),
            _ => None,
        }
    }
}

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

    pub fn edge_line_node(&self, id: GraphEdgeId) -> Option<SemanticNodeId> {
        self.edge_lines.get(&id).copied()
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
        self.require_store(object.integration_store())?;
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
        self.require_store(family.integration_store())?;
        self.require_unbound_semantic_identity(family.node_id())?;
        self.edges.insert(id, family.node_id());
        Ok(())
    }

    /// Bind the ordinary analytic Line child that carries one edge's endpoints.
    ///
    /// The family root remains the graph edge's semantic identity; this typed
    /// component binding avoids inferring child meaning from family order.
    /// The component must be a valid analytic Line directly contained by this
    /// family. A second binding is rejected; removal is required before rebinding.
    /// Membership/content are checked at admission, not frozen by the binding.
    pub fn bind_edge_line(
        &mut self,
        topology: &GraphTopology,
        id: GraphEdgeId,
        line: &Mobject,
    ) -> Result<(), GraphBindingError> {
        if topology.edge(id).is_none() {
            return Err(GraphTopologyError::UnknownEdge(id).into());
        }
        let family_node = self
            .edge_node(id)
            .ok_or(GraphBindingError::EdgeNotBound(id))?;
        if self.edge_lines.contains_key(&id) {
            return Err(GraphBindingError::EdgeLineAlreadyBound(id));
        }
        self.require_store(line.integration_store())?;
        let state = line.state().map_err(GraphBindingError::Authoring)?;
        if !matches!(
            state.content.geometry(),
            Some(noon_core::StoredGeometry::Line { .. })
        ) {
            return Err(GraphBindingError::EdgeLineNotAnalytic(line.node_id()));
        }
        {
            let store = line.integration_store().borrow();
            let family = store
                .semantic_family_checked(family_node)
                .map_err(crate::AuthoringError::from)
                .map_err(GraphBindingError::Authoring)?;
            if !family.contains_member(line.node_id()) {
                return Err(GraphBindingError::EdgeLineNotMember {
                    edge: id,
                    node: line.node_id(),
                });
            }
        }
        self.require_unbound_semantic_identity(line.node_id())?;
        self.edge_lines.insert(id, line.node_id());
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
        self.edge_lines.remove(&id);
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

    fn require_store(
        &mut self,
        store: &Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    ) -> Result<(), GraphBindingError> {
        match &self.store {
            Some(bound) if !Rc::ptr_eq(bound, store) => {
                Err(GraphBindingError::ForeignSemanticStore)
            }
            Some(_) => Ok(()),
            None => {
                self.store = Some(Rc::clone(store));
                Ok(())
            }
        }
    }

    fn require_unbound_semantic_identity(
        &self,
        node: SemanticNodeId,
    ) -> Result<(), GraphBindingError> {
        if self
            .vertices
            .values()
            .chain(self.edges.values())
            .chain(self.edge_lines.values())
            .any(|&bound| bound == node)
        {
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
        let va = scene
            .geometry(ManimGeometryOptions::circle(0.2).unwrap())
            .unwrap();
        let vb = scene
            .geometry(ManimGeometryOptions::circle(0.2).unwrap())
            .unwrap();
        let vc = scene
            .geometry(ManimGeometryOptions::circle(0.2).unwrap())
            .unwrap();
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
        let vertex = scene
            .geometry(ManimGeometryOptions::circle(0.2).unwrap())
            .unwrap();

        let mut bindings = GraphSemanticBindings::new();
        bindings.bind_vertex(&topology, a, &vertex).unwrap();
        assert!(matches!(
            bindings.bind_vertex(&topology, b, &vertex),
            Err(GraphBindingError::SemanticIdentityAlreadyBound(_))
        ));
        assert_eq!(bindings.vertex_node(b), None);
    }

    #[test]
    fn bindings_reject_cross_scene_identity_before_mutation() {
        let mut topology = GraphTopology::new();
        let a = topology.add_vertex().unwrap();
        let b = topology.add_vertex().unwrap();
        let mut first = Scene::new();
        let mut second = Scene::new();
        let va = first
            .geometry(ManimGeometryOptions::circle(0.2).unwrap())
            .unwrap();
        let vb = second
            .geometry(ManimGeometryOptions::circle(0.2).unwrap())
            .unwrap();

        let mut bindings = GraphSemanticBindings::new();
        bindings.bind_vertex(&topology, a, &va).unwrap();
        assert_eq!(
            bindings.bind_vertex(&topology, b, &vb),
            Err(GraphBindingError::ForeignSemanticStore)
        );
        assert_eq!(bindings.vertex_node(b), None);
    }

    fn line_fixture() -> (GraphTopology, GraphEdgeId, Scene, GraphSemanticBindings) {
        let mut topology = GraphTopology::new();
        let a = topology.add_vertex().unwrap();
        let b = topology.add_vertex().unwrap();
        let edge = topology.add_edge(a, b, false).unwrap();
        (topology, edge, Scene::new(), GraphSemanticBindings::new())
    }

    #[test]
    fn line_binding_rejects_non_line_without_changing_bindings_or_scene() {
        let (topology, edge, mut scene, mut bindings) = line_fixture();
        let circle = scene
            .geometry(ManimGeometryOptions::circle(0.2).unwrap())
            .unwrap();
        let family = scene.family(&[(&circle).into()]).unwrap();
        bindings.bind_edge(&topology, edge, &family).unwrap();
        let revision = scene.revision();
        let nodes = scene.integration_store().borrow().len();
        assert!(bindings.bind_edge_line(&topology, edge, &circle).is_err());
        assert_eq!(bindings.edge_line_node(edge), None);
        assert_eq!(bindings.edge_node(edge), Some(family.node_id()));
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
    }

    #[test]
    fn line_binding_rejects_line_outside_bound_family() {
        let (topology, edge, mut scene, mut bindings) = line_fixture();
        let inside = scene
            .geometry(ManimGeometryOptions::line(0.0, 0.0, 1.0, 0.0).unwrap())
            .unwrap();
        let outside = scene
            .geometry(ManimGeometryOptions::line(0.0, 1.0, 1.0, 1.0).unwrap())
            .unwrap();
        let family = scene.family(&[(&inside).into()]).unwrap();
        bindings.bind_edge(&topology, edge, &family).unwrap();
        let revision = scene.revision();
        assert!(bindings.bind_edge_line(&topology, edge, &outside).is_err());
        assert_eq!(bindings.edge_line_node(edge), None);
        bindings.bind_edge_line(&topology, edge, &inside).unwrap();
        assert_eq!(bindings.edge_line_node(edge), Some(inside.node_id()));
        assert_eq!(scene.revision(), revision);
    }

    #[test]
    fn line_binding_rejects_silent_component_replacement() {
        let (topology, edge, mut scene, mut bindings) = line_fixture();
        let first = scene
            .geometry(ManimGeometryOptions::line(0.0, 0.0, 1.0, 0.0).unwrap())
            .unwrap();
        let second = scene
            .geometry(ManimGeometryOptions::line(0.0, 1.0, 1.0, 1.0).unwrap())
            .unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        bindings.bind_edge(&topology, edge, &family).unwrap();
        bindings.bind_edge_line(&topology, edge, &first).unwrap();
        let revision = scene.revision();
        assert!(bindings.bind_edge_line(&topology, edge, &second).is_err());
        assert_eq!(bindings.edge_line_node(edge), Some(first.node_id()));
        assert_eq!(scene.revision(), revision);
        bindings.remove_edge(&topology, edge).unwrap();
        assert_eq!(bindings.edge_line_node(edge), None);
        bindings.bind_edge(&topology, edge, &family).unwrap();
        bindings.bind_edge_line(&topology, edge, &second).unwrap();
        assert_eq!(bindings.edge_line_node(edge), Some(second.node_id()));
    }

    #[test]
    fn line_binding_accepts_explicit_arrow_shaft_without_assuming_child_order() {
        let (mut topology, _, scene, mut bindings) = line_fixture();
        let vertices = topology.vertices().to_vec();
        let edge = topology.add_edge(vertices[0], vertices[1], true).unwrap();
        let arrow = ManimArrow::create(
            Rc::clone(scene.integration_store()),
            ManimArrowOptions::arrow(0.0, 0.0, 2.0, 1.0).unwrap(),
        )
        .unwrap();
        let family = arrow.family();
        let revision = scene.revision();
        bindings.bind_edge(&topology, edge, family).unwrap();
        bindings
            .bind_edge_line(&topology, edge, arrow.shaft())
            .unwrap();
        assert_eq!(bindings.edge_node(edge), Some(family.node_id()));
        assert_eq!(bindings.edge_line_node(edge), Some(arrow.shaft().node_id()));
        assert_eq!(scene.revision(), revision);
    }
}
