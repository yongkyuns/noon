use super::*;
use crate::WorldAffineEdit;
use noon_core::{AnimationOptions, RateFunction, SemanticProjection3D, SemanticRotation3D};
use noon_geometry::{SurfaceSample, UvSurfacePlan};

fn camera() -> SemanticCamera3D {
    SemanticCamera3D::new(
        SemanticVec3::new(0.0, 0.0, 5.0),
        SemanticRotation3D::IDENTITY,
        SemanticProjection3D::Perspective {
            vertical_fov_radians: 1.0,
            near: 0.1,
            far: 100.0,
        },
    )
    .unwrap()
}

fn cube() -> MeshOptions {
    MeshOptions::new(noon_geometry::cube_mesh(1.0).unwrap())
}

fn offset_mesh() -> MeshOptions {
    let positions = vec![
        SemanticVec3::new(10.0, 0.0, 0.0),
        SemanticVec3::new(12.0, 0.0, 0.0),
        SemanticVec3::new(10.0, 2.0, 2.0),
        SemanticVec3::new(12.0, 2.0, 2.0),
    ];
    MeshOptions::new(MeshResource::new(positions, None, vec![0, 1, 2, 1, 3, 2]).unwrap())
}

fn flat_grid() -> SurfaceGrid {
    let plan = UvSurfacePlan::new([0.0, 1.0], [0.0, 1.0], [2, 2]).unwrap();
    let samples = plan
        .coordinates()
        .map(|(u, v)| SurfaceSample::position(SemanticVec3::new(u, v, 0.0)))
        .collect::<Vec<_>>();
    plan.finish_samples(samples).unwrap()
}

#[test]
fn surface_family_retains_uv_roles_and_applies_defaults_and_atomic_checkerboard() {
    let mut scene = Scene::new();
    let surface = scene
        .surface_family(flat_grid(), SurfaceOptions::default())
        .unwrap();
    let store = std::rc::Rc::clone(scene.integration_store());
    let leaves = store
        .borrow()
        .ordered_leaf_nodes(surface.family().node_id())
        .unwrap();
    let colors = leaves
        .iter()
        .enumerate()
        .map(|(index, leaf)| {
            let borrowed = store.borrow();
            let state = borrowed.semantic_object_state_checked(*leaf).unwrap();
            assert_eq!(state.surface_uv_cell(), Some([index / 2, index % 2]));
            assert_eq!(state.style.stroke_width, 0.005);
            assert_eq!(
                state.style.stroke_width_mode,
                noon_core::StrokeWidthMode::ScreenSpace
            );
            assert_eq!(state.spatial_material(), SemanticSpatialMaterial::PointLit);
            match state.style.fill.as_ref().unwrap() {
                SemanticPaint::Solid(color) => *color,
                _ => panic!("solid cell fill"),
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        colors,
        [Color::BLUE_D, Color::BLUE_E, Color::BLUE_E, Color::BLUE_D]
    );

    scene
        .set_surface_checkerboard(&surface, [Color::RED, Color::GREEN], 0.4)
        .unwrap();
    let changed = leaves
        .iter()
        .map(|leaf| {
            store
                .borrow()
                .semantic_object_state_checked(*leaf)
                .unwrap()
                .clone()
        })
        .collect::<Vec<_>>();
    assert_eq!(changed[0].style.fill_opacity, 0.4);
    assert_eq!(changed[1].style.fill_opacity, 0.4);
    assert_eq!(
        changed[0].style.fill,
        Some(SemanticPaint::Solid(Color::RED))
    );
    assert_eq!(
        changed[1].style.fill,
        Some(SemanticPaint::Solid(Color::GREEN))
    );
    let before = changed;
    let revision = scene.revision();
    assert!(scene
        .set_surface_checkerboard(&surface, [Color::BLUE, Color::RED], f64::NAN)
        .is_err());
    assert_eq!(scene.revision(), revision);
    for (leaf, state) in leaves.iter().zip(before) {
        assert_eq!(
            store.borrow().semantic_object_state_checked(*leaf).unwrap(),
            &state
        );
    }
}

#[test]
fn surface_family_unlit_is_explicit_and_invalid_members_fail_atomically() {
    let mut scene = Scene::new();
    let surface = scene
        .surface_family(
            flat_grid(),
            SurfaceOptions {
                point_lit: false,
                ..SurfaceOptions::default()
            },
        )
        .unwrap();
    let leaves = scene
        .integration_store()
        .borrow()
        .ordered_leaf_nodes(surface.family().node_id())
        .unwrap();
    assert!(leaves.iter().all(|leaf| {
        scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(*leaf)
            .unwrap()
            .spatial_material()
            == SemanticSpatialMaterial::Unlit
    }));
    assert!(SurfaceFamily::from_family(surface.family().clone()).is_ok());

    // The wrapper carries no shadow membership list: a later family edit is
    // observed and checked against each current leaf before any style changes.
    let ordinary = scene.circle(0.25).unwrap();
    surface.family().add((&ordinary).into()).unwrap();
    let before = leaves
        .iter()
        .map(|leaf| {
            scene
                .integration_store()
                .borrow()
                .semantic_object_state_checked(*leaf)
                .unwrap()
                .clone()
        })
        .collect::<Vec<_>>();
    let revision = scene.revision();
    assert_eq!(
        scene.set_surface_checkerboard(&surface, [Color::RED, Color::GREEN], 0.5),
        Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::SurfaceCellRole
        ))
    );
    assert_eq!(scene.revision(), revision);
    for (leaf, expected) in leaves.iter().zip(before) {
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_object_state_checked(*leaf)
                .unwrap(),
            &expected
        );
    }

    let impostor = scene.mesh(cube().with_surface_uv_cell([0, 0])).unwrap();
    let impostor_family = scene.family(&[(&impostor).into()]).unwrap();
    assert!(matches!(
        SurfaceFamily::from_family(impostor_family),
        Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::SurfaceCellRole
        ))
    ));
}

