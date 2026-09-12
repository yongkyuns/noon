use noon::{BooleanOperation as Op, ManimGeometryOptions, Scene};

#[test]
fn constructor_is_inert_uses_world_geometry_and_leaves_operands_unchanged() {
    let scene = Scene::new();
    let mut a = scene.square(2.).unwrap();
    let mut b = scene.square(2.).unwrap();
    a.shift(-1., 2.).unwrap();
    b.shift(0., 2.).unwrap();
    a.set_fill(1., 0., 0., 0.7).unwrap();
    let before = [a.state().unwrap(), b.state().unwrap()];
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    let options =
        ManimGeometryOptions::boolean_geometry(Op::Union, &[a.clone(), b.clone()]).unwrap();
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
    let result = scene.geometry(options).unwrap();
    let bounds = result.layout_bounds().unwrap().unwrap();
    assert_eq!(
        (bounds.min_x, bounds.max_x, bounds.min_y, bounds.max_y),
        (-2., 1., 1., 3.)
    );
    assert_ne!(result.node_id(), a.node_id());
    assert_eq!([a.state().unwrap(), b.state().unwrap()], before);
    assert_eq!(result.fill_opacity().unwrap(), 0.);
}

#[test]
fn invalid_operand_set_cannot_allocate_resource_or_identity() {
    let scene = Scene::new();
    let a = scene.square(2.).unwrap();
    let foreign = Scene::new().square(2.).unwrap();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    assert!(ManimGeometryOptions::boolean_geometry(Op::Union, std::slice::from_ref(&a)).is_err());
    assert!(ManimGeometryOptions::boolean_geometry(Op::Union, &[a, foreign]).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
}

#[test]
fn live_boolean_constructor_observes_current_affine_without_copying_paint() {
    use noon::{AnimationOptions, RateFunction};
    let mut scene = Scene::new();
    let a = scene.square(2.).unwrap();
    let b = scene.square(2.).unwrap();
    let mut target = a.target_editor().unwrap();
    target.shift(4., 0.).unwrap();
    scene.add(&a).unwrap();
    let animation = scene
        .declare_transform_to(
            &a,
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
    let authored_options =
        ManimGeometryOptions::boolean_geometry(Op::Union, &[a.clone(), b.clone()]).unwrap();
    let live_options = live
        .boolean_geometry_options(Op::Union, &[a.clone(), b.clone()])
        .unwrap();
    // Options are snapshots; later execution cannot move either captured region.
    live.advance_segment_to(segment, 2.).unwrap();
    live.complete_segment(segment).unwrap();
    let authored_result = live.create_manim_geometry(authored_options).unwrap();
    let result = live.create_manim_geometry(live_options).unwrap();
    let authored_bounds = authored_result.layout_bounds().unwrap().unwrap();
    let bounds = result.layout_bounds().unwrap().unwrap();
    assert_eq!((authored_bounds.min_x, authored_bounds.max_x), (-1., 1.));
    assert_eq!((bounds.min_x, bounds.max_x), (-1., 3.));
    assert_eq!(a.layout_bounds().unwrap().unwrap().max_x, 5.);
    live.add(&result).unwrap();
    assert!(live.effective_path_query(&result).is_ok());
}

#[test]
fn live_boolean_constructor_rejects_active_content_override_atomically() {
    use noon::{AnimationOptions, AuthoringError, LiveSessionError, UnsupportedAuthoringOperation};

    let scene = Scene::new();
    let a = scene.circle(1.).unwrap();
    let b = scene.square(2.).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    live.declare_and_activate_create(&a, AnimationOptions::new())
        .unwrap();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();

    let error = live
        .boolean_geometry_options(Op::Union, &[a, b])
        .unwrap_err();
    assert!(matches!(
        error,
        LiveSessionError::Authoring(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::EffectivePathRenderOverride
        ))
    ));
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
}

#[test]
fn live_boolean_constructor_preserves_foreign_store_error() {
    let scene = Scene::new();
    let a = scene.square(2.).unwrap();
    let foreign = Scene::new().square(2.).unwrap();
    let mut session = scene.execution_session().unwrap();
    let live = scene.live(&mut session);

    assert!(matches!(
        live.boolean_geometry_options(Op::Union, &[a, foreign]),
        Err(noon::LiveSessionError::ForeignMobjectStore)
    ));
}

#[test]
fn paired_boolean_example_executes_through_shared_runtime() {
    let mut session = noon::example_scenes::boolean_geometry::session().unwrap();
    session.seek(0.1).unwrap();
    assert_eq!(session.frame().objects.len(), 4);
}
