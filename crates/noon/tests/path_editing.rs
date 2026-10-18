use noon::{AnimationOptions, AuthoringError, ManimGeometryOptions, Scene};
use noon_core::{Vec2, VectorPath};

#[test]
fn corner_replacement_preserves_identity_paint_and_copies_with_local_publication() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut source = scene.circle(1.).unwrap();
        source.shift(3., 4.).unwrap();
        source.rotate(0.6).unwrap();
        source.set_fill(0., 1., 0., 0.5).unwrap();
        source.set_z_index(3.5).unwrap();
        let copy = source.copy_handle().unwrap();
        let unrelated = scene.square(0.4).unwrap();
        scene
            .add_many(&[(&source).into(), (&copy).into(), (&unrelated).into()])
            .unwrap();
        let before = source.state().unwrap();
        let other = unrelated.state().unwrap();
        let identity = source.node_id();
        let revision = scene.revision();
        let count = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        let corners = [Vec2::new(-2., -1.), Vec2::new(0., 1.), Vec2::new(2., -1.)];
        let mut session = scene.execution_session().unwrap();
        if live_mode {
            noon::integration::publish_borrowed_path_edit(
                scene.integration_store(),
                scene.root(),
                &mut session,
                noon::integration::BorrowedPathEdit::Corners {
                    object: &source,
                    points: &corners,
                },
            )
            .unwrap();
        } else {
            source.set_points_as_corners(&corners).unwrap();
        }
        let after = source.state().unwrap();
        assert_ne!(scene.revision(), revision);
        assert_eq!(source.node_id(), identity);
        assert_eq!(after.style, before.style);
        assert_eq!(after.presentation(), before.presentation());
        assert_eq!(copy.state().unwrap().content, before.content);
        assert_eq!(copy.state().unwrap().transform, before.transform);
        assert_eq!(unrelated.state().unwrap(), other);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            count + 1
        );
        assert_eq!(source.path_query().unwrap().start().unwrap(), (-2., -1.));
        assert_eq!(source.path_query().unwrap().end().unwrap(), (2., -1.));
        let revision = scene.revision();
        if live_mode {
            assert_eq!(
                noon::integration::effective_path_query(
                    scene.integration_store(),
                    &session,
                    &source
                )
                .unwrap()
                .end()
                .unwrap(),
                (2., -1.)
            );
            noon::integration::publish_borrowed_path_edit(
                scene.integration_store(),
                scene.root(),
                &mut session,
                noon::integration::BorrowedPathEdit::Corners {
                    object: &source,
                    points: &corners,
                },
            )
            .unwrap();
        } else {
            source.set_points_as_corners(&corners).unwrap();
        }
        assert_eq!(
            scene.revision(),
            revision,
            "identical content must not republish"
        );
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            count + 1
        );
    }
}

#[test]
fn raw_points_preserve_incomplete_arrays_and_append_through_shared_publication() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut object = scene.square(1.).unwrap();
        object.shift(3., 4.).unwrap();
        let identity = object.node_id();
        let initial = (0..7)
            .map(|index| Vec2::new(index as f32, (index * 2) as f32))
            .collect::<Vec<_>>();
        let mut execution = scene.execution_session().unwrap();

        if live_mode {
            noon::integration::publish_borrowed_path_edit(
                scene.integration_store(),
                scene.root(),
                &mut execution,
                noon::integration::BorrowedPathEdit::RawPoints {
                    object: &object,
                    points: &initial,
                },
            )
            .unwrap();
        } else {
            object.set_points(&initial).unwrap();
        }

        let query = object.path_query().unwrap();
        assert_eq!(query.point_count(), 7);
        assert_eq!(query.curve_count(), 1);
        assert_eq!(
            query.points().unwrap(),
            (0..7)
                .map(|index| (index as f64, (index * 2) as f64))
                .collect::<Vec<_>>()
        );
        assert_eq!(query.start_anchors(), [(0., 0.), (4., 8.)]);
        assert_eq!(query.start().unwrap(), (0., 0.));
        assert_eq!(query.end().unwrap(), (6., 12.));
        assert_eq!(query.point_from_proportion(1.).unwrap(), (6., 12.));

        let appended = [Vec2::new(7., 14.), Vec2::new(8., 16.)];
        if live_mode {
            noon::integration::publish_borrowed_path_edit(
                scene.integration_store(),
                scene.root(),
                &mut execution,
                noon::integration::BorrowedPathEdit::AppendRawPoints {
                    object: &object,
                    points: &appended,
                },
            )
            .unwrap();
        } else {
            object.append_points(&appended).unwrap();
        }

        let query = object.path_query().unwrap();
        assert_eq!(object.node_id(), identity);
        assert_eq!(query.point_count(), 9);
        assert_eq!(query.curve_count(), 2);
        assert_eq!(query.start_anchors(), [(0., 0.), (4., 8.), (8., 16.)]);
        assert_eq!(query.end().unwrap(), (8., 16.));
        assert_eq!(query.points().unwrap().last(), Some(&(8., 16.)));
    }
}