fn assert_vec3_near(actual: SemanticVec3, expected: SemanticVec3) {
    assert!(
        (actual.x - expected.x).abs() < 1e-12,
        "{actual:?} != {expected:?}"
    );
    assert!(
        (actual.y - expected.y).abs() < 1e-12,
        "{actual:?} != {expected:?}"
    );
    assert!(
        (actual.z - expected.z).abs() < 1e-12,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn world_center_uses_transformed_local_bounds_for_offset_meshes() {
    let mut scene = Scene::new();
    let object = scene.mesh(offset_mesh()).unwrap();
    let world = SemanticWorldTransform3D::new(
        SemanticVec3::new(1.0, 2.0, 3.0),
        SemanticRotation3D::from_axis_angle(
            SemanticVec3::new(0.0, 0.0, 1.0),
            std::f64::consts::FRAC_PI_2,
        )
        .unwrap(),
        SemanticVec3::new(2.0, 1.0, 1.0),
    )
    .unwrap();
    scene.set_world_transform(&object, world).unwrap();
    scene.add(&object).unwrap();
    assert_vec3_near(
        object.world_center().unwrap(),
        SemanticVec3::new(0.0, 24.0, 4.0),
    );

    // Effective track sampling must win over the unchanged authored pose.
    let mut endpoint = world;
    endpoint.translation = SemanticVec3::new(-3.0, 5.0, 7.0);
    let animation = scene
        .declare_world_transform(
            &object,
            endpoint,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let root = transaction.create_animation_composition(
        noon_core::SemanticAnimationCompositionKind::Parallel,
        [animation.node_id()],
        AnimationOptions::new(),
    );
    let root = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap()
        .resolve(root)
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .activate_animation_segment(
            &scene.integration_store().borrow(),
            root,
            AnimationOptions::new(),
        )
        .unwrap();
    session.advance_to(1.0).unwrap();
    scene.install_execution(session);
    assert_vec3_near(
        scene.effective_world_center(&object).unwrap(),
        SemanticVec3::new(-2.0, 25.5, 6.0),
    );
    assert_eq!(object.world_transform().unwrap(), world);
}

#[test]
fn world_center_handles_native_cylinder_camera_light_and_detached_live_mesh() {
    let mut scene = Scene::new();
    let camera = scene.camera_3d(camera()).unwrap();
    assert_eq!(
        camera.world_center().unwrap(),
        SemanticVec3::new(0.0, 0.0, 5.0)
    );
    let light = scene
        .point_light_3d(SemanticVec3::new(1.0, -2.0, 3.0), Color::WHITE, 0.5)
        .unwrap();
    assert_eq!(
        light.world_center().unwrap(),
        SemanticVec3::new(1.0, -2.0, 3.0)
    );

    let cylinder = scene
        .mesh(MeshOptions::new(
            noon_geometry::cylinder_mesh(1.0, 4.0, 16).unwrap(),
        ))
        .unwrap();
    assert_eq!(
        cylinder.world_center().unwrap(),
        SemanticVec3::new(0.0, 0.0, 2.0)
    );

    scene.install_execution(scene.execution_session().unwrap());
    let detached = scene.mesh(offset_mesh()).unwrap();
    assert_eq!(
        scene.effective_world_center(&detached).unwrap(),
        SemanticVec3::new(11.0, 1.0, 1.0)
    );
    scene
        .world_affine(
            (&detached).into(),
            WorldAffineEdit::Shift(SemanticVec3::new(2.0, 0.0, -1.0)),
        )
        .unwrap();
    assert_vec3_near(
        scene.effective_world_center(&detached).unwrap(),
        SemanticVec3::new(13.0, 1.0, 0.0),
    );
    scene
        .world_affine(
            (&detached).into(),
            WorldAffineEdit::Rotate {
                axis: SemanticVec3::new(0.0, 0.0, 1.0),
                radians: std::f64::consts::FRAC_PI_2,
                about: None,
            },
        )
        .unwrap();
    assert_vec3_near(
        scene.effective_world_center(&detached).unwrap(),
        SemanticVec3::new(13.0, 1.0, 0.0),
    );
}

#[test]
fn world_center_rejects_foreign_and_stale_handles() {
    let mut first = Scene::new();
    let second = Scene::new();
    let object = first.mesh(offset_mesh()).unwrap();
    assert!(matches!(
        second.effective_world_center(&object),
        Err(AuthoringError::ForeignStore)
    ));
    let stale = object.clone();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_node(object.node_id());
    transaction
        .apply(&mut first.integration_store().borrow_mut())
        .unwrap();
    assert!(stale.world_center().is_err());

    let mut running = Scene::new();
    let detached = running.mesh(offset_mesh()).unwrap();
    running.install_execution(running.execution_session().unwrap());
    let mut external = SemanticMutationTransaction::new();
    external.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 0.5 },
    )));
    external
        .apply(&mut running.integration_store().borrow_mut())
        .unwrap();
    assert!(running.effective_world_center(&detached).is_err());
}

