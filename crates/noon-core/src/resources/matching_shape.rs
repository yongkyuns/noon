use std::{error::Error, fmt};

use crate::{PathCommand, Transform2D, Vec2, VectorPath};

/// Deterministic Manim-style shape identity for vector-path matching.
///
/// Coordinates are evaluated in the supplied effective transform, centered by their
/// point bounds, normalized to unit height, and quantized to three decimals. The
/// ordered path-command structure is retained so geometrically different control
/// layouts cannot be paired accidentally.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MatchingShapeKey(Vec<i32>);

impl MatchingShapeKey {
    pub fn quantized_components(&self) -> &[i32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchingShapeKeyError {
    EmptyPath,
    DegenerateHeight,
    MorphTarget,
    NonFinite,
    QuantizationOverflow,
}

impl fmt::Display for MatchingShapeKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyPath => "matching-shape key requires at least one vector-path point",
            Self::DegenerateHeight => "matching-shape key requires non-zero path height",
            Self::MorphTarget => {
                "matching-shape key requires resolved geometry, not a morph target"
            }
            Self::NonFinite => "matching-shape key requires finite geometry and transform values",
            Self::QuantizationOverflow => {
                "matching-shape key exceeds deterministic quantization range"
            }
        };
        f.write_str(message)
    }
}

impl Error for MatchingShapeKeyError {}

/// Build a stable shape key from the vector points visible under `transform`.
///
/// This mirrors the invariants used by ManimCE `TransformMatchingShapes`: absolute
/// translation and positive uniform scale do not affect the key, while rotation,
/// reflection, non-uniform scale, point order, and control topology remain visible.
pub fn vector_path_matching_shape_key(
    path: &VectorPath,
    transform: Transform2D,
) -> Result<MatchingShapeKey, MatchingShapeKeyError> {
    if path.morph_target().is_some() {
        return Err(MatchingShapeKeyError::MorphTarget);
    }
    if !path.is_finite()
        || !transform.translation.x.is_finite()
        || !transform.translation.y.is_finite()
        || !transform.rotation.is_finite()
        || !transform.scale.x.is_finite()
        || !transform.scale.y.is_finite()
    {
        return Err(MatchingShapeKeyError::NonFinite);
    }

    let mut points = Vec::new();
    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } | PathCommand::LineTo { to } => {
                points.push(transform.transform_point(to));
            }
            PathCommand::QuadraticTo { control, to } => {
                points.push(transform.transform_point(control));
                points.push(transform.transform_point(to));
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                points.push(transform.transform_point(control1));
                points.push(transform.transform_point(control2));
                points.push(transform.transform_point(to));
            }
            PathCommand::Close => {}
        }
    }

    let first = *points.first().ok_or(MatchingShapeKeyError::EmptyPath)?;
    let mut min = first;
    let mut max = first;
    for point in points.iter().copied().skip(1) {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    let height = max.y - min.y;
    if !height.is_finite() || height == 0.0 {
        return Err(MatchingShapeKeyError::DegenerateHeight);
    }
    let center = (min + max) * 0.5;

    let mut key = Vec::with_capacity(path.commands().len() * 7);
    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                key.push(0);
                push_normalized_point(&mut key, to, transform, center, height)?;
            }
            PathCommand::LineTo { to } => {
                key.push(1);
                push_normalized_point(&mut key, to, transform, center, height)?;
            }
            PathCommand::QuadraticTo { control, to } => {
                key.push(2);
                push_normalized_point(&mut key, control, transform, center, height)?;
                push_normalized_point(&mut key, to, transform, center, height)?;
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                key.push(3);
                push_normalized_point(&mut key, control1, transform, center, height)?;
                push_normalized_point(&mut key, control2, transform, center, height)?;
                push_normalized_point(&mut key, to, transform, center, height)?;
            }
            PathCommand::Close => key.push(4),
        }
    }

    Ok(MatchingShapeKey(key))
}

fn push_normalized_point(
    key: &mut Vec<i32>,
    point: Vec2,
    transform: Transform2D,
    center: Vec2,
    height: f32,
) -> Result<(), MatchingShapeKeyError> {
    let normalized = (transform.transform_point(point) - center) / height;
    push_quantized(key, normalized.x)?;
    push_quantized(key, normalized.y)
}

fn push_quantized(key: &mut Vec<i32>, value: f32) -> Result<(), MatchingShapeKeyError> {
    let rounded = (value * 1000.0).round();
    if !rounded.is_finite() || rounded < i32::MIN as f32 || rounded > i32::MAX as f32 {
        return Err(MatchingShapeKeyError::QuantizationOverflow);
    }
    key.push(rounded as i32);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PI;

    fn asymmetric_path() -> VectorPath {
        VectorPath::new()
            .move_to(Vec2::new(-1.0, -1.0))
            .line_to(Vec2::new(2.0, -0.25))
            .quadratic_to(Vec2::new(1.5, 1.5), Vec2::new(-0.5, 2.0))
            .close()
    }

    #[test]
    fn key_ignores_translation_and_positive_uniform_scale() {
        let path = asymmetric_path();
        let base = vector_path_matching_shape_key(&path, Transform2D::IDENTITY).unwrap();
        let moved_scaled = vector_path_matching_shape_key(
            &path,
            Transform2D {
                translation: Vec2::new(17.0, -9.0),
                rotation: 0.0,
                scale: Vec2::new(3.25, 3.25),
            },
        )
        .unwrap();
        assert_eq!(base, moved_scaled);
    }

    #[test]
    fn key_remains_rotation_sensitive() {
        let path = asymmetric_path();
        let base = vector_path_matching_shape_key(&path, Transform2D::IDENTITY).unwrap();
        let rotated = vector_path_matching_shape_key(
            &path,
            Transform2D {
                translation: Vec2::ZERO,
                rotation: PI / 3.0,
                scale: Vec2::ONE,
            },
        )
        .unwrap();
        assert_ne!(base, rotated);
    }

    #[test]
    fn key_preserves_control_topology_and_order() {
        let quadratic = asymmetric_path();
        let line_only = VectorPath::new()
            .move_to(Vec2::new(-1.0, -1.0))
            .line_to(Vec2::new(2.0, -0.25))
            .line_to(Vec2::new(-0.5, 2.0))
            .close();
        assert_ne!(
            vector_path_matching_shape_key(&quadratic, Transform2D::IDENTITY).unwrap(),
            vector_path_matching_shape_key(&line_only, Transform2D::IDENTITY).unwrap()
        );
    }

    #[test]
    fn unresolved_or_flat_paths_fail_closed() {
        let flat = VectorPath::new()
            .move_to(Vec2::new(0.0, 1.0))
            .line_to(Vec2::new(2.0, 1.0));
        assert_eq!(
            vector_path_matching_shape_key(&flat, Transform2D::IDENTITY),
            Err(MatchingShapeKeyError::DegenerateHeight)
        );

        let morph = asymmetric_path().with_morph_target(asymmetric_path());
        assert_eq!(
            vector_path_matching_shape_key(&morph, Transform2D::IDENTITY),
            Err(MatchingShapeKeyError::MorphTarget)
        );
    }
}