#[test]
fn start_new_path_preserves_raw_points_and_pads_by_the_configured_stride() {
    for live_mode in [false, true] {
        for stride in [4, 5, -4] {
            let mut scene = Scene::new();
            let mut object = scene.square(1.).unwrap();
            let initial = (0..7)
                .map(|index| Vec2::new(index as f32, (index * 2) as f32))
                .collect::<Vec<_>>();
            object.set_points(&initial).unwrap();
            let mut execution = scene.execution_session().unwrap();
            let new_point = Vec2::new(9., 9.);

            if live_mode {
                noon::integration::publish_borrowed_path_edit(
                    scene.integration_store(),
                    scene.root(),
                    &mut execution,
                    noon::integration::BorrowedPathEdit::StartWithStride {
                        object: &object,
                        point: new_point,
                        stride,
                    },
                )
                .unwrap();
            } else {
                object
                    .start_new_path_with_stride(new_point, stride)
                    .unwrap();
            }

            let points = object.path_query().unwrap().points().unwrap();
            match stride {
                4 => {
                    assert_eq!(points.len(), 9);
                    assert_eq!(
                        points[..7],
                        (0..7)
                            .map(|i| (i as f64, (i * 2) as f64))
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(points[7..], [(4., 8.), (9., 9.)]);
                }
                5 => {
                    assert_eq!(points.len(), 11);
                    assert_eq!(
                        points[..7],
                        (0..7)
                            .map(|i| (i as f64, (i * 2) as f64))
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(points[7..], [(5., 10.), (5., 10.), (5., 10.), (9., 9.)]);
                }
                -4 => {
                    assert_eq!(points.len(), 8);
                    assert_eq!(
                        points[..7],
                        (0..7)
                            .map(|i| (i as f64, (i * 2) as f64))
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(points[7], (9., 9.));
                }
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn clear_points_uses_shared_authored_and_live_transactions() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut object = scene.square(2.).unwrap();
        object.shift(3., 4.).unwrap();
        object.set_fill(0.2, 0.4, 0.8, 0.5).unwrap();
        object.set_z_index(2.5).unwrap();
        let copy = object.copy_handle().unwrap();
        scene.add(&object).unwrap();
        scene.add(&copy).unwrap();

        let original = object.state().unwrap();
        let original_id = object.node_id();
        let original_point_count = object.path_query().unwrap().point_count();
        assert!(original_point_count > 0);
        let mut execution = scene.execution_session().unwrap();
        let store = std::rc::Rc::clone(scene.integration_store());
        let root = scene.root();
        let revision = scene.revision();

        if live_mode {
            noon::integration::publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                noon::integration::BorrowedPathEdit::Clear { object: &object },
            )
            .unwrap();
            assert_eq!(
                noon::integration::effective_path_query(&store, &execution, &object)
                    .unwrap()
                    .point_count(),
                0
            );
        } else {
            object.clear_points().unwrap();
        }

        assert_ne!(scene.revision(), revision);
        assert_eq!(object.node_id(), original_id);
        assert_eq!(object.path_query().unwrap().point_count(), 0);
        assert_eq!(
            copy.path_query().unwrap().point_count(),
            original_point_count
        );
        let cleared = object.state().unwrap();
        assert_eq!(cleared.style, original.style);
        assert_eq!(cleared.presentation(), original.presentation());

        let revision = scene.revision();
        if live_mode {
            noon::integration::publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                noon::integration::BorrowedPathEdit::Clear { object: &object },
            )
            .unwrap();
        } else {
            object.clear_points().unwrap();
        }
        assert_eq!(
            scene.revision(),
            revision,
            "clearing an empty path is a no-op"
        );
    }
}

#[test]
fn cubic_curve_appends_connected_and_disconnected_segments_authored_and_live() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut object = scene
            .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
            .unwrap();
        scene.add(&object).unwrap();
        let mut execution = scene.execution_session().unwrap();
        let store = std::rc::Rc::clone(scene.integration_store());
        let root = scene.root();
        let append = |object: &mut noon::Mobject,
                      execution: &mut noon::ExecutionSession,
                      anchor1,
                      handle1,
                      handle2,
                      anchor2| {
            if live_mode {
                noon::integration::publish_borrowed_path_edit(
                    &store,
                    root,
                    execution,
                    noon::integration::BorrowedPathEdit::CubicCurve {
                        object,
                        anchor1,
                        handle1,
                        handle2,
                        anchor2,
                    },
                )
            } else {
                object.add_cubic_bezier_curve(anchor1, handle1, handle2, anchor2)
            }
        };

        append(
            &mut object,
            &mut execution,
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(2., 1.),
        )
        .unwrap();
        append(
            &mut object,
            &mut execution,
            Vec2::new(2., 1.),
            Vec2::new(3., 1.),
            Vec2::new(3., 2.),
            Vec2::new(4., 2.),
        )
        .unwrap();
        append(
            &mut object,
            &mut execution,
            Vec2::new(6., 2.),
            Vec2::new(7., 2.),
            Vec2::new(7., 3.),
            Vec2::new(8., 3.),
        )
        .unwrap();

        let query = if live_mode {
            noon::integration::effective_path_query(&store, &execution, &object).unwrap()
        } else {
            object.path_query().unwrap()
        };
        assert_eq!(query.curve_count(), 3);
        assert_eq!(query.subpaths().len(), 2);
        assert_eq!(query.curve_points(0).unwrap()[0], (0., 0.));
        assert_eq!(query.curve_points(1).unwrap()[0], (2., 1.));
        assert_eq!(query.curve_points(2).unwrap()[0], (6., 2.));
        assert_eq!(query.end().unwrap(), (8., 3.));
    }
}

#[test]
fn cubic_curve_batches_are_atomic_and_reject_nonfinite_points() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut object = scene
            .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
            .unwrap();
        scene.add(&object).unwrap();
        let mut execution = scene.execution_session().unwrap();
        let store = std::rc::Rc::clone(scene.integration_store());
        let root = scene.root();
        let curves = [
            [
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(2., 1.),
            ],
            [
                Vec2::new(2., 1.),
                Vec2::new(3., 1.),
                Vec2::new(3., 2.),
                Vec2::new(4., 2.),
            ],
        ];
        let revision = scene.revision();
        if live_mode {
            noon::integration::publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                noon::integration::BorrowedPathEdit::CubicCurves {
                    object: &object,
                    curves: &curves,
                },
            )
            .unwrap();
        } else {
            object.add_cubic_bezier_curves(&curves).unwrap();
        }
        assert_ne!(scene.revision(), revision);

        let query = if live_mode {
            noon::integration::effective_path_query(&store, &execution, &object).unwrap()
        } else {
            object.path_query().unwrap()
        };
        assert_eq!(query.curve_count(), 2);
        assert_eq!(query.subpaths().len(), 1);
        assert_eq!(query.end().unwrap(), (4., 2.));

        let invalid = [[
            Vec2::new(4., 2.),
            Vec2::new(f32::NAN, 0.),
            Vec2::new(5., 3.),
            Vec2::new(6., 3.),
        ]];
        let revision = scene.revision();
        let result = if live_mode {
            noon::integration::publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                noon::integration::BorrowedPathEdit::CubicCurves {
                    object: &object,
                    curves: &invalid,
                },
            )
        } else {
            object.add_cubic_bezier_curves(&invalid)
        };
        assert!(result.is_err());
        assert_eq!(scene.revision(), revision);
        let query = if live_mode {
            noon::integration::effective_path_query(&store, &execution, &object).unwrap()
        } else {
            object.path_query().unwrap()
        };
        assert_eq!(query.curve_count(), 2);
        assert_eq!(query.end().unwrap(), (4., 2.));
    }
}

