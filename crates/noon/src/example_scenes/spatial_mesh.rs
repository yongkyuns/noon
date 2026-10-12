//! Shared typed Rust/WASM qualification scene for immutable spatial meshes.

use crate::ExecutionSession;
use noon_core::{
    AnimationOptions, Color, CompositionTimeMap, MeshResource, RateFunction,
    SemanticAnimationCompositionKind, SemanticMutationTransaction, SemanticObjectRole,
    SemanticObjectState, SemanticObjectTrackProperty, SemanticObjectTrackValues, SemanticPaint,
    SemanticProjection3D, SemanticStore, SemanticStyle, SemanticVec3, SemanticWorldTransform3D,
    StoredGeometry, TrackTiming,
};

/// Create a camera and two differently ordered instances of one retained triangle mesh.
///
/// The red front instance moves through the blue rear instance on an authored world
/// transform track. Both objects retain the same immutable mesh resource in the semantic
/// store, so native and direct-WASM execution exercise identical mesh/camera semantics.
pub fn session() -> Result<ExecutionSession, String> {
    let mut store = SemanticStore::new();
    let scene_root = store.insert_family();
    let mesh = MeshResource::new(
        vec![
            SemanticVec3::new(-1.0, -1.0, 0.0),
            SemanticVec3::new(1.0, -1.0, 0.0),
            SemanticVec3::new(0.0, 1.0, 0.0),
        ],
        None,
        vec![0, 1, 2],
    )
    .map_err(|error| error.to_string())?;
    let mesh_ref = store.insert_geometry_mesh(mesh);

    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
    camera.set_role(SemanticObjectRole::Camera3D);
    camera
        .set_camera_projection(Some(SemanticProjection3D::Perspective {
            vertical_fov_radians: 1.0,
            near: 0.1,
            far: 30.0,
        }))
        .map_err(|error| error.to_string())?;
    camera.transform.translation = SemanticVec3::new(0.0, 0.0, 5.0);
    let camera_id = store.insert_semantic_object(camera);
    store
        .add_semantic_family_member(scene_root, camera_id)
        .map_err(|error| error.to_string())?;

    let mut front = SemanticObjectState::new(StoredGeometry::Resource(mesh_ref));
    front.style = opaque(Color::RED);
    front.transform.translation = SemanticVec3::new(0.0, 0.0, 1.0);
    front.set_z_index(-5.0);
    let front_id = store.insert_semantic_object(front);
    store
        .add_semantic_family_member(scene_root, front_id)
        .map_err(|error| error.to_string())?;

    let mut rear = SemanticObjectState::new(StoredGeometry::Resource(mesh_ref));
    rear.style = opaque(Color::BLUE);
    rear.set_z_index(5.0);
    let rear_id = store.insert_semantic_object(rear);
    store
        .add_semantic_family_member(scene_root, rear_id)
        .map_err(|error| error.to_string())?;

    let mut transaction = SemanticMutationTransaction::new();
    let object_track = transaction.create_object_property_track(
        front_id,
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform {
            from: world_transform(SemanticVec3::new(0.0, 0.0, 1.0)),
            to: world_transform(SemanticVec3::new(0.0, 0.0, -1.0)),
        },
        TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let camera_track = transaction.create_object_property_track(
        camera_id,
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform {
            from: world_transform(SemanticVec3::new(0.0, 0.0, 5.0)),
            to: world_transform(SemanticVec3::new(0.25, 0.0, 5.0)),
        },
        TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let animation_root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [object_track, camera_track],
        AnimationOptions::new(),
    );
    let committed = transaction
        .apply(&mut store)
        .map_err(|error| error.to_string())?;
    let animation_root = committed
        .resolve(animation_root)
        .expect("created composition");
    ExecutionSession::from_semantic_root_with_animation_root(&store, scene_root, animation_root)
        .map_err(|error| error.to_string())
}

fn opaque(color: Color) -> SemanticStyle {
    SemanticStyle {
        fill: Some(SemanticPaint::Solid(color)),
        fill_opacity: 1.0,
        stroke: None,
        stroke_opacity: 1.0,
        stroke_width: 0.0,
        object_opacity: 1.0,
        ..SemanticStyle::default()
    }
}

fn world_transform(translation: SemanticVec3) -> SemanticWorldTransform3D {
    SemanticWorldTransform3D::new(
        translation,
        noon_core::SemanticRotation3D::IDENTITY,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .expect("fixture transforms are finite and invertible")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red_position(session: &ExecutionSession) -> SemanticVec3 {
        session
            .frame()
            .objects
            .iter()
            .find(|row| row.style.fill == Some(Color::RED))
            .expect("one red mesh")
            .world_transform()
            .unwrap()
            .translation
    }

    #[test]
    fn shared_mesh_camera_scene_lowers_and_animates_through_normal_session() {
        let mut session = session().unwrap();
        let initial = session.frame();
        assert_eq!(initial.objects.len(), 3);
        assert_eq!(
            initial
                .objects
                .iter()
                .filter(|row| row.camera_projection().is_some())
                .count(),
            1
        );
        assert!(initial.objects.iter().all(|row| row.spatial.is_some()));
        assert_eq!(
            session.wake_state().timeline(),
            noon_runtime::TimelineWakeState::Continuous
        );

        session.seek(1.0).unwrap();
        assert_eq!(red_position(&session).z, 0.0);
        assert_eq!(session.camera_3d().unwrap().unwrap().position.x, 0.125);
        session.seek(2.0).unwrap();
        assert_eq!(red_position(&session).z, -1.0);
        assert_eq!(
            session.wake_state().timeline(),
            noon_runtime::TimelineWakeState::Quiescent
        );
    }

    #[test]
    fn authored_mesh_and_camera_samples_match_forward_and_direct_seek() {
        let sample_times = [0.0, 0.5, 1.0, 1.5, 2.0];
        let mut forward = session().unwrap();
        let mut expected = Vec::with_capacity(sample_times.len());
        for time in sample_times {
            forward.advance_to(time).unwrap();
            assert_eq!(red_position(&forward).z, 1.0 - time);
            assert_eq!(
                forward.camera_3d().unwrap().unwrap().position.x,
                time * 0.125
            );
            expected.push(forward.frame().clone());
        }

        let mut sought = session().unwrap();
        for (time, expected_frame) in sample_times.into_iter().zip(expected) {
            sought.seek(time).unwrap();
            assert_eq!(
                sought.frame(),
                &expected_frame,
                "direct seek at t={time} must match source-forward advancement"
            );
        }
    }
}
