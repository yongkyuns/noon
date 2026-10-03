use super::*;
use crate::Scene;
use noon_core::{
    AnimationOptions, GeometryRef, GeometryResource, GeometryResourceLookup, MeshResource,
    RateFunction, SemanticCamera3D, SemanticProjection3D, SemanticRotation3D, SemanticVec3,
    SemanticWorldTransform3D,
};
use std::sync::Arc;

fn fixture() -> (Scene, Vec<Mobject>, ExecutionSession, ExecutionSegment) {
    let mut scene = Scene::new();
    scene
        .camera_3d(
            SemanticCamera3D::new(
                SemanticVec3::new(0.0, 0.0, 5.0),
                SemanticRotation3D::IDENTITY,
                SemanticProjection3D::Perspective {
                    vertical_fov_radians: 1.0,
                    near: 0.1,
                    far: 100.0,
                },
            )
            .unwrap(),
        )
        .unwrap();

    let mut objects = Vec::new();
    for index in 0..12 {
        let mesh = noon_geometry::cube_mesh(0.5 + index as f64 * 0.05).unwrap();
        let mut transform = SemanticWorldTransform3D::IDENTITY;
        transform.translation = SemanticVec3::new(index as f64 * 2.0, 0.0, 0.0);
        let object = scene
            .mesh(MeshOptions::new(mesh).with_transform(transform))
            .unwrap();
        scene.add(&object).unwrap();
        objects.push(object);
    }

    let mut target = objects[0].world_transform().unwrap();
    target.translation = SemanticVec3::new(2.0, 0.5, 0.25);
    target.rotation =
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 0.0, 1.0), 0.8).unwrap();
    let animation = scene
        .declare_world_transform(
            &objects[0],
            target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut execution = scene.execution_session().unwrap();
    let segment = execution
        .activate_animation_segment(
            &scene.integration_store().borrow(),
            animation.node_id(),
            AnimationOptions::new(),
        )
        .unwrap();
    execution.advance_to(0.5).unwrap();
    (scene, objects, execution, segment)
}

fn mesh_handle(object: &Mobject) -> noon_core::GeometryResourceHandle {
    object
        .state()
        .unwrap()
        .content
        .geometry()
        .and_then(|geometry| geometry.resource_handle())
        .unwrap()
}

fn execution_mesh(
    execution: &ExecutionSession,
    handle: noon_core::GeometryResourceHandle,
) -> Arc<MeshResource> {
    match execution.geometry_resources().get(handle).unwrap() {
        GeometryResource::Mesh(mesh) => Arc::clone(&mesh),
        _ => panic!("expected mesh resource"),
    }
}

#[test]
fn replacing_one_live_mesh_is_local_and_keeps_old_resource_arc_valid() {
    let (scene, objects, mut execution, segment) = fixture();
    let target = &objects[0];
    let authored_world_before = target.world_transform().unwrap();
    let execution_id = execution.execution_object_id(target.node_id()).unwrap();
    let target_frame_index = execution
        .frame()
        .objects
        .iter()
        .position(|row| row.id == execution_id)
        .unwrap();
    let effective_before = execution.frame().objects[target_frame_index]
        .world_transform()
        .unwrap();
    assert_ne!(effective_before, authored_world_before);

    let original_handles = objects.iter().map(mesh_handle).collect::<Vec<_>>();
    let prior_runtime_mesh = execution_mesh(&execution, original_handles[0]);
    let other_snapshot_meshes = original_handles[1..]
        .iter()
        .map(|&handle| execution_mesh(&execution, handle))
        .collect::<Vec<_>>();
    let execution_ids = objects
        .iter()
        .map(|object| execution.execution_object_id(object.node_id()).unwrap())
        .collect::<Vec<_>>();
    let old_positions = prior_runtime_mesh.positions().to_vec();
    let midpoint_frame = execution.frame().clone();
    execution.take_frame_changes();
    let midpoint_context = execution.publication_context();
    let midpoint_resource_stats = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();

    // An active ordinary animation segment retains the existing completion barrier.
    let replacement = noon_geometry::cube_mesh(2.25).unwrap();
    {
        let mut live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        assert!(live
            .replace_mesh_geometry(target, replacement.clone())
            .is_err());
    }
    assert_eq!(execution.frame(), &midpoint_frame);
    assert_eq!(execution.publication_context(), midpoint_context);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        midpoint_resource_stats
    );

    execution.advance_to(segment.end_time()).unwrap();
    {
        let mut live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        live.complete_segment(segment).unwrap();
        let effective_at_barrier = live.effective_world_transform(target).unwrap();
        let authored_at_barrier = target.world_transform().unwrap();
        assert_eq!(effective_at_barrier, authored_at_barrier);
        let authored_state_at_barrier = target.state().unwrap();
        let context_at_barrier = live.session.publication_context();
        live.replace_mesh_geometry(target, replacement).unwrap();

        let authored_after = target.state().unwrap();
        assert_eq!(target.world_transform().unwrap(), authored_at_barrier);
        assert_eq!(
            authored_after.transform,
            authored_state_at_barrier.transform
        );
        assert_eq!(authored_after.style, authored_state_at_barrier.style);
        assert_eq!(
            live.session.publication_context().scene_revision(),
            context_at_barrier.scene_revision().checked_next().unwrap()
        );
        assert_eq!(
            live.effective_world_transform(target).unwrap(),
            effective_at_barrier
        );
    }

    assert_eq!(
        execution.take_frame_changes().object_indices(),
        &[target_frame_index]
    );
    let frame_geometry_id = match execution.frame().objects[target_frame_index]
        .content
        .geometry()
    {
        Some(GeometryRef::External(id)) => Some(*id),
        _ => None,
    };
    assert_eq!(
        frame_geometry_id.and_then(|id| execution.geometry_resources().current_handle(id)),
        Some(mesh_handle(target))
    );

    for (index, object) in objects.iter().enumerate() {
        assert_eq!(
            execution.execution_object_id(object.node_id()),
            Some(execution_ids[index]),
            "replacement preserves stable execution identities"
        );
    }
    for (mesh, &handle) in other_snapshot_meshes.iter().zip(&original_handles[1..]) {
        assert!(Arc::ptr_eq(mesh, &execution_mesh(&execution, handle)));
    }
    assert_ne!(mesh_handle(target), original_handles[0]);

    // The old runtime resource Arc remains immutable and usable after replacement.
    assert_eq!(prior_runtime_mesh.positions(), old_positions);
    assert_eq!(prior_runtime_mesh.indices().len(), 36);

    let after_resource_stats = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    assert!(
        after_resource_stats.live_resources <= midpoint_resource_stats.live_resources + 1,
        "one replacement may retain its prior immutable resource version"
    );
}

#[test]
fn replacing_non_mesh_geometry_fails_without_resource_or_publication_changes() {
    let (mut scene, _objects, _execution, _segment) = fixture();
    let path = scene.circle(0.25).unwrap();
    scene.add(&path).unwrap();
    // Bring execution to the same semantic revision through ordinary publication.
    let mut execution = scene.execution_session().unwrap();
    let before_context = execution.publication_context();
    let before_frame = execution.frame().clone();
    let before_resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    let before_state = path.state().unwrap();

    let mut live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
    assert!(live
        .replace_mesh_geometry(&path, noon_geometry::cube_mesh(1.0).unwrap())
        .is_err());

    assert_eq!(path.state().unwrap(), before_state);
    assert_eq!(live.session.publication_context(), before_context);
    assert_eq!(live.session.frame(), &before_frame);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        before_resources
    );
}
