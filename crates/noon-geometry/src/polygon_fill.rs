use noon_core::{PathCommand, Vec2, VectorPath};

const MAX_POLYGON_FILL_COMMANDS: usize = 4_097;

/// Tests a line-only path with Lyon's default non-zero fill rule.
///
/// `None` means the path is outside this bounded polygon policy: it exceeds
/// 4,097 commands, contains curves, morphs or malformed/non-finite coordinates.
/// Open contours are implicitly closed, matching the fill tessellator. Boundary
/// points count as inside. The scan allocates no memory and is proportional to
/// command count.
pub fn polygon_fill_contains(path: &VectorPath, point: [f64; 2]) -> Option<bool> {
    if path.commands().len() > MAX_POLYGON_FILL_COMMANDS
        || path.morph_target().is_some()
        || !point[0].is_finite()
        || !point[1].is_finite()
    {
        return None;
    }

    let point = (point[0], point[1]);
    let mut start = None;
    let mut previous = None;
    let mut winding = 0_i32;
    let mut has_area = false;
    let mut boundary_hit = false;
    let mut any_boundary_hit = false;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                if !finite(to) {
                    return None;
                }
                any_boundary_hit |= finish_contour(
                    previous,
                    start,
                    point,
                    &mut winding,
                    &mut has_area,
                    &mut boundary_hit,
                );
                start = Some(to);
                previous = Some(to);
                has_area = false;
                boundary_hit = false;
            }
            PathCommand::LineTo { to } => {
                if !finite(to) {
                    return None;
                }
                let (Some(last), Some(_)) = (previous, start) else {
                    return None;
                };
                if let Some(first) = start {
                    let cross = (f64::from(last.x) - f64::from(first.x))
                        * (f64::from(to.y) - f64::from(first.y))
                        - (f64::from(last.y) - f64::from(first.y))
                            * (f64::from(to.x) - f64::from(first.x));
                    has_area |= cross != 0.0;
                }
                boundary_hit |= update_winding(last, to, point, &mut winding);
                previous = Some(to);
            }
            PathCommand::Close => {
                if previous.is_none() || start.is_none() {
                    return None;
                }
                any_boundary_hit |= finish_contour(
                    previous,
                    start,
                    point,
                    &mut winding,
                    &mut has_area,
                    &mut boundary_hit,
                );
                start = None;
                previous = None;
                has_area = false;
                boundary_hit = false;
            }
            PathCommand::QuadraticTo { .. } | PathCommand::CubicTo { .. } => return None,
        }
    }

    any_boundary_hit |= finish_contour(
        previous,
        start,
        point,
        &mut winding,
        &mut has_area,
        &mut boundary_hit,
    );

    Some(any_boundary_hit || winding != 0)
}

fn finite(point: Vec2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn finish_contour(
    previous: Option<Vec2>,
    start: Option<Vec2>,
    point: (f64, f64),
    winding: &mut i32,
    has_area: &mut bool,
    boundary_hit: &mut bool,
) -> bool {
    if let (Some(last), Some(first)) = (previous, start) {
        *boundary_hit |= update_winding(last, first, point, winding);
    }
    *has_area && *boundary_hit
}

/// Updates non-zero winding and reports an inclusive point on an edge.
fn update_winding(from: Vec2, to: Vec2, point: (f64, f64), winding: &mut i32) -> bool {
    let (x, y) = (f64::from(from.x), f64::from(from.y));
    let (next_x, next_y) = (f64::from(to.x), f64::from(to.y));
    let (px, py) = point;
    let cross = (next_x - x) * (py - y) - (px - x) * (next_y - y);

    if cross == 0.0
        && px >= x.min(next_x)
        && px <= x.max(next_x)
        && py >= y.min(next_y)
        && py <= y.max(next_y)
    {
        return true;
    }
    if y <= py {
        if next_y > py && cross > 0.0 {
            *winding += 1;
        }
    } else if next_y <= py && cross < 0.0 {
        *winding -= 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concave_polygon_and_implicit_closure_use_nonzero_fill() {
        let path = VectorPath::new()
            .move_to(Vec2::new(0.0, 0.0))
            .line_to(Vec2::new(3.0, 0.0))
            .line_to(Vec2::new(3.0, 1.0))
            .line_to(Vec2::new(1.0, 1.0))
            .line_to(Vec2::new(1.0, 3.0))
            .line_to(Vec2::new(0.0, 3.0));

        assert_eq!(polygon_fill_contains(&path, [0.5, 2.0]), Some(true));
        assert_eq!(polygon_fill_contains(&path, [2.0, 2.0]), Some(false));
        assert_eq!(polygon_fill_contains(&path, [1.0, 2.0]), Some(true));
    }

    #[test]
    fn opposite_winding_inner_contour_is_a_hole() {
        let path = VectorPath::new()
            .move_to(Vec2::new(-2.0, -2.0))
            .line_to(Vec2::new(2.0, -2.0))
            .line_to(Vec2::new(2.0, 2.0))
            .line_to(Vec2::new(-2.0, 2.0))
            .close()
            .move_to(Vec2::new(-1.0, -1.0))
            .line_to(Vec2::new(-1.0, 1.0))
            .line_to(Vec2::new(1.0, 1.0))
            .line_to(Vec2::new(1.0, -1.0))
            .close();

        assert_eq!(polygon_fill_contains(&path, [1.5, 0.0]), Some(true));
        assert_eq!(polygon_fill_contains(&path, [0.0, 0.0]), Some(false));
        assert_eq!(polygon_fill_contains(&path, [1.0, 0.0]), Some(true));
    }

    #[test]
    fn curved_and_morph_paths_are_outside_the_policy() {
        let boundary_then_curve = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(2.0, 0.0))
            .line_to(Vec2::new(0.0, 2.0))
            .close()
            .move_to(Vec2::ZERO)
            .quadratic_to(Vec2::ONE, Vec2::new(2.0, 0.0))
            .close();
        assert_eq!(
            polygon_fill_contains(&boundary_then_curve, [0.0, 0.0]),
            None
        );

        let morph = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::ONE)
            .close()
            .with_morph_target(VectorPath::new());
        assert_eq!(polygon_fill_contains(&morph, [0.0, 0.0]), None);
    }

    #[test]
    fn open_zero_area_segment_is_not_a_fill_hit() {
        let line = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(2.0, 0.0));
        assert_eq!(polygon_fill_contains(&line, [1.0, 0.0]), Some(false));
    }

    #[test]
    fn command_count_is_bounded() {
        let mut path = VectorPath::new().move_to(Vec2::ZERO);
        for index in 0..MAX_POLYGON_FILL_COMMANDS {
            path = path.line_to(Vec2::new(index as f32, (index % 2) as f32));
        }
        assert_eq!(polygon_fill_contains(&path, [0.0, 0.0]), None);
    }
}
