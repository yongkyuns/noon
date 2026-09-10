use noon_core::{GeometryRef, SemanticLoweringError, SemanticVec3, Transform2D, Vec2, VectorPath};
use std::borrow::Cow;

use crate::{canonical_outline_path, PathProportionError, PathProportionPlan};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeometryProportionError {
    Path(PathProportionError),
    UnsupportedGeometry,
}

impl std::fmt::Display for GeometryProportionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path(error) => error.fmt(formatter),
            Self::UnsupportedGeometry => {
                formatter.write_str("point_from_proportion requires retained path-like geometry")
            }
        }
    }
}

impl std::error::Error for GeometryProportionError {}

impl From<PathProportionError> for GeometryProportionError {
    fn from(value: PathProportionError) -> Self {
        Self::Path(value)
    }
}

/// Borrow retained paths or construct a bounded canonical primitive outline.
fn geometry_path(geometry: &GeometryRef) -> Result<Cow<'_, VectorPath>, GeometryProportionError> {
    if let GeometryRef::VectorPath(path) = geometry {
        return Ok(Cow::Borrowed(path));
    }
    canonical_outline_path(geometry)
        .map(Cow::Owned)
        .ok_or(GeometryProportionError::UnsupportedGeometry)
}

/// Return a local-space point using the same canonical outline and sampled
/// measure as public path observations and renderer path progress.
pub fn point_from_geometry_proportion(
    geometry: &GeometryRef,
    alpha: f32,
) -> Result<Vec2, GeometryProportionError> {
    validate_proportion_f32(alpha)?;
    Ok(PathProportionPlan::new(geometry_path(geometry)?.as_ref())?.point(alpha)?)
}

/// High-precision local-space query over the canonical retained path measure.
pub fn point_from_geometry_proportion_f64(
    geometry: &GeometryRef,
    alpha: f64,
) -> Result<SemanticVec3, GeometryProportionError> {
    validate_proportion_f64(alpha)?;
    Ok(PathProportionPlan::new(geometry_path(geometry)?.as_ref())?.point_f64(alpha)?)
}

/// A retained world-space line segment produced from ManimCE-compatible tangent sampling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TangentSegment {
    pub start: Vec2,
    pub end: Vec2,
}

impl TangentSegment {
    pub fn length(self) -> f32 {
        (self.end - self.start).length()
    }

    pub fn center(self) -> Vec2 {
        (self.start + self.end) * 0.5
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TangentSegmentError {
    Geometry(GeometryProportionError),
    NonFiniteLength(f64),
    InvalidDelta(f64),
    DegenerateSample,
    Lowering(SemanticLoweringError),
}

impl std::fmt::Display for TangentSegmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Geometry(error) => error.fmt(formatter),
            Self::NonFiniteLength(length) => {
                write!(formatter, "tangent length must be finite, got {length}")
            }
            Self::InvalidDelta(delta) => write!(
                formatter,
                "tangent sample delta must be finite and nonzero, got {delta}"
            ),
            Self::DegenerateSample => {
                formatter.write_str("tangent sample points collapse to one world-space point")
            }
            Self::Lowering(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TangentSegmentError {}

impl From<GeometryProportionError> for TangentSegmentError {
    fn from(value: GeometryProportionError) -> Self {
        Self::Geometry(value)
    }
}

impl From<SemanticLoweringError> for TangentSegmentError {
    fn from(value: SemanticLoweringError) -> Self {
        Self::Lowering(value)
    }
}

/// Sample and normalize a ManimCE v0.21 finite-difference tangent in world space.
///
/// Manim samples `alpha - d_alpha` and `alpha + d_alpha`, clipped to `[0, 1]`,
/// creates an ordinary line between those points, then scales that line about its
/// center to the requested length. This implementation preserves those operations
/// in f64 through sampling, the current retained transform, and normalization,
/// lowering only the final retained line endpoints to f32.
///
/// Sampling before normalization is required for non-uniformly transformed paths.
/// Negative finite lengths and deltas retain Manim's ordinary line/scale behavior;
/// zero `d_alpha` is rejected because it cannot define a tangent direction.
pub fn tangent_segment_from_geometry_proportion(
    geometry: &GeometryRef,
    transform: Transform2D,
    alpha: f64,
    length: f64,
    d_alpha: f64,
) -> Result<TangentSegment, TangentSegmentError> {
    validate_proportion_f64(alpha)?;
    if !length.is_finite() {
        return Err(TangentSegmentError::NonFiniteLength(length));
    }
    if !d_alpha.is_finite() || d_alpha == 0.0 {
        return Err(TangentSegmentError::InvalidDelta(d_alpha));
    }

    let lower_alpha = (alpha - d_alpha).clamp(0.0, 1.0);
    let upper_alpha = (alpha + d_alpha).clamp(0.0, 1.0);
    let plan = PathProportionPlan::with_scale(
        geometry_path(geometry)?.as_ref(),
        (f64::from(transform.scale.x), f64::from(transform.scale.y)),
    )
    .map_err(GeometryProportionError::from)?;
    let lower = transform_semantic_point(
        transform,
        plan.point_f64(lower_alpha)
            .map_err(GeometryProportionError::from)?,
    );
    let upper = transform_semantic_point(
        transform,
        plan.point_f64(upper_alpha)
            .map_err(GeometryProportionError::from)?,
    );

    let dx = upper.x - lower.x;
    let dy = upper.y - lower.y;
    let chord_length = dx.hypot(dy);
    if !chord_length.is_finite() || chord_length == 0.0 {
        return Err(TangentSegmentError::DegenerateSample);
    }

    let center = SemanticVec3::new(
        (lower.x + upper.x) * 0.5,
        (lower.y + upper.y) * 0.5,
        (lower.z + upper.z) * 0.5,
    );
    let half_scale = length / chord_length * 0.5;
    let half_x = dx * half_scale;
    let half_y = dy * half_scale;
    let start = SemanticVec3::new(center.x - half_x, center.y - half_y, center.z);
    let end = SemanticVec3::new(center.x + half_x, center.y + half_y, center.z);

    Ok(TangentSegment {
        start: start.lower_xy_f32()?,
        end: end.lower_xy_f32()?,
    })
}

fn validate_proportion_f32(alpha: f32) -> Result<(), GeometryProportionError> {
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(PathProportionError::InvalidProportion(alpha).into());
    }
    Ok(())
}

fn validate_proportion_f64(alpha: f64) -> Result<(), GeometryProportionError> {
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(PathProportionError::InvalidProportion(alpha as f32).into());
    }
    Ok(())
}

fn transform_semantic_point(transform: Transform2D, point: SemanticVec3) -> SemanticVec3 {
    let scaled_x = point.x * f64::from(transform.scale.x);
    let scaled_y = point.y * f64::from(transform.scale.y);
    let (sin, cos) = f64::from(transform.rotation).sin_cos();
    SemanticVec3::new(
        scaled_x * cos - scaled_y * sin + f64::from(transform.translation.x),
        scaled_x * sin + scaled_y * cos + f64::from(transform.translation.y),
        point.z,
    )
}

#[cfg(test)]
mod tests {
    use noon_core::VectorPath;

