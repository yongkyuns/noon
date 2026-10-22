use super::*;

#[test]
fn complete_controls_and_unfinished_tail_keep_all_semantic_coordinates() {
    let points = vec![
        SemanticVec3::new(0.123456789012345, 0.234567890123456, 0.345678901234567),
        SemanticVec3::new(0.5, 0.75, 1.25),
        SemanticVec3::new(1.0, 1.5, 2.25),
        SemanticVec3::new(2.0, 3.0, 4.25),
        SemanticVec3::new(5.0, 6.0, 7.25),
    ];
    let path = VectorPath::from_manim_points_3d(points.clone());
    assert_eq!(path.manim_points().unwrap(), points);
    assert_eq!(path.manim_points_xy().unwrap()[4], (5.0, 6.0));
    assert!(matches!(
        path.commands(),
        [
            PathCommand::MoveTo { .. },
            PathCommand::CubicTo { .. },
            PathCommand::MoveTo { .. }
        ]
    ));
    assert!(path.is_finite());
    assert_ne!(points[0].z, f64::from(points[0].z as f32));
}

#[test]
fn semantic_affine_transform_preserves_third_axis_and_planar_transform_leaves_it_unchanged() {
    let point = SemanticVec3::new(2.0, 3.0, 4.0);
    let path = VectorPath::from_manim_points_3d(vec![point]);
    let transform = SemanticTransform2_5D {
        translation: SemanticVec3::new(0.5, -0.25, 1.5),
        scale: SemanticVec3::new(2.0, 0.5, 1.25),
        rotation_z: 0.0,
    };
    let transformed = path.transformed_semantic(transform);
    assert_eq!(
        transformed.manim_points().unwrap(),
        &[SemanticVec3::new(4.5, 1.25, 6.5)]
    );
    let planar = path.transformed(Transform2D {
        translation: Vec2::new(0.5, -0.25),
        scale: Vec2::new(2.0, 0.5),
        rotation: 0.0,
    });
    assert_eq!(
        planar.manim_points().unwrap(),
        &[SemanticVec3::new(4.5, 1.25, 4.0)]
    );
    assert_eq!(path.manim_points().unwrap(), &[point]);
}

#[test]
fn nonfinite_z_is_invalid_even_when_drawable_xy_is_finite() {
    for z in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let path = VectorPath::from_manim_points_3d(vec![SemanticVec3::new(1.0, 2.0, z)]);
        assert!(!path.is_finite());
        assert!(!path.has_finite_transformed_output(Transform2D::default(), Vec2::ZERO));
    }
    let path = VectorPath::from_manim_points_3d(vec![SemanticVec3::new(1.0, 2.0, 1.0e100)]);
    assert!(
        path.is_finite(),
        "finite semantic Z is not limited by the planar f32 projection"
    );
}

#[test]
fn corner_interpolation_and_closing_compare_all_three_axes() {
    let corners = [
        SemanticVec3::new(0.0, 0.0, 0.0),
        SemanticVec3::new(3.0, 6.0, 9.0),
    ];
    let path = VectorPath::from_manim_corners_3d(&corners);
    assert_eq!(
        path.manim_points().unwrap(),
        &[
            corners[0],
            SemanticVec3::new(1.0, 2.0, 3.0),
            SemanticVec3::new(2.0, 4.0, 6.0),
            corners[1],
        ]
    );
    let coincident_xy = VectorPath::from_manim_corners_3d(&[
        SemanticVec3::new(0.0, 0.0, 0.0),
        SemanticVec3::new(1.0, 1.0, 1.0),
        SemanticVec3::new(0.0, 0.0, 2.0),
    ]);
    assert!(!matches!(
        coincident_xy.commands().last(),
        Some(PathCommand::Close)
    ));
}
