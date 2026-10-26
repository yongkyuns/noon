use noon::{Mobject, MobjectTarget, Scene};
use noon_core::{
    SemanticMutationTransaction, SemanticNodeId, SemanticObjectProperty, SemanticVec3,
};

fn attach(scene: &Scene, parent: SemanticNodeId, members: &[SemanticNodeId]) {
    let mut transaction = SemanticMutationTransaction::new();
    for &member in members {
        transaction.add_member(parent, member);
    }
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
}

fn members(scene: &Scene, root: SemanticNodeId) -> Vec<SemanticNodeId> {
    scene
        .integration_store()
        .borrow()
        .node(root)
        .unwrap()
        .members_iter()
        .collect()
}

#[test]
fn object_root_copy_preserves_own_content_descendant_order_and_internal_aliases_atomically() {
    let mut scene = Scene::new();
    let parent = scene.square(2.0).unwrap();
    let child = scene.circle(0.5).unwrap();
    let nested_object = scene.square(0.25).unwrap();
    let nested = scene
        .family(&[(&child).into(), (&nested_object).into()])
        .unwrap();
    attach(&scene, nested_object.node_id(), &[child.node_id()]);
    attach(
        &scene,
        parent.node_id(),
        &[child.node_id(), nested.node_id()],
    );
    let saved = scene.circle(0.75).unwrap();
    attach(&scene, saved.node_id(), &[child.node_id()]);
    let before = scene.integration_store().borrow().scene_revision();
    let nodes_before = scene.integration_store().borrow().len();
    let copied = parent
        .copy_with_references(&[(&saved).into(), (&child).into()])
        .unwrap();
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before.checked_next().unwrap()
    );
    assert_eq!(scene.integration_store().borrow().len(), nodes_before + 5);
    let copy_parent = copied.root();
    let mut copy_child = copied.mobject(&child).unwrap();
    let copy_nested_object = copied.mobject(&nested_object).unwrap();
    let copy_nested = copied.family(&nested).unwrap();
    let copy_saved = copied.mobject(&saved).unwrap();
    assert_eq!(
        copied.mobject(&parent).unwrap().node_id(),
        copy_parent.node_id()
    );
    assert_eq!(
        members(&scene, copy_parent.node_id()),
        [copy_child.node_id(), copy_nested.node_id()]
    );
    assert_eq!(
        members(&scene, copy_nested.node_id()),
        [copy_child.node_id(), copy_nested_object.node_id()]
    );
    assert_eq!(
        members(&scene, copy_nested_object.node_id()),
        [copy_child.node_id()]
    );
    assert_eq!(
        members(&scene, copy_saved.node_id()),
        [copy_child.node_id()]
    );
    for (source, target) in [
        (&parent, copy_parent),
        (&child, &copy_child),
        (&nested_object, &copy_nested_object),
        (&saved, &copy_saved),
    ] {
        assert_ne!(source.node_id(), target.node_id());
        let source = source.state().unwrap();
        let target = target.state().unwrap();
        assert_eq!(source.content, target.content);
        assert_eq!(source.style, target.style);
        assert_eq!(source.transform, target.transform);
    }
    copy_child.shift(3.0, 0.0).unwrap();
    assert_eq!(child.center().unwrap(), (0.0, 0.0));
    assert_eq!(copy_child.center().unwrap(), (3.0, 0.0));
    assert_eq!(
        members(&scene, parent.node_id()),
        [child.node_id(), nested.node_id()]
    );
}

#[test]
fn family_copy_and_object_target_editor_share_complete_object_descendant_copies() {
    let mut scene = Scene::new();
    let parent = scene.square(2.0).unwrap();
    let child = scene.circle(0.5).unwrap();
    attach(&scene, parent.node_id(), &[child.node_id()]);
    let family = scene.family(&[(&parent).into(), (&child).into()]).unwrap();
    let copied = family.copy_family().unwrap();
    assert_eq!(
        members(&scene, copied.mobject(&parent).unwrap().node_id()),
        [copied.mobject(&child).unwrap().node_id()]
    );
    for target in [
        parent.copy_handle().unwrap(),
        parent.target_editor().unwrap(),
    ] {
        let children = members(&scene, target.node_id());
        assert_eq!(children.len(), 1);
        assert_ne!(children[0], child.node_id());
        let copy_child =
            Mobject::from_node(scene.integration_store().clone(), children[0]).unwrap();
        assert_eq!(
            copy_child.state().unwrap().content,
            child.state().unwrap().content
        );
        assert_eq!(
            target.state().unwrap().content,
            parent.state().unwrap().content
        );
    }
}

