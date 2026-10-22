use super::*;
use crate::integration::{publish_borrowed_path_edit, BorrowedPathEdit};

const PREFIX: [Point; 4] = [
    (0.123456789012345, 0.234567890123456),
    (0.345678901234567, 0.456789012345678),
    (0.567890123456789, 0.67890123456789),
    (0.789012345678901, 0.890123456789012),
];
const POINT: Point = (1.23456789012345, -0.456789012345678);
const HANDLE1: Point = (0.123456789012345, 0.987654321098765);
const HANDLE2: Point = (0.876543210987654, 0.567890123456789);

#[test]
fn vec2_conveniences_do_not_lower_an_existing_semantic_precision_prefix() {
    for edit in 0..6 {
        let mut scene = crate::Scene::new();
        let mut object = scene.path(VectorPath::new(), Default::default()).unwrap();
        object.set_points_f64(&PREFIX).unwrap();
        let point = Vec2::new(1.25, -0.5);
        match edit {
            0 => object.start_new_path(point),
            1 => object.add_line_to(point),
            2 => {
                object.add_cubic_bezier_curve_to(Vec2::new(1.0, 0.5), Vec2::new(1.25, 0.25), point)
            }
            3 => object.add_cubic_bezier_curve(
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.5),
                Vec2::new(1.25, 0.25),
                point,
            ),
            4 => object.add_points_as_corners(&[point]),
            _ => object.close_path(),
        }
        .unwrap();
        assert_eq!(
            &object.path_query().unwrap().points().unwrap()[..4],
            &PREFIX,
            "edit {edit}"
        );
    }
}

#[test]
fn scalar_edits_match_pinned_ce_in_authored_scene_owned_and_borrowed_execution() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/manim-v0.21/scalar-path-precision.json"
    ))
    .unwrap();
    assert_eq!(fixture["manimVersion"], "0.21.0");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 10);
    for mode in 0..3 {
        for expected in cases {
            let method = expected["method"].as_str().unwrap();
            let mut scene = crate::Scene::new();
            let mut object = scene.path(VectorPath::new(), Default::default()).unwrap();
            object.set_points_f64(&PREFIX).unwrap();
            scene.add(&object).unwrap();
            let unrelated = scene.square(2.0).unwrap();
            let before = unrelated.state().unwrap();
            let points = [HANDLE1, POINT];
            let corners = [PREFIX[0], HANDLE1, POINT];
            match mode {
                0 => match method {
                    "set_points" => object.set_points_f64(&points),
                    "append_points" => object.append_points_f64(&points),
                    "start_new_path" => object.start_new_path_f64(POINT),
                    "add_line_to" => object.add_line_to_f64(POINT),
                    "add_quadratic_bezier_curve_to" => {
                        object.add_quadratic_bezier_curve_to_f64(HANDLE1, POINT)
                    }
                    "add_cubic_bezier_curve_to" => {
                        object.add_cubic_bezier_curve_to_f64(HANDLE1, HANDLE2, POINT)
                    }
                    "add_cubic_bezier_curve" => {
                        object.add_cubic_bezier_curve_f64(PREFIX[0], HANDLE1, HANDLE2, POINT)
                    }
                    "add_points_as_corners" => object.add_points_as_corners_f64(&points),
                    "set_points_as_corners" => object.set_points_as_corners_f64(&corners),
                    "close_path" => object.close_path(),
                    _ => panic!("uncovered pinned method: {method}"),
                }
                .unwrap(),
                1 => {
                    let execution = scene.execution_session().unwrap();
                    scene.install_execution(execution);
                    match method {
                        "set_points" => scene.set_points_f64(&object, &points),
                        "append_points" => scene.append_points_f64(&object, &points),
                        "start_new_path" => scene.start_new_path_f64(&object, POINT),
                        "add_line_to" => scene.add_line_to_f64(&object, POINT),
                        "add_quadratic_bezier_curve_to" => {
                            scene.add_quadratic_bezier_curve_to_f64(&object, HANDLE1, POINT)
                        }
                        "add_cubic_bezier_curve_to" => {
                            scene.add_cubic_bezier_curve_to_f64(&object, HANDLE1, HANDLE2, POINT)
                        }
                        "add_cubic_bezier_curve" => scene.add_cubic_bezier_curve_f64(
                            &object, PREFIX[0], HANDLE1, HANDLE2, POINT,
                        ),
                        "add_points_as_corners" => {
                            scene.add_points_as_corners_f64(&object, &points)
                        }
                        "set_points_as_corners" => {
                            scene.set_points_as_corners_f64(&object, &corners)
                        }
                        "close_path" => scene.close_path(&object),
                        _ => panic!("uncovered pinned method: {method}"),
                    }
                    .unwrap();
                }
                _ => {
                    let mut execution = scene.execution_session().unwrap();
                    let edit = match method {
                        "set_points" => BorrowedPathEdit::RawPoints {
                            object: &object,
                            points: &points,
                        },
                        "append_points" => BorrowedPathEdit::AppendRawPoints {
                            object: &object,
                            points: &points,
                        },
                        "start_new_path" => BorrowedPathEdit::StartF64WithStride {
                            object: &object,
                            point: POINT,
                            stride: 4,
                        },
                        "add_line_to" => BorrowedPathEdit::LineF64WithSamples {
                            object: &object,
                            point: POINT,
                            stride: 4,
                            bezier_t_values: &LINE_SAMPLES,
                        },
                        "add_quadratic_bezier_curve_to" => {
                            BorrowedPathEdit::QuadraticF64WithStride {
                                object: &object,
                                control: HANDLE1,
                                anchor: POINT,
                                stride: 4,
                            }
                        }
                        "add_cubic_bezier_curve_to" => BorrowedPathEdit::CubicF64WithStride {
                            object: &object,
                            control1: HANDLE1,
                            control2: HANDLE2,
                            anchor: POINT,
                            stride: 4,
                        },
                        "add_cubic_bezier_curve" => BorrowedPathEdit::CubicCurveF64 {
                            object: &object,
                            anchor1: PREFIX[0],
                            handle1: HANDLE1,
                            handle2: HANDLE2,
                            anchor2: POINT,
                        },
                        "add_points_as_corners" => BorrowedPathEdit::AppendCornersF64WithStride {
                            object: &object,
                            points: &points,
                            stride: 4,
                            bezier_t_values: &LINE_SAMPLES,
                        },
                        "set_points_as_corners" => BorrowedPathEdit::CornersF64WithSamples {
                            object: &object,
                            points: &corners,
                            bezier_t_values: &LINE_SAMPLES,
                        },
                        "close_path" => BorrowedPathEdit::Close { object: &object },
                        _ => panic!("uncovered pinned method: {method}"),
                    };
                    publish_borrowed_path_edit(
                        scene.integration_store(),
                        scene.root(),
                        &mut execution,
                        edit,
                    )
                    .unwrap();
                }
            }
            let actual = object.path_query().unwrap().points().unwrap();
            let expected = expected["pointsXY"].as_array().unwrap();
            assert_eq!(actual.len(), expected.len(), "{mode}/{method}");
            for (actual, expected) in actual.iter().zip(expected) {
                let expected = (expected[0].as_f64().unwrap(), expected[1].as_f64().unwrap());
                assert!(
                    (actual.0 - expected.0).abs() <= 1e-12 * (1.0 + expected.0.abs()),
                    "{mode}/{method}: {actual:?} != {expected:?}"
                );
                assert!(
                    (actual.1 - expected.1).abs() <= 1e-12 * (1.0 + expected.1.abs()),
                    "{mode}/{method}: {actual:?} != {expected:?}"
                );
            }
            assert_eq!(unrelated.state().unwrap(), before);
        }
    }
}

