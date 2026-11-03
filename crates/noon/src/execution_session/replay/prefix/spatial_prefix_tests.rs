use crate::{
    example_scenes::{spatial_camera_labels, spatial_surface},
    AnimationOptions, Color, MeshOptions, MobjectTarget, SemanticPaint,
    SemanticSpatialCompositionDomain, SemanticSpatialMaterial, SemanticStyle, SurfaceSample,
    UvSurfacePlan,
};
use noon_core::{
    NativeInputModifiers, NativePointerId, NativePointerInput, NativePointerInputKind,
    NativePointerPosition, SemanticRotation3D, SemanticVec3, SemanticWorldTransform3D, Vec2,
};
use noon_runtime::ReplayLimits;

const POINTER: NativePointerId = NativePointerId {
    source: 3,
    pointer: 5,
};

#[test]
fn prepared_prefix_admits_world_spatial_transform_history() {
    let (scene, surface, _light, light_movement) = spatial_surface::lighting_scene().unwrap();
    let surface_target = SemanticWorldTransform3D::new(
        SemanticVec3::new(0.2, -0.1, 0.0),
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.25).unwrap(),
        SemanticVec3::new(1.1, 0.9, 1.0),
    )
    .unwrap();
    let surface_movement = scene
        .declare_world_transform(
            &surface,
            surface_target,
            AnimationOptions::new()
                .run_time(0.5)
                .rate_func(crate::RateFunction::Linear),
        )
        .unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();

    let wait = source.wait_segment(0.25).unwrap();
    source.advance_to(wait.end_time()).unwrap();
    scene.live(&mut source).complete_segment(wait).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(wait).unwrap();
    let paused_frame = reader.frame().clone();
    let paused_publication = reader.publication_context();

    let transform = source
        .activate_animation_segment(
            &mut scene.integration_store().borrow_mut(),
            surface_movement.node_id(),
            AnimationOptions::new(),
        )
        .unwrap();
    source.advance_to(transform.end_time()).unwrap();
    scene.live(&mut source).complete_segment(transform).unwrap();

    let batch = source
        .publish_prepared_prefix(transform, &mut publisher)
        .expect("typed world-transform history should be supported");
    reader.preflight_append(&batch).unwrap();
    reader.append(batch).unwrap();
    assert_eq!(reader.frame(), &paused_frame, "adoption must not seek");
    assert_eq!(reader.publication_context(), paused_publication);

    let light_transform = source
        .activate_animation_segment(
            &mut scene.integration_store().borrow_mut(),
            light_movement.node_id(),
            AnimationOptions::new(),
        )
        .unwrap();
    source.advance_to(light_transform.end_time()).unwrap();
    scene
        .live(&mut source)
        .complete_segment(light_transform)
        .unwrap();
    let batch = source
        .publish_prepared_prefix(light_transform, &mut publisher)
        .expect("point-light world motion should share the spatial transform owner");
    reader.preflight_append(&batch).unwrap();
    reader.append(batch).unwrap();
    assert_eq!(reader.frame(), &paused_frame, "future suffix stays hidden");
    assert_eq!(reader.publication_context(), paused_publication);

    source.seal_replay().unwrap();
    let terminal = source
        .finish_prepared_prefix(light_transform, &mut publisher)
        .unwrap();
    reader.append(terminal).unwrap();
    assert_eq!(reader.frame(), &paused_frame, "sealing must not seek");

    source.configure_native_pointer_input(POINTER, 1).unwrap();
    reader.configure_native_pointer_input(POINTER, 1).unwrap();
    for (sequence, time) in [0.0, 0.5, 1.75, 0.9, 1.2].into_iter().enumerate() {
        source.seek(time).unwrap();
        reader.seek(time).unwrap();
        assert_eq!(source.frame().time, time, "source seek time");
        assert_eq!(reader.frame().time, time, "reader seek time");
        assert_eq!(reader.frame(), source.frame());
        assert_eq!(reader.session().camera_3d(), source.camera_3d());

        let viewport = crate::Rect::new(Vec2::new(-8.0, -8.0), Vec2::new(8.0, 8.0));
        let source_query = source.query_viewport(viewport);
        let reader_query = reader.query_viewport(viewport);
        assert_eq!(reader_query.object_indices(), source_query.object_indices());

        let source_token = source.native_pointer_input_token().unwrap();
        let reader_token = reader.native_pointer_input_token().unwrap();
        let pointer_input = |context| {
            NativePointerInput::new(
                sequence as u64,
                POINTER,
                context,
                NativeInputModifiers::default(),
                NativePointerInputKind::Press {
                    position: NativePointerPosition::new(Vec2::ZERO, Vec2::new(100.0, 100.0))
                        .unwrap(),
                    button: 0,
                },
            )
        };
        let source_pick = source
            .pick_native_pointer_fill(&source_token, pointer_input(source_token.context()), |_| {
                true
            })
            .unwrap();
        let reader_pick = reader
            .pick_native_pointer_fill(&reader_token, pointer_input(reader_token.context()), |_| {
                true
            })
            .unwrap();
        assert_eq!(reader_pick.outcome(), source_pick.outcome());
    }
}

