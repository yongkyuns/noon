//! Deterministic anchor-handle editing of retained contours. No frame-time solver.
use noon_core::{PathCommand, Vec2, VectorPath};

use crate::PathProportionError;

/// Replace handles with a C2 cubic spline, or straight segments in jagged mode.
/// Each explicit contour is independent. Existing curve anchors/order and closed
/// joins survive; unfinished anchors have no curve and are omitted.
/// Time and temporary storage are linear in the number of anchors.
pub fn change_path_anchor_mode(
    path: &VectorPath,
    smooth: bool,
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
                result = append_contour(result, &anchors, closed, smooth)?;
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
    append_contour(result, &anchors, closed, smooth)
}

fn append_contour(
    mut path: VectorPath,
    anchors: &[Vec2],
    closed: bool,
    smooth: bool,
) -> Result<VectorPath, PathProportionError> {
    if anchors.len() < 2 {
        return Ok(path);
    }
    path = path.move_to(anchors[0]);
    if smooth {
        let closed = closed || anchors.first() == anchors.last();
        let handles = smooth_handles(anchors, closed);
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

/// Solve the natural/open or periodic/closed spline equations in f64. The
/// periodic system uses a rank-one correction to the same tridiagonal solve.
fn smooth_handles(anchors: &[Vec2], closed: bool) -> Vec<[Vec2; 2]> {
    let count = anchors.len() - 1;
    if count == 1 {
        return vec![[
            anchors[0] + (anchors[1] - anchors[0]) / 3.,
            anchors[0] + (anchors[1] - anchors[0]) * (2. / 3.),
        ]];
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
    let mut result = vec![[Vec2::ZERO; 2]; count];
    for axis in 0..2 {
        let coordinate = |index: usize| -> f64 {
            if axis == 0 {
                f64::from(anchors[index].x)
            } else {
                f64::from(anchors[index].y)
            }
        };
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
            if axis == 0 {
                result[index][0].x = first[index] as f32;
                result[index][1].x = second as f32;
            } else {
                result[index][0].y = first[index] as f32;
                result[index][1].y = second as f32;
            }
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
