use super::*;
use noon_core::{SemanticMutationTransaction, StoredGeometry};

fn graph(scene: &mut Scene) -> Graph<&'static str> {
    scene
        .graph(
            [("a", (-2.0, 0.0)), ("b", (0.0, 1.0)), ("c", (2.0, 0.0))],
            [("a", "b"), ("b", "c")],
        )
        .unwrap()
}

#[test]
fn graph_binds_stable_ids_and_publishes_once_in_edge_then_vertex_order() {
    let mut scene = Scene::new();
    let before = scene.revision();
    let graph = graph(&mut scene);
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    let a = graph.vertex_id(&"a").unwrap();
    let b = graph.vertex_id(&"b").unwrap();
    let c = graph.vertex_id(&"c").unwrap();
    assert_eq!(
        graph.topology().unwrap().vertices().collect::<Vec<_>>(),
        vec![a, b, c]
    );
    let ab = graph.edge_id(&"a", &"b").unwrap();
    assert_eq!(graph.edge_id(&"b", &"a"), Some(ab));
    let declaration = graph.semantic_declaration().unwrap();
    assert_eq!(
        declaration.vertex_node(a),
        Some(graph.vertex(&"a").unwrap().node_id())
    );
    let binding = declaration.edge_binding(ab).unwrap();
    assert_eq!(
        binding.family(),
        graph.edge(&"a", &"b").unwrap().family().node_id()
    );
    assert_eq!(
        binding.line(),
        graph.edge(&"a", &"b").unwrap().line().node_id()
    );
    assert_eq!(declaration.incident_edge_nodes(b).unwrap().len(), 2);
    let store = scene.integration_store().borrow();
    let members = store
        .semantic_family_members_checked(graph.family().node_id())
        .unwrap();
    assert_eq!(members.len(), 5);
    assert_eq!(members[0], binding.family());
    assert_eq!(members[2], graph.vertex(&"a").unwrap().node_id());
    assert!(matches!(
        graph
            .edge(&"a", &"b")
            .unwrap()
            .line()
            .state()
            .unwrap()
            .content
            .geometry(),
        Some(StoredGeometry::Line { .. })
    ));
}

#[test]
fn topology_and_declaration_reads_borrow_the_authority_without_cloning() {
    let mut scene = Scene::new();
    let graph = graph(&mut scene);
    let before = scene.revision();
    let topology = graph.topology().unwrap();
    let declaration = graph.semantic_declaration().unwrap();
    let store = scene.integration_store().borrow();
    let authored = store
        .semantic_graph_declaration(graph.family().node_id())
        .unwrap()
        .unwrap();
    assert!(std::ptr::eq(&*declaration, authored));
    assert!(std::ptr::eq(&*topology, authored.topology()));
    assert_eq!(scene.revision(), before);
}

#[test]
fn declaration_survives_wrapper_drop() {
    let mut scene = Scene::new();
    let graph = graph(&mut scene);
    let root = graph.family().node_id();
    let edge_id = graph.edge_id(&"a", &"b").unwrap();
    let edge_family = graph.edge(&"a", &"b").unwrap().family().node_id();
    drop(graph);
    let store = scene.integration_store().borrow();
    assert_eq!(
        store
            .semantic_graph_declaration(root)
            .unwrap()
            .unwrap()
            .edge_binding(edge_id)
            .unwrap()
            .family(),
        edge_family
    );
}

#[test]
fn stale_root_reads_return_typed_errors_even_after_slot_reuse() {
    let mut scene = Scene::new();
    let graph = graph(&mut scene);
    let root = graph.family().node_id();
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(root);
    tx.apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let replacement = scene.family(&[]).unwrap();
    assert_eq!(replacement.node_id().slot(), root.slot());
    assert_ne!(replacement.node_id(), root);
    assert!(matches!(graph.semantic_declaration(),
        Err(GraphAuthoringError::Store(SemanticStoreError::UnknownNode(node))) if node == root));
    assert!(matches!(
        graph.topology(),
        Err(GraphAuthoringError::Store(_))
    ));
    assert_eq!(graph.edge_id(&"a", &"b"), None);
}

