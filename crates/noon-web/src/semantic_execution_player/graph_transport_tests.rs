use std::collections::BTreeSet;

use noon_core::{
    HostCallbackId, NativeInputValue, NativeStateSource, ObjectId, SemanticMutationTransaction,
    Vec2,
};

use super::*;

fn object_ids(delta: &RetainedFamilyExecutionDeltaEnvelope) -> BTreeSet<ObjectId> {
    delta
        .retained
        .objects
        .iter()
        .map(|object| object.object)
        .collect()
}

fn assert_transform_only_graph_delta(
    delta: &RetainedFamilyExecutionDeltaEnvelope,
    expected: [ObjectId; 3],
    initial: &RetainedFamilyExecutionDeltaEnvelope,
) {
    assert!(delta.resource_additions.is_none());
    assert_eq!(object_ids(delta), expected.into_iter().collect());
    for id in &expected[1..] {
        let row = delta
            .retained
            .objects
            .iter()
            .find(|row| row.object == *id)
            .unwrap();
        let initial_row = initial
            .retained
            .objects
            .iter()
            .find(|row| row.object == *id)
            .unwrap();
        assert_eq!(row.content, initial_row.content);
        assert!(row.render_geometry.is_none());
        assert_eq!(
            row.render_geometry_resource,
            initial_row.render_geometry_resource
        );
        assert!(row.render_geometry_resource.is_some());
        let transform = row
            .render_transform
            .expect("Graph endpoint uses a transform");
        assert!(transform.translation.x.is_finite());
        assert!(transform.translation.y.is_finite());
        assert!(transform.rotation.is_finite());
        assert!(transform.scale.x.is_finite());
        assert!(transform.scale.y.is_finite());
    }
}

fn live_player(scene: &noon::Scene, session: ExecutionSession) -> SemanticExecutionPlayer {
    SemanticExecutionPlayer::from_live_session(
        session,
        std::rc::Rc::clone(scene.integration_store()),
        scene.root(),
        1.0,
        91,
    )
    .unwrap()
}

#[test]
fn copied_graph_ordinary_motion_is_local_and_reuses_endpoint_content() {
    let mut scene = noon::Scene::new();
    let graph = scene
        .digraph([("a", (-1.0, 0.0)), ("b", (1.0, 0.0))], [("a", "b")])
        .unwrap();
    let source_arrow = graph.edge(&"a", &"b").unwrap().arrow().unwrap();
    let copy = graph.family().copy_family().unwrap();
    let copied_start = copy.mobject(graph.vertex(&"a").unwrap()).unwrap();
    let copied_arrow = copy.rebind_manim_arrow(source_arrow).unwrap();
    scene
        .add_many(&[
            noon::MobjectTarget::Family(graph.family()),
            noon::MobjectTarget::Family(copy.root()),
        ])
        .unwrap();

    let session = scene.execution_session().unwrap();
    let original_start = session
        .execution_object_id(graph.vertex(&"a").unwrap().node_id())
        .unwrap();
    let copied_ids = [
        session.execution_object_id(copied_start.node_id()).unwrap(),
        session
            .execution_object_id(copied_arrow.shaft().node_id())
            .unwrap(),
        session
            .execution_object_id(copied_arrow.end_tip().node_id())
            .unwrap(),
    ];
    let mut player = live_player(&scene, session);
    let resources = player.resource_bundle_bytes();
    let mut mirror =
        crate::InstalledRetainedExecutionMirror::from_bundle_bytes(&resources).unwrap();
    let initial = player.delta(true).unwrap().unwrap();
    assert!(initial.resource_additions.is_some());
    mirror.apply_family(initial.clone()).unwrap();

    player.live_shift(&copied_start, -0.5, 0.25).unwrap();
    let delta = player.delta(false).unwrap().unwrap();

    assert_transform_only_graph_delta(&delta, copied_ids, &initial);
    assert!(!object_ids(&delta).contains(&original_start));
    assert_eq!(player.resource_bundle_bytes(), resources);
    mirror.apply_family(delta).unwrap();
}

