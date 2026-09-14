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

/// Exact transformed point bounds used by matching-shape activation and leftover
/// group positioning.
///
/// Bounds include every endpoint and Bézier control point used by the matching key,
/// so their center follows the same point family that Manim uses for matching-shape
/// centering rather than introducing a second geometric interpretation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatchingShapeBounds {
    pub min: Vec2,
    pub max: Vec2,
}

impl MatchingShapeBounds {
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    pub fn center(self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            min: Vec2::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Vec2::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
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

/// Return the exact transformed point bounds used to normalize one matching-shape key.
///
/// This intentionally has the same acceptance domain as
/// [`vector_path_matching_shape_key`]: unresolved morph targets, non-finite inputs,
/// empty paths, and zero-height paths fail closed.
pub fn vector_path_matching_shape_bounds(
    path: &VectorPath,
    transform: Transform2D,
) -> Result<MatchingShapeBounds, MatchingShapeKeyError> {
    validate_matching_shape_input(path, transform)?;

    let mut bounds = None;
    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } | PathCommand::LineTo { to } => {
                include_transformed_point(&mut bounds, transform.transform_point(to));
            }
            PathCommand::QuadraticTo { control, to } => {
                include_transformed_point(&mut bounds, transform.transform_point(control));
                include_transformed_point(&mut bounds, transform.transform_point(to));
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                include_transformed_point(&mut bounds, transform.transform_point(control1));
                include_transformed_point(&mut bounds, transform.transform_point(control2));
                include_transformed_point(&mut bounds, transform.transform_point(to));
            }
            PathCommand::Close => {}
        }
    }

    let bounds = bounds.ok_or(MatchingShapeKeyError::EmptyPath)?;
    let height = bounds.max.y - bounds.min.y;
    if !height.is_finite() || height == 0.0 {
        return Err(MatchingShapeKeyError::DegenerateHeight);
    }
    Ok(bounds)
}

/// Build a stable shape key from the vector points visible under `transform`.
///
/// This mirrors the invariants used by ManimCE `TransformMatchingShapes`: absolute
/// translation and positive uniform scale do not affect the key, while rotation,
/// reflection, non-uniform scale, point order, and control topology remain visible.
pub fn vector_path_matching_shape_key(
    path: &VectorPath,
    transform: Transform2D,
) -> Result<MatchingShapeKey, MatchingShapeKeyError> {
    let bounds = vector_path_matching_shape_bounds(path, transform)?;
    let height = bounds.max.y - bounds.min.y;
    let center = bounds.center();

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

fn validate_matching_shape_input(
    path: &VectorPath,
    transform: Transform2D,
) -> Result<(), MatchingShapeKeyError> {
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
    Ok(())
}

fn include_transformed_point(bounds: &mut Option<MatchingShapeBounds>, point: Vec2) {
    *bounds = Some(match *bounds {
        Some(current) => current.union(MatchingShapeBounds::new(point, point)),
        None => MatchingShapeBounds::new(point, point),
    });
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
    fn transformed_bounds_share_the_matching_point_family() {
        let path = asymmetric_path();
        let bounds = vector_path_matching_shape_bounds(
            &path,
            Transform2D {
                translation: Vec2::new(5.0, -3.0),
                rotation: 0.0,
                scale: Vec2::new(2.0, 2.0),
            },
        )
        .unwrap();
        assert_eq!(bounds.min, Vec2::new(3.0, -5.0));
        assert_eq!(bounds.max, Vec2::new(9.0, 1.0));
        assert_eq!(bounds.center(), Vec2::new(6.0, -2.0));

        let other = MatchingShapeBounds::new(Vec2::new(-4.0, -1.0), Vec2::new(-2.0, 7.0));
        let union = bounds.union(other);
        assert_eq!(union.min, Vec2::new(-4.0, -5.0));
        assert_eq!(union.max, Vec2::new(9.0, 7.0));
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
        assert_eq!(
            vector_path_matching_shape_bounds(&flat, Transform2D::IDENTITY),
            Err(MatchingShapeKeyError::DegenerateHeight)
        );

        let morph = asymmetric_path().with_morph_target(asymmetric_path());
        assert_eq!(
            vector_path_matching_shape_key(&morph, Transform2D::IDENTITY),
            Err(MatchingShapeKeyError::MorphTarget)
        );
        assert_eq!(
            vector_path_matching_shape_bounds(&morph, Transform2D::IDENTITY),
            Err(MatchingShapeKeyError::MorphTarget)
        );
    }
}
