use noon_core::{Vec2, VectorPath};

const MANIM_CLOSE_RTOL: f64 = 1.0e-5;
const MANIM_CLOSE_ATOL: f64 = 1.0e-8;

/// Errors produced while converting sampled/corner anchors into Manim-compatible
/// smooth cubic Bezier geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSmoothingError {
    NonFiniteAnchor { index: usize, point: Vec2 },
    CoordinateOverflow { x: f64, y: f64 },
}

impl std::fmt::Display for PathSmoothingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::NonFiniteAnchor { index, point } => write!(
                formatter,
                "smooth path anchor {index} must be finite: ({}, {})",
                point.x, point.y
            ),
            Self::CoordinateOverflow { x, y } => write!(
                formatter,
                "smooth path control point cannot be represented as f32: ({x}, {y})"
            ),
        }
    }
}

impl std::error::Error for PathSmoothingError {}

/// Compute the first/second cubic Bezier handles used by ManimCE v0.21
/// `get_smooth_cubic_bezier_handle_points` for one 2D anchor sequence.
///
/// This is a semantic geometry operation, not a renderer interpolation. The
/// solve is performed in f64 and lowered to Noon's current f32 `Vec2` path
/// representation only after the complete spline has been solved.
pub fn smooth_cubic_bezier_handles(
    anchors: &[Vec2],
) -> Result<(Vec<Vec2>, Vec<Vec2>), PathSmoothingError> {
    validate_anchors(anchors)?;
    if anchors.len() < 2 {
        return Ok((Vec::new(), Vec::new()));
    }

    let anchors64 = anchors.iter().copied().map(Vec2d::from).collect::<Vec<_>>();
    let (first, second) = if anchors64.len() == 2 {
        straight_handles(anchors64[0], anchors64[1])
    } else if manim_anchors_are_closed(anchors) {
        smooth_closed_handles(&anchors64)
    } else {
        smooth_open_handles(&anchors64)
    };

    Ok((lower_points(&first)?, lower_points(&second)?))
}

/// Convert independent corner/sample subpaths into one retained cubic `VectorPath`
/// using ManimCE v0.21 `VMobject.make_smooth()` handle semantics.
///
/// Each input slice is one explicit subpath. Empty subpaths are ignored and a
/// one-anchor subpath is retained as a `MoveTo`. Multiple subpaths therefore
/// stay disconnected, which is required for plotting discontinuities.
///
/// A closed anchor sequence already contains its final cubic back to the first
/// anchor. No extra `Close` command is appended: in Noon's path model `Close`
/// is itself a drawable segment and would create a spurious extra curve.
pub fn smooth_cubic_path_from_subpaths(
    subpaths: &[Vec<Vec2>],
) -> Result<VectorPath, PathSmoothingError> {
    let mut path = VectorPath::new();
    for anchors in subpaths {
        validate_anchors(anchors)?;
        let Some(&first_anchor) = anchors.first() else {
            continue;
        };

        path = path.move_to(first_anchor);
        if anchors.len() < 2 {
            continue;
        }

        let (first_handles, second_handles) = smooth_cubic_bezier_handles(anchors)?;
        debug_assert_eq!(first_handles.len(), anchors.len() - 1);
        debug_assert_eq!(second_handles.len(), anchors.len() - 1);
        for index in 0..anchors.len() - 1 {
            path = path.cubic_to(
                first_handles[index],
                second_handles[index],
                anchors[index + 1],
            );
        }
    }
    Ok(path)
}

fn validate_anchors(anchors: &[Vec2]) -> Result<(), PathSmoothingError> {
    for (index, &point) in anchors.iter().enumerate() {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(PathSmoothingError::NonFiniteAnchor { index, point });
        }
    }
    Ok(())
}

/// Match Manim v0.21's `utils.bezier.is_closed` test for the 2D coordinates
/// supported by the current retained vector path.
fn manim_anchors_are_closed(anchors: &[Vec2]) -> bool {
    let Some((&start, &end)) = anchors.first().zip(anchors.last()) else {
        return false;
    };
    let tolerance_x = MANIM_CLOSE_ATOL + MANIM_CLOSE_RTOL * f64::from(start.x);
    if (f64::from(end.x) - f64::from(start.x)).abs() > tolerance_x {
        return false;
    }
    let tolerance_y = MANIM_CLOSE_ATOL + MANIM_CLOSE_RTOL * f64::from(start.y);
    (f64::from(end.y) - f64::from(start.y)).abs() <= tolerance_y
}