#[test]
fn smooth_curve_to_calculates_or_accepts_the_second_handle_authored_and_live() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut object = scene
            .geometry(
                ManimGeometryOptions::path(
                    VectorPath::new()
                        .move_to(Vec2::ZERO)
                        .line_to(Vec2::new(3., 0.)),
                )
                .unwrap(),
            )
            .unwrap();
        scene.add(&object).unwrap();
        let mut session = scene.execution_session().unwrap();
        let publish =
            |object: &mut noon::Mobject, session: &mut noon::ExecutionSession, handle2, anchor| {
                if live_mode {
                    noon::integration::publish_borrowed_path_edit(
                        scene.integration_store(),
                        scene.root(),
                        session,
                        noon::integration::BorrowedPathEdit::SmoothCurve {
                            object,
                            handle2,
                            anchor,
                        },
                    )
                } else {
                    object.add_smooth_curve_to(handle2, anchor)
                }
            };

        publish(&mut object, &mut session, None, Vec2::new(6., 3.)).unwrap();
        let query = if live_mode {
            noon::integration::effective_path_query(scene.integration_store(), &session, &object)
                .unwrap()
        } else {
            object.path_query().unwrap()
        };
        let curve = query.curve_points(1).unwrap();
        let expected = [(3., 0.), (4., 0.), (6., 2.), (6., 3.)];
        for (actual, expected) in curve.into_iter().zip(expected) {
            assert!((actual.0 - expected.0).abs() < 1e-5);
            assert!((actual.1 - expected.1).abs() < 1e-5);
        }

        publish(
            &mut object,
            &mut session,
            Some(Vec2::new(6., 0.)),
            Vec2::new(7., -2.),
        )
        .unwrap();
        let query = if live_mode {
            noon::integration::effective_path_query(scene.integration_store(), &session, &object)
                .unwrap()
        } else {
            object.path_query().unwrap()
        };
        let curve = query.curve_points(2).unwrap();
        let expected = [(6., 3.), (6., 4.), (6., 0.), (7., -2.)];
        for (actual, expected) in curve.into_iter().zip(expected) {
            assert!((actual.0 - expected.0).abs() < 1e-5);
            assert!((actual.1 - expected.1).abs() < 1e-5);
        }
    }
}

