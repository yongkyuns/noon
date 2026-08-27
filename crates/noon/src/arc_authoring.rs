//! Shared Rust authoring semantics for Manim-compatible circular arcs.
//!
//! The authored representation stays on Noon's retained [`VectorPath`] pipeline.
//! Arc curvature is expressed with the same cubic Bézier construction used by
//! ManimCE's Cairo VMobject implementation instead of line-segment approximation.
//! Query methods inspect the retained path after transforms, matching Manim's
//! point-derived `get_start`, `get_end`, `get_arc_center`, and `stop_angle`
//! behavior while keeping constructor metadata available separately.

use crate::legacy::{IntoSnapshot, Path};
use noon_core::{Color, GeometryRef, ObjectSnapshot, PathCommand, Vec2, VectorPath, TAU};

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
        pub struct $name {
            snapshot: ObjectSnapshot,
            radius: f32,
            start_angle: f32,
            angle: f32,
            num_components: usize,
        }

        impl $name {
            fn from_parts(
                snapshot: ObjectSnapshot,
                radius: f32,
                start_angle: f32,
                angle: f32,
                num_components: usize,
            ) -> Self {
                Self {
                    snapshot,
                    radius,
                    start_angle,
                    angle,
                    num_components,
                }
            }

            pub fn color(mut self, color: Color) -> Self {
                self.snapshot = self.snapshot.set_color(color);
                self
            }

            pub fn shift(mut self, offset: Vec2) -> Self {
                self.snapshot = self.snapshot.shift(offset);
                self
            }

            pub fn move_to(mut self, point: Vec2) -> Self {
                self.snapshot = self.snapshot.move_to(point);
                self
            }

            pub fn scale(mut self, factor: f32) -> Self {
                self.snapshot = self.snapshot.scale_by(factor);
                self
            }

            pub fn scale_xy(mut self, factor: Vec2) -> Self {
                self.snapshot = self.snapshot.scale_xy(factor);
                self
            }

            pub fn rotate(mut self, angle: f32) -> Self {
                self.snapshot = self.snapshot.rotate_by(angle);
                self
            }

            pub fn set_fill(mut self, color: Option<Color>, opacity: Option<f32>) -> Self {
                self.snapshot = self.snapshot.set_fill(color, opacity);
                self
            }

            pub fn set_stroke(mut self, color: Option<Color>, width: Option<f32>) -> Self {
                self.snapshot = self.snapshot.set_stroke(color, width);
                self
            }

            pub fn set_opacity(mut self, opacity: f32) -> Self {
                self.snapshot = self.snapshot.set_opacity(opacity);
                self
            }

            pub fn snapshot(&self) -> &ObjectSnapshot {
                &self.snapshot
            }

            /// Constructor-time Manim `radius` metadata. Ordinary affine transforms
            /// alter points, not this authored attribute, matching Manim.
            pub fn radius(&self) -> f32 {
                self.radius
            }

            pub fn start_angle(&self) -> f32 {
                self.start_angle
            }

            pub fn angle(&self) -> f32 {
                self.angle
            }

            pub fn num_components(&self) -> usize {
                self.num_components
            }

            /// Return the transformed first path anchor, matching VMobject `get_start()`.
            pub fn get_start(&self) -> Vec2 {
                path_start(&self.snapshot).expect("Arc retains a non-empty VectorPath")
            }

            /// Return the transformed final path anchor, matching VMobject `get_end()`.
            pub fn get_end(&self) -> Vec2 {
                path_end(&self.snapshot).expect("Arc retains a non-empty VectorPath")
            }

            /// Match Manim `Arc.get_arc_center()` by intersecting normals derived from
            /// the transformed first cubic segment. A degenerate zero-radius arc returns
            /// its first anchor; a line/parallel-normal fallback returns world ORIGIN.
            pub fn get_arc_center(&self) -> Vec2 {
                arc_center_from_snapshot(&self.snapshot)
            }

            pub fn move_arc_center_to(mut self, point: Vec2) -> Self {
                let center = self.get_arc_center();
                self.snapshot = self.snapshot.shift(point - center);
                self
            }

            /// Match Manim `Arc.stop_angle()` in the 2D plane.
            pub fn stop_angle(&self) -> f32 {
                let delta = self.get_end() - self.get_arc_center();
                delta.y.atan2(delta.x).rem_euclid(TAU)
            }
        }

        impl IntoSnapshot for $name {
            fn into_snapshot(self) -> ObjectSnapshot {
                self.snapshot
            }
        }
    };
}

