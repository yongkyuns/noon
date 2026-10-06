//! Small shared native/direct-WASM fixture for the typed UV surface lane.

use crate::{
    surface_mesh, AnimationOptions, Color, DeclaredAnimation, ExecutionSession, MeshOptions,
    RateFunction, Scene, SemanticCamera3D, SemanticPaint, SemanticProjection3D, SemanticRotation3D,
    SemanticSpatialMaterial, SemanticStyle, SemanticVec3, SemanticWorldTransform3D, SurfaceOptions,
    SurfaceSample, UvSurfacePlan,
};

/// Author an opaque unlit UV surface, ordinary semantic camera, and world-rotation intent.
fn author_surface(material: SemanticSpatialMaterial) -> Result<(Scene, crate::Mobject), String> {
    let mut scene = Scene::new();
    scene
        .camera_3d(
            SemanticCamera3D::new(
                SemanticVec3::new(0.0, 0.0, 5.0),
                SemanticRotation3D::IDENTITY,
                SemanticProjection3D::Perspective {
                    vertical_fov_radians: 1.0,
                    near: 0.1,
                    far: 30.0,
                },
            )
            .ok_or("invalid spatial-surface camera")?,
        )
        .map_err(|error| error.to_string())?;

    let plan =
        UvSurfacePlan::new([-1.5, 1.5], [-1.5, 1.5], [8, 8]).map_err(|error| error.to_string())?;
    let mesh = surface_mesh(plan, |u, v| {
        SurfaceSample::position(SemanticVec3::new(u, v, 0.25 * u * v))
    })
    .map_err(|error| error.to_string())?;
    let style = SemanticStyle {
        fill: Some(SemanticPaint::Solid(Color::rgba(0.2, 0.55, 0.85, 1.0))),
        fill_opacity: 1.0,
        stroke: None,
        stroke_width: 0.0,
        object_opacity: 1.0,
        ..SemanticStyle::default()
    };
    let surface = scene
        .mesh(
            MeshOptions::new(mesh)
                .with_style(style)
                .with_material(material),
        )
        .map_err(|error| error.to_string())?;
    scene.add(&surface).map_err(|error| error.to_string())?;
    Ok((scene, surface))
}