#[test]
fn smooth_curve_to_pending_path_is_a_line_and_empty_path_rejects() {
    let mut scene = Scene::new();
    let mut object = scene
        .geometry(
            ManimGeometryOptions::path(
                VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(Vec2::new(1., 0.))
                    .move_to(Vec2::new(2., 0.)),
            )
            .unwrap(),
        )
        .unwrap();
    object
        .add_smooth_curve_to(Some(Vec2::new(20., 20.)), Vec2::new(4., 0.))
        .unwrap();
    let curve = object.path_query().unwrap().curve_points(1).unwrap();
    let expected = [(2., 0.), (8. / 3., 0.), (10. / 3., 0.), (4., 0.)];
    for (actual, expected) in curve.into_iter().zip(expected) {
        assert!((actual.0 - expected.0).abs() < 1e-5);
        assert!((actual.1 - expected.1).abs() < 1e-5);
    }

    let mut empty = scene
        .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
        .unwrap();
    assert!(empty.add_smooth_curve_to(None, Vec2::new(1., 1.)).is_err());
}

#[test]
fn set_cubic_bezier_curves_replaces_paths_authored_and_live() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut object = scene
            .geometry(
                ManimGeometryOptions::path(
                    VectorPath::new()
                        .move_to(Vec2::new(10., 10.))
                        .line_to(Vec2::new(11., 10.)),
                )
                .unwrap(),
            )
            .unwrap();
        scene.add(&object).unwrap();
        let mut execution = scene.execution_session().unwrap();
        let store = std::rc::Rc::clone(scene.integration_store());
        let root = scene.root();
        let curves = [
            [
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(2., 1.),
            ],
            [
                Vec2::new(2., 1.),
                Vec2::new(3., 1.),
                Vec2::new(3., 2.),
                Vec2::new(4., 2.),
            ],
        ];
        if live_mode {
            noon::integration::publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                noon::integration::BorrowedPathEdit::SetCubicCurves {
                    object: &object,
                    curves: &curves,
                },
            )
            .unwrap();
        } else {
            object.set_cubic_bezier_curves(&curves).unwrap();
        }
        let query = if live_mode {
            noon::integration::effective_path_query(&store, &execution, &object).unwrap()
        } else {
            object.path_query().unwrap()
        };
        assert_eq!(query.curve_count(), 2);
        assert_eq!(query.subpaths().len(), 1);
        assert_eq!(query.start().unwrap(), (0., 0.));
        assert_eq!(query.end().unwrap(), (4., 2.));

        if live_mode {
            noon::integration::publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                noon::integration::BorrowedPathEdit::SetCubicCurves {
                    object: &object,
                    curves: &[],
                },
            )
            .unwrap();
            assert_eq!(
                noon::integration::effective_path_query(&store, &execution, &object)
                    .unwrap()
                    .curve_count(),
                0
            );
        } else {
            object.set_cubic_bezier_curves(&[]).unwrap();
            assert_eq!(object.path_query().unwrap().curve_count(), 0);
        }
    }
}