#[test]
fn cubic_batches_preserve_incoming_and_existing_semantic_precision() {
    let curves = [[PREFIX[0], HANDLE1, HANDLE2, POINT]];
    for mode in 0..3 {
        for replace in [false, true] {
            let mut scene = crate::Scene::new();
            let mut object = scene.path(VectorPath::new(), Default::default()).unwrap();
            object.set_points_f64(&PREFIX).unwrap();
            scene.add(&object).unwrap();
            match mode {
                0 => {
                    if replace {
                        object.set_cubic_bezier_curves_f64(&curves)
                    } else {
                        object.add_cubic_bezier_curves_f64(&curves)
                    }
                    .unwrap();
                }
                1 => {
                    let execution = scene.execution_session().unwrap();
                    scene.install_execution(execution);
                    if replace {
                        scene.set_cubic_bezier_curves_f64(&object, &curves)
                    } else {
                        scene.add_cubic_bezier_curves_f64(&object, &curves)
                    }
                    .unwrap();
                }
                _ => {
                    let mut execution = scene.execution_session().unwrap();
                    let edit = if replace {
                        BorrowedPathEdit::SetCubicCurvesF64 {
                            object: &object,
                            curves: &curves,
                        }
                    } else {
                        BorrowedPathEdit::CubicCurvesF64 {
                            object: &object,
                            curves: &curves,
                        }
                    };
                    publish_borrowed_path_edit(
                        scene.integration_store(),
                        scene.root(),
                        &mut execution,
                        edit,
                    )
                    .unwrap();
                }
            }
            let expected = if replace {
                curves[0].to_vec()
            } else {
                [PREFIX.as_slice(), curves[0].as_slice()].concat()
            };
            assert_eq!(object.path_query().unwrap().points().unwrap(), expected);
        }
    }
}

#[test]
fn scalar_invalid_inputs_preserve_payload_revision_and_resources() {
    for live in [false, true] {
        for edit in 0..3 {
            let mut scene = crate::Scene::new();
            let mut object = scene.path(VectorPath::new(), Default::default()).unwrap();
            object.set_points_f64(&PREFIX).unwrap();
            scene.add(&object).unwrap();
            let before = object.state().unwrap();
            let revision = scene.revision();
            let resources = scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len();
            if live {
                let execution = scene.execution_session().unwrap();
                scene.install_execution(execution);
            }
            let invalid = (f64::NAN, 0.0);
            let result = match (live, edit) {
                (false, 0) => object.start_new_path_f64(invalid),
                (false, 1) => object.add_line_to_f64(invalid),
                (false, _) => object.set_points_as_corners_f64(&[PREFIX[0], invalid]),
                (true, 0) => scene.start_new_path_f64(&object, invalid),
                (true, 1) => scene.add_line_to_f64(&object, invalid),
                (true, _) => scene.set_points_as_corners_f64(&object, &[PREFIX[0], invalid]),
            };
            assert!(result.is_err());
            assert_eq!(object.state().unwrap(), before);
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
}
