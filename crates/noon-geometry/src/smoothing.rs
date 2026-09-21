//! Deterministic anchor-handle editing of retained contours. No frame-time solver.
use noon_core::{PathCommand, Vec2, VectorPath};

use crate::PathProportionError;

/// Selects the boundary condition used by shared cubic spline smoothing.
///
/// `ExactClosure` preserves Noon’s existing exact-closure behavior. The Manim policy
/// intentionally reproduces ManimCE's signed `is_closed` tolerance for callers
/// that require pinned authoring parity; it changes spline handles only, never
/// the retained path's explicit `Close` commands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SplineBoundary {
    #[default]
    ExactClosure,
    ManimSignedClosure,
}

impl SplineBoundary {
    fn spline_is_closed(self, anchors: &[Vec2], explicit_close: bool) -> bool {
        match self {
            Self::ExactClosure => explicit_close || anchors.first() == anchors.last(),
            Self::ManimSignedClosure => manim_signed_is_closed(anchors),
        }
    }
}

/// Replace handles with a C2 cubic spline, or straight segments in jagged mode.
/// Each explicit contour is independent. Existing curve anchors/order and closed
/// joins survive; unfinished anchors have no curve and are omitted.
/// Time and temporary storage are linear in the number of anchors.
pub fn change_path_anchor_mode(
    path: &VectorPath,
    smooth: bool,
) -> Result<VectorPath, PathProportionError> {
    change_path_anchor_mode_with_boundary(path, smooth, SplineBoundary::ExactClosure)
}

/// Replace handles using the requested spline boundary condition.
///
/// This shares the ordinary contour walker and solver with
/// [`change_path_anchor_mode`]. Boundary selection affects only whether the
/// smooth solver is natural or periodic; path closure remains authored topology.
pub fn change_path_anchor_mode_with_boundary(
    path: &VectorPath,
    smooth: bool,
    boundary: SplineBoundary,
) -> Result<VectorPath, PathProportionError> {
    if !path.is_finite() {
        return Err(PathProportionError::InvalidMetric);
    }
    let mut result = VectorPath::new();
    let mut anchors = Vec::new();
    let mut closed = false;
    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                result = append_contour(result, &anchors, closed, smooth, boundary)?;
                anchors.clear();
                anchors.push(to);
                closed = false;
            }
            PathCommand::LineTo { to }
            | PathCommand::QuadraticTo { to, .. }
            | PathCommand::CubicTo { to, .. } => anchors.push(to),
            PathCommand::Close => {
                if let (Some(first), Some(last)) = (anchors.first().copied(), anchors.last()) {
                    if *last != first {
                        anchors.push(first);
                    }
                    closed = true;
                }
            }
        }
    }
    append_contour(result, &anchors, closed, smooth, boundary)
}

fn append_contour(
    mut path: VectorPath,
    anchors: &[Vec2],
    closed: bool,
    smooth: bool,
    boundary: SplineBoundary,
) -> Result<VectorPath, PathProportionError> {
    if anchors.len() < 2 {
        return Ok(path);
    }
    path = path.move_to(anchors[0]);
    if smooth {
        let spline_closed = boundary.spline_is_closed(anchors, closed);
        let handles = smooth_handles(anchors, spline_closed);
        for (index, [first, second]) in handles.into_iter().enumerate() {
            path = path.cubic_to(first, second, anchors[index + 1]);
        }
    } else {
        for &anchor in &anchors[1..] {
            path = path.line_to(anchor);
        }
    }
    if closed || anchors.first() == anchors.last() {
        path = path.close();
    }
    if !path.is_finite() {
        return Err(PathProportionError::InvalidMetric);
    }
    Ok(path)
}

/// ManimCE's `bezier.is_closed` uses the signed first coordinate when deriving
/// each tolerance. This is intentionally not an absolute tolerance: a negative
/// start coordinate makes even an exactly repeated endpoint choose the natural
/// spline path in the pinned implementation.
fn manim_signed_is_closed(anchors: &[Vec2]) -> bool {
    let (Some(start), Some(end)) = (anchors.first(), anchors.last()) else {
        return false;
    };
    manim_signed_endpoint_closure(
        [f64::from(start.x), f64::from(start.y)],
        [f64::from(end.x), f64::from(end.y)],
    )
}

fn manim_signed_endpoint_closure(start: [f64; 2], end: [f64; 2]) -> bool {
    (0..2).all(|axis| (end[axis] - start[axis]).abs() <= 1.0e-8 + 1.0e-5 * start[axis])
}

