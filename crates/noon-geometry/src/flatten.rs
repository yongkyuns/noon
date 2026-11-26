//! Shared contour flattening for morph preparation and boolean geometry.
use crate::GeometryError;
use noon_core::{PathCommand, Vec2, VectorPath};
const MAX_FLATTEN_DEPTH: u32 = 16;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlattenedContour {
    pub(crate) points: Vec<Vec2>,
    pub(crate) feature_indices: Vec<usize>,
    pub(crate) closed: bool,
    pub(crate) reveal: Vec<f32>,
    pub(crate) end_reveal: f32,
}

pub(crate) fn flatten_path(
    path: &VectorPath,
    tolerance: f32,
) -> Result<Vec<FlattenedContour>, GeometryError> {
    flatten_path_impl::<false>(path, tolerance).map(|(contours, _)| contours)
}

/// Retain the authored curve index plus Bézier parameter for each flattened point.
pub(crate) fn flatten_path_with_reveal(
    path: &VectorPath,
    tolerance: f32,
) -> Result<(Vec<FlattenedContour>, usize), GeometryError> {
    flatten_path_impl::<true>(path, tolerance)
}

fn flatten_path_impl<const REVEAL: bool>(
    path: &VectorPath,
    tolerance: f32,
) -> Result<(Vec<FlattenedContour>, usize), GeometryError> {
    let mut contours = Vec::new();
    let mut contour = FlattenedContour {
        points: Vec::new(),
        feature_indices: Vec::new(),
        closed: false,
        reveal: Vec::new(),
        end_reveal: 0.0,
    };
    let mut current = Vec2::ZERO;
    let mut start = Vec2::ZERO;
    let mut active = false;
    let mut curve_index = 0;
    let finish = |contour: &mut FlattenedContour, closed, curve_index| FlattenedContour {
        points: std::mem::take(&mut contour.points),
        feature_indices: std::mem::take(&mut contour.feature_indices),
        closed,
        reveal: std::mem::take(&mut contour.reveal),
        end_reveal: curve_index as f32,
    };

    for command in path.commands() {
        let mut push = |point, parameter| {
            if contour.points.last().copied() != Some(point) {
                contour.points.push(point);
                if REVEAL {
                    contour.reveal.push(curve_index as f32 + parameter);
                }
            }
        };
        match *command {
            PathCommand::MoveTo { to } => {
                finite(to)?;
                if active {
                    contours.push(finish(&mut contour, false, curve_index));
                }
                contour.points.push(to);
                contour.feature_indices.push(0);
                if REVEAL {
                    contour.reveal.push(curve_index as f32);
                }
                current = to;
                start = to;
                active = true;
                continue;
            }
            PathCommand::LineTo { to } => {
                require_active(active)?;
                finite(to)?;
                push(to, 1.0);
                current = to;
            }
            PathCommand::QuadraticTo { control, to } => {
                require_active(active)?;
                finite(control)?;
                finite(to)?;
                flatten_quadratic(current, control, to, tolerance, 0, [0.0, 1.0], &mut push);
                current = to;
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                require_active(active)?;
                finite(control1)?;
                finite(control2)?;
                finite(to)?;
                flatten_cubic(
                    current,
                    control1,
                    control2,
                    to,
                    tolerance,
                    0,
                    [0.0, 1.0],
                    &mut push,
                );
                current = to;
            }
            PathCommand::Close => {
                if !active {
                    return Err(GeometryError::CloseBeforeMove);
                }
                if REVEAL && (current.x - start.x).hypot(current.y - start.y) > f32::EPSILON {
                    curve_index += 1;
                }
                if contour.points.last().copied() == Some(start) {
                    let removed = contour.points.len() - 1;
                    contour.points.pop();
                    contour.reveal.pop();
                    contour.feature_indices.retain(|index| *index != removed);
                }
                contours.push(finish(&mut contour, true, curve_index));
                current = start;
                active = false;
                continue;
            }
        }
        if REVEAL {
            curve_index += 1;
        }
        mark_feature(&mut contour.feature_indices, contour.points.len() - 1);
    }
    if active {
        contours.push(finish(&mut contour, false, curve_index));
    }
    Ok((contours, curve_index))
}

fn flatten_quadratic(
    from: Vec2,
    control: Vec2,
    to: Vec2,
    tolerance: f32,
    depth: u32,
    [start, end]: [f32; 2],
    push: &mut impl FnMut(Vec2, f32),
) {
    if depth >= MAX_FLATTEN_DEPTH || point_line_distance(control, from, to) <= tolerance {
        push(to, end);
        return;
    }
    let from_control = midpoint(from, control);
    let control_to = midpoint(control, to);
    let middle = midpoint(from_control, control_to);
    let half = (start + end) * 0.5;
    flatten_quadratic(
        from,
        from_control,
        middle,
        tolerance,
        depth + 1,
        [start, half],
        push,
    );
    flatten_quadratic(
        middle,
        control_to,
        to,
        tolerance,
        depth + 1,
        [half, end],
        push,
    );
}