#[test]
fn mesh_creation_motion_and_residency_use_ordinary_scene_publication() {
    let mut scene = Scene::new();
    scene.camera_3d(camera()).unwrap();
    let object = scene.mesh(cube()).unwrap();
    scene.add(&object).unwrap();
    let state = object.state().unwrap();
    let handle = state.content.geometry().unwrap().resource_handle().unwrap();
    let initial_resource = match scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .get(handle)
        .unwrap()
    {
        GeometryResource::Mesh(mesh) => Arc::clone(mesh),
        _ => panic!("mesh resource"),
    };
    let mut target = SemanticWorldTransform3D::IDENTITY;
    target.translation = SemanticVec3::new(2.0, -1.0, 3.0);
    target.rotation =
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.8).unwrap();
    let animation = scene
        .declare_world_transform(
            &object,
            target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let root = transaction.create_animation_composition(
        noon_core::SemanticAnimationCompositionKind::Parallel,
        [animation.node_id()],
        AnimationOptions::new(),
    );
    let root = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap()
        .resolve(root)
        .unwrap();
    let animation = DeclaredAnimation::new(Rc::clone(scene.integration_store()), root);
    let revision = scene.revision();
    let activate = || {
        let mut session = scene.execution_session().unwrap();
        session
            .activate_animation_segment(
                &scene.integration_store().borrow(),
                animation.node_id(),
                AnimationOptions::new(),
            )
            .unwrap();
        session
    };
    let mut forward = activate();
    let mut seek = activate();
    for time in [0.0, 0.5, 1.5, 2.0] {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        assert_eq!(forward.frame(), seek.frame());
        assert_eq!(scene.revision(), revision);
        assert_eq!(object.state().unwrap(), state);
    }
    assert_eq!(
        forward
            .frame()
            .objects
            .iter()
            .find(|row| row.camera_projection().is_none())
            .unwrap()
            .world_transform()
            .unwrap(),
        target
    );
    let resource = scene.integration_store().borrow();
    let GeometryResource::Mesh(retained) = resource.geometry_resources().get(handle).unwrap()
    else {
        panic!("mesh resource")
    };
    assert!(Arc::ptr_eq(&initial_resource, retained));
}

