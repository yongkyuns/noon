use noon::{AnimationOptions, GeometryRef, RateFunction, Scene, StoredGeometry};

#[test]
fn shared_cross_kind_transform_publishes_target_content_without_replacing_identity() {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0).unwrap();
    let square = scene.square(2.0).unwrap();
    scene.add(&circle).unwrap();
    let mut session = scene.execution_session().unwrap();
    let id = session.frame().objects[0].id;
    assert!(matches!(
        session.frame().render_geometry(0),
        Some(GeometryRef::Circle { .. })
    ));
    let mut live = scene.live(&mut session);
    let segment = live
        .declare_and_activate_transform_to(
            &circle,
            &square,
            AnimationOptions::new()
                .run_time(1.5)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    live.advance_segment_to(segment, 0.75).unwrap();
    // Completion, not a second authoring model, publishes the target geometry.
    live.advance_segment_to(segment, 1.5).unwrap();
    live.complete_segment(segment).unwrap();
    assert!(matches!(
        live.authored(&circle).unwrap().content.geometry(),
        Some(StoredGeometry::Rectangle { .. })
    ));
    assert_eq!(session.frame().objects[0].id, id);
    assert!(matches!(
        session.frame().render_geometry(0),
        Some(GeometryRef::Rectangle { .. })
    ));
}
