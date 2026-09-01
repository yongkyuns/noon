use noon_core::{GeometryRef, SemanticLoweringError, SemanticVec3, Transform2D, Vec2, TAU};

use crate::{point_from_proportion, point_from_proportion_f64, PathProportionError};

const MANIM_CIRCLE_COMPONENTS: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeometryProportionError {
    Path(PathProportionError),
    UnsupportedGeometry,
}

impl std::fmt::Display for GeometryProportionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path(error) => error.fmt(formatter),
            Self::UnsupportedGeometry => formatter.write_str(
                "point_from_proportion requires retained circle, line, or vector-path geometry",
            ),
        }
    }
}

impl std::error::Error for GeometryProportionError {}

impl From<PathProportionError> for GeometryProportionError {
    fn from(value: PathProportionError) -> Self {
        Self::Path(value)
    }
}

/// Return the local-space point at `alpha` for path-like retained geometry.
///
/// Vector paths reuse [`point_from_proportion`]. Lines remain exact analytic
/// segments. Circles reproduce ManimCE v0.21's nine-component quadratic-Bezier
/// `Arc(TAU)` representation rather than substituting an ideal trigonometric
/// circle, so downstream tangent/path queries observe the same authored curve.
pub fn point_from_geometry_proportion(
    geometry: &GeometryRef,
    alpha: f32,
) -> Result<Vec2, GeometryProportionError> {
    validate_proportion_f32(alpha)?;
    match geometry {
        GeometryRef::Circle { radius } => Ok(circle_point_from_proportion(*radius, alpha)),
        GeometryRef::Line { start, end } => Ok(*start + (*end - *start) * alpha),
        GeometryRef::VectorPath(path) => Ok(point_from_proportion(path, alpha)?),
        GeometryRef::Rectangle { .. } | GeometryRef::External(_) => {
            Err(GeometryProportionError::UnsupportedGeometry)
        }
    }
}

