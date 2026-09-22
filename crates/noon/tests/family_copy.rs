use noon::{
    AnimationOptions, ManimArrow, ManimArrowOptions, ManimArrowVectorField, RateFunction, Scene,
    VectorFieldAxisRange, VectorFieldPoint, VectorFieldRanges2D,
};
use noon_core::{SemanticObjectProperty, SemanticVec3};
use std::rc::Rc;

#[test]
fn arrow_aggregate_rebind_uses_only_copied_components_for_queries_and_scale() {
    let mut scene = Scene::new();
    let mut options = ManimArrowOptions::double_arrow(-2.0, 0.0, 2.0, 0.0).unwrap();
    options.set_buff(0.0).unwrap();
    let source = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
    let copied = source.family().copy_family().unwrap();
    let target = copied.rebind_manim_arrow(&source).unwrap();

    assert_ne!(target.family().node_id(), source.family().node_id());
    assert_ne!(target.shaft().node_id(), source.shaft().node_id());
    assert_ne!(target.end_tip().node_id(), source.end_tip().node_id());
    assert_eq!(
        target.start_tip().unwrap().node_id(),
        copied
            .mobject(source.start_tip().unwrap())
            .unwrap()
            .node_id()
    );
    assert_eq!(
        target.manim_endpoints().unwrap(),
        source.manim_endpoints().unwrap()
    );
    assert_eq!(
        target.manim_length().unwrap(),
        source.manim_length().unwrap()
    );

    let source_endpoints = source.manim_endpoints().unwrap();
    let source_length = source.manim_length().unwrap();
    target.scale(0.5, true).unwrap();
    assert_eq!(source.manim_endpoints().unwrap(), source_endpoints);
    assert_eq!(source.manim_length().unwrap(), source_length);
    assert!((target.manim_length().unwrap() - source_length * 0.5).abs() < 1.0e-6);

    let unrelated = scene.family(&[]).unwrap().copy_family().unwrap();
    assert!(unrelated.rebind_manim_arrow(&source).is_err());
}

#[test]
fn vector_field_aggregate_rebind_keeps_each_vector_on_the_copied_family_graph() {
    let scene = Scene::new();
    let source = ManimArrowVectorField::create(
        Rc::clone(scene.integration_store()),
        |point| VectorFieldPoint::new(point.x + 1.0, point.y + 1.0),
        VectorFieldRanges2D::new(
            VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            VectorFieldAxisRange::new(0.0, 0.0, 1.0),
        ),
    )
    .unwrap();
    let copied = source.family().copy_family().unwrap();
    let target = copied.rebind_manim_arrow_vector_field(&source).unwrap();

    assert_ne!(target.family().node_id(), source.family().node_id());
    assert_eq!(target.vectors().len(), 1);
    let source_vector = &source.vectors()[0];
    let target_vector = &target.vectors()[0];
    assert_eq!(
        target_vector.family().node_id(),
        copied.family(source_vector.family()).unwrap().node_id()
    );
    assert_eq!(
        target_vector.manim_length().unwrap(),
        source_vector.manim_length().unwrap()
    );

    let source_length = source_vector.manim_length().unwrap();
    target_vector.scale(0.5, false).unwrap();
    assert_eq!(source_vector.manim_length().unwrap(), source_length);
    assert!((target_vector.manim_length().unwrap() - source_length * 0.5).abs() < 1.0e-6);

    let selected_copy = source_vector.family().copy_family().unwrap();
    let selected = selected_copy.rebind_manim_arrow_vector(&source, 0).unwrap();
    assert_eq!(
        selected.manim_endpoints().unwrap(),
        source_vector.manim_endpoints().unwrap()
    );
    selected.scale(0.5, true).unwrap();
    assert_eq!(source_vector.manim_length().unwrap(), source_length);
    assert!((selected.manim_length().unwrap() - source_length * 0.5).abs() < 1.0e-6);
    assert!(selected_copy.rebind_manim_arrow_vector(&source, 1).is_err());
}