    use super::*;

    fn assert_point(actual: Vec2, expected: Vec2) {
        assert!(
            (actual.x - expected.x).abs() <= 1.0e-5 && (actual.y - expected.y).abs() <= 1.0e-5,
            "{actual:?} != {expected:?}"
        );
    }

    fn assert_semantic(actual: SemanticVec3, expected: SemanticVec3, tolerance: f64) {
        assert!(
            (actual.x - expected.x).abs() <= tolerance
                && (actual.y - expected.y).abs() <= tolerance
                && (actual.z - expected.z).abs() <= tolerance,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn circle_uses_canonical_cairo_path_proportion_samples() {
        let circle = GeometryRef::circle(2.0);
        assert_point(
            point_from_geometry_proportion(&circle, 0.0).unwrap(),
            Vec2::new(2.0, 0.0),
        );
        assert_point(
            point_from_geometry_proportion(&circle, 0.25).unwrap(),
            Vec2::new(0.0, 2.0),
        );
        assert_point(
            point_from_geometry_proportion(&circle, 0.125).unwrap(),
            Vec2::new(2f32.sqrt(), 2f32.sqrt()),
        );
        assert_point(
            point_from_geometry_proportion(&circle, 1.0).unwrap(),
            Vec2::new(2.0, 0.0),
        );
    }

    #[test]
    fn precise_circle_query_preserves_default_tangent_sample_delta() {
        let circle = GeometryRef::circle(2.0);
        let lower = point_from_geometry_proportion_f64(&circle, 0.125 - 1e-6).unwrap();
        let upper = point_from_geometry_proportion_f64(&circle, 0.125 + 1e-6).unwrap();
        assert!(lower.x > upper.x && lower.y < upper.y);
        assert!(((upper.x - lower.x) / (upper.y - lower.y) + 1.).abs() < 1e-5);
    }

    #[test]
    fn line_and_vector_path_keep_existing_shared_measures() {
        let line = GeometryRef::line(Vec2::new(-2.0, 1.0), Vec2::new(2.0, 1.0));
        assert_point(
            point_from_geometry_proportion(&line, 0.75).unwrap(),
            Vec2::new(1.0, 1.0),
        );

        let path = GeometryRef::VectorPath(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .quadratic_to(Vec2::new(1.0, 2.0), Vec2::new(2.0, 0.0)),
        );
        assert_point(
            point_from_geometry_proportion(&path, 0.5).unwrap(),
            Vec2::new(1.0, 1.0),
        );
        assert_semantic(
            point_from_geometry_proportion_f64(&path, 0.5).unwrap(),
            SemanticVec3::new(1.0, 1.0, 0.0),
            1.0e-12,
        );
    }

    #[test]
    fn tangent_segment_matches_pinned_manim_circle_default_precision() {
        let segment = tangent_segment_from_geometry_proportion(
            &GeometryRef::circle(2.0),
            Transform2D::IDENTITY,
            0.125,
            4.0,
            1.0e-6,
        )
        .unwrap();

        assert_point(segment.start, Vec2::new(2.0 * 2f32.sqrt(), 0.0));
        assert_point(segment.end, Vec2::new(0.0, 2.0 * 2f32.sqrt()));
        assert!((segment.length() - 4.0).abs() <= 1.0e-5);
    }

    #[test]
    fn tangent_segment_normalizes_after_world_transform_and_clips_samples() {
        let transform = Transform2D {
            translation: Vec2::new(1.0, -1.0),
            rotation: 0.3,
            scale: Vec2::new(2.0, 0.5),
        };
        let transformed = tangent_segment_from_geometry_proportion(
            &GeometryRef::path(
                VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(Vec2::new(1.0, 0.0))
                    .line_to(Vec2::new(1.0, 1.0)),
            ),
            transform,
            0.9,
            3.0,
            1.0e-4,
        )
        .unwrap();
        // World lengths are 2 and 0.5: alpha=.9 is halfway up the vertical
        // segment. Using the local metric would put it at y=.8 instead.
        let (sin, cos) = transform.rotation.sin_cos();
        let center = Vec2::new(1.0 + 2.0 * cos - 0.25 * sin, -1.0 + 2.0 * sin + 0.25 * cos);
        let half_tangent = Vec2::new(-sin, cos) * 1.5;
        assert_point(transformed.start, center - half_tangent);
        assert_point(transformed.end, center + half_tangent);
        assert!((transformed.length() - 3.0).abs() <= 1.0e-5);

        let endpoint = tangent_segment_from_geometry_proportion(
            &GeometryRef::circle(2.0),
            Transform2D::IDENTITY,
            0.0,
            4.0,
            1.0e-6,
        )
        .unwrap();
        assert_point(endpoint.start, Vec2::new(2.0000057, -1.9999934));
        assert_point(endpoint.end, Vec2::new(1.9999942, 2.0000067));
    }

    #[test]
    fn tangent_segment_preserves_zero_negative_and_invalid_input_policy() {
        let line = GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0));
        let zero = tangent_segment_from_geometry_proportion(
            &line,
            Transform2D::IDENTITY,
            0.5,
            0.0,
            1.0e-6,
        )
        .unwrap();
        assert_point(zero.start, Vec2::ZERO);
        assert_point(zero.end, Vec2::ZERO);

        let negative = tangent_segment_from_geometry_proportion(
            &line,
            Transform2D::IDENTITY,
            0.5,
            -2.0,
            1.0e-6,
        )
        .unwrap();
        assert_point(negative.start, Vec2::new(1.0, 0.0));
        assert_point(negative.end, Vec2::new(-1.0, 0.0));

        let negative_delta = tangent_segment_from_geometry_proportion(
            &line,
            Transform2D::IDENTITY,
            0.5,
            2.0,
            -1.0e-3,
        )
        .unwrap();
        assert_point(negative_delta.start, Vec2::new(1.0, 0.0));
        assert_point(negative_delta.end, Vec2::new(-1.0, 0.0));

        assert!(matches!(
            tangent_segment_from_geometry_proportion(&line, Transform2D::IDENTITY, 0.5, 1.0, 0.0,),
            Err(TangentSegmentError::InvalidDelta(0.0))
        ));
        assert!(matches!(
            tangent_segment_from_geometry_proportion(
                &GeometryRef::circle(0.0),
                Transform2D::IDENTITY,
                0.5,
                1.0,
                1.0e-6,
            ),
            Err(TangentSegmentError::DegenerateSample)
        ));
    }

    #[test]
    fn invalid_proportion_and_non_path_geometry_are_rejected() {
        let circle = GeometryRef::circle(1.0);
        assert!(matches!(
            point_from_geometry_proportion(&circle, f32::NAN),
            Err(GeometryProportionError::Path(
                PathProportionError::InvalidProportion(_)
            ))
        ));
        assert!(matches!(
            point_from_geometry_proportion_f64(&circle, f64::NAN),
            Err(GeometryProportionError::Path(
                PathProportionError::InvalidProportion(_)
            ))
        ));
        assert_eq!(
            point_from_geometry_proportion(
                &GeometryRef::External(noon_core::GeometryId::new(0)),
                0.5
            ),
            Err(GeometryProportionError::UnsupportedGeometry)
        );
    }
}
