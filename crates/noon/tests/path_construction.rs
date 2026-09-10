use noon::{ManimGeometryOptions, Scene};
use noon_core::{PathCommand, Vec2, VectorPath};

#[test]
fn unfinished_anchors_and_discontinuous_curves_have_distinct_endpoints_and_length() {
    let scene = Scene::new();
    let mut object = scene
        .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
        .unwrap();
    assert!(object.add_line_to(Vec2::ONE).is_err());
    object.start_new_path(Vec2::new(-2., 0.)).unwrap();
    let first = object.path_query().unwrap();
    assert_eq!(first.start().unwrap(), (-2., 0.));
    assert_eq!(first.end().unwrap(), (-2., 0.));
    assert_eq!(first.point_from_proportion(1.).unwrap(), (-2., 0.));
    assert_eq!(first.arc_length(None).unwrap(), 0.);
    assert!(first.point_from_proportion(0.5).is_err());
    object.start_new_path(Vec2::ZERO).unwrap();
    object.add_line_to(Vec2::new(2., 0.)).unwrap();
    object.start_new_path(Vec2::new(5., 1.)).unwrap();
    let query = object.path_query().unwrap();
    assert_eq!(query.start().unwrap(), (-2., 0.));
    assert_eq!(query.end().unwrap(), (5., 1.));
    assert_eq!(query.point_from_proportion(1.).unwrap(), (5., 1.));
    assert_eq!(query.point_from_proportion(0.5).unwrap(), (1., 0.));
    assert_eq!(query.arc_length(None).unwrap(), 2.);
    // Prepared observations remain coherent when their source is edited again.
    object
        .add_quadratic_bezier_curve_to(Vec2::new(6., 2.), Vec2::new(7., 1.))
        .unwrap();
    assert_eq!(query.end().unwrap(), (5., 1.));
    assert_eq!(object.path_query().unwrap().end().unwrap(), (7., 1.));
}

#[test]
fn live_world_space_appends_preserve_transformed_geometry_and_use_one_publication_lane() {
    let mut scene = Scene::new();
    let mut object = scene.line((0., 0.), (1., 0.)).unwrap();
    object.scale(2., 1.).unwrap();
    object.rotate(std::f64::consts::FRAC_PI_2).unwrap();
    object.shift(3., 1.).unwrap();
    scene.add(&object).unwrap();
    let before = object.path_query().unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    live.add_line_to(&object, Vec2::new(4., 3.)).unwrap();
    live.add_quadratic_bezier_curve_to(&object, Vec2::new(5., 4.), Vec2::new(6., 3.))
        .unwrap();
    live.add_cubic_bezier_curve_to(
        &object,
        Vec2::new(7., 3.),
        Vec2::new(7., 1.),
        Vec2::new(6., 1.),
    )
    .unwrap();
    live.close_path(&object).unwrap();
    let query = live.effective_path_query(&object).unwrap();
    let start = before.start().unwrap();
    assert!((query.start().unwrap().0 - start.0).abs() < 1e-6);
    assert!((query.start().unwrap().1 - start.1).abs() < 1e-6);
    assert_eq!(query.start().unwrap(), query.end().unwrap());
    let before = object.state().unwrap();
    let resources = object
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    assert!(live
        .add_line_to(&object, Vec2::new(f32::INFINITY, 0.))
        .is_err());
    assert_eq!(object.state().unwrap(), before);
    assert_eq!(
        object
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        resources
    );
    live.start_new_path(&object, Vec2::new(-2., -1.)).unwrap();
    live.add_line_to(&object, Vec2::new(-1., 0.)).unwrap();
    assert_eq!(
        live.effective_path_query(&object).unwrap().end().unwrap(),
        (-1., 0.)
    );
}

#[test]
fn extending_a_closed_contour_keeps_its_closing_edge_and_earlier_subpaths() {
    let path = VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::ONE)
        .close()
        .move_to(Vec2::new(2., 0.))
        .line_to(Vec2::new(3., 1.))
        .close();
    let open = path.open_last_subpath();
    assert!(matches!(open.commands()[2], PathCommand::Close));
    assert_eq!(
        open.commands().last(),
        Some(&PathCommand::LineTo {
            to: Vec2::new(2., 0.)
        })
    );
    assert_eq!(open.endpoints(), Some((Vec2::ZERO, Vec2::new(2., 0.))));
    let scene = Scene::new();
    let mut object = scene
        .geometry(ManimGeometryOptions::path(open.close()).unwrap())
        .unwrap();
    let old_length = object.path_query().unwrap().arc_length(None).unwrap();
    object.add_line_to(Vec2::new(4., 0.)).unwrap();
    assert!(
        (object.path_query().unwrap().arc_length(None).unwrap() - old_length - 2.).abs() < 1e-6
    );
    assert_eq!(object.path_query().unwrap().end().unwrap(), (4., 0.));
}

#[test]
fn paired_path_construction_runs_through_shared_runtime() {
    let mut session = noon::example_scenes::path_construction::session().unwrap();
    session.seek(0.).unwrap();
    assert_eq!(session.frame().objects.len(), 2);
    let start = session.frame().objects.clone();
    session.seek(0.2).unwrap();
    assert_eq!(session.frame().objects, start);
}
