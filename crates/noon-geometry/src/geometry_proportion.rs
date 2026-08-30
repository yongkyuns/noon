use noon_core::{GeometryRef, Transform2D, Vec2, TAU};

use crate::{point_from_proportion, PathProportionError};

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
    validate_proportion(alpha)?;
    match geometry {
        GeometryRef::Circle { radius } => Ok(circle_point_from_proportion(*radius, alpha)),
        GeometryRef::Line { start, end } => Ok(*start + (*end - *start) * alpha),
        GeometryRef::VectorPath(path) => Ok(point_from_proportion(path, alpha)?),
        GeometryRef::Rectangle { .. } | GeometryRef::External(_) => {
            Err(GeometryProportionError::UnsupportedGeometry)
        }
    }
}

/// A world-space finite-difference tangent segment sampled from retained geometry.
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
    NonFiniteLength(f32),
    InvalidDelta(f32),
    DegenerateSample,
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
                "tangent sample delta must be finite and greater than zero, got {delta}"
            ),
            Self::DegenerateSample => {
                formatter.write_str("tangent sample points collapse to one world-space point")
            }
        }
    }
}

impl std::error::Error for TangentSegmentError {}

impl From<GeometryProportionError> for TangentSegmentError {
    fn from(value: GeometryProportionError) -> Self {
        Self::Geometry(value)
    }
}

/// Sample and normalize a finite-difference tangent in world space.
///
/// This mirrors the geometry kernel used by ManimCE v0.21 `TangentLine`: sample
/// `alpha - d_alpha` and `alpha + d_alpha` after clipping to `[0, 1]`, construct
/// the chord between those current transformed points, then scale that chord about
/// its center to `length`. Sampling before normalization is important for
/// non-uniformly transformed paths because their world-space tangent differs from
/// the local tangent transformed after normalization.
///
/// Negative finite lengths are intentionally supported and reverse the segment,
/// matching a negative scale factor. A zero length collapses both endpoints to the
/// sampled chord center.
pub fn tangent_segment_from_geometry_proportion(
    geometry: &GeometryRef,
    transform: Transform2D,
    alpha: f32,
    length: f32,
    d_alpha: f32,
) -> Result<TangentSegment, TangentSegmentError> {
    validate_proportion(alpha)?;
    if !length.is_finite() {
        return Err(TangentSegmentError::NonFiniteLength(length));
    }
    if !d_alpha.is_finite() || d_alpha <= 0.0 {
        return Err(TangentSegmentError::InvalidDelta(d_alpha));
    }

    let lower = (alpha - d_alpha).clamp(0.0, 1.0);
    let upper = (alpha + d_alpha).clamp(0.0, 1.0);
    let lower = transform.transform_point(point_from_geometry_proportion(geometry, lower)?);
    let upper = transform.transform_point(point_from_geometry_proportion(geometry, upper)?);
    let direction = (upper - lower)
        .normalized()
        .ok_or(TangentSegmentError::DegenerateSample)?;
    let center = (lower + upper) * 0.5;
    let half = direction * (length * 0.5);

    Ok(TangentSegment {
        start: center - half,
        end: center + half,
    })
}

fn validate_proportion(alpha: f32) -> Result<(), GeometryProportionError> {
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(PathProportionError::InvalidProportion(alpha).into());
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

    fn assert_point_loose(actual: Vec2, expected: Vec2) {
        assert!(
            (actual.x - expected.x).abs() <= 2.0e-4 && (actual.y - expected.y).abs() <= 2.0e-4,
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
    }

    #[test]
    fn tangent_segment_matches_manim_circle_finite_difference() {
        let segment = tangent_segment_from_geometry_proportion(
            &GeometryRef::circle(2.0),
            Transform2D::IDENTITY,
            0.4,
            4.0,
            1.0e-6,
        )
        .unwrap();

        assert_point_loose(segment.start, Vec2::new(-0.4482279, 2.8014423));
        assert_point_loose(segment.end, Vec2::new(-2.7901278, -0.44131965));
        assert!((segment.length() - 4.0).abs() <= 2.0e-4);
    }

    #[test]
    fn tangent_segment_normalizes_after_world_transform_and_clips_endpoint_samples() {
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
        assert_point_loose(transformed.start, Vec2::new(-1.058928, -0.50566184));
        assert_point_loose(transformed.end, Vec2::new(-3.4772415, -2.2809815));
        assert!((transformed.length() - 3.0).abs() <= 2.0e-4);

        let endpoint = tangent_segment_from_geometry_proportion(
            &GeometryRef::circle(2.0),
            Transform2D::IDENTITY,
            0.0,
            4.0,
            1.0e-6,
        )
        .unwrap();
        assert_point_loose(endpoint.start, Vec2::new(2.0000057, -1.9999934));
        assert_point_loose(endpoint.end, Vec2::new(1.9999942, 2.0000067));
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

        assert!(matches!(
            tangent_segment_from_geometry_proportion(
                &line,
                Transform2D::IDENTITY,
                0.5,
                1.0,
                0.0,
            ),
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
        assert_eq!(
            point_from_geometry_proportion(&GeometryRef::rectangle(1.0, 1.0), 0.5),
            Err(GeometryProportionError::UnsupportedGeometry)
        );
    }
}