#[test]
fn add_points_as_corners_appends_atomically_authored_and_live() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut object = scene
            .geometry(
                ManimGeometryOptions::path(
                    VectorPath::new()
                        .move_to(Vec2::new(0., 0.))
                        .line_to(Vec2::new(1., 0.)),
                )
                .unwrap(),
            )
            .unwrap();
        scene.add(&object).unwrap();
        let mut execution = scene.execution_session().unwrap();
        let store = std::rc::Rc::clone(scene.integration_store());
        let root = scene.root();
        let corners = [Vec2::new(2., 0.), Vec2::new(3., 1.)];

        if live_mode {
            noon::integration::publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                noon::integration::BorrowedPathEdit::AppendCorners {
                    object: &object,
                    points: &corners,
                },
            )
            .unwrap();
        } else {
            object.add_points_as_corners(&corners).unwrap();
        }

        let query = if live_mode {
            noon::integration::effective_path_query(&store, &execution, &object).unwrap()
        } else {
            object.path_query().unwrap()
        };
        assert_eq!(query.curve_count(), 3);
        assert_eq!(query.end().unwrap(), (3., 1.));

        let invalid = [Vec2::new(4., 1.), Vec2::new(f32::NAN, 2.)];
        let revision = scene.revision();
        let result = if live_mode {
            noon::integration::publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                noon::integration::BorrowedPathEdit::AppendCorners {
                    object: &object,
                    points: &invalid,
                },
            )
        } else {
            object.add_points_as_corners(&invalid)
        };
        assert!(result.is_err());
        assert_eq!(scene.revision(), revision);

        let empty_revision = scene.revision();
        let result = if live_mode {
            noon::integration::publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                noon::integration::BorrowedPathEdit::AppendCorners {
                    object: &object,
                    points: &[],
                },
            )
        } else {
            object.add_points_as_corners(&[])
        };
        result.unwrap();
        assert_eq!(scene.revision(), empty_revision);
    }
}

#[test]
fn add_points_as_corners_uses_pending_new_path_anchor() {
    let mut empty = Scene::new()
        .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
        .unwrap();
    assert!(empty.add_points_as_corners(&[]).is_err());

    let mut object = Scene::new()
        .geometry(
            ManimGeometryOptions::path(
                VectorPath::new()
                    .move_to(Vec2::new(0., 0.))
                    .line_to(Vec2::new(1., 0.))
                    .move_to(Vec2::new(5., 5.)),
            )
            .unwrap(),
        )
        .unwrap();
    object.add_points_as_corners(&[Vec2::new(6., 5.)]).unwrap();
    let query = object.path_query().unwrap();
    assert_eq!(query.curve_count(), 2);
    assert_eq!(query.end().unwrap(), (6., 5.));
}

