use crate::{
    GraphTopology, SemanticMutationImpact, SemanticMutationTransaction as Transaction,
    SemanticMutationTransactionError as Error, SemanticNodeCreation, SemanticObjectState,
    SemanticStore, SemanticTransactionGraphDeclaration, SemanticTransactionGraphEdgeBinding,
    StoredGeometry, Vec2,
};

fn circle() -> SemanticObjectState {
    SemanticObjectState::new(StoredGeometry::Circle { radius: 0.2 })
}

fn line() -> SemanticObjectState {
    SemanticObjectState::new(StoredGeometry::Line {
        start: Vec2::new(-1.0, 0.0),
        end: Vec2::new(1.0, 0.0),
    })
}

#[test]
fn pending_graph_topology_and_bindings_commit_atomically_with_stable_ids() {
    let mut store = SemanticStore::new();
    let revision = store.scene_revision();

    let mut topology = GraphTopology::new();
    let a_id = topology.add_vertex();
    let b_id = topology.add_vertex();
    let ab_id = topology.add_edge(a_id, b_id, false).unwrap();

    let mut tx = Transaction::new();
    let root = tx.create_node(SemanticNodeCreation::family());
    let edge_family = tx.create_node(SemanticNodeCreation::family());
    let edge_line = tx.create_node(SemanticNodeCreation::object(line()));
    let a = tx.create_node(SemanticNodeCreation::object(circle()));
    let b = tx.create_node(SemanticNodeCreation::object(circle()));
    tx.add_member(edge_family, edge_line)
        .add_member(root, edge_family)
        .add_member(root, a)
        .add_member(root, b)
        .set_graph_declaration(
            root,
            SemanticTransactionGraphDeclaration::new(
                topology,
                [(a_id, a), (b_id, b)],
                [SemanticTransactionGraphEdgeBinding::new(
                    ab_id,
                    edge_family.into(),
                    edge_line.into(),
                )],
            ),
        );

    let result = tx.apply(&mut store).unwrap();
    let root = result.resolve(root).unwrap();
    let edge_family = result.resolve(edge_family).unwrap();
    let edge_line = result.resolve(edge_line).unwrap();
    let a = result.resolve(a).unwrap();
    let b = result.resolve(b).unwrap();
    let graph = store.semantic_graph_declaration(root).unwrap().unwrap();

    assert_eq!(graph.vertex_node(a_id), Some(a));
    assert_eq!(graph.vertex_node(b_id), Some(b));
    assert_eq!(graph.edge_between(b_id, a_id, false), Some(ab_id));
    assert_eq!(graph.edge_binding(ab_id).unwrap().family(), edge_family);
    assert_eq!(graph.edge_binding(ab_id).unwrap().line(), edge_line);
    assert_eq!(graph.incident_edges(a_id).unwrap(), &[ab_id]);
    assert_eq!(
        result.impacts().last(),
        Some(&SemanticMutationImpact::GraphDeclaration { scope: root })
    );
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
}

#[test]
fn invalid_graph_declarations_do_not_publish_pending_nodes_or_revision() {
    for mode in 0..4 {
        let mut store = SemanticStore::new();
        let revision = store.scene_revision();
        let next = store.preview_node_allocations().next();

        let mut topology = GraphTopology::new();
        let a_id = topology.add_vertex();
        let b_id = topology.add_vertex();
        let ab_id = topology.add_edge(a_id, b_id, false).unwrap();

        let mut tx = Transaction::new();
        let root = tx.create_node(SemanticNodeCreation::family());
        let edge_family = tx.create_node(SemanticNodeCreation::family());
        let edge_line = tx.create_node(SemanticNodeCreation::object(if mode == 1 {
            circle()
        } else {
            line()
        }));
        let a = tx.create_node(SemanticNodeCreation::object(circle()));
        let b = tx.create_node(SemanticNodeCreation::object(circle()));

        tx.add_member(edge_family, edge_line)
            .add_member(root, edge_family)
            .add_member(root, a);
        if mode != 0 {
            tx.add_member(root, b);
        }

        // The declaration constructor performs the conversion. Keeping tokens
        // here avoids an unconstrained intermediate Into target type.
        let vertices = match mode {
            2 => vec![(a_id, a)],
            3 => vec![(a_id, a), (a_id, b)],
            _ => vec![(a_id, a), (b_id, b)],
        };
        tx.set_graph_declaration(
            root,
            SemanticTransactionGraphDeclaration::new(
                topology,
                vertices,
                [SemanticTransactionGraphEdgeBinding::new(
                    ab_id,
                    edge_family.into(),
                    edge_line.into(),
                )],
            ),
        );

        assert!(
            matches!(tx.apply(&mut store), Err(Error::InvalidGraphDeclaration { .. })),
            "mode={mode}"
        );
        assert_eq!(store.scene_revision(), revision, "mode={mode}");
        assert_eq!(store.preview_node_allocations().next(), next, "mode={mode}");
        assert_eq!(store.len(), 0, "mode={mode}");
    }
}

#[test]
fn graph_dependency_references_prevent_a_dangling_declaration_on_generic_removal() {
    let mut store = SemanticStore::new();
    let mut topology = GraphTopology::new();
    let a_id = topology.add_vertex();
    let b_id = topology.add_vertex();
    let ab_id = topology.add_edge(a_id, b_id, false).unwrap();

    let mut tx = Transaction::new();
    let root = tx.create_node(SemanticNodeCreation::family());
    let edge_family = tx.create_node(SemanticNodeCreation::family());
    let edge_line = tx.create_node(SemanticNodeCreation::object(line()));
    let a = tx.create_node(SemanticNodeCreation::object(circle()));
    let b = tx.create_node(SemanticNodeCreation::object(circle()));
    tx.add_member(edge_family, edge_line)
        .add_member(root, edge_family)
        .add_member(root, a)
        .add_member(root, b)
        .set_graph_declaration(
            root,
            SemanticTransactionGraphDeclaration::new(
                topology,
                [(a_id, a), (b_id, b)],
                [SemanticTransactionGraphEdgeBinding::new(
                    ab_id,
                    edge_family.into(),
                    edge_line.into(),
                )],
            ),
        );
    let result = tx.apply(&mut store).unwrap();
    let root = result.resolve(root).unwrap();
    let a = result.resolve(a).unwrap();
    assert!(store.semantic_graph_declaration(root).unwrap().is_some());

    let mut remove = Transaction::new();
    remove.remove_node(a);
    remove.apply(&mut store).unwrap();

    assert!(store.node(a).is_none());
    assert!(
        store.node(root).is_none(),
        "generic child removal must not leave stale graph topology behind"
    );
}