/// Author the ordinary world-rotation intent on the shared surface profile.
pub fn scene() -> Result<(Scene, crate::Mobject, DeclaredAnimation), String> {
    let (scene, surface) = author_surface(SemanticSpatialMaterial::Unlit)?;
    let rotation = SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 0.0, 1.0), 0.6)
        .ok_or("invalid spatial-surface target rotation")?;
    let target = SemanticWorldTransform3D::new(
        SemanticVec3::ZERO,
        rotation,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .ok_or("invalid spatial-surface target transform")?;
    let animation = scene
        .declare_world_transform(
            &surface,
            target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .map_err(|error| error.to_string())?;
    Ok((scene, surface, animation))
}

/// Duration-one animated session used by native and renderer-smoke hosts.
pub fn session() -> Result<ExecutionSession, String> {
    let (scene, _, animation) = scene()?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    session
        .activate_animation_segment(
            &scene.integration_store().borrow(),
            animation.node_id(),
            crate::AnimationOptions::new(),
        )
        .map_err(|error| error.to_string())?;
    Ok(session)
}

/// Point-lit counterpart used to qualify real worker resource capture and light-only updates.
/// The surface stays fixed while the ordinary point-light world track moves.
pub fn lighting_scene() -> Result<(Scene, crate::Mobject, crate::Mobject, DeclaredAnimation), String>
{
    let (mut scene, surface) = author_surface(SemanticSpatialMaterial::PointLit)?;

    let light = scene
        .point_light_3d(SemanticVec3::new(4.0, -3.0, 6.0), Color::WHITE, 1.0)
        .map_err(|error| error.to_string())?;
    scene.add(&light).map_err(|error| error.to_string())?;
    let target = SemanticWorldTransform3D::new(
        SemanticVec3::new(-3.0, 3.0, 4.0),
        SemanticRotation3D::IDENTITY,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .ok_or("invalid spatial-light target transform")?;
    let animation = scene
        .declare_world_transform(
            &light,
            target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .map_err(|error| error.to_string())?;
    Ok((scene, surface, light, animation))
}

/// Duration-one session for point-lit surface qualification.
pub fn lighting_session() -> Result<ExecutionSession, String> {
    let (scene, _, _, animation) = lighting_scene()?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    session
        .activate_animation_segment(
            &scene.integration_store().borrow(),
            animation.node_id(),
            AnimationOptions::new(),
        )
        .map_err(|error| error.to_string())?;
    Ok(session)
}

/// Default Cairo-shaded Manim Surface subset, retained as a UV cell family.
pub fn cairo_scene() -> Result<(Scene, crate::SurfaceFamily), String> {
    let mut scene = Scene::new();
    scene
        .camera_3d(
            SemanticCamera3D::new(
                SemanticVec3::new(0.0, 0.0, 5.0),
                SemanticRotation3D::IDENTITY,
                SemanticProjection3D::Perspective {
                    vertical_fov_radians: 1.0,
                    near: 0.1,
                    far: 30.0,
                },
            )
            .ok_or("invalid spatial-surface Cairo camera")?,
        )
        .map_err(|error| error.to_string())?;
    let plan =
        UvSurfacePlan::new([-1.0, 1.0], [-1.0, 1.0], [8, 8]).map_err(|error| error.to_string())?;
    let grid = plan
        .sample_cairo(|u, v| SemanticVec3::new(u, v, 0.35 * (u * u + v * v)))
        .map_err(|error| error.to_string())?;
    let surface = scene
        .surface_cairo_family(
            grid,
            SurfaceOptions {
                material: SemanticSpatialMaterial::CairoSurface,
                ..SurfaceOptions::default()
            },
        )
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[crate::MobjectTarget::Family(surface.family())])
        .map_err(|error| error.to_string())?;
    scene.wait(1.0)?;
    Ok((scene, surface))
}

/// One-second static execution session for the Cairo Surface raster case.
pub fn cairo_session() -> Result<ExecutionSession, String> {
    let (scene, _) = cairo_scene()?;
    scene.execution_session().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::GeometryResource;
    use noon_runtime::TimelineWakeState;
    use std::sync::Arc;

    #[test]
    fn surface_fixture_retains_bounded_mesh_topology_and_bounds() {
        let (scene, surface, _) = scene().unwrap();
        let state = surface.state().unwrap();
        let handle = state
            .content
            .geometry()
            .and_then(|geometry| geometry.resource_handle())
            .unwrap();
        let store = scene.integration_store().borrow();
        let Some(GeometryResource::Mesh(mesh)) = store.geometry_resources().get(handle) else {
            panic!("fixture surface uses one retained mesh resource");
        };
        assert_eq!(Arc::strong_count(mesh), 1);
        assert_eq!(mesh.positions().len(), 81);
        assert_eq!(mesh.normals().unwrap().len(), 81);
        assert_eq!(mesh.indices().len(), 8 * 8 * 6);
        assert_eq!(mesh.bounds().min, SemanticVec3::new(-1.5, -1.5, -0.5625));
        assert_eq!(mesh.bounds().max, SemanticVec3::new(1.5, 1.5, 0.5625));
        assert_eq!(
            state.spatial_material(),
            noon_core::SemanticSpatialMaterial::Unlit
        );
        assert_eq!(
            state.style.fill,
            Some(SemanticPaint::Solid(Color::rgba(0.2, 0.55, 0.85, 1.0)))
        );
    }

    #[test]
    fn rotating_surface_fixture_matches_forward_and_seek_at_start_midpoint_and_endpoint() {
        let mut forward = session().unwrap();
        assert_eq!(
            forward.wake_state().timeline(),
            TimelineWakeState::Continuous
        );
        let expected_angles = [(0.0, 0.0f64), (0.5, 0.3), (1.0, 0.6)];
        let mut expected_frames = Vec::new();
        for &(time, angle) in &expected_angles {
            forward.advance_to(time).unwrap();
            let row = forward
                .frame()
                .objects
                .iter()
                .find(|row| row.style.fill == Some(Color::rgba(0.2, 0.55, 0.85, 1.0)))
                .unwrap();
            let world = row.world_transform().unwrap();
            assert!((world.rotation.components()[0] - (angle * 0.5).cos()).abs() < 1.0e-12);
            assert!((world.rotation.components()[3] - (angle * 0.5).sin()).abs() < 1.0e-12);
            assert_eq!(forward.camera_3d().unwrap().unwrap().position.z, 5.0);
            expected_frames.push(forward.frame().clone());
        }
        assert_eq!(
            forward.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );

        let mut sought = session().unwrap();
        for (&(time, _), expected) in expected_angles.iter().zip(expected_frames) {
            sought.seek(time).unwrap();
            assert_eq!(sought.frame(), &expected);
        }
    }

    #[test]
    fn point_lit_surface_uses_normals_while_only_the_light_moves() {
        let (scene, surface, light, _) = lighting_scene().unwrap();
        let surface_state = surface.state().unwrap();
        assert_eq!(
            surface_state.spatial_material(),
            SemanticSpatialMaterial::PointLit
        );
        let mesh_handle = surface_state
            .content
            .geometry()
            .and_then(|geometry| geometry.resource_handle())
            .unwrap();
        let store = scene.integration_store().borrow();
        let Some(GeometryResource::Mesh(mesh)) = store.geometry_resources().get(mesh_handle) else {
            panic!("lighting fixture retains its indexed surface mesh");
        };
        assert_eq!(mesh.normals().unwrap().len(), 81);
        assert_eq!(
            light.state().unwrap().role(),
            noon_core::SemanticObjectRole::PointLight3D
        );

        let mut session = lighting_session().unwrap();
        let mut observed = Vec::new();
        for time in [0.0, 0.5, 1.0] {
            session.advance_to(time).unwrap();
            let mesh_row = session
                .frame()
                .objects
                .iter()
                .find(|row| {
                    row.world_transform()
                        .is_some_and(|world| world.translation.z.abs() < 0.6)
                })
                .unwrap();
            let light_row = session
                .frame()
                .objects
                .iter()
                .find(|row| {
                    row.spatial
                        .as_deref()
                        .is_some_and(|state| state.point_light)
                })
                .unwrap();
            observed.push((
                mesh_row.world_transform().unwrap(),
                light_row.world_transform().unwrap(),
            ));
        }
        assert!(observed.iter().all(|(mesh, _)| *mesh == observed[0].0));
        assert_eq!(observed[0].1.translation, SemanticVec3::new(4.0, -3.0, 6.0));
        assert_eq!(observed[1].1.translation, SemanticVec3::new(0.5, 0.0, 5.0));
        assert_eq!(observed[2].1.translation, SemanticVec3::new(-3.0, 3.0, 4.0));
    }

    #[test]
    fn cairo_surface_fixture_retains_bounded_cells_and_default_appearance() {
        let (scene, surface) = cairo_scene().unwrap();
        let leaves = scene
            .integration_store()
            .borrow()
            .ordered_leaf_nodes(surface.family().node_id())
            .unwrap();
        assert_eq!(leaves.len(), 64);
        let store = scene.integration_store().borrow();
        for (index, leaf) in leaves.iter().enumerate() {
            let state = store.semantic_object_state_checked(*leaf).unwrap();
            let uv_cell = [index / 8, index % 8];
            assert_eq!(state.surface_uv_cell(), Some(uv_cell));
            assert_eq!(
                state.spatial_material(),
                SemanticSpatialMaterial::CairoSurface
            );
            assert_eq!(state.style.stroke_width, 0.005);
            assert_eq!(state.style.stroke_opacity, 1.0);
            let [u, v] = uv_cell;
            assert_eq!(
                state.style.fill,
                Some(SemanticPaint::Solid(if (u + v) % 2 == 0 {
                    Color::BLUE_D
                } else {
                    Color::BLUE_E
                }))
            );
            let handle = state
                .content
                .geometry()
                .and_then(|geometry| geometry.resource_handle())
                .unwrap();
            let Some(GeometryResource::Mesh(mesh)) = store.geometry_resources().get(handle) else {
                panic!("Cairo Surface family retains mesh cells");
            };
            assert!(mesh.cairo_appearance().is_some());
            assert_eq!(mesh.positions().len(), 4);
            assert_eq!(mesh.indices(), [0, 1, 3, 1, 2, 3]);
        }
    }

    #[test]
    fn cairo_surface_session_is_static_at_first_and_last_30_fps_samples() {
        let mut session = cairo_session().unwrap();
        session.advance_to(0.0).unwrap();
        let mut first = session.frame().clone();
        assert_eq!(first.objects.len(), 65);
        session.advance_to(29.0 / 30.0).unwrap();
        let last = session.frame().clone();
        first.time = last.time;
        assert_eq!(first, last);
        assert_eq!(session.effective_time(), 29.0 / 30.0);
        session.advance_to(1.0).unwrap();
        first.time = 1.0;
        assert_eq!(session.frame(), &first);
    }
}
