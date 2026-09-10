//! Cubic circular-arc construction shared by Manim-compatible retained geometry.
use crate::{AuthoringError, ManimGeometryOptions, Mobject};
use noon_core::{SemanticStore, Vec2, VectorPath};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Debug, PartialEq)]
pub enum ArcAuthoringError {
    TooFewComponents(usize),
    NonFiniteRadius(f32),
    NonFiniteAngle(f32),
    NonFiniteStartAngle(f32),
    NonFinitePoint(Vec2),
    RadiusTooSmall { radius: f32, half_distance: f32 },
    DegenerateChordAngle(f32),
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
            Self::RadiusTooSmall {
                radius,
                half_distance,
            } => write!(
                formatter,
                "ArcBetweenPoints radius {radius} is smaller than half the endpoint distance {half_distance}"
            ),
            Self::DegenerateChordAngle(value) => write!(
                formatter,
                "ArcBetweenPoints angle {value} has a zero-length source chord"
            ),
        }
    }
}

impl std::error::Error for ArcAuthoringError {}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ResolvedArcBetweenPoints {
    base_radius: f32,
    radius: f32,
    angle: f32,
    scale: f32,
    rotation: f32,
}

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

fn resolve_arc_between_points(
    start: Vec2,
    end: Vec2,
    angle: f32,
    radius: Option<f32>,
) -> Result<ResolvedArcBetweenPoints, ArcAuthoringError> {
    if !point_is_finite(start) {
        return Err(ArcAuthoringError::NonFinitePoint(start));
    }
    if !point_is_finite(end) {
        return Err(ArcAuthoringError::NonFinitePoint(end));
    }
    if !angle.is_finite() {
        return Err(ArcAuthoringError::NonFiniteAngle(angle));
    }

    let chord = end - start;
    let chord_length = chord.length();
    let radius_was_explicit = radius.is_some();
    let (base_radius, resolved_angle) = match radius {
        Some(radius) => {
            if !radius.is_finite() {
                return Err(ArcAuthoringError::NonFiniteRadius(radius));
            }
            let angle_sign = if radius < 0.0 { -2.0 } else { 2.0 };
            let radius = radius.abs();
            let half_distance = chord_length * 0.5;
            if radius < half_distance {
                return Err(ArcAuthoringError::RadiusTooSmall {
                    radius,
                    half_distance,
                });
            }
            if radius == 0.0 {
                return Err(ArcAuthoringError::DegenerateChordAngle(angle));
            }
            let adjacent = (radius * radius - half_distance * half_distance)
                .max(0.0)
                .sqrt();
            (radius, (adjacent / radius).acos() * angle_sign)
        }
        None => (1.0, angle),
    };

    if resolved_angle == 0.0 {
        return Ok(ResolvedArcBetweenPoints {
            base_radius,
            radius: if radius_was_explicit {
                base_radius
            } else {
                f32::INFINITY
            },
            angle: resolved_angle,
            scale: 1.0,
            rotation: 0.0,
        });
    }

    let base_start = Vec2::new(base_radius, 0.0);
    let (end_sin, end_cos) = resolved_angle.sin_cos();
    let base_end = Vec2::new(base_radius * end_cos, base_radius * end_sin);
    let base_chord = base_end - base_start;
    let base_chord_length = base_chord.length();
    if base_chord_length <= f32::EPSILON {
        return Err(ArcAuthoringError::DegenerateChordAngle(resolved_angle));
    }
    let scale = chord_length / base_chord_length;
    let rotation = chord.y.atan2(chord.x) - base_chord.y.atan2(base_chord.x);
    Ok(ResolvedArcBetweenPoints {
        base_radius,
        radius: if radius_was_explicit {
            base_radius
        } else {
            base_radius * scale
        },
        angle: resolved_angle,
        scale,
        rotation,
    })
}

