use noon::{ManimGeometryOptions, Scene};
use noon_core::{PathCommand, Vec2, VectorPath};

#[test]
fn unfinished_anchors_and_discontinuous_curves_have_distinct_endpoints_and_length() {
    let mut scene = Scene::new();
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
    let midpoint = query.point_from_proportion(0.5).unwrap();
    assert!((midpoint.0 - 1.).abs() < 1e-6);
    assert!((midpoint.1).abs() < 1e-6);
    assert!((query.arc_length(None).unwrap() - 2.).abs() < 1e-6);
    // Prepared observations remain coherent when their source is edited again.
    object
        .add_quadratic_bezier_curve_to(Vec2::new(6., 2.), Vec2::new(7., 1.))
        .unwrap();
    assert_eq!(query.end().unwrap(), (5., 1.));
    assert_eq!(object.path_query().unwrap().end().unwrap(), (7., 1.));
}

#[test]
fn scalar_curve_appends_preserve_incomplete_raw_points_in_authored_and_live_paths() {
    #[derive(Clone, Copy)]
    enum Edit {
        Line,
        Quadratic,
        Cubic,
        Smooth,
        SmoothWithHandle,
    }

    for point_count in [5, 7] {
        let initial = (0..point_count)
            .map(|index| Vec2::new(index as f32, (index % 2) as f32))
            .collect::<Vec<_>>();
        for edit in [
            Edit::Line,
            Edit::Quadratic,
            Edit::Cubic,
            Edit::Smooth,
            Edit::SmoothWithHandle,
        ] {
            for live in [false, true] {
                let mut scene = Scene::new();
                let mut object = scene
                    .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
                    .unwrap();
                object.set_points(&initial).unwrap();
                if live {
                    scene.add(&object).unwrap();
                }

                let start = *initial.last().unwrap();
                let (control1, control2, anchor) = match edit {
                    Edit::Line => {
                        let anchor = Vec2::new(9., 0.);
                        (
                            start + (anchor - start) / 3.0,
                            start + (anchor - start) * (2.0 / 3.0),
                            anchor,
                        )
                    }
                    Edit::Quadratic => {
                        let control = Vec2::new(8., 2.);
                        let anchor = Vec2::new(10., 0.);
                        (
                            control * (2.0 / 3.0) + start / 3.0,
                            control * (2.0 / 3.0) + anchor / 3.0,
                            anchor,
                        )
                    }
                    Edit::Cubic => (Vec2::new(7., 1.), Vec2::new(9., 2.), Vec2::new(11., 0.)),
                    Edit::Smooth | Edit::SmoothWithHandle => {
                        let anchor = Vec2::new(11., -1.);
                        if point_count % 4 == 1 {
                            let direction = anchor - start;
                            (
                                start + direction / 3.0,
                                start + direction * (2.0 / 3.0),
                                anchor,
                            )
                        } else {
                            let last_handle2 = initial[initial.len() - 2];
                            let last_anchor = initial[initial.len() - 1];
                            let tangent = last_anchor - last_handle2;
                            let control2 = if matches!(edit, Edit::SmoothWithHandle) {
                                Vec2::new(7., 2.)
                            } else {
                                let direction = anchor - last_anchor;
                                let reflected = direction.normalized().map_or(tangent, |axis| {
                                    axis * (2.0 * (tangent.x * axis.x + tangent.y * axis.y))
                                        - tangent
                                });
                                anchor - reflected
                            };
                            (last_anchor + tangent, control2, anchor)
                        }
                    }
                };

                let mut expected = initial.clone();
                if point_count % 4 != 1 {
                    expected.push(start);
                }
                expected.extend([control1, control2, anchor]);

                if live {
                    let mut session = scene.execution_session().unwrap();
                    let operation = match edit {
                        Edit::Line => noon::integration::BorrowedPathEdit::Line {
                            object: &object,
                            point: anchor,
                        },
                        Edit::Quadratic => noon::integration::BorrowedPathEdit::Quadratic {
                            object: &object,
                            control: Vec2::new(8., 2.),
                            anchor,
                        },
                        Edit::Cubic => noon::integration::BorrowedPathEdit::Cubic {
                            object: &object,
                            control1,
                            control2,
                            anchor,
                        },
                        Edit::Smooth => noon::integration::BorrowedPathEdit::SmoothCurve {
                            object: &object,
                            handle2: None,
                            anchor,
                        },
                        Edit::SmoothWithHandle => {
                            noon::integration::BorrowedPathEdit::SmoothCurve {
                                object: &object,
                                handle2: Some(Vec2::new(7., 2.)),
                                anchor,
                            }
                        }
                    };
                    noon::integration::publish_borrowed_path_edit(
                        scene.integration_store(),
                        scene.root(),
                        &mut session,
                        operation,
                    )
                    .unwrap();
                    let actual = noon::integration::effective_path_query(
                        scene.integration_store(),
                        &session,
                        &object,
                    )
                    .unwrap()
                    .points()
                    .unwrap();
                    assert_raw_points_match(&actual, &expected);
                } else {
                    match edit {
                        Edit::Line => object.add_line_to(anchor).unwrap(),
                        Edit::Quadratic => object
                            .add_quadratic_bezier_curve_to(Vec2::new(8., 2.), anchor)
                            .unwrap(),
                        Edit::Cubic => object
                            .add_cubic_bezier_curve_to(control1, control2, anchor)
                            .unwrap(),
                        Edit::Smooth => object.add_smooth_curve_to(None, anchor).unwrap(),
                        Edit::SmoothWithHandle => object
                            .add_smooth_curve_to(Some(Vec2::new(7., 2.)), anchor)
                            .unwrap(),
                    }
                    let actual = object.path_query().unwrap().points().unwrap();
                    assert_raw_points_match(&actual, &expected);
                }
            }
        }
    }
}

