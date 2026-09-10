//! Shared contour flattening for morph preparation and boolean geometry.
use crate::GeometryError;
use noon_core::{PathCommand, Vec2, VectorPath};
const MAX_FLATTEN_DEPTH: u32 = 16;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlattenedContour {
    pub(crate) points: Vec<Vec2>,
    pub(crate) feature_indices: Vec<usize>,
    pub(crate) closed: bool,
}

pub(crate) fn flatten_path(
    path: &VectorPath,
    tolerance: f32,
) -> Result<Vec<FlattenedContour>, GeometryError> {
    let mut contours = Vec::new();
    let mut points = Vec::new();
    let mut feature_indices = Vec::new();
    let mut current = Vec2::ZERO;
    let mut start = Vec2::ZERO;
    let mut active = false;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                finite(to)?;
                if active {
                    contours.push(FlattenedContour {
                        points: std::mem::take(&mut points),
                        feature_indices: std::mem::take(&mut feature_indices),
                        closed: false,
                    });
                }
                points.push(to);
                feature_indices.push(0);
                current = to;
                start = to;
                active = true;
            }
            PathCommand::LineTo { to } => {
                require_active(active)?;
                finite(to)?;
                push_distinct(&mut points, to);
                mark_feature(&mut feature_indices, points.len() - 1);
                current = to;
            }
            PathCommand::QuadraticTo { control, to } => {
                require_active(active)?;
                finite(control)?;
                finite(to)?;
                flatten_quadratic(current, control, to, tolerance, 0, &mut points);
                mark_feature(&mut feature_indices, points.len() - 1);
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
                flatten_cubic(current, control1, control2, to, tolerance, 0, &mut points);
                mark_feature(&mut feature_indices, points.len() - 1);
                current = to;
            }
            PathCommand::Close => {
                if !active {
                    return Err(GeometryError::CloseBeforeMove);
                }
                if points.last().copied() == Some(start) {
                    let removed = points.len() - 1;
                    points.pop();
                    feature_indices.retain(|index| *index != removed);
                }
                contours.push(FlattenedContour {
                    points: std::mem::take(&mut points),
                    feature_indices: std::mem::take(&mut feature_indices),
                    closed: true,
                });
                current = start;
                active = false;
            }
        }
    }
    if active {
        contours.push(FlattenedContour {
            points,
            feature_indices,
            closed: false,
        });
    }
    Ok(contours)
}

fn flatten_quadratic(
    from: Vec2,
    control: Vec2,
    to: Vec2,
    tolerance: f32,
    depth: u32,
    points: &mut Vec<Vec2>,
) {
    if depth >= MAX_FLATTEN_DEPTH || point_line_distance(control, from, to) <= tolerance {
        push_distinct(points, to);
        return;
    }
    let from_control = midpoint(from, control);
    let control_to = midpoint(control, to);
    let middle = midpoint(from_control, control_to);
    flatten_quadratic(from, from_control, middle, tolerance, depth + 1, points);
    flatten_quadratic(middle, control_to, to, tolerance, depth + 1, points);
}

#[allow(clippy::too_many_arguments)]
fn flatten_cubic(
    from: Vec2,
    control1: Vec2,
    control2: Vec2,
    to: Vec2,
    tolerance: f32,
    depth: u32,
    points: &mut Vec<Vec2>,
) {
    let flatness =
        point_line_distance(control1, from, to).max(point_line_distance(control2, from, to));
    if depth >= MAX_FLATTEN_DEPTH || flatness <= tolerance {
        push_distinct(points, to);
        return;
    }
    let a = midpoint(from, control1);
    let b = midpoint(control1, control2);
    let c = midpoint(control2, to);
    let d = midpoint(a, b);
    let e = midpoint(b, c);
    let middle = midpoint(d, e);
    flatten_cubic(from, a, d, middle, tolerance, depth + 1, points);
    flatten_cubic(middle, e, c, to, tolerance, depth + 1, points);
}

fn mark_feature(features: &mut Vec<usize>, index: usize) {
    if features.last().copied() != Some(index) {
        features.push(index);
    }
}

fn push_distinct(points: &mut Vec<Vec2>, point: Vec2) {
    if points.last().copied() != Some(point) {
        points.push(point);
    }
}

fn midpoint(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new(a.x * 0.5 + b.x * 0.5, a.y * 0.5 + b.y * 0.5)
}

fn point_line_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    // Distance to the finite chord, not its infinite supporting line. Collinear
    // handles outside the endpoints can form a real excursion/backtracking curve.
    // f64 keeps the intermediate dot products finite for all finite f32 inputs.
    let dx = f64::from(end.x) - f64::from(start.x);
    let dy = f64::from(end.y) - f64::from(start.y);
    let px = f64::from(point.x) - f64::from(start.x);
    let py = f64::from(point.y) - f64::from(start.y);
    let length_squared = dx * dx + dy * dy;
    let t = if length_squared == 0. {
        0.
    } else {
        ((px * dx + py * dy) / length_squared).clamp(0., 1.)
    };
    (px - t * dx).hypot(py - t * dy) as f32
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
}