#[test]
fn conflicting_integration_borrow_returns_error_instead_of_refcell_panic() {
    let mut scene = Scene::new();
    let graph = graph(&mut scene);
    let exclusive = scene.integration_store().borrow_mut();
    assert!(matches!(
        graph.semantic_declaration(),
        Err(GraphAuthoringError::StoreBorrowed)
    ));
    assert!(matches!(
        graph.topology(),
        Err(GraphAuthoringError::StoreBorrowed)
    ));
    drop(exclusive);
    assert!(graph.semantic_declaration().is_ok());
}

#[test]
fn digraph_reuses_arrow_family_and_preserves_direction() {
    let mut scene = Scene::new();
    let before = scene.revision();
    let graph = scene
        .digraph(
            [("a", (-1.0, 0.0)), ("b", (1.0, 0.0))],
            [("a", "b"), ("b", "a")],
        )
        .unwrap();
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    let ab = graph.edge_id(&"a", &"b").unwrap();
    assert_ne!(ab, graph.edge_id(&"b", &"a").unwrap());
    assert!(graph.edge(&"a", &"b").unwrap().arrow().is_some());
    assert!(graph.edge(&"b", &"a").unwrap().arrow().is_some());
    let declaration = graph.semantic_declaration().unwrap();
    assert_eq!(
        declaration.edge_binding(ab).unwrap().line(),
        graph.edge(&"a", &"b").unwrap().line().node_id()
    );
    let topology = graph.topology().unwrap();
    assert!(std::ptr::eq(&*topology, declaration.topology()));
}

#[test]
fn invalid_keys_endpoints_duplicate_edges_and_self_loops_do_not_publish() {
    let mut scene = Scene::new();
    let revision = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    assert!(matches!(
        scene.graph(
            [("a", (0.0, 0.0)), ("a", (1.0, 0.0))],
            std::iter::empty::<(&str, &str)>(),
        ),
        Err(GraphAuthoringError::DuplicateVertexKey { .. })
    ));
    assert!(matches!(
        scene.graph([("a", (0.0, 0.0))], [("a", "missing")]),
        Err(GraphAuthoringError::UnknownEdgeEndpoint {
            endpoint: GraphEndpoint::End,
            ..
        })
    ));
    assert!(matches!(
        scene.graph([("a", (0.0, 0.0))], [("missing", "a")]),
        Err(GraphAuthoringError::UnknownEdgeEndpoint {
            endpoint: GraphEndpoint::Start,
            ..
        })
    ));
    assert!(matches!(
        scene.graph(
            [("a", (-1.0, 0.0)), ("b", (1.0, 0.0))],
            [("a", "b"), ("b", "a")],
        ),
        Err(GraphAuthoringError::Topology(
            GraphTopologyError::DuplicateEdge {
                directed: false,
                ..
            }
        ))
    ));
    assert!(matches!(
        scene.graph([("a", (0.0, 0.0))], [("a", "a")]),
        Err(GraphAuthoringError::SelfEdgeUnsupported { edge_index: 0 })
    ));
    assert!(matches!(
        scene.digraph([("a", (0.0, 0.0))], [("a", "a")]),
        Err(GraphAuthoringError::SelfEdgeUnsupported { edge_index: 0 })
    ));
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
}

#[test]
fn invalid_options_are_rejected_even_without_vertices_or_edges() {
    let bad_options = [
        GraphOptions {
            vertex_radius: f64::NAN,
            ..GraphOptions::default()
        },
        GraphOptions {
            vertex_stroke_width: -1.0,
            ..GraphOptions::default()
        },
        GraphOptions {
            edge_stroke_width: f64::NAN,
            ..GraphOptions::default()
        },
        GraphOptions {
            directed_edge_buff: Some(f64::INFINITY),
            ..GraphOptions::default()
        },
    ];
    for options in bad_options {
        let mut scene = Scene::new();
        let revision = scene.revision();
        let nodes = scene.integration_store().borrow().len();
        assert!(scene
            .graph_with_options(
                std::iter::empty::<(u8, (f64, f64))>(),
                std::iter::empty::<(u8, u8)>(),
                options.clone(),
            )
            .is_err());
        assert!(scene
            .digraph_with_options([(0, (0.0, 0.0))], std::iter::empty::<(u8, u8)>(), options)
            .is_err());
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
    }
}

