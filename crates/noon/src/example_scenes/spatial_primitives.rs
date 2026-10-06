//! Shared direct and native/WASM qualification scene for public spatial primitives.

use crate::{
    AnimationOptions, Color, ExecutionSession, MeshOptions, MobjectTarget, RateFunction, Scene,
    SemanticCamera3D, SemanticProjection3D, SemanticSpatialCompositionDomain, SemanticVec3,
    SemanticWorldTransform3D, SurfaceOptions, WorldAffineEdit,
};
use noon_core::{
    CompositionTimeMap, ManimCamera3DProfile, SemanticAnimationCompositionKind,
    SemanticMutationTransaction, SemanticObjectTrackProperty, SemanticObjectTrackValues,
    SemanticPaint, SemanticRotation3D, SemanticSpatialMaterial, SemanticStyle, TrackTiming,
};

const DURATION: f64 = 1.0;
const NEAR: f64 = 0.1;
const FAR: f64 = 30.0;

/// A capped Line3D tube, explicit triangle, translucent Prism, and
/// open/closed Cylinder/Cone meshes plus a bounded partial Sphere patch.
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

    let mut prism = scene
        .prism_face_family(SemanticVec3::new(1.0, 0.7, 0.5), Color::BLUE, 0.75, false)
        .map_err(|error| error.to_string())?;
    let prism_center = SemanticVec3::new(1.6, -1.9, 0.25);
    // Keep the translucent Prism away from the opaque triangle and the axial
    // solids so the fixture isolates face-family alpha without intersections.
    prism
        .world_affine(WorldAffineEdit::Shift(prism_center))
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[MobjectTarget::Family(&prism)])
        .map_err(|error| error.to_string())?;

    for (geometry, direction, offset, placement, color) in [
        (
            crate::cylinder_mesh(0.2, 1.1, 16),
            SemanticVec3::new(1.0, 2.0, 1.0),
            -0.55,
            SemanticVec3::new(-1.2, 0.8, 0.0),
            Color::GREEN,
        ),
        (
            noon_geometry::cylinder_mesh_range(0.2, 1.1, 16, false, [0.0, std::f64::consts::TAU]),
            SemanticVec3::new(1.0, 2.0, 1.0),
            -0.55,
            SemanticVec3::new(-0.55, 0.8, 0.0),
            Color::TEAL,
        ),
        (
            crate::cone_mesh(0.25, 0.9, 16),
            SemanticVec3::new(-2.0, 1.0, -1.0),
            -0.9,
            SemanticVec3::new(0.85, 0.8, 0.0),
            Color::YELLOW,
        ),
        (
            noon_geometry::cone_mesh_range(0.25, 0.9, 16, false, [0.0, std::f64::consts::TAU]),
            SemanticVec3::new(-2.0, 1.0, -1.0),
            -0.9,
            SemanticVec3::new(1.75, 0.8, 0.0),
            Color::RED,
        ),
    ] {
        let mut pose = SemanticWorldTransform3D::from_axial_direction(direction, offset)
            .ok_or("invalid axial primitive pose")?;
        pose.translation.x += placement.x;
        pose.translation.y += placement.y;
        pose.translation.z += placement.z;
        let object = scene
            .mesh(
                MeshOptions::new(geometry.map_err(|error| error.to_string())?)
                    .with_transform(pose)
                    .with_style(opaque(color)),
            )
            .map_err(|error| error.to_string())?;
        scene.add(&object).map_err(|error| error.to_string())?;
    }

    let sphere_geometry = noon_geometry::sphere_mesh_range(
        0.35,
        [24, 12],
        [
            std::f64::consts::FRAC_PI_4,
            3.0 * std::f64::consts::FRAC_PI_4,
        ],
        [
            std::f64::consts::FRAC_PI_6,
            5.0 * std::f64::consts::FRAC_PI_6,
        ],
    )
    .map_err(|error| error.to_string())?;
    let sphere = scene
        .mesh(
            MeshOptions::new(sphere_geometry)
                .with_transform(world_transform(
                    SemanticVec3::new(0.0, 1.9, 0.0),
                    SemanticRotation3D::IDENTITY,
                ))
                .with_style(opaque(Color::PINK)),
        )
        .map_err(|error| error.to_string())?;
    scene.add(&sphere).map_err(|error| error.to_string())?;

    let triangle_rotation =
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.4)
            .ok_or("invalid triangle rotation")?;
    let mut transaction = SemanticMutationTransaction::new();
    let line_track = transaction.create_object_property_track(
        line.node_id(),
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform {
            from: world_transform(SemanticVec3::ZERO, SemanticRotation3D::IDENTITY),
            to: world_transform(
                SemanticVec3::new(0.0, 0.0, 0.25),
                SemanticRotation3D::IDENTITY,
            ),
        },
        TrackTiming::new(0.0, DURATION, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let mut tracks = vec![line_track];
    tracks.push(transaction.create_object_property_track(
        triangle.node_id(),
        SemanticObjectTrackProperty::WorldTransform,
        SemanticObjectTrackValues::WorldTransform {
            from: world_transform(SemanticVec3::ZERO, SemanticRotation3D::IDENTITY),
            to: world_transform(SemanticVec3::ZERO, triangle_rotation),
        },
        TrackTiming::new(0.0, DURATION, RateFunction::Linear),
        CompositionTimeMap::identity(),
    ));
    let animation_root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        tracks,
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

/// Default shaded, translucent Cube and Prism faces under finite camera motion.
pub fn cairo_cube_prism_session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let camera = scene
        .camera_3d_profile(
            ManimCamera3DProfile {
                phi: 0.6,
                theta: -1.2,
                gamma: 0.0,
                focal_distance: 5.0,
                zoom: 1.0,
                frame_height: 8.0,
                frame_center: SemanticVec3::ZERO,
            },
            NEAR,
            FAR,
        )
        .map_err(|error| error.to_string())?;

    let mut cube = scene
        .cube_face_family(1.0, Color::RED, 0.75, true)
        .map_err(|error| error.to_string())?;
    cube.world_affine(WorldAffineEdit::Shift(SemanticVec3::new(-1.2, 0.0, 0.0)))
        .map_err(|error| error.to_string())?;
    scene
        .add_in_spatial_composition_domain(
            MobjectTarget::Family(&cube),
            SemanticSpatialCompositionDomain::World,
        )
        .map_err(|error| error.to_string())?;

    let mut prism = scene
        .prism_face_family(SemanticVec3::new(1.0, 0.8, 0.6), Color::BLUE, 0.75, true)
        .map_err(|error| error.to_string())?;
    prism
        .world_affine(WorldAffineEdit::Shift(SemanticVec3::new(1.2, 0.0, 0.0)))
        .map_err(|error| error.to_string())?;
    scene
        .add_in_spatial_composition_domain(
            MobjectTarget::Family(&prism),
            SemanticSpatialCompositionDomain::World,
        )
        .map_err(|error| error.to_string())?;

    let movement = scene
        .declare_camera_profile_move(
            &camera,
            ManimCamera3DProfile {
                phi: 0.8,
                theta: -0.1,
                gamma: 0.2,
                focal_distance: 5.0,
                zoom: 1.1,
                frame_height: 8.0,
                frame_center: SemanticVec3::new(0.2, 0.0, 0.0),
            },
            AnimationOptions::new()
                .run_time(DURATION)
                .rate_func(RateFunction::Linear),
        )
        .map_err(|error| error.to_string())?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    session
        .activate_animation_segment(
            &scene.integration_store().borrow(),
            movement.node_id(),
            AnimationOptions::new(),
        )
        .map_err(|error| error.to_string())?;
    Ok(session)
}

/// Pinned default Cylinder caps and optional Cone base with retained Cairo surfaces.
pub fn cairo_cylinder_cone_caps_session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let camera = scene
        .camera_3d_profile(
            ManimCamera3DProfile {
                phi: 0.6,
                theta: -1.2,
                gamma: 0.0,
                focal_distance: 5.0,
                zoom: 1.0,
                frame_height: 8.0,
                frame_center: SemanticVec3::ZERO,
            },
            NEAR,
            FAR,
        )
        .map_err(|error| error.to_string())?;

    let default_cylinder = cairo_cylinder_family(
        &mut scene,
        1.0,
        2.0,
        [24, 24],
        SemanticVec3::new(0.0, 0.0, 1.0),
        SemanticVec3::new(-2.0, 0.0, 0.0),
        Color::BLUE_D,
        true,
        true,
    )?;
    let oriented_cylinder = cairo_cylinder_family(
        &mut scene,
        0.55,
        1.25,
        [8, 8],
        SemanticVec3::new(1.0, 2.0, 1.0),
        SemanticVec3::new(0.0, 0.0, 0.0),
        Color::TEAL,
        false,
        true,
    )?;
    let capped_cone = cairo_cone_family(
        &mut scene,
        0.55,
        1.3,
        [8, 8],
        SemanticVec3::new(-1.0, 2.0, -1.0),
        SemanticVec3::new(2.0, 0.0, 0.0),
    )?;
    for family in [&default_cylinder, &oriented_cylinder, &capped_cone] {
        scene
            .add_many(&[MobjectTarget::Family(family.family())])
            .map_err(|error| error.to_string())?;
    }

    let movement = scene
        .declare_camera_profile_move(
            &camera,
            ManimCamera3DProfile {
                phi: 0.8,
                theta: -0.1,
                gamma: 0.2,
                focal_distance: 5.0,
                zoom: 1.1,
                frame_height: 8.0,
                frame_center: SemanticVec3::new(0.2, 0.0, 0.0),
            },
            AnimationOptions::new()
                .run_time(DURATION)
                .rate_func(RateFunction::Linear),
        )
        .map_err(|error| error.to_string())?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    session
        .activate_animation_segment(
            &scene.integration_store().borrow(),
            movement.node_id(),
            AnimationOptions::new(),
        )
        .map_err(|error| error.to_string())?;
    Ok(session)
}