#[test]
fn invalid_face_batch_rolls_back_all_new_resources_and_nodes() {
    let mut scene = Scene::new();
    scene.camera_3d(camera()).unwrap();
    let existing = scene.mesh(cube()).unwrap();
    let before = existing.state().unwrap();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    let mut invalid = cube();
    invalid.transform.translation.x = f64::NAN;
    assert!(scene.mesh_family(vec![cube(), invalid]).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(existing.state().unwrap(), before);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        resources
    );
}

#[test]
fn live_mesh_creation_and_local_world_edit_preserve_existing_slots() {
    let mut scene = Scene::new();
    scene.camera_3d(camera()).unwrap();
    let object = scene.mesh(cube()).unwrap();
    scene.add(&object).unwrap();
    scene.install_execution(scene.execution_session().unwrap());
    let original_id = scene
        .owned_execution()
        .execution_object_id(object.node_id())
        .unwrap();
    let initial_pose =
        SemanticWorldTransform3D::from_axial_direction(SemanticVec3::new(1.0, 2.0, 3.0), -1.0)
            .unwrap();
    let detached = scene
        .mesh(
            MeshOptions::new(noon_geometry::cylinder_mesh(0.3, 2.0, 16).unwrap())
                .with_transform(initial_pose),
        )
        .unwrap();
    assert_eq!(scene.owned_execution().frame().objects.len(), 2);
    scene.add(&detached).unwrap();
    assert_eq!(scene.owned_execution().frame().objects.len(), 3);
    assert_eq!(detached.world_transform().unwrap(), initial_pose);
    let state = detached.state().unwrap();
    let handle = state.content.geometry().unwrap().resource_handle().unwrap();
    let retained = match scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .get(handle)
        .unwrap()
    {
        GeometryResource::Mesh(mesh) => Arc::clone(mesh),
        _ => panic!("cylinder mesh"),
    };
    let resources_before = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    let mut target =
        SemanticWorldTransform3D::from_axial_direction(SemanticVec3::new(-1.0, 2.0, -3.0), -1.0)
            .unwrap();
    target.translation.z = -2.0;
    scene.set_world_transform(&detached, target).unwrap();
    assert_eq!(detached.world_transform().unwrap(), target);
    let borrowed = scene.integration_store().borrow();
    assert_eq!(borrowed.geometry_resources().stats(), resources_before);
    let GeometryResource::Mesh(after) = borrowed.geometry_resources().get(handle).unwrap() else {
        panic!("cylinder mesh")
    };
    assert!(Arc::ptr_eq(&retained, after));
    assert_eq!(
        scene
            .owned_execution()
            .execution_object_id(object.node_id())
            .unwrap(),
        original_id
    );
    assert_eq!(
        object.world_transform().unwrap(),
        SemanticWorldTransform3D::IDENTITY
    );
}

#[test]
fn camera_initialization_and_light_validation_are_atomic() {
    let mut scene = Scene::new();
    let revision = scene.revision();
    let mut invalid = camera();
    invalid.position.x = f64::NAN;
    assert!(scene.camera_3d(invalid).is_err());
    assert_eq!(scene.revision(), revision);
    let camera = scene.camera_3d(camera()).unwrap();
    assert!(scene.camera_3d(super::tests::camera()).is_err());
    let revision = scene.revision();
    assert!(scene
        .point_light_3d(SemanticVec3::ZERO, Color::WHITE, f64::NAN)
        .is_err());
    assert_eq!(scene.revision(), revision);
    let light = scene
        .point_light_3d(SemanticVec3::new(4.0, -3.0, 6.0), Color::WHITE, 0.5)
        .unwrap();
    scene.add(&light).unwrap();
    let execution = scene.execution_session().unwrap();
    assert_eq!(camera.state().unwrap().role(), SemanticObjectRole::Camera3D);
    assert_eq!(
        light.state().unwrap().role(),
        SemanticObjectRole::PointLight3D
    );
    assert_eq!(
        execution
            .frame()
            .objects
            .iter()
            .filter(|row| row
                .spatial
                .as_ref()
                .is_some_and(|spatial| spatial.point_light))
            .count(),
        1
    );
}