#[test]
fn world_spatial_prefix_budget_failure_keeps_source_and_reader_unchanged() {
    let (scene, _surface, movement) = spatial_surface::scene().unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 1,
            payloads: 1,
        })
        .unwrap();

    let wait = source.wait_segment(0.25).unwrap();
    source.advance_to(wait.end_time()).unwrap();
    scene.live(&mut source).complete_segment(wait).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(wait).unwrap();
    let reader_frame = reader.frame().clone();
    let reader_publication = reader.publication_context();

    let transform = source
        .activate_animation_segment(
            &mut scene.integration_store().borrow_mut(),
            movement.node_id(),
            AnimationOptions::new(),
        )
        .unwrap();
    source.advance_to(transform.end_time()).unwrap();
    scene.live(&mut source).complete_segment(transform).unwrap();
    let source_frame = source.frame().clone();

    assert!(matches!(
        source.publish_prepared_prefix(transform, &mut publisher),
        Err(noon_runtime::PrefixError::Replay(
            noon_runtime::ReplayError::RetentionLimit
        ))
    ));
    assert_eq!(source.frame(), &source_frame);
    assert_eq!(reader.frame(), &reader_frame);
    assert_eq!(reader.publication_context(), reader_publication);
    reader.seek(0.1).unwrap();
    assert_eq!(reader.frame().time, 0.1);
}

#[test]
fn camera_light_and_surface_history_retains_fixed_orientation_and_fixed_frame_text() {
    let (mut scene, _camera, camera_movement) = spatial_camera_labels::scene().unwrap();
    let plan = UvSurfacePlan::new([-1.7, 1.7], [-1.25, 1.25], [8, 8]).unwrap();
    let mesh = crate::surface_mesh(plan, |u, v| {
        SurfaceSample::position(SemanticVec3::new(u, v, 0.28 * (u * u - v * v)))
    })
    .unwrap();
    let surface = scene
        .mesh(
            MeshOptions::new(mesh)
                .with_style(SemanticStyle {
                    fill: Some(SemanticPaint::Solid(Color::rgba(0.12, 0.58, 0.86, 1.0))),
                    fill_opacity: 1.0,
                    ..SemanticStyle::default()
                })
                .with_material(SemanticSpatialMaterial::PointLit),
        )
        .unwrap();
    let surface_target = crate::SemanticWorldTransform3D::new(
        SemanticVec3::new(0.2, -0.1, 0.0),
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.25).unwrap(),
        SemanticVec3::new(1.1, 0.9, 1.0),
    )
    .unwrap();
    let surface_movement = scene
        .declare_world_transform(
            &surface,
            surface_target,
            AnimationOptions::new()
                .run_time(0.4)
                .rate_func(crate::RateFunction::Linear),
        )
        .unwrap();
    scene
        .add_in_spatial_composition_domain(
            MobjectTarget::Object(&surface),
            SemanticSpatialCompositionDomain::World,
        )
        .unwrap();
    let light = scene
        .point_light_3d(SemanticVec3::new(3.8, -3.2, 5.0), Color::WHITE, 1.0)
        .unwrap();
    scene
        .add_in_spatial_composition_domain(
            MobjectTarget::Object(&light),
            SemanticSpatialCompositionDomain::World,
        )
        .unwrap();
    let light_target = crate::SemanticWorldTransform3D::new(
        SemanticVec3::new(-3.2, 2.8, 4.2),
        SemanticRotation3D::IDENTITY,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let light_movement = scene
        .declare_world_transform(
            &light,
            light_target,
            AnimationOptions::new()
                .run_time(0.5)
                .rate_func(crate::RateFunction::Linear),
        )
        .unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();

    let wait = source.wait_segment(0.25).unwrap();
    source.advance_to(wait.end_time()).unwrap();
    scene.live(&mut source).complete_segment(wait).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(wait).unwrap();
    let held_frame = reader.frame().clone();

    let camera = source
        .activate_animation_segment(
            &mut scene.integration_store().borrow_mut(),
            camera_movement.node_id(),
            AnimationOptions::new(),
        )
        .unwrap();
    source.advance_to(camera.end_time()).unwrap();
    scene.live(&mut source).complete_segment(camera).unwrap();
    let suffix = source
        .publish_prepared_prefix(camera, &mut publisher)
        .expect("camera history should retain fixed-orientation and fixed-frame text");
    reader.preflight_append(&suffix).unwrap();
    reader.append(suffix).unwrap();
    assert_eq!(reader.frame(), &held_frame, "suffix adoption does not seek");

    let light = source
        .activate_animation_segment(
            &mut scene.integration_store().borrow_mut(),
            light_movement.node_id(),
            AnimationOptions::new(),
        )
        .unwrap();
    source.advance_to(light.end_time()).unwrap();
    scene.live(&mut source).complete_segment(light).unwrap();
    let suffix = source
        .publish_prepared_prefix(light, &mut publisher)
        .expect("point-light motion should share the world-transform history owner");
    reader.preflight_append(&suffix).unwrap();
    reader.append(suffix).unwrap();
    assert_eq!(
        reader.frame(),
        &held_frame,
        "future light motion stays hidden"
    );
    let surface = source
        .activate_animation_segment(
            &mut scene.integration_store().borrow_mut(),
            surface_movement.node_id(),
            AnimationOptions::new(),
        )
        .unwrap();
    source.advance_to(surface.end_time()).unwrap();
    scene.live(&mut source).complete_segment(surface).unwrap();
    let suffix = source
        .publish_prepared_prefix(surface, &mut publisher)
        .expect("surface motion should retain alongside camera and point-light tracks");
    reader.preflight_append(&suffix).unwrap();
    reader.append(suffix).unwrap();
    assert_eq!(
        reader.frame(),
        &held_frame,
        "future surface motion stays hidden"
    );
    source.seal_replay().unwrap();
    let terminal = source
        .finish_prepared_prefix(surface, &mut publisher)
        .unwrap();
    reader.append(terminal).unwrap();

    for time in [0.0, 0.25, 0.75, 1.25, 1.5, 1.75, 2.0, 0.5] {
        source.seek(time).unwrap();
        reader.seek(time).unwrap();
        assert_eq!(reader.frame(), source.frame());
    }
}