/// Smooth a coordinate-space polyline without narrowing its anchors or handles.
///
/// Closed contours repeat their first anchor. Boundary selection is performed in
/// this source space, before any affine mapping into renderable scene units.
/// The ordinary retained-path smoother uses the same f64 spline solver below.
pub fn smooth_curve_handles(
    anchors: &[[f64; 2]],
    boundary: SplineBoundary,
) -> Result<Vec<[[f64; 2]; 2]>, PathProportionError> {
    if anchors.iter().flatten().any(|value| !value.is_finite()) {
        return Err(PathProportionError::InvalidMetric);
    }
    let (Some(first), Some(last)) = (anchors.first(), anchors.last()) else {
        return Ok(Vec::new());
    };
    if anchors.len() < 2 {
        return Ok(Vec::new());
    }
    let closed = match boundary {
        SplineBoundary::ExactClosure => first == last,
        SplineBoundary::ManimSignedClosure => manim_signed_endpoint_closure(*first, *last),
    };
    let handles = smooth_handles_f64(anchors, closed);
    if handles
        .iter()
        .flatten()
        .flatten()
        .any(|value| !value.is_finite())
    {
        return Err(PathProportionError::InvalidMetric);
    }
    Ok(handles)
}

/// Solve the natural/open or periodic/closed spline equations in f64. The
/// periodic system uses a rank-one correction to the same tridiagonal solve.
fn smooth_handles(anchors: &[Vec2], closed: bool) -> Vec<[Vec2; 2]> {
    // Preserve the existing two-anchor f32 path exactly for ordinary callers.
    if anchors.len() == 2 {
        return vec![[
            anchors[0] + (anchors[1] - anchors[0]) / 3.,
            anchors[0] + (anchors[1] - anchors[0]) * (2. / 3.),
        ]];
    }
    let anchors = anchors
        .iter()
        .map(|point| [f64::from(point.x), f64::from(point.y)])
        .collect::<Vec<_>>();
    smooth_handles_f64(&anchors, closed)
        .into_iter()
        .map(|[first, second]| {
            [
                Vec2::new(first[0] as f32, first[1] as f32),
                Vec2::new(second[0] as f32, second[1] as f32),
            ]
        })
        .collect()
}