fn straight_handles(start: Vec2d, end: Vec2d) -> (Vec<Vec2d>, Vec<Vec2d>) {
    let delta = end.sub(start);
    (
        vec![start.add(delta.scale(1.0 / 3.0))],
        vec![start.add(delta.scale(2.0 / 3.0))],
    )
}

/// Manim's open-spline tridiagonal solve (`get_smooth_open_cubic_bezier_handle_points`).
fn smooth_open_handles(anchors: &[Vec2d]) -> (Vec<Vec2d>, Vec<Vec2d>) {
    let curve_count = anchors.len() - 1;
    debug_assert!(curve_count >= 2);

    let mut c_prime = vec![0.0_f64; curve_count - 1];
    c_prime[0] = 0.5;
    for index in 1..curve_count - 1 {
        c_prime[index] = 1.0 / (4.0 - c_prime[index - 1]);
    }

    let mut d_prime = vec![Vec2d::ZERO; curve_count];
    d_prime[0] = anchors[0].scale(0.5).add(anchors[1]);
    for index in 1..curve_count - 1 {
        let rhs = anchors[index]
            .scale(4.0)
            .add(anchors[index + 1].scale(2.0))
            .sub(d_prime[index - 1]);
        d_prime[index] = rhs.scale(c_prime[index]);
    }

    let last = curve_count - 1;
    let last_factor = 1.0 / (7.0 - 2.0 * c_prime[last - 1]);
    d_prime[last] = anchors[last]
        .scale(8.0)
        .add(anchors[last + 1])
        .sub(d_prime[last - 1].scale(2.0))
        .scale(last_factor);

    let mut first_handles = d_prime;
    for index in (0..last).rev() {
        first_handles[index] =
            first_handles[index].sub(first_handles[index + 1].scale(c_prime[index]));
    }

    let mut second_handles = vec![Vec2d::ZERO; curve_count];
    for index in 0..last {
        second_handles[index] = anchors[index + 1].scale(2.0).sub(first_handles[index + 1]);
    }
    second_handles[last] = anchors[last + 1].add(first_handles[last]).scale(0.5);

    (first_handles, second_handles)
}

/// Manim's closed-spline cyclic tridiagonal solve
/// (`get_smooth_closed_cubic_bezier_handle_points`).
fn smooth_closed_handles(anchors: &[Vec2d]) -> (Vec<Vec2d>, Vec<Vec2d>) {
    let curve_count = anchors.len() - 1;
    debug_assert!(curve_count >= 2);

    let mut c_prime = vec![0.0_f64; curve_count - 1];
    let mut u_prime = vec![0.0_f64; curve_count - 1];
    c_prime[0] = 1.0 / 3.0;
    u_prime[0] = 1.0 / 3.0;
    for index in 1..curve_count - 1 {
        c_prime[index] = 1.0 / (4.0 - c_prime[index - 1]);
        u_prime[index] = -c_prime[index] * u_prime[index - 1];
    }

    let last = curve_count - 1;
    let last_division = 1.0 / (3.0 - c_prime[last - 1]);
    let u_last = last_division * (1.0 - u_prime[last - 1]);

    let mut q = vec![0.0_f64; curve_count];
    q[last] = u_last;
    for index in (0..last).rev() {
        q[index] = u_prime[index] - c_prime[index] * q[index + 1];
    }

    let mut d_prime = vec![Vec2d::ZERO; curve_count];
    d_prime[0] = anchors[0]
        .scale(4.0)
        .add(anchors[1].scale(2.0))
        .scale(1.0 / 3.0);
    for index in 1..last {
        let rhs = anchors[index]
            .scale(4.0)
            .add(anchors[index + 1].scale(2.0))
            .sub(d_prime[index - 1]);
        d_prime[index] = rhs.scale(c_prime[index]);
    }
    d_prime[last] = anchors[last]
        .scale(4.0)
        .add(anchors[last + 1].scale(2.0))
        .sub(d_prime[last - 1])
        .scale(last_division);

    let mut y = d_prime.clone();
    for index in (0..last).rev() {
        y[index] = d_prime[index].sub(y[index + 1].scale(c_prime[index]));
    }

    let endpoint_sum = y[0].add(y[last]);
    let correction_scale = 1.0 / (1.0 + q[0] + q[last]);
    let mut first_handles = vec![Vec2d::ZERO; curve_count];
    for index in 0..curve_count {
        first_handles[index] = y[index].sub(endpoint_sum.scale(correction_scale * q[index]));
    }

    let mut second_handles = vec![Vec2d::ZERO; curve_count];
    for index in 0..last {
        second_handles[index] = anchors[index + 1].scale(2.0).sub(first_handles[index + 1]);
    }
    second_handles[last] = anchors[last + 1].scale(2.0).sub(first_handles[0]);

    (first_handles, second_handles)
}