fn assert_raw_points_match(actual: &[(f64, f64)], expected: &[Vec2]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((actual.0 - f64::from(expected.x)).abs() < 1e-6);
        assert!((actual.1 - f64::from(expected.y)).abs() < 1e-6);
    }
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
    noon::integration::publish_borrowed_path_edit(
        scene.integration_store(),
        scene.root(),
        &mut session,
        noon::integration::BorrowedPathEdit::Line {
            object: &object,
            point: Vec2::new(4., 3.),
        },
    )
    .unwrap();
    noon::integration::publish_borrowed_path_edit(
        scene.integration_store(),
        scene.root(),
        &mut session,
        noon::integration::BorrowedPathEdit::Quadratic {
            object: &object,
            control: Vec2::new(5., 4.),
            anchor: Vec2::new(6., 3.),
        },
    )
    .unwrap();
    noon::integration::publish_borrowed_path_edit(
        scene.integration_store(),
        scene.root(),
        &mut session,
        noon::integration::BorrowedPathEdit::Cubic {
            object: &object,
            control1: Vec2::new(7., 3.),
            control2: Vec2::new(7., 1.),
            anchor: Vec2::new(6., 1.),
        },
    )
    .unwrap();
    noon::integration::publish_borrowed_path_edit(
        scene.integration_store(),
        scene.root(),
        &mut session,
        noon::integration::BorrowedPathEdit::Close { object: &object },
    )
    .unwrap();
    let query =
        noon::integration::effective_path_query(scene.integration_store(), &session, &object)
            .unwrap();
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
    assert!(noon::integration::publish_borrowed_path_edit(
        scene.integration_store(),
        scene.root(),
        &mut session,
        noon::integration::BorrowedPathEdit::Line {
            object: &object,
            point: Vec2::new(f32::INFINITY, 0.),
        },
    )
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
    noon::integration::publish_borrowed_path_edit(
        scene.integration_store(),
        scene.root(),
        &mut session,
        noon::integration::BorrowedPathEdit::Start {
            object: &object,
            point: Vec2::new(-2., -1.),
        },
    )
    .unwrap();
    noon::integration::publish_borrowed_path_edit(
        scene.integration_store(),
        scene.root(),
        &mut session,
        noon::integration::BorrowedPathEdit::Line {
            object: &object,
            point: Vec2::new(-1., 0.),
        },
    )
    .unwrap();
    assert_eq!(
        noon::integration::effective_path_query(scene.integration_store(), &session, &object)
            .unwrap()
            .end()
            .unwrap(),
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
    let mut scene = Scene::new();
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