#[allow(clippy::too_many_arguments)]
fn flatten_cubic(
    from: Vec2,
    control1: Vec2,
    control2: Vec2,
    to: Vec2,
    tolerance: f32,
    depth: u32,
    [start, end]: [f32; 2],
    push: &mut impl FnMut(Vec2, f32),
) {
    let flatness =
        point_line_distance(control1, from, to).max(point_line_distance(control2, from, to));
    if depth >= MAX_FLATTEN_DEPTH || flatness <= tolerance {
        push(to, end);
        return;
    }
    let a = midpoint(from, control1);
    let b = midpoint(control1, control2);
    let c = midpoint(control2, to);
    let d = midpoint(a, b);
    let e = midpoint(b, c);
    let middle = midpoint(d, e);
    let half = (start + end) * 0.5;
    flatten_cubic(
        from,
        a,
        d,
        middle,
        tolerance,
        depth + 1,
        [start, half],
        push,
    );
    flatten_cubic(middle, e, c, to, tolerance, depth + 1, [half, end], push);
}

fn mark_feature(features: &mut Vec<usize>, index: usize) {
    if features.last().copied() != Some(index) {
        features.push(index);
    }
}

fn midpoint(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new(a.x * 0.5 + b.x * 0.5, a.y * 0.5 + b.y * 0.5)
}

fn point_line_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    // Distance to the finite chord, not its infinite supporting line. Collinear
    // handles outside the endpoints can form a real excursion/backtracking curve.
    // f64 keeps the intermediate dot products finite for all finite f32 inputs.
    let as_f64 = |v: Vec2| [f64::from(v.x), f64::from(v.y)];
    stroke_segment_distance(as_f64(point), as_f64(start), as_f64(end)) as f32
}

fn finite(value: Vec2) -> Result<(), GeometryError> {
    if value.x.is_finite() && value.y.is_finite() {
        Ok(())
    } else {
        Err(GeometryError::NonFinitePoint)
    }
}

fn require_active(active: bool) -> Result<(), GeometryError> {
    if active {
        Ok(())
    } else {
        Err(GeometryError::DrawingBeforeMove)
    }
}

/// A view pick against a revealed stroke. Closed-only paths return `None` so
/// the caller can keep its separate interior-picking policy for outlines.
pub fn open_path_stroke_hit(
    path: &VectorPath,
    point: [f64; 2],
    scale: [f64; 2],
    radius: f64,
    reveal: f32,
) -> Option<bool> {
    let contours = flatten_path(path, 0.002).ok()?;
    if contours.iter().all(|contour| contour.closed) {
        return None;
    }
    let scaled = |p: Vec2| [f64::from(p.x) * scale[0], f64::from(p.y) * scale[1]];
    let segments = || {
        contours.iter().flat_map(|contour| {
            let points = &contour.points;
            points
                .windows(2)
                .map(|w| (scaled(w[0]), scaled(w[1])))
                .chain(
                    (contour.closed && points.len() > 1)
                        .then(|| (scaled(points[points.len() - 1]), scaled(points[0]))),
                )
        })
    };
    let mut remaining = if reveal >= 1.0 {
        f64::INFINITY
    } else {
        segments()
            .map(|(a, b)| (b[0] - a[0]).hypot(b[1] - a[1]))
            .sum::<f64>()
            * f64::from(reveal.max(0.0))
    };
    for (a, mut b) in segments() {
        if remaining <= 0.0 {
            break;
        }
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        if remaining < length {
            let fraction = remaining / length;
            b = [
                a[0] + (b[0] - a[0]) * fraction,
                a[1] + (b[1] - a[1]) * fraction,
            ];
        }
        if point_near_stroke_segment(point, a, b, radius) {
            return Some(true);
        }
        remaining -= length;
    }
    Some(false)
}

pub fn point_near_stroke_segment(point: [f64; 2], a: [f64; 2], b: [f64; 2], radius: f64) -> bool {
    stroke_segment_distance(point, a, b) <= radius
}

fn stroke_segment_distance(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length_sq = dx * dx + dy * dy;
    let at = if length_sq == 0.0 {
        0.0
    } else {
        (((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / length_sq).clamp(0.0, 1.0)
    };
    (point[0] - a[0] - at * dx).hypot(point[1] - a[1] - at * dy)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collinear_control_excursions_are_not_discarded() {
        let path = VectorPath::new().move_to(Vec2::ZERO).cubic_to(
            Vec2::new(4., 0.),
            Vec2::new(-4., 0.),
            Vec2::new(1., 0.),
        );
        let contours = flatten_path(&path, 0.001).unwrap();
        assert!(contours[0].points.iter().any(|p| p.x > 1.));
        assert!(contours[0].points.iter().any(|p| p.x < 0.));
    }
    #[test]
    fn open_stroke_respects_distance_and_partial_reveal() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(4.0, 4.0));
        assert_eq!(
            open_path_stroke_hit(&path, [2.0, 2.0], [1.0; 2], 0.1, 1.0),
            Some(true)
        );
        assert_eq!(
            open_path_stroke_hit(&path, [2.0, 2.4], [1.0; 2], 0.1, 1.0),
            Some(false)
        );
        assert_eq!(
            open_path_stroke_hit(&path, [2.0, 2.0], [1.0; 2], 0.1, 0.25),
            Some(false)
        );
    }
}
