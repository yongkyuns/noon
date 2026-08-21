use noon_core::{StrokeCap, StrokeJoin, Vec2, VectorPath};
use noon_geometry::{tessellate_styled, TessellatedPath};

fn open_corner() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-2.0, 0.0))
        .line_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(0.0, 2.0))
}

fn bounds(mesh: &TessellatedPath, target: bool) -> (Vec2, Vec2) {
    let mut points = mesh
        .vertices
        .iter()
        .filter(|vertex| matches!(vertex.surface, noon_geometry::PathSurface::Stroke))
        .map(|vertex| {
            if target {
                vertex.target_position
            } else {
                vertex.position
            }
        });
    let first = points.next().expect("stroke mesh must contain vertices");
    let mut min = first;
    let mut max = first;
    for point in points {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    (min, max)
}

fn assert_bounds_close(left: (Vec2, Vec2), right: (Vec2, Vec2), tolerance: f32) {
    for (actual, expected) in [
        (left.0.x, right.0.x),
        (left.0.y, right.0.y),
        (left.1.x, right.1.x),
        (left.1.y, right.1.y),
    ] {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{actual} != {expected}"
        );
    }
}

#[test]
fn static_and_identity_morph_have_matching_endpoint_bounds_for_all_styles() {
    let source = open_corner();
    for join in [StrokeJoin::Round, StrokeJoin::Miter, StrokeJoin::Bevel] {
        for cap in [StrokeCap::Round, StrokeCap::Butt, StrokeCap::Square] {
            let static_mesh = tessellate_styled(&source, 0.4, join, cap).unwrap();
            let morph = source.clone().with_morph_target(source.clone());
            let morph_mesh = tessellate_styled(&morph, 0.4, join, cap).unwrap();
            assert_bounds_close(
                bounds(&static_mesh, false),
                bounds(&morph_mesh, false),
                1.0e-4,
            );
            assert_bounds_close(
                bounds(&static_mesh, false),
                bounds(&morph_mesh, true),
                1.0e-4,
            );
        }
    }
}

#[test]
fn open_caps_match_theoretical_extents() {
    let path = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0));
    let half_width = 0.25;
    for (cap, expected_x) in [
        (StrokeCap::Butt, 1.0),
        (StrokeCap::Round, 1.0 + half_width),
        (StrokeCap::Square, 1.0 + half_width),
    ] {
        let morph = path.clone().with_morph_target(path.clone());
        let mesh = tessellate_styled(&morph, half_width * 2.0, StrokeJoin::Round, cap).unwrap();
        let (min, max) = bounds(&mesh, false);
        assert!((min.x + expected_x).abs() < 1.0e-5);
        assert!((max.x - expected_x).abs() < 1.0e-5);
        assert!((min.y + half_width).abs() < 1.0e-5);
        assert!((max.y - half_width).abs() < 1.0e-5);
    }
}

#[test]
fn right_angle_miter_reaches_closed_form_intersection() {
    let path = open_corner();
    let morph = path.clone().with_morph_target(path);
    let mesh = tessellate_styled(&morph, 0.4, StrokeJoin::Miter, StrokeCap::Butt).unwrap();
    // A left turn has its outer miter at (+h, -h) about the corner (0,0).
    assert!(mesh.vertices.iter().any(|vertex| {
        (vertex.position.x - 0.2).abs() < 1.0e-6 && (vertex.position.y + 0.2).abs() < 1.0e-6
    }));
}

#[test]
fn round_join_and_cap_topology_is_fixed_when_turn_direction_changes() {
    let source = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0, 1.0));
    let target = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0, -1.0));
    let mesh = tessellate_styled(
        &source.with_morph_target(target),
        0.2,
        StrokeJoin::Round,
        StrokeCap::Round,
    )
    .unwrap();
    assert!(mesh.morphing);
    assert!(!mesh.vertices.is_empty());
    assert!(!mesh.indices.is_empty());
    assert!(mesh
        .indices
        .iter()
        .all(|index| (*index as usize) < mesh.vertices.len()));
}
