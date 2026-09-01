use noon_core::{GeometryRef, Vec2, TAU};

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
            (actual.x - expected.x).abs() <= 1.0e-5
                && (actual.y - expected.y).abs() <= 1.0e-5,
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
    fn invalid_proportion_and_non_path_geometry_are_rejected() {
        let circle = GeometryRef::circle(1.0);
        assert!(matches!(
            point_from_geometry_proportion(&circle, f32::NAN),
            Err(GeometryProportionError::Path(PathProportionError::InvalidProportion(_)))
        ));
        assert_eq!(
            point_from_geometry_proportion(&GeometryRef::rectangle(1.0, 1.0), 0.5),
            Err(GeometryProportionError::UnsupportedGeometry)
        );
    }
}