#[test]
fn authored_copy_preserves_dag_aliases_order_resources_and_source_state_in_one_commit() {
    let mut scene = Scene::new();
    let leaf = scene.square(0.5).unwrap();
    let nested = scene.family(&[(&leaf).into()]).unwrap();
    let source = scene.family(&[(&leaf).into(), (&nested).into()]).unwrap();
    let before = scene.integration_store().borrow().scene_revision();
    let copied = source.copy_family().unwrap();
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before.checked_next().unwrap()
    );
    let mut copy_leaf = copied.mobject(&leaf).unwrap();
    let copy_nested = copied.family(&nested).unwrap();
    assert_ne!(copy_leaf.node_id(), leaf.node_id());
    assert_ne!(copied.root().node_id(), source.node_id());
    let copied_state = copy_leaf.state().unwrap();
    let source_state = leaf.state().unwrap();
    assert_eq!(copied_state.content, source_state.content);
    assert_eq!(copied_state.transform, source_state.transform);
    assert_eq!(copied_state.style, source_state.style);
    assert_eq!(
        copied_state.presentation().z_index,
        source_state.presentation().z_index
    );
    assert_ne!(
        copied_state.presentation().insertion_order,
        source_state.presentation().insertion_order
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(copied.root().node_id())
            .unwrap(),
        vec![copy_leaf.node_id(), copy_nested.node_id()]
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(copy_nested.node_id())
            .unwrap(),
        vec![copy_leaf.node_id()]
    );
    copy_leaf.shift(2.0, 0.0).unwrap();
    assert_eq!(leaf.center().unwrap(), (0.0, 0.0));
    assert_eq!(copy_nested.layout().unwrap().center(), (2.0, 0.0));
    let empty = scene.family(&[]).unwrap();
    assert_eq!(
        empty.copy_family().unwrap().root().layout_bounds().unwrap(),
        None
    );
    assert!(copied.family(&empty).is_err());
    let foreign = Scene::new().square(1.0).unwrap();
    assert!(copied.mobject(&foreign).is_err());
    scene
        .integration_store()
        .borrow_mut()
        .remove_node(leaf.node_id())
        .unwrap();
    assert!(copied.mobject(&leaf).is_err());
    assert_eq!(copy_leaf.center().unwrap(), (2.0, 0.0));
}

#[test]
fn live_copy_obeys_completion_and_captures_completed_state_without_changing_source() {
    let mut scene = Scene::new();
    let leaf = scene.square(0.5).unwrap();
    let family = scene.family(&[(&leaf).into()]).unwrap();
    let mut target = leaf.target_editor().unwrap();
    target.shift(4.0, 0.0).unwrap();
    scene.add(&leaf).unwrap();
    let animation = scene
        .declare_transform_to(
            &leaf,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut execution = scene.execution_session().unwrap();
    let mut live = scene.live(&mut execution);
    let segment = live.play_animation(&animation).unwrap();
    live.advance_segment_to(segment, 1.0).unwrap();
    let halfway = live.effective(&leaf).unwrap();
    assert!(live.copy_family(&family).is_err());
    assert_eq!(live.effective(&leaf).unwrap(), halfway);
    live.advance_segment_to(segment, 2.0).unwrap();
    live.complete_segment(segment).unwrap();
    let before = live.effective(&leaf).unwrap();
    let copied = live.copy_family(&family).unwrap();
    let copy_leaf = copied.mobject(&leaf).unwrap();
    assert_eq!(copy_leaf.center().unwrap(), (4.0, 0.0));
    assert_eq!(leaf.center().unwrap(), (4.0, 0.0));
    assert_eq!(live.effective(&leaf).unwrap().transform, before.transform);
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before.publication.scene_revision().checked_next().unwrap()
    );
    live.shift(&copy_leaf, 1.0, 0.0).unwrap();
    assert_eq!(copy_leaf.center().unwrap(), (5.0, 0.0));
    assert_eq!(leaf.center().unwrap(), (4.0, 0.0));
}

