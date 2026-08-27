//! Shared Rust authoring semantics for Manim-compatible circular arcs.
//!
//! The authored representation stays on Noon's retained [`VectorPath`] pipeline.
//! Arc curvature is expressed with the same cubic Bézier construction used by
//! ManimCE's Cairo VMobject implementation instead of line-segment approximation.

use crate::legacy::{IntoSnapshot, Path};
use noon_core::{Color, ObjectSnapshot, Vec2, VectorPath, TAU};

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

macro_rules! define_arc_shape {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name(ObjectSnapshot);

        impl $name {
            pub fn color(mut self, color: Color) -> Self {
                self.0 = self.0.set_color(color);
                self
            }

            pub fn shift(mut self, offset: Vec2) -> Self {
                self.0 = self.0.shift(offset);
                self
            }

            pub fn move_to(mut self, point: Vec2) -> Self {
                self.0 = self.0.move_to(point);
                self
            }

            pub fn scale(mut self, factor: f32) -> Self {
                self.0 = self.0.scale_by(factor);
                self
            }

            pub fn scale_xy(mut self, factor: Vec2) -> Self {
                self.0 = self.0.scale_xy(factor);
                self
            }

            pub fn rotate(mut self, angle: f32) -> Self {
                self.0 = self.0.rotate_by(angle);
                self
            }

            pub fn set_fill(mut self, color: Option<Color>, opacity: Option<f32>) -> Self {
                self.0 = self.0.set_fill(color, opacity);
                self
            }

            pub fn set_stroke(mut self, color: Option<Color>, width: Option<f32>) -> Self {
                self.0 = self.0.set_stroke(color, width);
                self
            }

            pub fn set_opacity(mut self, opacity: f32) -> Self {
                self.0 = self.0.set_opacity(opacity);
                self
            }

            pub fn snapshot(&self) -> &ObjectSnapshot {
                &self.0
            }
        }

        impl IntoSnapshot for $name {
            fn into_snapshot(self) -> ObjectSnapshot {
                self.0
            }
        }
    };
}

define_arc_shape!(Arc);
define_arc_shape!(ArcBetweenPoints);

