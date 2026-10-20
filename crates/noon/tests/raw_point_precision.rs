use noon::{integration::BorrowedPathEdit, ManimGeometryOptions, Scene};
use noon_core::{SemanticTransform2_5D, SemanticVec3, VectorPath};

fn raw_points() -> Vec<(f64, f64)> {
    let delta = 2.0_f64.powi(-40);
    let x = 1.0 + delta;
    let y = -2.0 - 2.0 * delta;
    vec![
        (x, y),
        (x + 0.25, y + 0.5),
        (x + 0.5, y - 0.25),
        (x + 1.0, y),
    ]
}

#[test]
fn raw_coordinates_survive_authored_and_effective_edits_without_renderer_quantization() {
    for live in [false, true] {
        let mut scene = Scene::new();
        let mut object = scene.square(1.0).unwrap();
        scene.add(&object).unwrap();
        let mut execution = scene.execution_session().unwrap();
        let points = raw_points();
        if live {
            noon::integration::publish_borrowed_path_edit(
                scene.integration_store(),
                scene.root(),
                &mut execution,
                BorrowedPathEdit::RawPoints {
                    object: &object,
                    points: &points,
                },
            )
            .unwrap();
        } else {
            object.set_points_f64(&points).unwrap();
        }
        let query = object.path_query().unwrap();
        assert_eq!(query.points().unwrap(), points);
        assert_eq!(query.start().unwrap(), points[0]);
        assert_eq!(query.end().unwrap(), points[3]);
        assert_eq!(query.point_from_proportion(0.0).unwrap(), points[0]);
        assert_eq!(query.point_from_proportion(1.0).unwrap(), points[3]);
        assert_eq!(
            query
                .proportion_from_point((points[3].0, points[3].1, 0.0))
                .unwrap(),
            Some(1.0)
        );
        let tail = [(3.0 + 2.0_f64.powi(-39), 4.0)];
        if live {
            noon::integration::publish_borrowed_path_edit(
                scene.integration_store(),
                scene.root(),
                &mut execution,
                BorrowedPathEdit::AppendRawPoints {
                    object: &object,
                    points: &tail,
                },
            )
            .unwrap();
            let effective = noon::integration::effective_path_query(
                scene.integration_store(),
                &execution,
                &object,
            )
            .unwrap();
            assert_eq!(
                effective.points().unwrap(),
                [points.clone(), tail.to_vec()].concat()
            );
        } else {
            object.append_points_f64(&tail).unwrap();
        }
        assert_eq!(
            query.points().unwrap(),
            points,
            "prepared snapshots remain immutable"
        );
        let mut expected = points.clone();
        expected.extend(tail);
        if live {
            noon::integration::publish_borrowed_path_edit(
                scene.integration_store(),
                scene.root(),
                &mut execution,
                BorrowedPathEdit::ReversePoints { object: &object },
            )
            .unwrap();
        } else {
            object.reverse_points().unwrap();
        }
        expected.reverse();
        assert_eq!(object.path_query().unwrap().points().unwrap(), expected);
        if live {
            noon::integration::publish_borrowed_path_edit(
                scene.integration_store(),
                scene.root(),
                &mut execution,
                BorrowedPathEdit::RepeatPoints {
                    object: &object,
                    count: 2,
                },
            )
            .unwrap();
        } else {
            object.repeat_points(2).unwrap();
        }
        let expected = [expected.clone(), expected].concat();
        assert_eq!(object.path_query().unwrap().points().unwrap(), expected);
        if live {
            assert_eq!(
                noon::integration::effective_path_query(
                    scene.integration_store(),
                    &execution,
                    &object
                )
                .unwrap()
                .points()
                .unwrap(),
                expected
            );
        }
        let copied = object.copy_handle().unwrap();
        assert_eq!(copied.path_query().unwrap().points().unwrap(), expected);
    }
}

#[test]
fn raw_curve_measure_resolves_intervals_smaller_than_one_renderer_coordinate() {
    let delta = 2.0_f64.powi(-40);
    let points = (0..4)
        .map(|i| (1.0 + i as f64 * delta, 2.0))
        .collect::<Vec<_>>();
    let mut scene = Scene::new();
    let object = scene
        .geometry(
            ManimGeometryOptions::path(VectorPath::from_manim_points_f64(points.clone())).unwrap(),
        )
        .unwrap();
    let query = object.path_query().unwrap();
    assert_eq!(query.points().unwrap(), points);
    let length = query.arc_length(None).unwrap();
    assert!((length / delta - 3.0).abs() < 1e-5);
    assert_eq!(length, query.curve_length(0, None).unwrap());
    assert_eq!(
        query.arc_length(Some(4)).unwrap(),
        query.curve_length(0, Some(4)).unwrap()
    );
    assert_eq!(
        query.point_from_proportion(0.5).unwrap(),
        (1.0 + 1.5 * delta, 2.0)
    );
    assert_eq!(query.curve_points(0).unwrap(), points.as_slice());
}

