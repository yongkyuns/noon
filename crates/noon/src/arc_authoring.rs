//! Cubic circular-arc construction shared by annular semantic geometry.
use crate::AuthoringError;
use noon_core::{Vec2, VectorPath};

#[derive(Clone, Debug, PartialEq)]
pub enum ArcAuthoringError {
    TooFewComponents(usize),
    NonFiniteRadius(f32),
    NonFiniteAngle(f32),
    NonFiniteStartAngle(f32),
    NonFinitePoint(Vec2),
}

impl std::fmt::Display for ArcAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooFewComponents(value) => {
                write!(formatter, "arc requires at least 2 components, got {value}")
            }
            Self::NonFiniteRadius(value) => {
                write!(formatter, "arc radius must be finite, got {value}")
            }
            Self::NonFiniteAngle(value) => {
                write!(formatter, "arc angle must be finite, got {value}")
            }
            Self::NonFiniteStartAngle(value) => {
                write!(formatter, "arc start angle must be finite, got {value}")
            }
            Self::NonFinitePoint(value) => write!(
                formatter,
                "arc point must be finite, got ({}, {})",
                value.x, value.y
            ),
        }
    }
}

impl std::error::Error for ArcAuthoringError {}

fn point_is_finite(point: Vec2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

pub(crate) fn authored_f32(value: f64, label: &str) -> Result<f32, AuthoringError> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(AuthoringError::InvalidRenderNumber {
            name: label.to_owned(),
            value,
        });
    }
    Ok(value as f32)
}

fn validate_arc_inputs(
    radius: f32,
    start_angle: f32,
    angle: f32,
    num_components: usize,
    center: Vec2,
) -> Result<(), ArcAuthoringError> {
    if num_components < 2 {
        return Err(ArcAuthoringError::TooFewComponents(num_components));
    }
    if !radius.is_finite() {
        return Err(ArcAuthoringError::NonFiniteRadius(radius));
    }
    if !start_angle.is_finite() {
        return Err(ArcAuthoringError::NonFiniteStartAngle(start_angle));
    }
    if !angle.is_finite() {
        return Err(ArcAuthoringError::NonFiniteAngle(angle));
    }
    if !point_is_finite(center) {
        return Err(ArcAuthoringError::NonFinitePoint(center));
    }
    Ok(())
}

pub(crate) fn circular_arc_path(
    radius: f32,
    start_angle: f32,
    angle: f32,
    num_components: usize,
    center: Vec2,
) -> Result<VectorPath, ArcAuthoringError> {
    validate_arc_inputs(radius, start_angle, angle, num_components, center)?;

    let segment_count = num_components - 1;
    let delta = angle / segment_count as f32;
    let handle_factor = (4.0 / 3.0) * (delta / 4.0).tan();

    let point_at = |theta: f32| {
        let (sin, cos) = theta.sin_cos();
        Vec2::new(radius * cos, radius * sin) + center
    };
    let tangent_at = |theta: f32| {
        let (sin, cos) = theta.sin_cos();
        Vec2::new(-radius * sin, radius * cos)
    };

    let mut path = VectorPath::new().move_to(point_at(start_angle));
    for index in 0..segment_count {
        let theta0 = start_angle + index as f32 * delta;
        let theta1 = theta0 + delta;
        let anchor0 = point_at(theta0);
        let anchor1 = point_at(theta1);
        let control1 = anchor0 + handle_factor * tangent_at(theta0);
        let control2 = anchor1 - handle_factor * tangent_at(theta1);
        path = path.cubic_to(control1, control2, anchor1);
    }
    Ok(path)
}
