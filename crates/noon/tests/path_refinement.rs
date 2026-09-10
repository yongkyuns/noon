use noon::{ManimGeometryOptions, Scene};
use noon_core::{PathCommand, Vec2, VectorPath};

#[test]
fn queries_promote_primitives_and_capture_transformed_controls_once() {
    let scene = Scene::new();
    let mut object = scene
        .geometry(
            ManimGeometryOptions::path(
                VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .quadratic_to(Vec2::new(3., 6.), Vec2::new(6., 0.))
                    .move_to(Vec2::new(10., 1.)),
            )
            .unwrap(),
        )
        .unwrap();
    let local = object.local_path_query().unwrap();
    assert_eq!(local.curve_count(), 1);
    assert_eq!(
        local.curve_points(0).unwrap(),
        [(0., 0.), (2., 4.), (4., 4.), (6., 0.)]
    );
    assert_eq!(local.start_anchors(), [(0., 0.), (10., 1.)]);
    assert_eq!(local.end_anchors(), [(6., 0.)]);
    assert!(local.curve_points(1).is_err());
    object.shift(2., -1.).unwrap();
    let world = object.path_query().unwrap();
    assert_eq!(world.first_handles(), [(4., 3.)]);
    assert_eq!(world.second_handles(), [(6., 3.)]);
    assert_eq!(world.anchors(), [(2., -1.), (8., -1.), (12., 0.)]);
    assert_eq!(local.first_handles(), [(2., 4.)]);
    object.reverse_direction().unwrap();
    assert_eq!(world.start_anchors(), [(2., -1.), (12., 0.)]);
    let empty = scene
        .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
        .unwrap();
    assert_eq!(empty.path_query().unwrap().curve_count(), 0);
    assert!(empty.path_query().unwrap().anchors().is_empty());
    assert!(empty.path_query().unwrap().curve_points(0).is_err());
}

#[test]
fn refinement_preserves_shape_discontinuities_identity_and_live_publication() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut object = scene
            .geometry(
                ManimGeometryOptions::path(
                    VectorPath::new()
                        .move_to(Vec2::ZERO)
                        .line_to(Vec2::new(2., 0.))
                        .move_to(Vec2::new(10., 0.))
                        .line_to(Vec2::new(18., 0.)),
                )
                .unwrap(),
            )
            .unwrap();
        object.shift(0., 2.).unwrap();
        let copy = object.copy_handle().unwrap();
        let old = object.state().unwrap();
        let id = object.node_id();
        scene.add(&object).unwrap();
        let revision = scene.revision();
        object.insert_n_curves(0).unwrap();
        assert_eq!(scene.revision(), revision);
        let mut session = scene.execution_session().unwrap();
        if live_mode {
            scene
                .live(&mut session)
                .insert_n_curves(&object, 3)
                .unwrap();
        } else {
            object.insert_n_curves(3).unwrap();
        }
        let query = object.path_query().unwrap();
        assert_eq!(query.curve_count(), 5);
        assert_eq!(query.curve_points(2).unwrap()[3], (2., 2.));
        assert_eq!(query.curve_points(3).unwrap()[0], (10., 2.));
        assert_eq!(query.curve_points(3).unwrap()[3], (14., 2.));
        assert!((query.arc_length(None).unwrap() - 10.).abs() < 1e-6);
        assert_eq!(object.node_id(), id);
        assert_eq!(object.state().unwrap().style, old.style);
        assert_eq!(copy.state().unwrap().content, old.content);
        assert_eq!(copy.state().unwrap().transform, old.transform);
        if live_mode {
            assert_eq!(
                scene
                    .live(&mut session)
                    .effective_path_query(&object)
                    .unwrap()
                    .curve_count(),
                5
            );
        }
    }
}

#[test]
fn subdivision_keeps_closed_joins_and_unfinished_subpaths() {
    let source = VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::new(2., 0.))
        .line_to(Vec2::new(1., 2.))
        .close()
        .move_to(Vec2::new(5., 5.));
    let refined = noon_geometry::subdivide_path(&source, 4).unwrap();
    assert_eq!(refined.endpoints(), source.endpoints());
    assert_eq!(
        refined
            .commands()
            .iter()
            .filter(|c| matches!(c, PathCommand::Close))
            .count(),
        1
    );
    assert_eq!(
        noon_geometry::PathProportionPlan::new(&refined)
            .unwrap()
            .curve_count(),
        7
    );
    let singleton = VectorPath::new().move_to(Vec2::ONE);
    let refined = noon_geometry::subdivide_path(&singleton, 3).unwrap();
    assert_eq!(
        noon_geometry::PathProportionPlan::new(&refined)
            .unwrap()
            .curve_count(),
        3
    );
    assert_eq!(refined.commands().last(), singleton.commands().last());
    assert!(noon_geometry::subdivide_path(&VectorPath::new(), 1).is_err());
}

#[test]
fn cubic_subdivision_preserves_the_polynomial_at_each_new_anchor() {
    let path = VectorPath::new().move_to(Vec2::ZERO).cubic_to(
        Vec2::new(0., 4.),
        Vec2::new(4., 4.),
        Vec2::new(4., 0.),
    );
    let refined = noon_geometry::subdivide_path(&path, 3).unwrap();
    let plan = noon_geometry::PathProportionPlan::new(&refined).unwrap();
    for i in 0..4 {
        let t = (i + 1) as f64 / 4.;
        let end = plan.curve_points(i).unwrap()[3];
        assert!((end.x - (12. * t * t - 8. * t * t * t)).abs() < 1e-6);
        assert!((end.y - 12. * t * (1. - t)).abs() < 1e-6);
    }
}

#[test]
fn paired_path_refinement_uses_the_shared_runtime() {
    let mut session = noon::example_scenes::path_refinement::session().unwrap();
    session.seek(0.).unwrap();
    let initial = session.frame().objects.clone();
    assert_eq!(initial.len(), 8);
    session.seek(0.2).unwrap();
    assert_eq!(session.frame().objects, initial);
}