define_arc_shape!(Arc);
define_arc_shape!(ArcBetweenPoints);

fn point_is_finite(point: Vec2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn path_start(snapshot: &ObjectSnapshot) -> Option<Vec2> {
    let GeometryRef::VectorPath(path) = &snapshot.geometry else {
        return None;
    };
    match path.commands().first().copied()? {
        PathCommand::MoveTo { to } => Some(snapshot.transform.transform_point(to)),
        _ => None,
    }
}

fn path_end(snapshot: &ObjectSnapshot) -> Option<Vec2> {
    let GeometryRef::VectorPath(path) = &snapshot.geometry else {
        return None;
    };
    path.commands().iter().rev().find_map(|command| {
        let point = match *command {
            PathCommand::MoveTo { to }
            | PathCommand::LineTo { to }
            | PathCommand::QuadraticTo { to, .. }
            | PathCommand::CubicTo { to, .. } => to,
            PathCommand::Close => return None,
        };
        Some(snapshot.transform.transform_point(point))
    })
}

fn cross(left: Vec2, right: Vec2) -> f32 {
    left.x * right.y - left.y * right.x
}

fn line_intersection(
    first_point: Vec2,
    first_direction: Vec2,
    second_point: Vec2,
    second_direction: Vec2,
) -> Option<Vec2> {
    let denominator = cross(first_direction, second_direction);
    if denominator.abs() <= f32::EPSILON {
        return None;
    }
    let parameter = cross(second_point - first_point, second_direction) / denominator;
    Some(first_point + first_direction * parameter)
}

fn arc_center_from_snapshot(snapshot: &ObjectSnapshot) -> Vec2 {
    let GeometryRef::VectorPath(path) = &snapshot.geometry else {
        return Vec2::ZERO;
    };
    let commands = path.commands();
    let Some(PathCommand::MoveTo { to: first_anchor }) = commands.first().copied() else {
        return Vec2::ZERO;
    };
    let Some(PathCommand::CubicTo {
        control1: first_handle,
        control2: second_handle,
        to: second_anchor,
    }) = commands.get(1).copied()
    else {
        return Vec2::ZERO;
    };

    let first_anchor = snapshot.transform.transform_point(first_anchor);
    let first_handle = snapshot.transform.transform_point(first_handle);
    let second_handle = snapshot.transform.transform_point(second_handle);
    let second_anchor = snapshot.transform.transform_point(second_anchor);
    if first_anchor == second_anchor {
        return first_anchor;
    }

    let first_tangent = first_handle - first_anchor;
    let second_tangent = second_handle - second_anchor;
    let first_normal = Vec2::new(-first_tangent.y, first_tangent.x);
    let second_normal = Vec2::new(-second_tangent.y, second_tangent.x);
    line_intersection(
        first_anchor,
        first_normal,
        second_anchor,
        second_normal,
    )
    .unwrap_or(Vec2::ZERO)
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
    pub fn new() -> Self {
        Self::with_options(1.0, 0.0, TAU / 4.0, 9, Vec2::ZERO)
            .expect("the built-in Arc default is valid")
    }

    pub fn with_options(
        radius: f32,
        start_angle: f32,
        angle: f32,
        num_components: usize,
        arc_center: Vec2,
    ) -> Result<Self, ArcAuthoringError> {
        let path = circular_arc_path(radius, start_angle, angle, num_components, arc_center)?;
        Ok(Self::from_parts(
            arc_snapshot(path),
            radius,
            start_angle,
            angle,
            num_components,
        ))
    }
}

impl Default for Arc {
    fn default() -> Self {
        Self::new()
    }
}

impl ArcBetweenPoints {
    pub fn new(start: Vec2, end: Vec2) -> Result<Self, ArcAuthoringError> {
        Self::with_options(start, end, TAU / 4.0, None, 9)
    }

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
        let radius_was_explicit = radius.is_some();
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
            return Ok(Self::from_parts(
                arc_snapshot(path),
                base_radius,
                0.0,
                resolved_angle,
                num_components,
            ));
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

        let resolved_radius = if radius_was_explicit {
            base_radius
        } else {
            base_radius * scale
        };
        Ok(Self::from_parts(
            arc_snapshot(path),
            resolved_radius,
            0.0,
            resolved_angle,
            num_components,
        ))
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

    fn assert_vec_close(actual: Vec2, expected: Vec2) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
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
        assert_close(arc.radius(), 1.0);
        assert_close(arc.start_angle(), 0.0);
        assert_close(arc.angle(), TAU / 4.0);
        assert_eq!(arc.num_components(), 9);
        assert_vec_close(arc.get_start(), Vec2::new(1.0, 0.0));
        assert_vec_close(arc.get_end(), Vec2::new(0.0, 1.0));
        assert_vec_close(arc.get_arc_center(), Vec2::ZERO);
        assert_close(arc.stop_angle(), TAU / 4.0);
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
        assert_close(arc.radius(), 2.0);
        assert_close(arc.start_angle(), TAU / 4.0);
        assert_close(arc.angle(), -TAU / 4.0);
        assert_vec_close(arc.get_arc_center(), Vec2::new(3.0, -1.0));
        assert_close(arc.stop_angle(), 0.0);
    }

    #[test]
    fn transformed_queries_follow_retained_path_but_metadata_stays_authored() {
        let arc = Arc::with_options(2.0, 0.0, TAU / 4.0, 9, Vec2::new(1.0, 2.0))
            .expect("valid arc")
            .scale(1.5)
            .rotate(TAU / 8.0)
            .shift(Vec2::new(-3.0, 4.0));
        let expected_center = (Vec2::new(1.0, 2.0) * 1.5).rotate(TAU / 8.0)
            + Vec2::new(-3.0, 4.0);
        assert_vec_close(arc.get_arc_center(), expected_center);
        assert_close(arc.radius(), 2.0);
        assert_close(arc.start_angle(), 0.0);
        assert_close(arc.angle(), TAU / 4.0);
        assert_close(arc.stop_angle(), 3.0 * TAU / 8.0);
    }

    #[test]
    fn move_arc_center_to_uses_derived_current_center() {
        let arc = Arc::default()
            .shift(Vec2::new(2.0, -3.0))
            .move_arc_center_to(Vec2::new(-4.0, 5.0));
        assert_vec_close(arc.get_arc_center(), Vec2::new(-4.0, 5.0));
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
        assert_vec_close(arc.get_start(), start);
        assert_vec_close(arc.get_end(), end);
        assert_close(arc.angle(), TAU / 4.0);
        assert_close(arc.start_angle(), 0.0);
        let center = arc.get_arc_center();
        assert_close(arc.radius(), (start - center).length());
    }

    #[test]
    fn negative_explicit_radius_becomes_positive_metadata_and_negative_angle() {
        let arc = ArcBetweenPoints::with_options(
            Vec2::new(-1.0, 0.0),
            Vec2::new(1.0, 0.0),
            TAU / 4.0,
            Some(-2.0),
            9,
        )
        .expect("negative radius selects the opposite bend");
        assert_close(arc.radius(), 2.0);
        assert!(arc.angle() < 0.0);
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
        assert_vec_close(arc.get_start(), start);
        assert_vec_close(arc.get_end(), end);
        assert_vec_close(arc.get_arc_center(), Vec2::ZERO);
        assert_close(arc.radius(), 1.0);
        assert_close(arc.angle(), 0.0);
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