fn smooth_handles_f64(anchors: &[[f64; 2]], closed: bool) -> Vec<[[f64; 2]; 2]> {
    let count = anchors.len() - 1;
    if count == 1 {
        let mut handles = [[0.0; 2]; 2];
        for axis in 0..2 {
            let delta = anchors[1][axis] - anchors[0][axis];
            handles[0][axis] = anchors[0][axis] + delta / 3.0;
            handles[1][axis] = anchors[0][axis] + delta * (2.0 / 3.0);
        }
        return vec![handles];
    }
    let mut upper = vec![0.; count - 1];
    upper[0] = if closed { 1. / 3. } else { 0.5 };
    for index in 1..count - 1 {
        upper[index] = 1. / (4. - upper[index - 1]);
    }
    let final_divisor = if closed {
        3. - upper[count - 2]
    } else {
        7. - 2. * upper[count - 2]
    };
    let mut correction = vec![0.; count];
    if closed {
        correction[0] = upper[0];
        for index in 1..count - 1 {
            correction[index] = -upper[index] * correction[index - 1];
        }
        correction[count - 1] = (1. - correction[count - 2]) / final_divisor;
        for index in (0..count - 1).rev() {
            correction[index] -= upper[index] * correction[index + 1];
        }
    }
    let mut result = vec![[[0.0; 2]; 2]; count];
    for axis in 0..2 {
        let coordinate = |index: usize| anchors[index][axis];
        let mut first = vec![0.; count];
        first[0] = if closed {
            (4. * coordinate(0) + 2. * coordinate(1)) / 3.
        } else {
            0.5 * coordinate(0) + coordinate(1)
        };
        for index in 1..count - 1 {
            first[index] = upper[index]
                * (4. * coordinate(index) + 2. * coordinate(index + 1) - first[index - 1]);
        }
        first[count - 1] = if closed {
            (4. * coordinate(count - 1) + 2. * coordinate(count) - first[count - 2]) / final_divisor
        } else {
            (8. * coordinate(count - 1) + coordinate(count) - 2. * first[count - 2]) / final_divisor
        };
        for index in (0..count - 1).rev() {
            first[index] -= upper[index] * first[index + 1];
        }
        if closed {
            let scale =
                (first[0] + first[count - 1]) / (1. + correction[0] + correction[count - 1]);
            for index in 0..count {
                first[index] -= scale * correction[index];
            }
        }
        for index in 0..count {
            let second = if index + 1 < count {
                2. * coordinate(index + 1) - first[index + 1]
            } else if closed {
                2. * coordinate(count) - first[0]
            } else {
                0.5 * (coordinate(count) + first[index])
            };
            result[index][0][axis] = first[index];
            result[index][1][axis] = second;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PathProportionPlan;

    fn near(a: Vec2, b: Vec2) {
        assert!((a - b).length() < 2e-6, "{a:?} != {b:?}");
    }

    #[test]
    fn natural_open_handles_have_expected_values_and_keep_discontinuities() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1., 1.))
            .line_to(Vec2::new(2., 0.))
            .move_to(Vec2::new(10., 0.))
            .line_to(Vec2::new(13., 0.));
        let smoothed = change_path_anchor_mode(&path, true).unwrap();
        let plan = PathProportionPlan::new(&smoothed).unwrap();
        let points = plan.curve_points(0).unwrap();
        near(
            Vec2::new(points[1].x as f32, points[1].y as f32),
            Vec2::new(1. / 3., 0.5),
        );
        near(
            Vec2::new(points[2].x as f32, points[2].y as f32),
            Vec2::new(2. / 3., 1.),
        );
        assert_eq!(plan.curve_count(), 3);
        assert_eq!(
            smoothed
                .commands()
                .iter()
                .filter(|c| matches!(c, PathCommand::MoveTo { .. }))
                .count(),
            2
        );
        assert_eq!(change_path_anchor_mode(&smoothed, false).unwrap(), path);
    }

    #[test]
    fn periodic_handles_are_translation_invariant_and_c2_at_the_seam() {
        let anchors = [
            Vec2::ZERO,
            Vec2::new(2., 0.),
            Vec2::new(2., 2.),
            Vec2::new(0., 2.),
            Vec2::ZERO,
        ];
        let handles = smooth_handles(&anchors, true);
        near(handles[0][0], Vec2::new(0.5, -0.5));
        near(handles[0][1], Vec2::new(1.5, -0.5));
        for index in 0..4 {
            let next = (index + 1) % 4;
            near(
                anchors[index + 1] - handles[index][1],
                handles[next][0] - anchors[next],
            );
            near(
                anchors[index + 1] - handles[index][1] * 2. + handles[index][0],
                anchors[next] - handles[next][0] * 2. + handles[next][1],
            );
        }
        let shifted = anchors.map(|p| p + Vec2::new(-5., -7.));
        for (original, shifted) in handles.iter().zip(smooth_handles(&shifted, true)) {
            for column in 0..2 {
                near(original[column] + Vec2::new(-5., -7.), shifted[column]);
            }
        }
    }

    fn first_cubic(path: &VectorPath) -> (Vec2, Vec2) {
        match path.commands() {
            [PathCommand::MoveTo { .. }, PathCommand::CubicTo {
                control1, control2, ..
            }, ..] => (*control1, *control2),
            commands => panic!("expected first cubic contour, got {commands:?}"),
        }
    }

    #[test]
    fn manim_signed_closure_uses_natural_handles_for_negative_closed_anchors() {
        let path = VectorPath::new()
            .move_to(Vec2::new(-1., -1.))
            .line_to(Vec2::new(1., -1.))
            .line_to(Vec2::new(1., 1.))
            .line_to(Vec2::new(-1., 1.))
            .close();
        let smoothed =
            change_path_anchor_mode_with_boundary(&path, true, SplineBoundary::ManimSignedClosure)
                .unwrap();
        let (first, second) = first_cubic(&smoothed);
        near(first, Vec2::new(-3. / 14., -17. / 14.));
        near(second, Vec2::new(4. / 7., -10. / 7.));
        assert!(matches!(
            smoothed.commands().last(),
            Some(PathCommand::Close)
        ));
    }

    #[test]
    fn manim_signed_closure_uses_periodic_handles_for_positive_closed_anchors() {
        let path = VectorPath::new()
            .move_to(Vec2::new(1., 1.))
            .line_to(Vec2::new(3., 1.))
            .line_to(Vec2::new(3., 3.))
            .line_to(Vec2::new(1., 3.))
            .close();
        let smoothed =
            change_path_anchor_mode_with_boundary(&path, true, SplineBoundary::ManimSignedClosure)
                .unwrap();
        let (first, second) = first_cubic(&smoothed);
        near(first, Vec2::new(1.5, 0.5));
        near(second, Vec2::new(2.5, 0.5));
        assert!(matches!(
            smoothed.commands().last(),
            Some(PathCommand::Close)
        ));
    }

    #[test]
    fn empty_singleton_and_nonfinite_inputs_are_deterministic() {
        assert!(change_path_anchor_mode(&VectorPath::new(), true)
            .unwrap()
            .is_empty());
        assert!(
            change_path_anchor_mode(&VectorPath::new().move_to(Vec2::ZERO), true)
                .unwrap()
                .is_empty()
        );
        assert!(
            change_path_anchor_mode(&VectorPath::new().move_to(Vec2::new(f32::NAN, 0.)), true)
                .is_err()
        );
    }
}