#[test]
fn unsupported_mesh_fade_is_rejected_before_activation_but_light_opacity_remains_live() {
    use crate::{FadeEndpoint, FadeTranslation};
    use noon_core::SemanticFadeDirection;

    for initial_opacity in [1.0, 0.5] {
        let mut scene = Scene::new();
        let mut options = cube();
        options.style.fill_opacity = initial_opacity;
        let mesh = scene.mesh(options).unwrap();
        scene.add(&mesh).unwrap();
        let state_before = mesh.state().unwrap();
        let revision_before = scene.revision();
        let handle = state_before
            .content
            .geometry()
            .unwrap()
            .resource_handle()
            .unwrap();
        let resources_before = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats();
        let mut execution = scene.execution_session().unwrap();

        let activation = {
            let mut live = scene.live(&mut execution);
            live.declare_and_activate_fade_with_endpoint(
                &mesh,
                SemanticFadeDirection::Out,
                FadeEndpoint::new(0.8, FadeTranslation::Shift(SemanticVec3::ZERO)),
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
        };
        assert!(activation.is_err(), "mesh fade must fail at activation");
        assert_eq!(scene.revision(), revision_before);
        assert_eq!(mesh.state().unwrap(), state_before);
        assert_eq!(
            execution_world_transform(&execution, mesh.node_id()),
            SemanticWorldTransform3D::IDENTITY
        );
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .stats(),
            resources_before
        );
        assert!(scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .get(handle)
            .is_some());
    }

    let mut scene = Scene::new();
    let light = scene
        .point_light_3d(SemanticVec3::new(1.0, 2.0, 3.0), Color::WHITE, 0.5)
        .unwrap();
    scene.add(&light).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        light.node_id(),
        noon_core::SemanticObjectTrackProperty::Opacity,
        noon_core::SemanticObjectTrackValues::Scalar { from: 0.5, to: 1.0 },
        noon_core::TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        noon_core::CompositionTimeMap::identity(),
    );
    let applied = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let track = applied.resolve(track).unwrap();
    let animation = scene
        .declare_animation(
            noon_core::SemanticAnimationIntent::Composition {
                kind: noon_core::SemanticAnimationCompositionKind::Parallel,
                children: vec![track],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let mut execution = scene
        .execution_session_with_animation_root(&animation)
        .unwrap();
    execution.seek(1.0).unwrap();
    let light_row = execution
        .frame()
        .objects
        .iter()
        .find(|row| row.spatial.as_ref().is_some_and(|state| state.point_light))
        .unwrap();
    assert!(light_row.spatial.as_ref().unwrap().point_light);
    assert_eq!(light_row.style.opacity, 0.75);
}

#[test]
fn live_world_compositions_capture_effective_pose_and_complete_authored_endpoint() {
    let mut scene = Scene::new();
    let camera = scene.camera_3d(camera()).unwrap();
    let mesh = scene.mesh(cube()).unwrap();
    scene.add(&mesh).unwrap();
    let authored_mesh = mesh.state().unwrap();
    let mut execution = scene.execution_session().unwrap();
    let first = SemanticWorldTransform3D::new(
        SemanticVec3::new(2.0, 0.0, 0.0),
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.6).unwrap(),
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let second = SemanticWorldTransform3D::new(
        SemanticVec3::new(4.0, 1.0, -2.0),
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 1.2).unwrap(),
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let sequential = crate::AnimationCompositionRequest::Composition {
        kind: noon_core::SemanticAnimationCompositionKind::Sequence,
        children: vec![
            crate::AnimationCompositionRequest::WorldTransform {
                target: &mesh,
                transform: first,
                options,
            },
            crate::AnimationCompositionRequest::WorldTransform {
                target: &mesh,
                transform: second,
                options,
            },
        ],
        options: AnimationOptions::new(),
    };
    let segment;
    {
        let mut live =
            crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        segment = live
            .declare_and_activate_composition(&sequential, AnimationOptions::new())
            .unwrap();
        live.advance_segment_to(segment, 0.5).unwrap();
    }
    assert_world_near(
        execution_world_transform(&execution, mesh.node_id()),
        SemanticWorldTransform3D::IDENTITY
            .interpolate(first, 0.5)
            .unwrap(),
    );
    {
        let mut live =
            crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        live.advance_segment_to(segment, 1.5).unwrap();
    }
    assert_world_near(
        execution_world_transform(&execution, mesh.node_id()),
        first.interpolate(second, 0.5).unwrap(),
    );
    assert_eq!(mesh.state().unwrap(), authored_mesh);
    {
        let mut live =
            crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
    }
    assert_eq!(mesh.world_transform().unwrap(), second);

    let camera_target = SemanticWorldTransform3D::new(
        SemanticVec3::new(0.5, -0.25, 5.0),
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 0.0, 1.0), 0.2).unwrap(),
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let mesh_target = SemanticWorldTransform3D::new(
        SemanticVec3::new(-1.0, 0.25, 1.5),
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(1.0, 0.0, 0.0), 0.3).unwrap(),
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let camera_start = camera.world_transform().unwrap();
    let parallel = crate::AnimationCompositionRequest::Composition {
        kind: noon_core::SemanticAnimationCompositionKind::Parallel,
        children: vec![
            crate::AnimationCompositionRequest::WorldTransform {
                target: &camera,
                transform: camera_target,
                options,
            },
            crate::AnimationCompositionRequest::WorldTransform {
                target: &mesh,
                transform: mesh_target,
                options,
            },
        ],
        options: AnimationOptions::new(),
    };
    let revision = scene.revision();
    let bad_camera = SemanticWorldTransform3D::new(
        camera_target.translation,
        camera_target.rotation,
        SemanticVec3::new(2.0, 1.0, 1.0),
    )
    .unwrap();
    let frame_before_invalid = execution.frame().clone();
    {
        let mut live =
            crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        let invalid = crate::AnimationCompositionRequest::WorldTransform {
            target: &camera,
            transform: bad_camera,
            options,
        };
        assert!(live
            .declare_and_activate_composition(&invalid, AnimationOptions::new())
            .is_err());
        assert_eq!(scene.revision(), revision);
    }
    assert_eq!(*execution.frame(), frame_before_invalid);
    let segment;
    {
        let mut live =
            crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        segment = live
            .declare_and_activate_composition(&parallel, AnimationOptions::new())
            .unwrap();
        live.advance_segment_to(segment, segment.start_time() + 0.5)
            .unwrap();
    }
    assert_world_near(
        execution_world_transform(&execution, camera.node_id()),
        camera_start.interpolate(camera_target, 0.5).unwrap(),
    );
    assert_world_near(
        execution_world_transform(&execution, mesh.node_id()),
        second.interpolate(mesh_target, 0.5).unwrap(),
    );
    {
        let mut live =
            crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
    }
    assert_eq!(camera.world_transform().unwrap(), camera_target);
    assert_eq!(mesh.world_transform().unwrap(), mesh_target);
}

fn execution_world_transform(
    execution: &crate::ExecutionSession,
    object: noon_core::SemanticNodeId,
) -> SemanticWorldTransform3D {
    let id = execution.execution_object_id(object).unwrap();
    execution
        .frame()
        .objects
        .iter()
        .find(|row| row.id == id)
        .unwrap()
        .world_transform()
        .unwrap()
}

fn assert_world_near(actual: SemanticWorldTransform3D, expected: SemanticWorldTransform3D) {
    for (actual, expected) in [
        (actual.translation.x, expected.translation.x),
        (actual.translation.y, expected.translation.y),
        (actual.translation.z, expected.translation.z),
        (actual.scale.x, expected.scale.x),
        (actual.scale.y, expected.scale.y),
        (actual.scale.z, expected.scale.z),
    ] {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }
    for (actual, expected) in actual
        .rotation
        .components()
        .into_iter()
        .zip(expected.rotation.components())
    {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }
}