fn cairo_cylinder_family(
    scene: &mut Scene,
    radius: f64,
    height: f64,
    resolution: [usize; 2],
    direction: SemanticVec3,
    placement: SemanticVec3,
    color: Color,
    checkerboard: bool,
    shade_caps: bool,
) -> Result<crate::SurfaceFamily, String> {
    let half_height = height * 0.5;
    let plan = crate::UvSurfacePlan::new(
        [-half_height, half_height],
        [0.0, std::f64::consts::TAU],
        resolution,
    )
    .map_err(|error| error.to_string())?;
    let grid = plan
        .sample_cairo(|z, angle| SemanticVec3::new(radius * angle.cos(), radius * angle.sin(), z))
        .map_err(|error| error.to_string())?;
    let pose = SemanticWorldTransform3D::from_axial_direction(direction, 0.0)
        .ok_or("invalid Cylinder axis")?;
    let caps = [-half_height, half_height]
        .into_iter()
        .map(|z| {
            crate::SpatialPathOptions::circle(
                radius,
                SemanticWorldTransform3D::new(
                    SemanticVec3::new(0.0, 0.0, z),
                    SemanticRotation3D::IDENTITY,
                    SemanticVec3::new(1.0, 1.0, 1.0),
                )
                .expect("fixture cap transform is finite"),
                color,
                shade_caps,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let options = SurfaceOptions {
        fill_colors: if checkerboard {
            [Color::BLUE_D, Color::BLUE_E]
        } else {
            [color, color]
        },
        material: SemanticSpatialMaterial::CairoSurface,
        ..SurfaceOptions::default()
    };
    let mut family = scene
        .surface_cairo_family_with_paths(grid, options, caps)
        .map_err(|error| error.to_string())?;
    apply_axial_pose(&mut family, pose)?;
    family
        .world_affine(WorldAffineEdit::Shift(placement))
        .map_err(|error| error.to_string())?;
    Ok(family)
}

fn cairo_cone_family(
    scene: &mut Scene,
    base_radius: f64,
    height: f64,
    resolution: [usize; 2],
    direction: SemanticVec3,
    placement: SemanticVec3,
) -> Result<crate::SurfaceFamily, String> {
    let slant = base_radius.hypot(height);
    let theta = std::f64::consts::PI - (base_radius / height).atan();
    let plan = crate::UvSurfacePlan::new([0.0, slant], [0.0, std::f64::consts::TAU], resolution)
        .map_err(|error| error.to_string())?;
    let grid = plan
        .sample_cairo(|u, angle| {
            SemanticVec3::new(
                u * theta.sin() * angle.cos(),
                u * theta.sin() * angle.sin(),
                u * theta.cos(),
            )
        })
        .map_err(|error| error.to_string())?;
    let base = crate::SpatialPathOptions::circle(
        base_radius,
        SemanticWorldTransform3D::new(
            SemanticVec3::new(0.0, 0.0, -height),
            SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .expect("fixture cone base transform is finite"),
        Color::BLUE_D,
        false,
    )
    .map_err(|error| error.to_string())?;
    let mut family = scene
        .surface_cairo_family_with_paths(
            grid,
            SurfaceOptions {
                fill_colors: [Color::BLUE_D; 2],
                material: SemanticSpatialMaterial::CairoSurface,
                ..SurfaceOptions::default()
            },
            vec![base],
        )
        .map_err(|error| error.to_string())?;
    let pose = SemanticWorldTransform3D::from_axial_direction(direction, 0.0)
        .ok_or("invalid Cone axis")?;
    apply_axial_pose(&mut family, pose)?;
    family
        .world_affine(WorldAffineEdit::Shift(placement))
        .map_err(|error| error.to_string())?;
    Ok(family)
}

fn apply_axial_pose(
    family: &mut crate::SurfaceFamily,
    pose: SemanticWorldTransform3D,
) -> Result<(), String> {
    let [w, x, y, z] = pose.rotation.components();
    let half_sine = (1.0 - w.clamp(-1.0, 1.0).powi(2)).sqrt();
    if half_sine > f64::EPSILON {
        let axis = SemanticVec3::new(x / half_sine, y / half_sine, z / half_sine);
        let radians = 2.0 * w.clamp(-1.0, 1.0).acos();
        family
            .world_affine(WorldAffineEdit::Rotate {
                axis,
                radians,
                about: Some(SemanticVec3::ZERO),
            })
            .map_err(|error| error.to_string())?;
    }
    Ok(())
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
    SemanticWorldTransform3D::new(translation, rotation, SemanticVec3::new(1.0, 1.0, 1.0))
        .expect("fixture transform is finite and invertible")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_line_and_explicit_triangle_share_the_native_timeline() {
        let mut session = session().unwrap();
        assert_eq!(session.frame().objects.len(), 14);
        assert_eq!(
            session
                .frame()
                .objects
                .iter()
                .filter(|row| row.style.fill
                    == Some(Color {
                        alpha: 0.75,
                        ..Color::BLUE
                    }))
                .count(),
            6
        );
        assert_eq!(
            session.wake_state().timeline(),
            noon_runtime::TimelineWakeState::Continuous
        );
        let rows = |session: &ExecutionSession| {
            let line = session
                .frame()
                .objects
                .iter()
                .find(|row| row.style.fill == Some(Color::RED))
                .unwrap();
            let triangle = session
                .frame()
                .objects
                .iter()
                .find(|row| row.style.fill == Some(Color::BLUE))
                .unwrap();
            (
                line.world_transform().unwrap(),
                triangle.world_transform().unwrap(),
            )
        };
        assert_eq!(rows(&session).0.translation, SemanticVec3::ZERO);
        assert_eq!(rows(&session).1.translation, SemanticVec3::ZERO);
        session.advance_to(0.5).unwrap();
        assert_eq!(rows(&session).0.translation.z, 0.125);
        assert!((rows(&session).1.rotation.components()[2] - (0.1_f64).sin()).abs() < 1.0e-12);
        session.advance_to(DURATION).unwrap();
        assert_eq!(rows(&session).0.translation.z, 0.25);
        assert!((rows(&session).1.rotation.components()[2] - 0.2_f64.sin()).abs() < 1.0e-12);
        let forward = session.frame().clone();
        session.seek(0.5).unwrap();
        session.seek(DURATION).unwrap();
        assert_eq!(session.frame(), &forward);
    }

    #[test]
    fn cairo_cylinder_ends_and_capped_cone_share_camera_timeline_and_leaf_count() {
        let mut session = cairo_cylinder_cone_caps_session().unwrap();
        assert_eq!(session.frame().objects.len(), 710);
        assert_eq!(
            session.wake_state().timeline(),
            noon_runtime::TimelineWakeState::Continuous
        );
        let initial = session.frame().clone();
        session.advance_to(0.5).unwrap();
        let midpoint = session.frame().clone();
        assert_ne!(midpoint, initial);
        session.advance_to(DURATION).unwrap();
        let endpoint = session.frame().clone();
        assert_ne!(endpoint, midpoint);
        session.seek(0.5).unwrap();
        assert_eq!(session.frame(), &midpoint);
        session.seek(0.0).unwrap();
        assert_eq!(session.frame(), &initial);
    }
}
