use super::*;

fn xyz(values: &serde_json::Value) -> Vec<SemanticVec3> {
    values
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            SemanticVec3::new(
                p[0].as_f64().unwrap(),
                p[1].as_f64().unwrap(),
                p[2].as_f64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn scalar_planar_line_preserves_xyz_prefix_from_actual_ce() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/manim-v0.21/scalar-xyz.json"
    ))
    .unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["spec"]["method"] == "add_line_to"
                && c["spec"]["prefixRows"] == 4
                && c["spec"]["stride"] == 4
                && c["spec"]["planarInput"] == true
                && c["spec"]["posed"] == false
        })
        .unwrap();
    let mut scene = crate::Scene::new();
    let mut object = scene
        .path(
            VectorPath::from_manim_points_3d(xyz(&case["prefixXYZ"])),
            Default::default(),
        )
        .unwrap();
    let point = xyz(&case["inputXYZ"])[2];
    object.add_line_to_f64(point.xy()).unwrap();
    assert_eq!(
        object.path_query().unwrap().points_3d().unwrap(),
        xyz(&case["pointsXYZ"])
    );
}

use crate::integration::{publish_borrowed_path_edit, BorrowedPathEdit};

fn edit_xyz_object(
    object: &mut Mobject,
    method: &str,
    points: &[SemanticVec3],
    stride: i32,
) -> Result<(), AuthoringError> {
    match method {
        "start_new_path" => object.start_new_path_3d_with_stride(points[2], stride),
        "add_line_to" => object.add_line_to_3d_with_stride(points[2], stride),
        "add_cubic_bezier_curve_to" => {
            object.add_cubic_bezier_curve_to_3d_with_stride(points[0], points[1], points[2], stride)
        }
        "add_cubic_bezier_curve" => {
            object.add_cubic_bezier_curve_3d(points[0], points[1], points[2], points[3])
        }
        "set_points_as_corners" => object.set_points_as_corners_3d(&points[2..]),
        "add_points_as_corners" => {
            object.add_points_as_corners_3d_with_stride(&points[2..], stride, &LINE_SAMPLES)
        }
        "add_quadratic_bezier_curve_to" => {
            object.add_quadratic_bezier_curve_to_3d_with_stride(points[0], points[2], stride)
        }
        "close_path" => object.close_path_with_stride(stride, 1e-6, &LINE_SAMPLES),
        _ => panic!("unhandled CE method {method}"),
    }
}
fn edit_xyz_scene(
    scene: &mut crate::Scene,
    object: &Mobject,
    method: &str,
    points: &[SemanticVec3],
    stride: i32,
) -> Result<(), AuthoringError> {
    match method {
        "start_new_path" => scene.start_new_path_3d_with_stride(object, points[2], stride),
        "add_line_to" => scene.add_line_to_3d_with_stride(object, points[2], stride),
        "add_cubic_bezier_curve_to" => scene.add_cubic_bezier_curve_to_3d_with_stride(
            object, points[0], points[1], points[2], stride,
        ),
        "add_cubic_bezier_curve" => {
            scene.add_cubic_bezier_curve_3d(object, points[0], points[1], points[2], points[3])
        }
        "set_points_as_corners" => scene.set_points_as_corners_3d(object, &points[2..]),
        "add_points_as_corners" => {
            scene.add_points_as_corners_3d_with_stride(object, &points[2..], stride, &LINE_SAMPLES)
        }
        "add_quadratic_bezier_curve_to" => {
            scene.add_quadratic_bezier_curve_to_3d_with_stride(object, points[0], points[2], stride)
        }
        "close_path" => scene.close_path_with_stride(object, stride, 1e-6, &LINE_SAMPLES),
        _ => panic!("unhandled CE method {method}"),
    }
}
fn borrowed_xyz_edit<'a>(
    object: &'a Mobject,
    method: &str,
    points: &'a [SemanticVec3],
    stride: i32,
) -> BorrowedPathEdit<'a> {
    match method {
        "start_new_path" => BorrowedPathEdit::Start3dWithStride {
            object,
            point: points[2],
            stride,
        },
        "add_line_to" => BorrowedPathEdit::Line3dWithStride {
            object,
            point: points[2],
            stride,
        },
        "add_cubic_bezier_curve_to" => BorrowedPathEdit::Cubic3dWithStride {
            object,
            control1: points[0],
            control2: points[1],
            anchor: points[2],
            stride,
        },
        "add_cubic_bezier_curve" => BorrowedPathEdit::CubicCurve3d {
            object,
            anchor1: points[0],
            handle1: points[1],
            handle2: points[2],
            anchor2: points[3],
        },
        "set_points_as_corners" => BorrowedPathEdit::Corners3d {
            object,
            points: &points[2..],
        },
        "add_points_as_corners" => BorrowedPathEdit::AppendCorners3dWithStride {
            object,
            points: &points[2..],
            stride,
            bezier_t_values: &LINE_SAMPLES,
        },
        "add_quadratic_bezier_curve_to" => BorrowedPathEdit::Quadratic3dWithStride {
            object,
            control: points[0],
            anchor: points[2],
            stride,
        },
        "close_path" => BorrowedPathEdit::CloseWithStride {
            object,
            stride,
            tolerance: 1e-6,
            bezier_t_values: &LINE_SAMPLES,
        },
        _ => panic!("unhandled CE method {method}"),
    }
}
fn assert_xyz_close(actual: &[SemanticVec3], expected: &serde_json::Value, context: &str) {
    let expected = xyz(expected);
    assert_eq!(actual.len(), expected.len(), "{context}");
    for (i, (a, b)) in actual.iter().zip(expected).enumerate() {
        for (coordinate, (a, b)) in [(a.x, b.x), (a.y, b.y), (a.z, b.z)].into_iter().enumerate() {
            assert!(
                (a - b).abs() <= 1e-14 * (1.0 + b.abs()),
                "{context}: row {i}, coordinate {coordinate}: {a} != {b}"
            );
        }
    }
}

