from pathlib import Path
import subprocess
import sys

path = Path('crates/noon/src/graph_semantic_bindings.rs')
source = path.read_text()

def replace(old, new):
    global source
    assert source.count(old) == 1, (old[:90], source.count(old))
    source = source.replace(old, new, 1)

TESTS = r'''
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
        let circle = scene.geometry(ManimGeometryOptions::circle(0.2).unwrap()).unwrap();
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
        let inside = scene.geometry(ManimGeometryOptions::line(0.0, 0.0, 1.0, 0.0).unwrap()).unwrap();
        let outside = scene.geometry(ManimGeometryOptions::line(0.0, 1.0, 1.0, 1.0).unwrap()).unwrap();
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
        let first = scene.geometry(ManimGeometryOptions::line(0.0, 0.0, 1.0, 0.0).unwrap()).unwrap();
        let second = scene.geometry(ManimGeometryOptions::line(0.0, 1.0, 1.0, 1.0).unwrap()).unwrap();
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
        ).unwrap();
        let family = arrow.family();
        let revision = scene.revision();
        bindings.bind_edge(&topology, edge, family).unwrap();
        bindings.bind_edge_line(&topology, edge, arrow.shaft()).unwrap();
        assert_eq!(bindings.edge_node(edge), Some(family.node_id()));
        assert_eq!(bindings.edge_line_node(edge), Some(arrow.shaft().node_id()));
        assert_eq!(scene.revision(), revision);
    }
'''

if sys.argv[1] == 'tests':
    assert source.endswith('}\n')
    assert 'fn line_fixture()' not in source
    source = source[:-2] + TESTS + '}\n'
elif sys.argv[1] == 'repair':
    replace('#[derive(Clone, Debug, PartialEq, Eq)]\npub enum GraphBindingError',
            '#[derive(Clone, Debug, PartialEq)]\npub enum GraphBindingError')
    replace('    Topology(GraphTopologyError),', '    Topology(GraphTopologyError),\n    Authoring(crate::AuthoringError),\n    EdgeLineAlreadyBound(GraphEdgeId),\n    EdgeLineNotAnalytic(SemanticNodeId),\n    EdgeLineNotMember { edge: GraphEdgeId, node: SemanticNodeId },')
    replace('            Self::Topology(error) => error.fmt(formatter),', '''            Self::Topology(error) => error.fmt(formatter),
            Self::Authoring(error) => error.fmt(formatter),
            Self::EdgeLineAlreadyBound(id) => write!(formatter, "graph edge {} already has a Line component", id.get()),
            Self::EdgeLineNotAnalytic(node) => write!(formatter, "graph Line component {node:?} is not an analytic Line"),
            Self::EdgeLineNotMember { edge, node } => write!(formatter, "Line component {node:?} is not a direct member of graph edge {}", edge.get()),''')
    replace('impl std::error::Error for GraphBindingError {}', '''impl std::error::Error for GraphBindingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Topology(error) => Some(error),
            Self::Authoring(error) => Some(error),
            _ => None,
        }
    }
}''')
    replace('''        if !self.edges.contains_key(&id) {
            return Err(GraphBindingError::EdgeNotBound(id));
        }
        self.require_store(line.integration_store())?;
        self.require_unbound_semantic_identity(line.node_id())?;
        self.edge_lines.insert(id, line.node_id());''', '''        let family_node = self.edge_node(id).ok_or(GraphBindingError::EdgeNotBound(id))?;
        if self.edge_lines.contains_key(&id) {
            return Err(GraphBindingError::EdgeLineAlreadyBound(id));
        }
        self.require_store(line.integration_store())?;
        let state = line.state().map_err(GraphBindingError::Authoring)?;
        if !matches!(state.content.geometry(), Some(noon_core::StoredGeometry::Line { .. })) {
            return Err(GraphBindingError::EdgeLineNotAnalytic(line.node_id()));
        }
        {
            let store = line.integration_store().borrow();
            let family = store.semantic_family_checked(family_node)
                .map_err(crate::AuthoringError::from)
                .map_err(GraphBindingError::Authoring)?;
            if !family.contains_member(line.node_id()) {
                return Err(GraphBindingError::EdgeLineNotMember { edge: id, node: line.node_id() });
            }
        }
        self.require_unbound_semantic_identity(line.node_id())?;
        self.edge_lines.insert(id, line.node_id());''')
    replace('''    /// component binding avoids inferring child meaning from family order.
''', '''    /// component binding avoids inferring child meaning from family order.
    /// The component must be a valid analytic Line directly contained by this
    /// family. A second binding is rejected; removal is required before rebinding.
    /// Membership/content are checked at admission, not frozen by the binding.
''')
else:
    raise ValueError(sys.argv[1])
path.write_text(source)