#[test]
fn semantic_transform_and_baked_append_keep_raw_tail_and_coordinate_precision() {
    let points = raw_points();
    let transform = SemanticTransform2_5D {
        translation: SemanticVec3::new(2.0_f64.powi(-42), -0.5, 0.0),
        ..Default::default()
    };
    let path = VectorPath::from_manim_points_f64(points.clone()).transformed_semantic(transform);
    let expected = points
        .iter()
        .map(|&(x, y)| transform.transform_xy(x, y))
        .collect::<Vec<_>>();
    assert_eq!(path.manim_points().unwrap(), expected);
    let mut scene = Scene::new();
    let mut object = scene
        .path(
            VectorPath::from_manim_points_f64(points),
            Default::default(),
        )
        .unwrap();
    object
        .shift(transform.translation.x, transform.translation.y)
        .unwrap();
    object.append_points_f64(&[(4.0, 5.0)]).unwrap();
    assert_eq!(
        object.path_query().unwrap().points().unwrap(),
        [expected, vec![(4.0, 5.0)]].concat()
    );
}

#[test]
fn rejected_raw_coordinate_edits_do_not_publish_or_admit_resources() {
    let mut scene = Scene::new();
    let mut object = scene
        .path(
            VectorPath::from_manim_points_f64(raw_points()),
            Default::default(),
        )
        .unwrap();
    let before = object.path_query().unwrap().points().unwrap();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    for invalid in [f64::NAN, f64::INFINITY, f64::MAX] {
        assert!(object.set_points_f64(&[(invalid, 0.0)]).is_err());
        assert!(object.append_points_f64(&[(0.0, invalid)]).is_err());
        assert_eq!(object.path_query().unwrap().points().unwrap(), before);
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
}

#[test]
fn interpolation_and_handle_scaling_retain_raw_double_coordinates() {
    let points = raw_points();
    let end = points
        .iter()
        .map(|&(x, y)| (x + 2.0, y + 1.0))
        .collect::<Vec<_>>();
    let mut scene = Scene::new();
    let start = scene
        .path(
            VectorPath::from_manim_points_f64(points.clone()),
            Default::default(),
        )
        .unwrap();
    let target = scene
        .path(VectorPath::from_manim_points_f64(end), Default::default())
        .unwrap();
    let mut output = scene.square(1.0).unwrap();
    output.interpolate_manim(&start, &target, 0.25).unwrap();
    let mut expected = points
        .iter()
        .map(|&(x, y)| (x + 0.5, y + 0.25))
        .collect::<Vec<_>>();
    assert_eq!(output.path_query().unwrap().points().unwrap(), expected);
    output.scale_handle_to_anchor_distances(0.5).unwrap();
    for (anchor, handle) in [(0, 1), (3, 2)] {
        expected[handle] = (
            expected[anchor].0 + (expected[handle].0 - expected[anchor].0) * 0.5,
            expected[anchor].1 + (expected[handle].1 - expected[anchor].1) * 0.5,
        );
    }
    assert_eq!(output.path_query().unwrap().points().unwrap(), expected);
}

#[test]
fn raw_drawable_commands_are_derived_after_semantic_transform_cancellation() {
    let delta = 2.0_f64.powi(-40);
    let path = VectorPath::from_manim_points_f64(vec![(1.0 + delta, 0.0); 4]);
    let scale = 2.0_f64.powi(40);
    let transformed = path.transformed_semantic(SemanticTransform2_5D {
        translation: SemanticVec3::new(-scale, 0.0, 0.0),
        scale: SemanticVec3::new(scale, 1.0, 1.0),
        ..Default::default()
    });
    assert_eq!(transformed.manim_points().unwrap(), [(1.0, 0.0); 4]);
    assert_eq!(
        transformed.endpoints().unwrap(),
        (
            noon_core::Vec2::new(1.0, 0.0),
            noon_core::Vec2::new(1.0, 0.0)
        )
    );
}