#[test]
fn scalar_xyz_edits_match_all_pinned_ce_controls_in_three_execution_modes() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/manim-v0.21/scalar-xyz.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 225);
    for mode in 0..3 {
        for case in cases {
            let spec = &case["spec"];
            let method = spec["method"].as_str().unwrap();
            let stride = spec["stride"].as_i64().unwrap() as i32;
            let context = format!("mode={mode}, spec={spec}");
            let mut scene = crate::Scene::new();
            let mut object = scene
                .path(
                    VectorPath::from_manim_points_3d(xyz(&case["prefixXYZ"])),
                    Default::default(),
                )
                .unwrap();
            if spec["posed"] == true {
                crate::LayoutAnchor::from(&object)
                    .stretch(
                        2.0,
                        crate::LayoutDimension::Depth,
                        crate::ManimRotationPivot::Point3d(SemanticVec3::ZERO),
                    )
                    .unwrap();
                object.shift_3d(SemanticVec3::new(1.5, -2.5, 3.0)).unwrap();
            }
            scene.add(&object).unwrap();
            let unrelated = scene.square(2.).unwrap();
            let unrelated_before = unrelated.state().unwrap();
            let snapshot = object.path_query().unwrap();
            assert_xyz_close(&snapshot.points_3d().unwrap(), &case["beforeXYZ"], &context);
            let inputs = xyz(&case["inputXYZ"]);
            let before = object.state().unwrap();
            let revision = scene.revision();
            let resources = scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len();
            let id = object.node_id();
            let mut borrowed = None;
            let result = match mode {
                0 => edit_xyz_object(&mut object, method, &inputs, stride),
                1 => {
                    let execution = scene.execution_session().unwrap();
                    scene.install_execution(execution);
                    edit_xyz_scene(&mut scene, &object, method, &inputs, stride)
                }
                _ => {
                    let mut execution = scene.execution_session().unwrap();
                    let result = publish_borrowed_path_edit(
                        scene.integration_store(),
                        scene.root(),
                        &mut execution,
                        borrowed_xyz_edit(&object, method, &inputs, stride),
                    );
                    borrowed = Some(execution);
                    result
                }
            };
            if case["outcome"]["kind"] == "raised" {
                assert!(
                    matches!(
                        result,
                        Err(AuthoringError::NoSubpathForClose {
                            point_count: 4,
                            stride: 5
                        })
                    ),
                    "{context}: {result:?}"
                );
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
            } else {
                result.unwrap_or_else(|e| panic!("{context}: {e}"));
            }
            let actual =
                if let Some(execution) = borrowed.as_ref().or_else(|| scene.running_execution()) {
                    crate::path_queries::effective_path_query(
                        scene.integration_store(),
                        execution,
                        &object,
                    )
                    .unwrap()
                } else {
                    object.path_query().unwrap()
                };
            assert_xyz_close(&actual.points_3d().unwrap(), &case["pointsXYZ"], &context);
            assert_xyz_close(&snapshot.points_3d().unwrap(), &case["beforeXYZ"], &context);
            assert_eq!(object.node_id(), id);
            assert_eq!(unrelated.state().unwrap(), unrelated_before);
        }
    }
}