fn arc_between_points_path(
    start: Vec2,
    end: Vec2,
    angle: f32,
    radius: Option<f32>,
    num_components: usize,
) -> Result<VectorPath, ArcAuthoringError> {
    if num_components < 2 {
        return Err(ArcAuthoringError::TooFewComponents(num_components));
    }
    let resolved = resolve_arc_between_points(start, end, angle, radius)?;
    if resolved.angle == 0.0 {
        return Ok(VectorPath::new().move_to(start).line_to(end));
    }

    let base_start = Vec2::new(resolved.base_radius, 0.0);
    let transform =
        |point: Vec2| start + (point - base_start).rotate(resolved.rotation) * resolved.scale;

    let segment_count = num_components - 1;
    let delta = resolved.angle / segment_count as f32;
    let handle_factor = (4.0 / 3.0) * (delta / 4.0).tan();
    let base_point_at = |theta: f32| {
        let (sin, cos) = theta.sin_cos();
        Vec2::new(resolved.base_radius * cos, resolved.base_radius * sin)
    };
    let base_tangent_at = |theta: f32| {
        let (sin, cos) = theta.sin_cos();
        Vec2::new(-resolved.base_radius * sin, resolved.base_radius * cos)
    };

    let mut path = VectorPath::new().move_to(start);
    for index in 0..segment_count {
        let theta0 = index as f32 * delta;
        let theta1 = theta0 + delta;
        let anchor0 = base_point_at(theta0);
        let anchor1 = base_point_at(theta1);
        let control1 = anchor0 + handle_factor * base_tangent_at(theta0);
        let control2 = anchor1 - handle_factor * base_tangent_at(theta1);
        path = path.cubic_to(transform(control1), transform(control2), transform(anchor1));
    }
    Ok(path)
}

impl ManimGeometryOptions {
    /// Build ManimCE-compatible retained circular-arc geometry.
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        radius: f64,
        start_angle: f64,
        angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, AuthoringError> {
        let path = circular_arc_path(
            authored_f32(radius, "arc radius")?,
            authored_f32(start_angle, "arc start angle")?,
            authored_f32(angle, "arc angle")?,
            num_components as usize,
            Vec2::new(
                authored_f32(center_x, "arc center x")?,
                authored_f32(center_y, "arc center y")?,
            ),
        )?;
        Self::path(path)
    }

    /// Resolve the observable Manim radius/angle metadata without frontend geometry math.
    #[allow(clippy::too_many_arguments)]
    pub fn arc_between_points_metadata(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        angle: f64,
        radius: Option<f64>,
    ) -> Result<(f64, f64), AuthoringError> {
        let resolved = resolve_arc_between_points(
            Vec2::new(
                authored_f32(start_x, "arc start x")?,
                authored_f32(start_y, "arc start y")?,
            ),
            Vec2::new(
                authored_f32(end_x, "arc end x")?,
                authored_f32(end_y, "arc end y")?,
            ),
            authored_f32(angle, "arc angle")?,
            radius
                .map(|value| authored_f32(value, "arc radius"))
                .transpose()?,
        )?;
        Ok((f64::from(resolved.radius), f64::from(resolved.angle)))
    }

    /// Build ManimCE-compatible retained geometry spanning two endpoints.
    #[allow(clippy::too_many_arguments)]
    pub fn arc_between_points(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        angle: f64,
        radius: Option<f64>,
        num_components: u32,
    ) -> Result<Self, AuthoringError> {
        let path = arc_between_points_path(
            Vec2::new(
                authored_f32(start_x, "arc start x")?,
                authored_f32(start_y, "arc start y")?,
            ),
            Vec2::new(
                authored_f32(end_x, "arc end x")?,
                authored_f32(end_y, "arc end y")?,
            ),
            authored_f32(angle, "arc angle")?,
            radius
                .map(|value| authored_f32(value, "arc radius"))
                .transpose()?,
            num_components as usize,
        )?;
        Self::path(path)
    }
}

impl Mobject {
    #[allow(clippy::too_many_arguments)]
    pub fn manim_arc(
        store: Rc<RefCell<SemanticStore>>,
        radius: f64,
        start_angle: f64,
        angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, AuthoringError> {
        Self::from_manim_geometry(
            store,
            ManimGeometryOptions::arc(
                radius,
                start_angle,
                angle,
                num_components,
                center_x,
                center_y,
            )?,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn manim_arc_between_points(
        store: Rc<RefCell<SemanticStore>>,
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        angle: f64,
        radius: Option<f64>,
        num_components: u32,
    ) -> Result<Self, AuthoringError> {
        Self::from_manim_geometry(
            store,
            ManimGeometryOptions::arc_between_points(
                start_x,
                start_y,
                end_x,
                end_y,
                angle,
                radius,
                num_components,
            )?,
        )
    }
}