#[test]
fn append_vectorized_mobject_appends_source_and_drops_pending_destination_anchor() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut destination = scene
            .geometry(
                ManimGeometryOptions::path(
                    VectorPath::new()
                        .move_to(Vec2::new(0., 0.))
                        .line_to(Vec2::new(1., 0.))
                        .move_to(Vec2::new(2., 0.)),
                )
                .unwrap(),
            )
            .unwrap();
        let source = scene
            .geometry(
                ManimGeometryOptions::path(
                    VectorPath::new()
                        .move_to(Vec2::new(4., 0.))
                        .line_to(Vec2::new(5., 1.)),
                )
                .unwrap(),
            )
            .unwrap();
        scene.add(&destination).unwrap();
        scene.add(&source).unwrap();
        let mut execution = scene.execution_session().unwrap();
        let store = std::rc::Rc::clone(scene.integration_store());
        let root = scene.root();

        if live_mode {
            noon::integration::publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                noon::integration::BorrowedPathEdit::AppendVectorized {
                    object: &destination,
                    source: &source,
                },
            )
            .unwrap();
        } else {
            destination.append_vectorized_mobject(&source).unwrap();
        }

        let query = if live_mode {
            noon::integration::effective_path_query(&store, &execution, &destination).unwrap()
        } else {
            destination.path_query().unwrap()
        };
        assert_eq!(query.curve_count(), 2);
        assert_eq!(query.subpaths().len(), 2);
        assert_eq!(query.end().unwrap(), (5., 1.));
    }
}

#[test]
fn append_vectorized_mobject_uses_mutable_destination_stride_and_preserves_raw_points() {
    let source_points = [Vec2::new(20., 1.), Vec2::new(21., 2.), Vec2::new(22., 3.)];
    for point_count in [5, 6] {
        for live_mode in [false, true] {
            let destination_points = (0..point_count)
                .map(|index| Vec2::new(index as f32, -(index as f32)))
                .collect::<Vec<_>>();
            let mut expected = destination_points.clone();
            if point_count % 5 == 1 {
                expected.pop();
            }
            expected.extend(source_points);

            let mut scene = Scene::new();
            let mut destination = scene
                .geometry(
                    ManimGeometryOptions::path(VectorPath::from_manim_points(destination_points))
                        .unwrap(),
                )
                .unwrap();
            let source = scene
                .geometry(
                    ManimGeometryOptions::path(VectorPath::from_manim_points(
                        source_points.to_vec(),
                    ))
                    .unwrap(),
                )
                .unwrap();
            scene.add(&destination).unwrap();
            scene.add(&source).unwrap();

            let query = if live_mode {
                let mut execution = scene.execution_session().unwrap();
                let store = std::rc::Rc::clone(scene.integration_store());
                noon::integration::publish_borrowed_path_edit(
                    &store,
                    scene.root(),
                    &mut execution,
                    noon::integration::BorrowedPathEdit::AppendVectorizedWithStride {
                        object: &destination,
                        source: &source,
                        stride: 5,
                    },
                )
                .unwrap();
                noon::integration::effective_path_query(&store, &execution, &destination).unwrap()
            } else {
                destination
                    .append_vectorized_mobject_with_stride(&source, 5)
                    .unwrap();
                destination.path_query().unwrap()
            };

            let points = query.points().unwrap();
            let expected = expected
                .into_iter()
                .map(|point| (f64::from(point.x), f64::from(point.y)))
                .collect::<Vec<_>>();
            assert_eq!(
                points, expected,
                "point_count={point_count}, live={live_mode}"
            );
        }
    }
}

#[test]
fn empty_corner_inputs_and_nonfinite_rejection_follow_shared_semantics() {
    let mut scene = Scene::new();
    let mut object = scene
        .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
        .unwrap();
    let revision = scene.revision();
    let count = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    for corners in [vec![], vec![Vec2::new(1., 2.)]] {
        object.set_points_as_corners(&corners).unwrap();
        assert_eq!(object.path_query().unwrap().arc_length(None).unwrap(), 0.);
        assert!(object.path_query().unwrap().start().is_err());
        assert_eq!(scene.revision(), revision);
    }
    let before = object.state().unwrap();
    assert!(matches!(
        object.set_points_as_corners(&[Vec2::new(f32::NAN, 0.)]),
        Err(AuthoringError::NonFiniteGeometry)
    ));
    assert_eq!(object.state().unwrap(), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        count
    );
}

