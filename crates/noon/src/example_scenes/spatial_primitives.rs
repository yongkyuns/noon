//! Shared direct and native/WASM qualification scene for public spatial primitives.

use crate::{
    AnimationOptions, Color, ExecutionSession, MeshOptions, RateFunction, Scene, SemanticCamera3D,
    SemanticProjection3D, SemanticVec3, SemanticWorldTransform3D,
};
use noon_core::{
    CompositionTimeMap, SemanticAnimationCompositionKind, SemanticMutationTransaction,
    SemanticObjectTrackProperty, SemanticObjectTrackValues, SemanticPaint, SemanticRotation3D,
    SemanticStyle, TrackTiming,
};

const DURATION: f64 = 1.0;
const NEAR: f64 = 0.1;
const FAR: f64 = 30.0;

/// One explicit capped Line3D tube and one wound triangular indexed mesh.
pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    scene
        .camera_3d(
            SemanticCamera3D::new(
                SemanticVec3::new(0.0, 0.0, 5.0),
                SemanticRotation3D::IDENTITY,
                SemanticProjection3D::Perspective {
                    vertical_fov_radians: 1.0,
                    near: NEAR,
                    far: FAR,
                },
            )
            .ok_or("invalid spatial-primitives camera")?,
        )
        .map_err(|error| error.to_string())?;

    let line_geometry = crate::line_3d_mesh(
        SemanticVec3::new(-2.0, -0.8, 0.0),
        SemanticVec3::new(-0.2, -0.8, 0.0),
        0.18,
        16,
    )
    .map_err(|error| error.to_string())?;
    let line = scene
        .mesh(MeshOptions::new(line_geometry).with_style(opaque(Color::RED)))
        .map_err(|error| error.to_string())?;
    scene.add(&line).map_err(|error| error.to_string())?;

    let triangle_geometry = crate::triangular_polyhedron_mesh(
        &[
            SemanticVec3::new(0.45, -1.0, 0.0),
            SemanticVec3::new(2.25, -1.0, 0.0),
            SemanticVec3::new(1.35, 1.0, 0.25),
        ],
        &[[0, 1, 2]],
    )
    .map_err(|error| error.to_string())?;
    let triangle = scene
        .mesh(MeshOptions::new(triangle_geometry).with_style(opaque(Color::BLUE)))
        .map_err(|error| error.to_string())?;
    scene.add(&triangle).map_err(|error| error.to_string())?;

    let triangle_rotation = SemanticRotation3D::from_axis_angle(
        SemanticVec3::new(0.0, 1.0, 0.0),
        0.4,
    )
    .ok_or("invalid triangle rotation")?;
    let mut transaction = SemanticMutationTransaction::new();
    let line_track = transaction.create_object_property_track(
        line.node_id(),
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform {
            from: world_transform(SemanticVec3::ZERO, SemanticRotation3D::IDENTITY),
            to: world_transform(SemanticVec3::new(0.0, 0.0, 0.25), SemanticRotation3D::IDENTITY),
        },
        TrackTiming::new(0.0, DURATION, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let triangle_track = transaction.create_object_property_track(
        triangle.node_id(),
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform {
            from: world_transform(SemanticVec3::ZERO, SemanticRotation3D::IDENTITY),
            to: world_transform(SemanticVec3::ZERO, triangle_rotation),
        },
        TrackTiming::new(0.0, DURATION, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let animation_root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [line_track, triangle_track],
        AnimationOptions::new(),
    );
    let committed = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .map_err(|error| error.to_string())?;
    let animation_root = committed
        .resolve(animation_root)
        .ok_or("missing spatial-primitives composition")?;
    let result = {
        let store = scene.integration_store();
        let store = store.borrow();
        ExecutionSession::from_semantic_root_with_animation_root(
            &store,
            scene.root(),
            animation_root,
        )
    };
    result.map_err(|error| error.to_string())
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

fn world_transform(
    translation: SemanticVec3,
    rotation: SemanticRotation3D,
) -> SemanticWorldTransform3D {
    SemanticWorldTransform3D::new(
        translation,
        rotation,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .expect("fixture transform is finite and invertible")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_line_and_explicit_triangle_share_the_native_execution_timeline() {
        let mut session = session().unwrap();
        assert_eq!(session.frame().objects.len(), 3);
        assert_eq!(session.wake_state().timeline(), noon_runtime::TimelineWakeState::Continuous);
        let rows = |session: &ExecutionSession| {
            let line = session.frame().objects.iter().find(|row| row.style.fill == Some(Color::RED)).unwrap();
            let triangle = session.frame().objects.iter().find(|row| row.style.fill == Some(Color::BLUE)).unwrap();
            (line.world_transform().unwrap(), triangle.world_transform().unwrap())
        };
        assert_eq!(rows(&session).0.translation, SemanticVec3::ZERO);
        session.advance_to(0.5).unwrap();
        assert_eq!(rows(&session).0.translation.z, 0.125);
        assert!((rows(&session).1.rotation.components()[2] - (0.1_f64).sin()).abs() < 1.0e-12);
        session.advance_to(DURATION).unwrap();
        assert_eq!(rows(&session).0.translation.z, 0.25);
        assert!((rows(&session).1.rotation.components()[2] - 0.2_f64.sin()).abs() < 1.0e-12);
    }
}
