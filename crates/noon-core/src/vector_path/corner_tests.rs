use super::*;

#[test]
fn canonical_corners_keep_f64_semantics_and_drawable_line_segments() {
    let corners = [
        (0.123456789012345, 0.234567890123456),
        (1.23456789012345, -0.456789012345678),
        (2.5, 1.25),
    ];
    let path = VectorPath::from_manim_corners_f64(&corners);
    assert!(matches!(
        path.commands(),
        [
            PathCommand::MoveTo { .. },
            PathCommand::LineTo { .. },
            PathCommand::LineTo { .. }
        ]
    ));
    let points = path.manim_points_xy().unwrap();
    assert_eq!(points.len(), 8);
    assert_eq!(points[0], corners[0]);
    assert_eq!(points[3], corners[1]);
    assert_eq!(points[4], corners[1]);
    assert_eq!(points[7], corners[2]);
    assert_ne!(points[0].0, f64::from(points[0].0 as f32));
    assert!(path
        .manim_points()
        .unwrap()
        .iter()
        .all(|point| point.z == 0.0));
}

#[test]
fn closed_corners_retain_their_contour_flag_and_semantic_endpoint() {
    let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 0.0)];
    let path = VectorPath::from_manim_corners_f64(&corners);
    assert!(matches!(path.commands().last(), Some(PathCommand::Close)));
    assert_eq!(path.manim_points().unwrap().len(), 8);
    assert_eq!(
        path.manim_points().unwrap().first(),
        path.manim_points().unwrap().last()
    );
}

#[test]
fn fewer_than_two_corners_have_no_complete_curve() {
    for corners in [&[][..], &[(0.123456789012345, 0.234567890123456)][..]] {
        let path = VectorPath::from_manim_corners_f64(corners);
        assert!(path.commands().is_empty());
        assert!(!path.has_drawable_segments());
    }
}

#[test]
fn affine_baking_preserves_line_topology_and_exact_transformed_controls() {
    let corners = [
        (0.123456789012345, 0.234567890123456),
        (1.23456789012345, -0.456789012345678),
        (0.123456789012345, 0.234567890123456),
    ];
    let path = VectorPath::from_manim_corners_f64(&corners);
    let transform = SemanticTransform2_5D {
        translation: SemanticVec3::new(0.345678901234567, -0.567890123456789, 0.0),
        scale: SemanticVec3::new(1.75, 0.6, 1.0),
        rotation_z: 0.37,
    };
    let expected = path
        .manim_points()
        .unwrap()
        .iter()
        .map(|&point| transform.transform_xyz(point))
        .collect::<Vec<_>>();
    let baked = path.transformed_semantic(transform);
    assert_eq!(baked.manim_points().unwrap(), expected);
    assert!(matches!(
        baked.commands(),
        [
            PathCommand::MoveTo { .. },
            PathCommand::LineTo { .. },
            PathCommand::LineTo { .. },
            PathCommand::Close
        ]
    ));
}
