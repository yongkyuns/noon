use noon::{AnimationOptions, RateFunction, Scene, Vec2, VectorPath};

fn near(actual: (f64, f64), expected: (f64, f64)) {
    assert!(
        (actual.0 - expected.0).abs() < 1e-6 && (actual.1 - expected.1).abs() < 1e-6,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn nonuniform_world_measure_and_local_measure_are_distinct_immutable_observations() {
    let scene = Scene::new();
    let path = VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::new(1., 0.))
        .line_to(Vec2::new(1., 1.));
    let mut object = scene.path(path, Default::default()).unwrap();
    object.set_scale(3., 1.).unwrap();
    object.set_translation(10., -2.).unwrap();
    let revision = scene.revision();
    let content = object.state().unwrap().content;
    let local = object.local_path_query().unwrap();
    let world = object.path_query().unwrap();
    near(local.point_from_proportion(0.5).unwrap(), (1., 0.));
    near(world.point_from_proportion(0.5).unwrap(), (12., -2.));
    near(world.start().unwrap(), (10., -2.));
    near(world.end().unwrap(), (13., -1.));
    assert!((world.arc_length(None).unwrap() - 4.).abs() < 1e-6);
    assert!((local.arc_length(Some(25)).unwrap() - 2.).abs() < 1e-6);
    assert!(world.point_from_proportion(f64::NAN).is_err());
    assert!(world.point_from_proportion(1.1).is_err());
    assert!(world.arc_length(Some(1)).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(object.state().unwrap().content, content);
    object.shift(1., 0.).unwrap();
    near(world.start().unwrap(), (10., -2.));
    near(object.path_query().unwrap().start().unwrap(), (11., -2.));
}

#[test]
fn canonical_primitives_and_subpath_breaks_keep_their_path_order() {
    let scene = Scene::new();
    let rectangle = scene.rectangle(4., 2.).unwrap();
    let query = rectangle.path_query().unwrap();
    near(query.start().unwrap(), (2., 1.));
    near(query.point_from_proportion(0.25).unwrap(), (-1., 1.));
    near(query.end().unwrap(), (2., 1.));
    assert!((query.arc_length(None).unwrap() - 12.).abs() < 1e-6);
    let circle = scene.circle(1.).unwrap();
    near(
        circle
            .path_query()
            .unwrap()
            .point_from_proportion(0.125)
            .unwrap(),
        (2f64.sqrt() / 2., 2f64.sqrt() / 2.),
    );
    let object = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1., 0.))
                .move_to(Vec2::new(10., 0.))
                .line_to(Vec2::new(11., 0.)),
            Default::default(),
        )
        .unwrap();
    let query = object.path_query().unwrap();
    assert!((query.arc_length(None).unwrap() - 2.).abs() < 1e-6);
    near(query.point_from_proportion(0.75).unwrap(), (10.5, 0.));
}

#[test]
fn effective_queries_capture_one_live_affine_publication() {
    let mut scene = Scene::new();
    let object = scene.rectangle(2., 1.).unwrap();
    let mut target = object.target_editor().unwrap();
    target.set_translation(0., 2.).unwrap();
    scene.add(&object).unwrap();
    let animation = scene
        .declare_transform_to(
            &object,
            &target,
            AnimationOptions::new()
                .run_time(2.)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live.play_animation(&animation).unwrap();
    live.advance_segment_to(segment, 1.).unwrap();
    let query = live.effective_path_query(&object).unwrap();
    near(query.start().unwrap(), (1., 1.5));
    near(object.path_query().unwrap().start().unwrap(), (1., 0.5));
    live.advance_segment_to(segment, 2.).unwrap();
    near(query.start().unwrap(), (1., 1.5));
    near(
        live.effective_path_query(&object).unwrap().start().unwrap(),
        (1., 2.5),
    );
}

#[test]
fn current_queries_reject_active_content_overrides() {
    let scene = Scene::new();
    let object = scene.circle(1.).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    live.declare_and_activate_create(&object, AnimationOptions::new())
        .unwrap();
    assert!(live.effective_path_query(&object).is_err());
}

#[test]
fn paired_query_example_uses_the_normal_execution_session() {
    let session = noon::example_scenes::path_queries::session().unwrap();
    assert_eq!(session.frame().objects.len(), 10);
}