#[test]
fn a_late_uncapturable_member_rejects_the_entire_live_copy() {
    let mut scene = Scene::new();
    let first = scene.square(0.5).unwrap();
    let reactive = scene.square(0.5).unwrap();
    let signal = scene
        .integration_store()
        .borrow_mut()
        .insert_semantic_input_signal(SemanticVec3::ZERO)
        .unwrap();
    scene
        .integration_store()
        .borrow_mut()
        .bind_semantic_signal(
            signal,
            reactive.node_id(),
            SemanticObjectProperty::Translation,
        )
        .unwrap();
    let source = scene
        .family(&[(&first).into(), (&reactive).into()])
        .unwrap();
    let mut execution = scene.execution_session().unwrap();
    let before = execution.publication_context();
    assert!(scene.live(&mut execution).copy_family(&source).is_err());
    assert_eq!(execution.publication_context(), before);
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before.scene_revision()
    );
    assert_eq!(first.center().unwrap(), (0.0, 0.0));
}

#[test]
fn detached_references_copy_atomically_without_changing_family_membership() {
    let mut scene = Scene::new();
    let leaf = scene.square(0.5).unwrap();
    let saved = leaf.target_editor().unwrap();
    let source = scene.family(&[(&leaf).into()]).unwrap();
    let referenced_family = scene.family(&[(&saved).into(), (&leaf).into()]).unwrap();
    let before = scene.integration_store().borrow().scene_revision();
    let copied = source
        .copy_with_references(&[(&saved).into(), (&referenced_family).into(), (&leaf).into()])
        .unwrap();
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before.checked_next().unwrap()
    );
    let copied_leaf = copied.mobject(&leaf).unwrap();
    let mut copied_saved = copied.mobject(&saved).unwrap();
    let store = scene.integration_store().borrow();
    assert_eq!(
        store
            .semantic_family_members_checked(copied.root().node_id())
            .unwrap(),
        vec![copied_leaf.node_id()]
    );
    assert_eq!(
        store
            .semantic_family_members_checked(copied.family(&referenced_family).unwrap().node_id())
            .unwrap(),
        vec![copied_saved.node_id(), copied_leaf.node_id()]
    );
    drop(store);
    copied_saved.shift(3.0, 0.0).unwrap();
    assert_eq!(saved.center().unwrap(), (0.0, 0.0));
    let foreign = Scene::new().square(1.0).unwrap();
    let before = scene.integration_store().borrow().scene_revision();
    assert!(source
        .copy_with_references(&[(&saved).into(), (&foreign).into()])
        .is_err());
    assert_eq!(scene.integration_store().borrow().scene_revision(), before);
}

#[test]
fn graph_family_copy_preserves_and_remaps_semantic_graph_declaration() {
    let mut scene = Scene::new();
    let graph = scene
        .graph([("a", (-1.0, 0.0)), ("b", (1.0, 0.0))], [("a", "b")])
        .unwrap();
    let a_id = graph.vertex_id(&"a").unwrap();
    let b_id = graph.vertex_id(&"b").unwrap();
    let ab_id = graph.edge_id(&"a", &"b").unwrap();
    let source_a = graph.vertex(&"a").unwrap();
    let source_b = graph.vertex(&"b").unwrap();
    let source_edge = graph.edge(&"a", &"b").unwrap();

    let copied = graph.family().copy_family().unwrap();
    let copied_a = copied.mobject(source_a).unwrap();
    let copied_b = copied.mobject(source_b).unwrap();
    let copied_edge_family = copied.family(source_edge.family()).unwrap();
    let copied_line = copied.mobject(source_edge.line()).unwrap();

    let store = scene.integration_store().borrow();
    let declaration = store
        .semantic_graph_declaration(copied.root().node_id())
        .unwrap()
        .expect("copied graph root retains graph semantics");

    assert_eq!(declaration.vertex_node(a_id), Some(copied_a.node_id()));
    assert_eq!(declaration.vertex_node(b_id), Some(copied_b.node_id()));
    assert_eq!(declaration.edge_between(b_id, a_id, false), Some(ab_id));
    let binding = declaration.edge_binding(ab_id).unwrap();
    assert_eq!(binding.family(), copied_edge_family.node_id());
    assert_eq!(binding.line(), copied_line.node_id());
    assert_ne!(binding.family(), source_edge.family().node_id());
    assert_ne!(binding.line(), source_edge.line().node_id());
}
