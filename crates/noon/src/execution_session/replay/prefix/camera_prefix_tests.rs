use crate::{example_scenes::spatial_three_d_axes, AnimationOptions, Rect};
use noon_core::Vec2;
use noon_runtime::ReplayLimits;

fn camera_profile(
    session: &crate::ExecutionSession,
) -> Option<(
    noon_core::ManimCamera3DProfile,
    noon_core::SemanticProjection3D,
)> {
    session.frame().objects.iter().find_map(|object| {
        let spatial = object.spatial.as_deref()?;
        Some((spatial.camera_profile?, spatial.camera_projection?))
    })
}

#[test]
fn prepared_prefix_admits_authored_camera_profile_history() {
    let (scene, _, _, _, movement) = spatial_three_d_axes::scene().unwrap();
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

    let camera_move = source
        .activate_animation_segment(
            &mut scene.integration_store().borrow_mut(),
            movement.node_id(),
            AnimationOptions::new(),
        )
        .unwrap();
    source.advance_to(camera_move.end_time()).unwrap();
    scene
        .live(&mut source)
        .complete_segment(camera_move)
        .unwrap();

    let batch = source
        .publish_prepared_prefix(camera_move, &mut publisher)
        .expect("typed camera-profile history should be supported");
    reader.preflight_append(&batch).unwrap();
    reader.append(batch).unwrap();
    assert_eq!(
        reader.frame(),
        &paused_frame,
        "adopting a camera track must not seek"
    );
    assert_eq!(reader.publication_context(), paused_publication);

    let bounds = Rect::new(Vec2::new(-4.0, -4.0), Vec2::new(4.0, 4.0));
    for time in [0.25, 0.75, f64::from_bits(1.25_f64.to_bits() - 1)] {
        source.seek(time).unwrap();
        reader.seek(time).unwrap();
        assert_eq!(
            reader.session().camera_3d().unwrap(),
            source.camera_3d().unwrap()
        );
        assert_eq!(camera_profile(reader.session()), camera_profile(&source));
        let source_query = source.query_viewport(bounds);
        let reader_query = reader.query_viewport(bounds);
        assert_eq!(reader_query.object_indices(), source_query.object_indices());
    }

    source.seek(camera_move.end_time()).unwrap();
    source.seal_replay().unwrap();
    let terminal = source
        .finish_prepared_prefix(camera_move, &mut publisher)
        .unwrap();
    reader.append(terminal).unwrap();
    source.seek(camera_move.end_time()).unwrap();
    reader.seek(camera_move.end_time()).unwrap();
    assert_eq!(
        reader.session().camera_3d().unwrap(),
        source.camera_3d().unwrap()
    );
    assert_eq!(camera_profile(reader.session()), camera_profile(&source));
    source.seek(0.75).unwrap();
    reader.seek(0.75).unwrap();
    assert_eq!(
        reader.session().camera_3d().unwrap(),
        source.camera_3d().unwrap()
    );
    assert_eq!(camera_profile(reader.session()), camera_profile(&source));
}

#[test]
fn camera_history_budget_failure_preserves_source_and_adopted_reader() {
    let (scene, _, _, _, movement) = spatial_three_d_axes::scene().unwrap();
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

    let camera_move = source
        .activate_animation_segment(
            &mut scene.integration_store().borrow_mut(),
            movement.node_id(),
            AnimationOptions::new(),
        )
        .unwrap();
    source.advance_to(camera_move.end_time()).unwrap();
    scene
        .live(&mut source)
        .complete_segment(camera_move)
        .unwrap();
    let source_camera = source.camera_3d().unwrap();

    assert!(matches!(
        source.publish_prepared_prefix(camera_move, &mut publisher),
        Err(noon_runtime::PrefixError::Replay(
            noon_runtime::ReplayError::RetentionLimit
        ))
    ));
    assert_eq!(source.camera_3d().unwrap(), source_camera);
    assert_eq!(reader.frame(), &reader_frame);
    assert_eq!(reader.publication_context(), reader_publication);
    reader.seek(0.1).unwrap();
    assert_eq!(reader.frame().time, 0.1);
}