#[test]
fn native_motion_expands_zero_length_arrow_without_resource_admission() {
    let mut scene = noon::Scene::new();
    let graph = scene
        .digraph_with_options(
            [("a", (0.0, 0.0)), ("b", (0.0, 0.0))],
            [("a", "b")],
            noon::GraphOptions {
                directed_edge_buff: Some(0.0),
                ..noon::GraphOptions::default()
            },
        )
        .unwrap();
    let arrow = graph.edge(&"a", &"b").unwrap().arrow().unwrap();
    let pointer = scene.pointer_position_signal().unwrap();
    scene
        .bind_native_translation(graph.vertex(&"a").unwrap(), &pointer)
        .unwrap();
    scene
        .add_many(&[noon::MobjectTarget::Family(graph.family())])
        .unwrap();
    let session = scene.execution_session().unwrap();
    let ids = [
        session
            .execution_object_id(graph.vertex(&"a").unwrap().node_id())
            .unwrap(),
        session
            .execution_object_id(arrow.shaft().node_id())
            .unwrap(),
        session
            .execution_object_id(arrow.end_tip().node_id())
            .unwrap(),
    ];
    let mut player = live_player(&scene, session);
    let revision = player.scene_revision();
    let resources = player.resource_bundle_bytes();
    let mut mirror =
        crate::InstalledRetainedExecutionMirror::from_bundle_bytes(&resources).unwrap();
    let initial = player.delta(true).unwrap().unwrap();
    mirror.apply_family(initial.clone()).unwrap();

    player
        .set_native_state_input(
            NativeStateSource::PointerPosition,
            NativeInputValue::Vec2(Vec2::new(-0.2, 0.0)),
        )
        .unwrap();
    let delta = player.delta(false).unwrap().unwrap();

    assert_eq!(player.scene_revision(), revision);
    assert_transform_only_graph_delta(&delta, ids, &initial);
    for id in &ids[1..] {
        let scale = delta
            .retained
            .objects
            .iter()
            .find(|row| row.object == *id)
            .unwrap()
            .render_transform
            .unwrap()
            .scale
            .x;
        assert!(
            scale > 0.0,
            "collapsed endpoint must recover to visible geometry"
        );
    }
    assert_eq!(player.resource_bundle_bytes(), resources);
    mirror.apply_family(delta).unwrap();
}

#[test]
fn callback_motion_publishes_vertex_and_endpoints_in_one_retained_delta() {
    let mut scene = noon::Scene::new();
    let graph = scene
        .digraph([("a", (-1.0, 0.0)), ("b", (1.0, 0.0))], [("a", "b")])
        .unwrap();
    let start = graph.vertex(&"a").unwrap();
    let arrow = graph.edge(&"a", &"b").unwrap().arrow().unwrap();
    scene
        .add_many(&[noon::MobjectTarget::Family(graph.family())])
        .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(start.node_id(), HostCallbackId::new(7), 0.0, None);
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let session = scene.execution_session().unwrap();
    let ids = [
        session.execution_object_id(start.node_id()).unwrap(),
        session
            .execution_object_id(arrow.shaft().node_id())
            .unwrap(),
        session
            .execution_object_id(arrow.end_tip().node_id())
            .unwrap(),
    ];
    let mut player = live_player(&scene, session);
    let revision = player.scene_revision();
    let resources = player.resource_bundle_bytes();
    let mut mirror =
        crate::InstalledRetainedExecutionMirror::from_bundle_bytes(&resources).unwrap();
    let initial = player.delta(true).unwrap().unwrap();
    mirror.apply_family(initial.clone()).unwrap();

    let phase: serde_json::Value =
        serde_json::from_str(&player.initial_callback_phase_json().unwrap().unwrap()).unwrap();
    let target = &phase["invocations"][0]["target"];
    let row = phase["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row.get("node") == Some(target))
        .unwrap();
    let mut transform = row["transform"].clone();
    transform["translation"]["x"] = serde_json::json!(-2.0);
    transform["translation"]["y"] = serde_json::json!(0.5);
    player
        .commit_callback_phase_json(
            &serde_json::json!({
                "token": phase["token"].clone(),
                "writes": [{
                    "kind": "transform",
                    "object": target.clone(),
                    "transform": transform,
                }],
            })
            .to_string(),
        )
        .unwrap();
    let delta = player.delta(false).unwrap().unwrap();

    assert_eq!(player.scene_revision(), revision);
    assert_transform_only_graph_delta(&delta, ids, &initial);
    assert_eq!(player.resource_bundle_bytes(), resources);
    mirror.apply_family(delta).unwrap();
}