/// High-precision authoring query for path-like retained geometry.
///
/// Retained coordinates remain renderer-facing f32, but the proportion,
/// interpolation, analytic circle reconstruction, and downstream finite
/// differences stay in f64. Vector paths reuse the same prepared sampled-length
/// measure as [`point_from_geometry_proportion`] rather than maintaining another
/// path-measure implementation.
pub fn point_from_geometry_proportion_f64(
    geometry: &GeometryRef,
    alpha: f64,
) -> Result<SemanticVec3, GeometryProportionError> {
    validate_proportion_f64(alpha)?;
    match geometry {
        GeometryRef::Circle { radius } => {
            Ok(circle_point_from_proportion_f64(f64::from(*radius), alpha))
        }
        GeometryRef::Line { start, end } => Ok(lerp_semantic(
            SemanticVec3::from_vec2(*start),
            SemanticVec3::from_vec2(*end),
            alpha,
        )),
        GeometryRef::VectorPath(path) => Ok(point_from_proportion_f64(path, alpha)?),
        GeometryRef::Rectangle { .. } | GeometryRef::External(_) => {
            Err(GeometryProportionError::UnsupportedGeometry)
        }
    }
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
    let lower = transform_semantic_point(
        transform,
        point_from_geometry_proportion_f64(geometry, lower_alpha)?,
    );
    let upper = transform_semantic_point(
        transform,
        point_from_geometry_proportion_f64(geometry, upper_alpha)?,
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

fn circle_point_from_proportion(radius: f32, alpha: f32) -> Vec2 {
    if alpha == 1.0 {
        return Vec2::new(radius, 0.0);
    }

    let component_count = MANIM_CIRCLE_COMPONENTS as f32;
    let scaled = alpha * component_count;
    let component = (scaled.floor() as usize).min(MANIM_CIRCLE_COMPONENTS - 1);
    let t = scaled - component as f32;
    let theta = TAU / component_count;
    let start_angle = component as f32 * theta;
    let end_angle = (component + 1) as f32 * theta;
    let middle_angle = (start_angle + end_angle) * 0.5;
    let control_radius = radius / (theta * 0.5).cos();

    let start = Vec2::new(start_angle.cos() * radius, start_angle.sin() * radius);
    let control = Vec2::new(
        middle_angle.cos() * control_radius,
        middle_angle.sin() * control_radius,
    );
    let end = Vec2::new(end_angle.cos() * radius, end_angle.sin() * radius);

    let first = start + (control - start) * t;
    let second = control + (end - control) * t;
    first + (second - first) * t
}

fn circle_point_from_proportion_f64(radius: f64, alpha: f64) -> SemanticVec3 {
    if alpha == 1.0 {
        return SemanticVec3::new(radius, 0.0, 0.0);
    }

    let component_count = MANIM_CIRCLE_COMPONENTS as f64;
    let scaled = alpha * component_count;
    let component = (scaled.floor() as usize).min(MANIM_CIRCLE_COMPONENTS - 1);
    let t = scaled - component as f64;
    let theta = std::f64::consts::TAU / component_count;
    let start_angle = component as f64 * theta;
    let end_angle = (component + 1) as f64 * theta;
    let middle_angle = (start_angle + end_angle) * 0.5;
    let control_radius = radius / (theta * 0.5).cos();

    let start = SemanticVec3::new(start_angle.cos() * radius, start_angle.sin() * radius, 0.0);
    let control = SemanticVec3::new(
        middle_angle.cos() * control_radius,
        middle_angle.sin() * control_radius,
        0.0,
    );
    let end = SemanticVec3::new(end_angle.cos() * radius, end_angle.sin() * radius, 0.0);

    let first = lerp_semantic(start, control, t);
    let second = lerp_semantic(control, end, t);
    lerp_semantic(first, second, t)
}

fn lerp_semantic(start: SemanticVec3, end: SemanticVec3, alpha: f64) -> SemanticVec3 {
    SemanticVec3::new(
        start.x + (end.x - start.x) * alpha,
        start.y + (end.y - start.y) * alpha,
        start.z + (end.z - start.z) * alpha,
    )
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
    fn circle_matches_manim_quadratic_arc_proportion_samples() {
        let circle = GeometryRef::circle(2.0);
        assert_point(
            point_from_geometry_proportion(&circle, 0.0).unwrap(),
            Vec2::new(2.0, 0.0),
        );
        assert_point(
            point_from_geometry_proportion(&circle, 0.25).unwrap(),
            Vec2::new(-0.0057401983, 2.0021698),
        );
        assert_point(
            point_from_geometry_proportion(&circle, 0.4).unwrap(),
            Vec2::new(-1.6191778, 1.1800613),
        );
        assert_point(
            point_from_geometry_proportion(&circle, 1.0).unwrap(),
            Vec2::new(2.0, 0.0),
        );
    }

    #[test]
    fn precise_circle_query_preserves_default_tangent_sample_delta() {
        let circle = GeometryRef::circle(2.0);
        let lower = point_from_geometry_proportion_f64(&circle, 0.4 - 1.0e-6).unwrap();
        let upper = point_from_geometry_proportion_f64(&circle, 0.4 + 1.0e-6).unwrap();

        assert_semantic(
            lower,
            SemanticVec3::new(-1.6191706294001942, 1.1800713157856422, 0.0),
            1.0e-12,
        );
        assert_semantic(
            upper,
            SemanticVec3::new(-1.6191850851338567, 1.1800512993440777, 0.0),
            1.0e-12,
        );
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
            0.4,
            4.0,
            1.0e-6,
        )
        .unwrap();

        assert_point(segment.start, Vec2::new(-0.4482279, 2.8014424));
        assert_point(segment.end, Vec2::new(-2.7901278, -0.44131964));
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
            &GeometryRef::circle(2.0),
            transform,
            0.4,
            3.0,
            1.0e-4,
        )
        .unwrap();
        assert_point(transformed.start, Vec2::new(-1.058928, -0.50566184));
        assert_point(transformed.end, Vec2::new(-3.4772415, -2.2809815));
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
            point_from_geometry_proportion(&GeometryRef::rectangle(1.0, 1.0), 0.5),
            Err(GeometryProportionError::UnsupportedGeometry)
        );
    }
}