#[test]
fn late_uncapturable_object_child_rejects_the_entire_live_copy_without_allocating() {
    let mut scene = Scene::new();
    let parent = scene.square(2.0).unwrap();
    let child = scene.circle(0.5).unwrap();
    attach(&scene, parent.node_id(), &[child.node_id()]);
    let signal = scene
        .integration_store()
        .borrow_mut()
        .insert_semantic_input_signal(SemanticVec3::ZERO)
        .unwrap();
    scene
        .integration_store()
        .borrow_mut()
        .bind_semantic_signal(signal, child.node_id(), SemanticObjectProperty::Translation)
        .unwrap();
    scene.add(&parent).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let publication = execution.publication_context();
    let nodes_before = scene.integration_store().borrow().len();
    let source_members = members(&scene, parent.node_id());
    assert!(scene.live(&mut execution).copy_mobject(&parent).is_err());
    assert_eq!(execution.publication_context(), publication);
    assert_eq!(scene.integration_store().borrow().len(), nodes_before);
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        publication.scene_revision()
    );
    assert_eq!(members(&scene, parent.node_id()), source_members);
}

#[test]
fn object_copy_foreign_reference_rejects_before_publication() {
    let mut scene = Scene::new();
    let parent = scene.square(2.0).unwrap();
    let foreign = Scene::new().circle(0.5).unwrap();
    let revision = scene.integration_store().borrow().scene_revision();
    assert!(parent
        .copy_with_references(&[MobjectTarget::Object(&foreign)])
        .is_err());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );
}

#[test]
fn live_object_copy_captures_graph_dependencies_below_object_roots_and_metadata_references() {
    for graph_is_child in [false, true] {
        let mut scene = Scene::new();
        let parent = scene.square(2.0).unwrap();
        let graph = scene
            .digraph_with_options(
                [("a", (-2.0, 0.0)), ("b", (2.0, 0.0))],
                [("a", "b")],
                noon::GraphOptions {
                    vertex_radius: 0.21,
                    ..Default::default()
                },
            )
            .unwrap();
        if graph_is_child {
            attach(&scene, parent.node_id(), &[graph.family().node_id()]);
            scene.add(&parent).unwrap();
        } else {
            scene.add_many(&[graph.family().into()]).unwrap();
        }
        let mut execution = scene.execution_session().unwrap();
        let count = execution.frame().objects.len();
        let copied = scene
            .live(&mut execution)
            .copy_mobject_with_references(&parent, &[graph.family().into()])
            .unwrap();
        assert_eq!(
            execution.frame().objects.len(),
            count,
            "detached copies do not enter execution"
        );
        let copied_graph = copied.family(graph.family()).unwrap();
        let source_arrow = graph.edge(&"a", &"b").unwrap().arrow().unwrap();
        let copied_line = copied.mobject(source_arrow.shaft()).unwrap();
        let copied_tip = copied.mobject(source_arrow.end_tip()).unwrap();
        let copied_edge_family = copied.family(source_arrow.family()).unwrap();
        let edge_id = graph.edge_id(&"a", &"b").unwrap();
        let store = scene.integration_store().borrow();
        let declaration = store
            .semantic_graph_declaration(copied_graph.node_id())
            .unwrap()
            .unwrap();
        let binding = declaration.edge_binding(edge_id).unwrap();
        assert_eq!(binding.line(), copied_line.node_id());
        assert_eq!(binding.family(), copied_edge_family.node_id());
        let noon_core::SemanticGraphEdgeDependency::Arrow { end_tip, .. } = binding.dependency()
        else {
            panic!("copied directed graph must retain its Arrow dependency");
        };
        assert_eq!(end_tip, copied_tip.node_id());
        drop(store);
        assert_eq!(
            members(&scene, copied.root().node_id()),
            if graph_is_child {
                vec![copied_graph.node_id()]
            } else {
                vec![]
            }
        );
        assert_eq!(
            copied_line.state().unwrap().transform,
            source_arrow.shaft().state().unwrap().transform
        );
    }
}

#[test]
fn shared_native_and_direct_wasm_example_runs_authored_and_coherent_object_copies() {
    let session = noon::example_scenes::object_membership::session().unwrap();
    assert_eq!(session.session().painter_order().len(), 3);
}