#[test]
fn rejected_live_edit_does_not_allocate_or_publish_and_completion_allows_edit() {
    let mut scene = Scene::new();
    let object = scene.square(1.).unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_create(&object, AnimationOptions::new())
        .unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    let before = object.state().unwrap();
    let revision = object.integration_store().borrow().scene_revision();
    let resources = object
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    let corners = [Vec2::ZERO, Vec2::new(1., 1.)];
    assert!(noon::integration::publish_borrowed_path_edit(
        scene.integration_store(),
        scene.root(),
        &mut session,
        noon::integration::BorrowedPathEdit::Corners {
            object: &object,
            points: &corners,
        },
    )
    .is_err());
    assert_eq!(object.state().unwrap(), before);
    assert_eq!(
        object.integration_store().borrow().scene_revision(),
        revision
    );
    assert_eq!(
        object
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        resources
    );
    scene
        .live(&mut session)
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    noon::integration::publish_borrowed_path_edit(
        scene.integration_store(),
        scene.root(),
        &mut session,
        noon::integration::BorrowedPathEdit::Corners {
            object: &object,
            points: &corners,
        },
    )
    .unwrap();
    assert_eq!(
        noon::integration::effective_path_query(scene.integration_store(), &session, &object)
            .unwrap()
            .end()
            .unwrap(),
        (1., 1.)
    );
}

#[test]
fn paired_path_editing_uses_the_shared_execution_session() {
    let mut session = noon::example_scenes::path_editing::session().unwrap();
    session.seek(0.).unwrap();
    assert_eq!(session.frame().objects.len(), 3);
    let start = session.frame().objects.clone();
    session.seek(0.2).unwrap();
    assert_eq!(session.frame().objects, start);
}

#[test]
fn handle_distance_scaling_matches_cubic_controls_in_authored_and_live_paths() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut object = scene
            .path(
                VectorPath::new().move_to(Vec2::ZERO).cubic_to(
                    Vec2::new(2., 0.),
                    Vec2::new(4., 0.),
                    Vec2::new(6., 0.),
                ),
                Default::default(),
            )
            .unwrap();
        scene.add(&object).unwrap();
        let mut session = scene.execution_session().unwrap();
        if live_mode {
            noon::integration::publish_borrowed_path_edit(
                scene.integration_store(),
                scene.root(),
                &mut session,
                noon::integration::BorrowedPathEdit::ScaleHandles {
                    object: &object,
                    factor: 0.5,
                },
            )
            .unwrap();
        } else {
            object.scale_handle_to_anchor_distances(0.5).unwrap();
        }
        let query = if live_mode {
            noon::integration::effective_path_query(scene.integration_store(), &session, &object)
                .unwrap()
        } else {
            object.path_query().unwrap()
        };
        assert_eq!(
            query.curve_points(0).unwrap(),
            [(0., 0.), (1., 0.), (5., 0.), (6., 0.)]
        );

        let revision = scene.revision();
        object.scale_handle_to_anchor_distances(1.0).unwrap();
        assert_eq!(scene.revision(), revision, "factor one must be a no-op");

        let state = object.state().unwrap();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        assert!(object.scale_handle_to_anchor_distances(f64::NAN).is_err());
        assert_eq!(object.state().unwrap(), state);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
    }
}

#[test]
fn handle_distance_scaling_promotes_lines_and_quadratics_exactly() {
    let mut scene = Scene::new();
    let mut line = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(6., 0.)),
            Default::default(),
        )
        .unwrap();
    line.scale_handle_to_anchor_distances(0.5).unwrap();
    assert_eq!(
        line.path_query().unwrap().curve_points(0).unwrap(),
        [(0., 0.), (1., 0.), (5., 0.), (6., 0.)]
    );

    let mut quadratic = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .quadratic_to(Vec2::new(3., 3.), Vec2::new(6., 0.)),
            Default::default(),
        )
        .unwrap();
    quadratic.scale_handle_to_anchor_distances(0.5).unwrap();
    assert_eq!(
        quadratic.path_query().unwrap().curve_points(0).unwrap(),
        [(0., 0.), (1., 1.), (5., 1.), (6., 0.)]
    );
}
