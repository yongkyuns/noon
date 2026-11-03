use crate::{example_scenes::spatial_surface, AnimationOptions};
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
