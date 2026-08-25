use noon_core::{GeometryRef, Vec2, VectorPath, TAU};

/// Convert renderer-supported geometry to a deterministic vector outline.
///
/// Analytic primitives stay analytic in semantic/runtime state and are converted
/// only when a renderer needs ordered path progress. The contour ordering here is
/// intentionally pinned to ManimCE v0.21.0's Cairo VMobject geometry so Create and
/// other path-sensitive operations see the same primitive path semantics.
pub fn canonical_outline_path(geometry: &GeometryRef) -> Option<VectorPath> {
    match geometry {
        GeometryRef::Circle { radius } => Some(circle_path(*radius)),
        GeometryRef::Rectangle { size } => Some(rectangle_path(*size)),
        GeometryRef::Line { start, end } => Some(VectorPath::new().move_to(*start).line_to(*end)),
        GeometryRef::VectorPath(path) => Some(path.clone()),
        GeometryRef::External(_) => None,
    }
}

fn circle_path(radius: f32) -> VectorPath {
    // ManimCE v0.21.0 Circle is Arc(angle=TAU) with Arc's default
    // num_components=9. Cairo's _set_pre_positioned_points therefore creates
    // nine anchors (the first/last coincide) and eight cubic Bezier segments.
    // For each 45-degree segment Manim uses 4/3 * tan(d_theta/4) as the tangent
    // handle factor. Preserve that exact parameterization instead of the common
    // four-cubic circle approximation: path proportion and partial-path reveal
    // depend on the segment structure, not only on the final silhouette.
    const CURVES: usize = 8;
    let step = TAU / CURVES as f32;
    let handle_factor = 4.0 / 3.0 * (step / 4.0).tan();
    let handle_length = radius * handle_factor;

    let mut path = VectorPath::new().move_to(Vec2::new(radius, 0.0));
    for index in 0..CURVES {
        let start_angle = index as f32 * step;
        let end_angle = (index + 1) as f32 * step;
        let (start_sin, start_cos) = start_angle.sin_cos();
        let (end_sin, end_cos) = end_angle.sin_cos();

        let start = Vec2::new(radius * start_cos, radius * start_sin);
        let end = Vec2::new(radius * end_cos, radius * end_sin);
        let start_tangent = Vec2::new(-start_sin, start_cos);
        let end_tangent = Vec2::new(-end_sin, end_cos);

        path = path.cubic_to(
            start + start_tangent * handle_length,
            end - end_tangent * handle_length,
            end,
        );
    }
    path.close()
}

fn rectangle_path(size: Vec2) -> VectorPath {
    let half = size * 0.5;
    // ManimCE Rectangle constructs Polygon(UR, UL, DL, DR): start at the
    // upper-right corner and proceed counter-clockwise around the four sides.
    // Do not insert midpoint subdivisions solely to make morph command counts
    // line up; those alter point_from_proportion/Create path semantics.
    VectorPath::new()
        .move_to(Vec2::new(half.x, half.y))
        .line_to(Vec2::new(-half.x, half.y))
        .line_to(Vec2::new(-half.x, -half.y))
        .line_to(Vec2::new(half.x, -half.y))
        .close()
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryId, PathCommand};

    use super::*;

    fn assert_vec2_close(actual: Vec2, expected: Vec2) {
        const EPSILON: f32 = 1.0e-6;
        assert!(
            (actual.x - expected.x).abs() <= EPSILON && (actual.y - expected.y).abs() <= EPSILON,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn analytic_shapes_have_stable_outline_paths() {
        for geometry in [
            GeometryRef::circle(1.25),
            GeometryRef::rectangle(2.0, 3.0),
            GeometryRef::line(Vec2::new(-1.0, 2.0), Vec2::new(3.0, -4.0)),
        ] {
            let first = canonical_outline_path(&geometry).expect("supported geometry");
            let second = canonical_outline_path(&geometry).expect("supported geometry");
            assert_eq!(first, second);
            assert!(!first.commands().is_empty());
        }
    }

    #[test]
    fn circle_outline_matches_manim_cairo_eight_curve_contract() {
        let radius = 2.0;
        let path = canonical_outline_path(&GeometryRef::circle(radius)).expect("circle path");
        let commands = path.commands();

        assert_eq!(commands.len(), 10, "move + 8 cubic curves + close");
        assert_eq!(
            commands[0],
            PathCommand::MoveTo {
                to: Vec2::new(radius, 0.0)
            }
        );
        assert_eq!(commands[9], PathCommand::Close);

        let step = TAU / 8.0;
        let factor = 4.0 / 3.0 * (step / 4.0).tan();
        let diagonal = std::f32::consts::FRAC_1_SQRT_2 * radius;
        match commands[1] {
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                assert_vec2_close(control1, Vec2::new(radius, radius * factor));
                assert_vec2_close(
                    control2,
                    Vec2::new(
                        diagonal + std::f32::consts::FRAC_1_SQRT_2 * radius * factor,
                        diagonal - std::f32::consts::FRAC_1_SQRT_2 * radius * factor,
                    ),
                );
                assert_vec2_close(to, Vec2::new(diagonal, diagonal));
            }
            command => panic!("expected first cubic segment, got {command:?}"),
        }
    }

    #[test]
    fn rectangle_outline_matches_manim_vertex_order_without_midpoints() {
        let path =
            canonical_outline_path(&GeometryRef::rectangle(4.0, 2.0)).expect("rectangle path");
        assert_eq!(
            path.commands(),
            &[
                PathCommand::MoveTo {
                    to: Vec2::new(2.0, 1.0),
                },
                PathCommand::LineTo {
                    to: Vec2::new(-2.0, 1.0),
                },
                PathCommand::LineTo {
                    to: Vec2::new(-2.0, -1.0),
                },
                PathCommand::LineTo {
                    to: Vec2::new(2.0, -1.0),
                },
                PathCommand::Close,
            ]
        );
    }

    #[test]
    fn line_outline_preserves_start_to_end_direction() {
        let start = Vec2::new(-3.0, 2.0);
        let end = Vec2::new(4.0, -1.0);
        let path = canonical_outline_path(&GeometryRef::line(start, end)).expect("line path");
        assert_eq!(
            path.commands(),
            &[
                PathCommand::MoveTo { to: start },
                PathCommand::LineTo { to: end },
            ]
        );
    }

    #[test]
    fn vector_path_is_preserved_exactly() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .quadratic_to(Vec2::new(0.5, 1.0), Vec2::ONE);
        assert_eq!(
            canonical_outline_path(&GeometryRef::path(path.clone())),
            Some(path)
        );
    }

    #[test]
    fn external_geometry_has_no_implicit_outline() {
        assert!(canonical_outline_path(&GeometryRef::External(GeometryId::new(7))).is_none());
    }
}
