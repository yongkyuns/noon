//! Quadratic degree elevation retains semantic input and prefix precision.
use super::*;

pub(super) fn append_quadratic_f64(
    path: VectorPath,
    control: (f64, f64),
    anchor: (f64, f64),
    stride: i32,
) -> Result<VectorPath, AuthoringError> {
    append_quadratic_3d(
        path,
        SemanticVec3::new(control.0, control.1, 0.0),
        SemanticVec3::new(anchor.0, anchor.1, 0.0),
        stride,
    )
}

pub(super) fn append_quadratic_3d(
    path: VectorPath,
    control: SemanticVec3,
    anchor: SemanticVec3,
    stride: i32,
) -> Result<VectorPath, AuthoringError> {
    point_arrays::validate(&[control, anchor])?;
    let points = point_arrays::points(&path)?;
    let start = points.last().copied().ok_or(AuthoringError::PathQuery(
        noon_geometry::PathProportionError::EmptyPath,
    ))?;
    // Match CE's degree-elevation arithmetic independently on all three axes.
    let elevate = |end: SemanticVec3| {
        SemanticVec3::new(
            (2.0 / 3.0) * control.x + (1.0 / 3.0) * end.x,
            (2.0 / 3.0) * control.y + (1.0 / 3.0) * end.y,
            (2.0 / 3.0) * control.z + (1.0 / 3.0) * end.z,
        )
    };
    append_manim_cubic_points_with_stride(
        points,
        [elevate(start), elevate(anchor), anchor],
        stride,
        VectorPath::from_manim_points_3d,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::{publish_borrowed_path_edit, BorrowedPathEdit};

    #[test]
    fn quadratic_native_vec2_api_keeps_ce_degree_elevation_precision() {
        let mut scene = crate::Scene::new();
        let mut object = scene.path(VectorPath::new(), Default::default()).unwrap();
        object.set_points_f64(&[(0.0, 0.0)]).unwrap();
        object
            .add_quadratic_bezier_curve_to(Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0))
            .unwrap();
        assert_eq!(
            object.path_query().unwrap().points().unwrap(),
            [
                (0.0, 0.0),
                (0.0, 0.6666666666666666),
                (0.3333333333333333, 0.6666666666666666),
                (1.0, 0.0),
            ]
        );
    }

    // Actual pinned CE result, including a complete raw prefix and coordinates
    // that cannot survive an f32 round-trip. Expected values are not computed
    // using the implementation's degree-elevation helper.
    const PREFIX: [(f64, f64); 4] = [
        (0.123456789012345, 0.234567890123456),
        (0.345678901234567, 0.456789012345678),
        (0.567890123456789, 0.67890123456789),
        (0.789012345678901, 0.890123456789012),
    ];
    const CONTROL: (f64, f64) = (0.123456789012345, 0.987654321098765);
    const ANCHOR: (f64, f64) = (1.23456789012345, -0.456789012345678);

    #[test]
    fn quadratic_f64_authored_running_and_borrowed_paths_match_pinned_ce() {
        for mode in 0..3 {
            let mut scene = crate::Scene::new();
            let mut object = scene.path(VectorPath::new(), Default::default()).unwrap();
            object.set_points_f64(&PREFIX).unwrap();
            scene.add(&object).unwrap();
            let unrelated = scene.square(2.0).unwrap();
            let before = unrelated.state().unwrap();
            match mode {
                0 => object
                    .add_quadratic_bezier_curve_to_f64(CONTROL, ANCHOR)
                    .unwrap(),
                1 => {
                    let execution = scene.execution_session().unwrap();
                    scene.install_execution(execution);
                    scene
                        .add_quadratic_bezier_curve_to_f64(&object, CONTROL, ANCHOR)
                        .unwrap();
                }
                _ => {
                    let mut execution = scene.execution_session().unwrap();
                    publish_borrowed_path_edit(
                        scene.integration_store(),
                        scene.root(),
                        &mut execution,
                        BorrowedPathEdit::QuadraticF64WithStride {
                            object: &object,
                            control: CONTROL,
                            anchor: ANCHOR,
                            stride: 4,
                        },
                    )
                    .unwrap();
                }
            }
            let actual = object.path_query().unwrap().points().unwrap();
            assert_eq!(&actual[..4], &PREFIX);
            assert_eq!(actual[4], PREFIX[3]);
            assert_eq!(actual[7], ANCHOR);
            let expected = [
                (0.3453086412345303, 0.9551440329955139),
                (0.49382715604938, 0.5061732099506173),
            ];
            for (actual, expected) in actual[5..7].iter().zip(expected) {
                assert!((actual.0 - expected.0).abs() < 1e-15, "mode {mode}");
                assert!((actual.1 - expected.1).abs() < 1e-15, "mode {mode}");
            }
            assert_eq!(unrelated.state().unwrap(), before);
        }
    }

    #[test]
    fn quadratic_f64_failure_preserves_revision_payload_and_resources() {
        for live in [false, true] {
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
            let result = if live {
                let execution = scene.execution_session().unwrap();
                scene.install_execution(execution);
                scene.add_quadratic_bezier_curve_to_f64(&object, (f64::NAN, 0.0), ANCHOR)
            } else {
                object.add_quadratic_bezier_curve_to_f64((f64::NAN, 0.0), ANCHOR)
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