#[test]
fn invalid_scalar_xyz_inputs_preserve_identity_payload_revision_and_resources() {
    for mode in 0..3 {
        for method in [
            "start_new_path",
            "add_line_to",
            "add_cubic_bezier_curve_to",
            "add_cubic_bezier_curve",
            "set_points_as_corners",
            "add_points_as_corners",
            "add_quadratic_bezier_curve_to",
        ] {
            let mut scene = crate::Scene::new();
            let mut object = scene
                .path(
                    VectorPath::from_manim_points_3d(vec![SemanticVec3::new(1., 2., 3.); 4]),
                    Default::default(),
                )
                .unwrap();
            scene.add(&object).unwrap();
            let before = object.state().unwrap();
            let revision = scene.revision();
            let resources = scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len();
            let mut points = [SemanticVec3::new(2., 3., 4.); 4];
            points[2].z = f64::NAN;
            let result = match mode {
                0 => edit_xyz_object(&mut object, method, &points, 4),
                1 => {
                    let execution = scene.execution_session().unwrap();
                    scene.install_execution(execution);
                    edit_xyz_scene(&mut scene, &object, method, &points, 4)
                }
                _ => {
                    let mut execution = scene.execution_session().unwrap();
                    publish_borrowed_path_edit(
                        scene.integration_store(),
                        scene.root(),
                        &mut execution,
                        borrowed_xyz_edit(&object, method, &points, 4),
                    )
                }
            };
            assert!(result.is_err(), "mode={mode}, method={method}");
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

#[test]
fn command_only_path_depth_is_captured_before_scalar_edits_and_empty_corners_are_a_noop() {
    // Actual CE Line([0,0,0], [1,0,0]).shift([0,0,3]).add_line_to([2,0,0])
    // interpolates Z from 3 to 0; an empty corner append preserves the array.
    for mode in 0..3 {
        for empty in [false, true] {
            let mut scene = crate::Scene::new();
            let path = VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1., 0.));
            assert!(path.manim_points().is_none());
            let mut object = scene.path(path, Default::default()).unwrap();
            let mut transform = object.state().unwrap().transform;
            transform.translation.z = 3.;
            transform.scale.z = 2.;
            let mut tx = SemanticMutationTransaction::new();
            tx.set_object_transform(object.node_id(), transform);
            tx.apply(&mut scene.integration_store().borrow_mut())
                .unwrap();
            scene.add(&object).unwrap();
            let before = object.state().unwrap();
            let revision = scene.revision();
            let resources = scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len();
            let mut borrowed = None;
            let endpoint = SemanticVec3::new(2., 0., 0.);
            match mode {
                0 => {
                    if empty {
                        object.add_points_as_corners_3d(&[])
                    } else {
                        object.add_line_to_3d(endpoint)
                    }
                }
                1 => {
                    let session = scene.execution_session().unwrap();
                    scene.install_execution(session);
                    if empty {
                        scene.add_points_as_corners_3d(&object, &[])
                    } else {
                        scene.add_line_to_3d(&object, endpoint)
                    }
                }
                _ => {
                    let mut session = scene.execution_session().unwrap();
                    let edit = if empty {
                        BorrowedPathEdit::AppendCorners3d {
                            object: &object,
                            points: &[],
                        }
                    } else {
                        BorrowedPathEdit::Line3dWithStride {
                            object: &object,
                            point: endpoint,
                            stride: 4,
                        }
                    };
                    let result = publish_borrowed_path_edit(
                        scene.integration_store(),
                        scene.root(),
                        &mut session,
                        edit,
                    );
                    borrowed = Some(session);
                    result
                }
            }
            .unwrap();
            if empty {
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
            let query =
                if let Some(session) = borrowed.as_ref().or_else(|| scene.running_execution()) {
                    crate::path_queries::effective_path_query(
                        scene.integration_store(),
                        session,
                        &object,
                    )
                    .unwrap()
                } else {
                    object.path_query().unwrap()
                };
            let points = query.points_3d().unwrap();
            assert_eq!(
                points.iter().take(4).map(|p| p.z).collect::<Vec<_>>(),
                vec![3.; 4]
            );
            if !empty {
                assert_eq!(points.len(), 8);
                assert_eq!(
                    points[4..].iter().map(|p| p.z).collect::<Vec<_>>(),
                    vec![3., 2., 1., 0.]
                );
            }
        }
    }
}