fn lower_points(points: &[Vec2d]) -> Result<Vec<Vec2>, PathSmoothingError> {
    points.iter().copied().map(Vec2d::lower).collect()
}

#[derive(Clone, Copy, Debug, Default)]
struct Vec2d {
    x: f64,
    y: f64,
}

impl Vec2d {
    const ZERO: Self = Self { x: 0.0, y: 0.0 };

    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }

    fn scale(self, factor: f64) -> Self {
        Self {
            x: self.x * factor,
            y: self.y * factor,
        }
    }

    fn lower(self) -> Result<Vec2, PathSmoothingError> {
        if !self.x.is_finite()
            || !self.y.is_finite()
            || self.x.abs() > f64::from(f32::MAX)
            || self.y.abs() > f64::from(f32::MAX)
        {
            return Err(PathSmoothingError::CoordinateOverflow {
                x: self.x,
                y: self.y,
            });
        }
        Ok(Vec2::new(self.x as f32, self.y as f32))
    }
}

impl From<Vec2> for Vec2d {
    fn from(value: Vec2) -> Self {
        Self {
            x: f64::from(value.x),
            y: f64::from(value.y),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::PathCommand;

    fn assert_point(actual: Vec2, expected: Vec2) {
        assert!(
            (actual - expected).length() <= 1.0e-5,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn two_anchors_keep_straight_line_handles() {
        let (first, second) =
            smooth_cubic_bezier_handles(&[Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0)]).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_point(first[0], Vec2::new(1.0, 0.0));
        assert_point(second[0], Vec2::new(2.0, 0.0));
    }

    #[test]
    fn open_three_anchor_handles_match_manim_v021_oracle() {
        let (first, second) = smooth_cubic_bezier_handles(&[
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 0.0),
        ])
        .unwrap();

        assert_point(first[0], Vec2::new(1.0 / 3.0, 0.5));
        assert_point(second[0], Vec2::new(2.0 / 3.0, 1.0));
        assert_point(first[1], Vec2::new(4.0 / 3.0, 1.0));
        assert_point(second[1], Vec2::new(5.0 / 3.0, 0.5));
    }

    #[test]
    fn closed_square_handles_match_manim_v021_oracle() {
        let (first, second) = smooth_cubic_bezier_handles(&[
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(0.0, 0.0),
        ])
        .unwrap();

        let expected_first = [
            Vec2::new(0.25, -0.25),
            Vec2::new(1.25, 0.25),
            Vec2::new(0.75, 1.25),
            Vec2::new(-0.25, 0.75),
        ];
        let expected_second = [
            Vec2::new(0.75, -0.25),
            Vec2::new(1.25, 0.75),
            Vec2::new(0.25, 1.25),
            Vec2::new(-0.25, 0.25),
        ];
        for index in 0..4 {
            assert_point(first[index], expected_first[index]);
            assert_point(second[index], expected_second[index]);
        }
    }

    #[test]
    fn smoothing_keeps_disconnected_subpaths_disconnected() {
        let path = smooth_cubic_path_from_subpaths(&[
            vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(2.0, 0.0),
            ],
            vec![Vec2::new(3.0, 0.0), Vec2::new(4.0, 0.0)],
        ])
        .unwrap();

        assert_eq!(path.commands().len(), 5);
        assert!(matches!(path.commands()[0], PathCommand::MoveTo { .. }));
        assert!(matches!(path.commands()[1], PathCommand::CubicTo { .. }));
        assert!(matches!(path.commands()[2], PathCommand::CubicTo { .. }));
        assert!(matches!(path.commands()[3], PathCommand::MoveTo { .. }));
        assert!(matches!(path.commands()[4], PathCommand::CubicTo { .. }));
    }

    #[test]
    fn one_anchor_subpath_remains_a_move_only_path() {
        let path = smooth_cubic_path_from_subpaths(&[vec![Vec2::new(2.0, -1.0)]]).unwrap();
        assert_eq!(
            path.commands(),
            &[PathCommand::MoveTo {
                to: Vec2::new(2.0, -1.0)
            }]
        );
    }

    #[test]
    fn non_finite_anchor_is_rejected_before_solve() {
        let anchors = [Vec2::ZERO, Vec2::new(f32::NAN, 1.0)];
        let error = smooth_cubic_bezier_handles(&anchors).unwrap_err();
        match error {
            PathSmoothingError::NonFiniteAnchor { index, point } => {
                assert_eq!(index, 1);
                assert!(point.x.is_nan());
                assert_eq!(point.y, 1.0);
            }
            other => panic!("expected non-finite anchor error, got {other:?}"),
        }
    }
}