#[test]
fn valid_empty_graph_publishes_one_detached_declared_family() {
    let mut scene = Scene::new();
    let revision = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    let graph = scene
        .graph(
            std::iter::empty::<(u8, (f64, f64))>(),
            std::iter::empty::<(u8, u8)>(),
        )
        .unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!(scene.integration_store().borrow().len(), nodes + 1);
    assert_eq!(graph.topology().unwrap().vertices().count(), 0);
    assert_eq!(graph.topology().unwrap().edges().count(), 0);
    assert!(scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .members()
        .is_empty());
}

#[test]
fn nonfinite_late_vertex_is_rejected_without_publication() {
    let mut scene = Scene::new();
    let revision = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    assert!(scene
        .digraph(
            [("a", (0.0, 0.0)), ("b", (1.0, 0.0)), ("c", (f64::NAN, 0.0))],
            [("a", "b")],
        )
        .is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
}

#[test]
fn running_scene_constructs_detached_graph_in_one_coherent_publication() {
    let mut scene = Scene::new();
    let sentinel = scene.circle(0.25).unwrap();
    scene.add(&sentinel).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let before = scene.revision();
    let graph = graph(&mut scene);
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    assert_eq!(
        scene
            .owned_execution()
            .publication_context()
            .scene_revision(),
        scene.revision()
    );
    assert!(scene
        .owned_execution()
        .execution_object_id(graph.vertex(&"a").unwrap().node_id())
        .is_none());
    scene
        .add_many(&[crate::MobjectTarget::Family(graph.family())])
        .unwrap();
    assert!(scene
        .owned_execution()
        .execution_object_id(graph.vertex(&"a").unwrap().node_id())
        .is_some());
    assert!(scene
        .owned_execution()
        .execution_object_id(graph.edge(&"a", &"b").unwrap().line().node_id())
        .is_some());
}

#[test]
fn stale_live_publication_rolls_back_directed_graph_nodes_and_tip_resources() {
    let mut scene = Scene::new();
    let mut sentinel = scene.circle(0.25).unwrap();
    scene.add(&sentinel).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    sentinel.shift(1.0, 0.0).unwrap();
    let revision = scene.revision();
    let publication = scene.owned_execution().publication_context();
    let nodes = scene.integration_store().borrow().len();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    let result = scene.digraph([("a", (-1.0, 0.0)), ("b", (1.0, 0.0))], [("a", "b")]);
    assert!(matches!(
        result,
        Err(GraphAuthoringError::Authoring(
            AuthoringError::ExecutionPublication(
                crate::ExecutionSessionPublicationError::StaleSceneRevision { .. }
            )
        ))
    ));
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.owned_execution().publication_context(), publication);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
}

#[test]
fn iteration_preserves_authored_vertex_and_edge_order() {
    let mut scene = Scene::new();
    let graph = scene
        .graph(
            [(10, (-1.0, 0.0)), (20, (0.0, 0.0)), (30, (1.0, 0.0))],
            [(20, 30), (10, 20)],
        )
        .unwrap();
    assert_eq!(
        graph.vertex_keys().copied().collect::<Vec<_>>(),
        vec![10, 20, 30]
    );
    assert_eq!(
        graph
            .edge_mobjects()
            .map(|(edge, _)| edge.id)
            .collect::<Vec<_>>(),
        graph
            .topology()
            .unwrap()
            .edges()
            .map(|edge| edge.id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn generic_family_edits_cannot_silently_stale_public_graph_semantics() {
    let mut scene = Scene::new();
    let graph = graph(&mut scene);
    let extra = scene.circle(0.2).unwrap();
    let ab = graph.edge_id(&"a", &"b").unwrap();
    let before = scene.revision();

    assert!(matches!(
        graph.family().add((&extra).into()),
        Err(AuthoringError::Transaction(
            noon_core::SemanticMutationTransactionError::InvalidGraphDeclaration { .. }
        ))
    ));
    assert_eq!(scene.revision(), before);

    let edge = graph.edge(&"a", &"b").unwrap();
    assert!(matches!(
        edge.family().remove(edge.line().into()),
        Err(AuthoringError::Transaction(
            noon_core::SemanticMutationTransactionError::InvalidGraphDeclaration { .. }
        ))
    ));
    assert_eq!(scene.revision(), before);

    let declaration = graph.semantic_declaration().unwrap();
    let binding = declaration.edge_binding(ab).unwrap();
    assert_eq!(binding.family(), edge.family().node_id());
    assert_eq!(binding.line(), edge.line().node_id());
}