fn point_is_finite(point: Vec2) -> bool {
    point.x.is_finite() && point.y.is_finite()
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

fn circular_arc_path(
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

fn arc_snapshot(path: VectorPath) -> ObjectSnapshot {
    Path::new(path).into_snapshot()
}

impl Arc {
    /// Build ManimCE's default quarter-circle arc.
    pub fn new() -> Self {
        Self::with_options(1.0, 0.0, TAU / 4.0, 9, Vec2::ZERO)
            .expect("the built-in Arc default is valid")
    }

    /// Build a circular arc using ManimCE's anchor/handle construction.
    ///
    /// `num_components` matches Manim's Cairo `Arc`: it is the number of anchors,
    /// so the retained path contains `num_components - 1` cubic Bézier segments.
    pub fn with_options(
        radius: f32,
        start_angle: f32,
        angle: f32,
        num_components: usize,
        arc_center: Vec2,
    ) -> Result<Self, ArcAuthoringError> {
        Ok(Self(arc_snapshot(circular_arc_path(
            radius,
            start_angle,
            angle,
            num_components,
            arc_center,
        )?)))
    }
}

impl Default for Arc {
    fn default() -> Self {
        Self::new()
    }
}

impl ArcBetweenPoints {
    /// Build the default quarter-turn arc spanning `start` to `end`.
    pub fn new(start: Vec2, end: Vec2) -> Result<Self, ArcAuthoringError> {
        Self::with_options(start, end, TAU / 4.0, None, 9)
    }

    /// Build an arc spanning two endpoints with ManimCE-compatible angle/radius rules.
    ///
    /// When `radius` is supplied, Manim derives the subtended angle from the chord;
    /// a negative radius selects the opposite bend direction. Without an explicit
    /// radius, the authored `angle` is preserved and the unit arc is similarity-
    /// transformed so its endpoints land exactly on `start` and `end`.
    pub fn with_options(
        start: Vec2,
        end: Vec2,
        angle: f32,
        radius: Option<f32>,
        num_components: usize,
    ) -> Result<Self, ArcAuthoringError> {
        if !point_is_finite(start) {
            return Err(ArcAuthoringError::NonFinitePoint(start));
        }
        if !point_is_finite(end) {
            return Err(ArcAuthoringError::NonFinitePoint(end));
        }
        if !angle.is_finite() {
            return Err(ArcAuthoringError::NonFiniteAngle(angle));
        }
        if num_components < 2 {
            return Err(ArcAuthoringError::TooFewComponents(num_components));
        }

        let chord = end - start;
        let chord_length = chord.length();
        let (base_radius, resolved_angle) = match radius {
            Some(radius) => {
                if !radius.is_finite() {
                    return Err(ArcAuthoringError::NonFiniteRadius(radius));
                }
                let sign = if radius < 0.0 { -2.0 } else { 2.0 };
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
                (radius, (adjacent / radius).acos() * sign)
            }
            None => (1.0, angle),
        };

        if resolved_angle == 0.0 {
            let path = VectorPath::new().move_to(start).line_to(end);
            return Ok(Self(arc_snapshot(path)));
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
        let transform = |point: Vec2| start + (point - base_start).rotate(rotation) * scale;

        let segment_count = num_components - 1;
        let delta = resolved_angle / segment_count as f32;
        let handle_factor = (4.0 / 3.0) * (delta / 4.0).tan();
        let base_point_at = |theta: f32| {
            let (sin, cos) = theta.sin_cos();
            Vec2::new(base_radius * cos, base_radius * sin)
        };
        let base_tangent_at = |theta: f32| {
            let (sin, cos) = theta.sin_cos();
            Vec2::new(-base_radius * sin, base_radius * cos)
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

        Ok(Self(arc_snapshot(path)))
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, PathCommand, StrokeWidthMode, WHITE};

    use super::*;

    fn commands(snapshot: &ObjectSnapshot) -> &[PathCommand] {
        match &snapshot.geometry {
            GeometryRef::VectorPath(path) => path.commands(),
            other => panic!("expected vector path geometry, got {other:?}"),
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    fn endpoint(command: PathCommand) -> Vec2 {
        match command {
            PathCommand::MoveTo { to }
            | PathCommand::LineTo { to }
            | PathCommand::QuadraticTo { to, .. }
            | PathCommand::CubicTo { to, .. } => to,
            other => panic!("command has no endpoint: {other:?}"),
        }
    }

    #[test]
    fn default_arc_matches_manim_quarter_circle_and_component_count() {
        let arc = Arc::default();
        let commands = commands(arc.snapshot());
        assert_eq!(commands.len(), 9);
        assert_eq!(endpoint(commands[0]), Vec2::new(1.0, 0.0));
        let end = endpoint(*commands.last().expect("arc has commands"));
        assert_close(end.x, 0.0);
        assert_close(end.y, 1.0);
        assert_eq!(arc.snapshot().style.stroke, Some(WHITE));
        assert_eq!(
            arc.snapshot().style.stroke_width_mode,
            StrokeWidthMode::ScreenSpace
        );
    }

    #[test]
    fn arc_honors_radius_start_angle_center_and_signed_angle() {
        let arc = Arc::with_options(2.0, TAU / 4.0, -TAU / 4.0, 3, Vec2::new(3.0, -1.0))
            .expect("valid arc");
        let commands = commands(arc.snapshot());
        assert_eq!(commands.len(), 3);
        let start = endpoint(commands[0]);
        let end = endpoint(*commands.last().expect("arc has commands"));
        assert_close(start.x, 3.0);
        assert_close(start.y, 1.0);
        assert_close(end.x, 5.0);
        assert_close(end.y, -1.0);
    }

    #[test]
    fn arc_between_points_lands_exactly_on_requested_endpoints() {
        let start = Vec2::new(-2.0, -0.5);
        let end = Vec2::new(3.0, 1.25);
        let arc = ArcBetweenPoints::new(start, end).expect("valid endpoint arc");
        let commands = commands(arc.snapshot());
        assert_eq!(commands.len(), 9);
        assert_eq!(endpoint(commands[0]), start);
        let actual_end = endpoint(*commands.last().expect("arc has commands"));
        assert_close(actual_end.x, end.x);
        assert_close(actual_end.y, end.y);
    }

    #[test]
    fn zero_angle_arc_between_points_matches_manim_line_fallback() {
        let start = Vec2::new(-1.0, 2.0);
        let end = Vec2::new(4.0, -3.0);
        let arc = ArcBetweenPoints::with_options(start, end, 0.0, None, 9)
            .expect("zero-angle fallback is valid");
        let commands = commands(arc.snapshot());
        assert_eq!(commands.len(), 2);
        assert_eq!(endpoint(commands[0]), start);
        assert_eq!(endpoint(commands[1]), end);
    }

    #[test]
    fn explicit_radius_rejects_impossible_chord() {
        let error = ArcBetweenPoints::with_options(
            Vec2::new(-2.0, 0.0),
            Vec2::new(2.0, 0.0),
            TAU / 4.0,
            Some(1.0),
            9,
        )
        .expect_err("radius shorter than half chord must fail");
        assert!(matches!(error, ArcAuthoringError::RadiusTooSmall { .. }));
    }
}
