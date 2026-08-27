//! Shared Rust authoring semantics for Manim-compatible rounded rectangles.
//!
//! `RoundedRectangle` stays on Noon's retained [`VectorPath`] pipeline. The
//! default path follows ManimCE's `Rectangle(...).round_corners(...)` traversal:
//! UR -> UL -> DL -> DR, with one cubic Bezier segment per rounded corner and
//! explicit straight edge segments between corners.

use crate::legacy::{IntoSnapshot, Path};
use noon_core::{Color, ObjectSnapshot, Vec2, VectorPath};

const DEFAULT_WIDTH: f32 = 4.0;
const DEFAULT_HEIGHT: f32 = 2.0;
pub const DEFAULT_ROUNDED_RECTANGLE_CORNER_RADIUS: f32 = 0.5;

#[derive(Clone, Debug, PartialEq)]
pub enum RoundedRectangleAuthoringError {
    InvalidWidth(f32),
    InvalidHeight(f32),
    NonFiniteCornerRadius(f32),
}

impl std::fmt::Display for RoundedRectangleAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWidth(value) => write!(
                formatter,
                "rounded rectangle width must be finite and positive, got {value}"
            ),
            Self::InvalidHeight(value) => write!(
                formatter,
                "rounded rectangle height must be finite and positive, got {value}"
            ),
            Self::NonFiniteCornerRadius(value) => {
                write!(
                    formatter,
                    "rounded rectangle corner radius must be finite, got {value}"
                )
            }
        }
    }
}

impl std::error::Error for RoundedRectangleAuthoringError {}

#[derive(Clone, Debug, PartialEq)]
pub struct RoundedRectangle(ObjectSnapshot);

impl RoundedRectangle {
    /// Build a rounded rectangle with one radius shared by all four corners.
    pub fn new(
        width: f32,
        height: f32,
        corner_radius: f32,
    ) -> Result<Self, RoundedRectangleAuthoringError> {
        Self::with_corner_radii(width, height, [corner_radius; 4])
    }

    /// Build a rounded rectangle with radii in Manim rectangle vertex order:
    /// `[UR, UL, DL, DR]`.
    ///
    /// Each radius is clamped to half the shorter adjacent edge, matching
    /// `Polygram.round_corners`. Negative radii retain the same cut points but
    /// reverse the corner sweep to create Manim's concave rounding behavior.
    pub fn with_corner_radii(
        width: f32,
        height: f32,
        corner_radii: [f32; 4],
    ) -> Result<Self, RoundedRectangleAuthoringError> {
        validate_dimensions(width, height)?;
        for radius in corner_radii {
            if !radius.is_finite() {
                return Err(RoundedRectangleAuthoringError::NonFiniteCornerRadius(
                    radius,
                ));
            }
        }

        let path = rounded_rectangle_path(width, height, corner_radii);
        Ok(Self(Path::new(path).into_snapshot()))
    }

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

impl Default for RoundedRectangle {
    fn default() -> Self {
        Self::new(
            DEFAULT_WIDTH,
            DEFAULT_HEIGHT,
            DEFAULT_ROUNDED_RECTANGLE_CORNER_RADIUS,
        )
        .expect("the built-in RoundedRectangle default is valid")
    }
}

impl IntoSnapshot for RoundedRectangle {
    fn into_snapshot(self) -> ObjectSnapshot {
        self.0
    }
}

fn validate_dimensions(width: f32, height: f32) -> Result<(), RoundedRectangleAuthoringError> {
    if !width.is_finite() || width <= 0.0 {
        return Err(RoundedRectangleAuthoringError::InvalidWidth(width));
    }
    if !height.is_finite() || height <= 0.0 {
        return Err(RoundedRectangleAuthoringError::InvalidHeight(height));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct CornerCurve {
    start: Vec2,
    control1: Vec2,
    control2: Vec2,
    end: Vec2,
    rounded: bool,
}

fn rounded_rectangle_path(width: f32, height: f32, corner_radii: [f32; 4]) -> VectorPath {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    // Manim Rectangle vertex order after stretching UR/UL/DL/DR to width/height.
    let vertices = [
        Vec2::new(half_width, half_height),
        Vec2::new(-half_width, half_height),
        Vec2::new(-half_width, -half_height),
        Vec2::new(half_width, -half_height),
    ];

    let corners: [CornerCurve; 4] = std::array::from_fn(|index| {
        let previous = vertices[(index + vertices.len() - 1) % vertices.len()];
        let vertex = vertices[index];
        let next = vertices[(index + 1) % vertices.len()];
        rounded_corner(previous, vertex, next, corner_radii[index])
    });

    let mut path = VectorPath::new().move_to(corners[0].start);
    for index in 0..corners.len() {
        let corner = corners[index];
        if corner.rounded {
            path = path.cubic_to(corner.control1, corner.control2, corner.end);
        } else if corner.end != corner.start {
            path = path.line_to(corner.end);
        }
        let next_start = corners[(index + 1) % corners.len()].start;
        if next_start != corner.end {
            path = path.line_to(next_start);
        }
    }
    path
}

fn rounded_corner(previous: Vec2, vertex: Vec2, next: Vec2, radius: f32) -> CornerCurve {
    let incoming = vertex - previous;
    let outgoing = next - vertex;
    let incoming_length = incoming.length();
    let outgoing_length = outgoing.length();
    let max_cutoff = incoming_length.min(outgoing_length) * 0.5;
    let cutoff = radius.abs().min(max_cutoff);

    if cutoff <= f32::EPSILON || radius == 0.0 {
        return CornerCurve {
            start: vertex,
            control1: vertex,
            control2: vertex,
            end: vertex,
            rounded: false,
        };
    }

    let incoming_unit = incoming / incoming_length;
    let outgoing_unit = outgoing / outgoing_length;
    let start = vertex - incoming_unit * cutoff;
    let end = vertex + outgoing_unit * cutoff;
    let sweep = std::f32::consts::FRAC_PI_2 * radius.signum();
    let (control1, control2) = cubic_controls_between(start, end, sweep);
    CornerCurve {
        start,
        control1,
        control2,
        end,
        rounded: true,
    }
}

/// Match Manim `ArcBetweenPoints(..., num_components=2)` for one corner.
fn cubic_controls_between(start: Vec2, end: Vec2, sweep: f32) -> (Vec2, Vec2) {
    let base_start = Vec2::new(1.0, 0.0);
    let (end_sin, end_cos) = sweep.sin_cos();
    let base_end = Vec2::new(end_cos, end_sin);
    let base_chord = base_end - base_start;
    let target_chord = end - start;
    let scale = target_chord.length() / base_chord.length();
    let rotation = target_chord.y.atan2(target_chord.x) - base_chord.y.atan2(base_chord.x);
    let transform = |point: Vec2| start + (point - base_start).rotate(rotation) * scale;

    let handle_factor = (4.0 / 3.0) * (sweep / 4.0).tan();
    let base_control1 = base_start + Vec2::new(0.0, 1.0) * handle_factor;
    let end_tangent = Vec2::new(-end_sin, end_cos);
    let base_control2 = base_end - end_tangent * handle_factor;
    (transform(base_control1), transform(base_control2))
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, PathCommand, StrokeWidthMode, WHITE};

    use super::*;

    fn commands(snapshot: &ObjectSnapshot) -> &[PathCommand] {
        match &snapshot.geometry {
            GeometryRef::VectorPath(path) => path.commands(),
            other => panic!("expected retained VectorPath geometry, got {other:?}"),
        }
    }

    fn endpoint(command: PathCommand) -> Vec2 {
        match command {
            PathCommand::MoveTo { to }
            | PathCommand::LineTo { to }
            | PathCommand::QuadraticTo { to, .. }
            | PathCommand::CubicTo { to, .. } => to,
            PathCommand::Close => panic!("rounded rectangle uses an explicit closing edge"),
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn default_matches_manim_dimensions_style_and_path_order() {
        let rectangle = RoundedRectangle::default();
        assert_close(rectangle.snapshot().width(), 4.0);
        assert_close(rectangle.snapshot().height(), 2.0);
        assert_eq!(rectangle.snapshot().style.stroke, Some(WHITE));
        assert_eq!(
            rectangle.snapshot().style.fill.map(|color| color.alpha),
            Some(0.0)
        );
        assert_eq!(
            rectangle.snapshot().style.stroke_width_mode,
            StrokeWidthMode::ScreenSpace
        );

        let commands = commands(rectangle.snapshot());
        // Move + four one-cubic corners + four explicit straight edges.
        assert_eq!(commands.len(), 9);
        assert_eq!(endpoint(commands[0]), Vec2::new(2.0, 0.5));
        assert!(matches!(commands[1], PathCommand::CubicTo { .. }));
        assert_eq!(endpoint(commands[1]), Vec2::new(1.5, 1.0));
        assert_eq!(endpoint(commands[2]), Vec2::new(-1.5, 1.0));
        assert!(matches!(commands[3], PathCommand::CubicTo { .. }));
    }

    #[test]
    fn oversized_radius_clamps_to_half_the_shorter_edge() {
        let rectangle = RoundedRectangle::new(4.0, 2.0, 10.0).expect("valid rectangle");
        let commands = commands(rectangle.snapshot());
        assert_eq!(endpoint(commands[0]), Vec2::new(2.0, 0.0));
        assert_eq!(endpoint(commands[1]), Vec2::new(1.0, 1.0));
        assert_eq!(endpoint(commands[2]), Vec2::new(-1.0, 1.0));
    }

    #[test]
    fn negative_radius_preserves_cut_points_and_reverses_curve_controls() {
        let positive = RoundedRectangle::new(4.0, 2.0, 0.5).expect("valid rectangle");
        let negative = RoundedRectangle::new(4.0, 2.0, -0.5).expect("valid rectangle");
        let positive_commands = commands(positive.snapshot());
        let negative_commands = commands(negative.snapshot());
        assert_eq!(
            endpoint(positive_commands[0]),
            endpoint(negative_commands[0])
        );
        assert_eq!(
            endpoint(positive_commands[1]),
            endpoint(negative_commands[1])
        );

        match (positive_commands[1], negative_commands[1]) {
            (
                PathCommand::CubicTo {
                    control1: positive_control,
                    ..
                },
                PathCommand::CubicTo {
                    control1: negative_control,
                    ..
                },
            ) => assert_ne!(positive_control, negative_control),
            other => panic!("expected cubic corner commands, got {other:?}"),
        }
    }

    #[test]
    fn zero_radius_reduces_to_explicit_rectangle_edges() {
        let rectangle = RoundedRectangle::new(4.0, 2.0, 0.0).expect("valid rectangle");
        let commands = commands(rectangle.snapshot());
        assert_eq!(commands.len(), 5);
        assert_eq!(endpoint(commands[0]), Vec2::new(2.0, 1.0));
        assert_eq!(endpoint(commands[1]), Vec2::new(-2.0, 1.0));
        assert_eq!(endpoint(commands[2]), Vec2::new(-2.0, -1.0));
        assert_eq!(endpoint(commands[3]), Vec2::new(2.0, -1.0));
        assert_eq!(endpoint(commands[4]), Vec2::new(2.0, 1.0));
    }

    #[test]
    fn per_corner_radii_follow_ur_ul_dl_dr_order() {
        let rectangle = RoundedRectangle::with_corner_radii(4.0, 2.0, [0.25, 0.5, 0.75, 1.0])
            .expect("valid rectangle");
        let commands = commands(rectangle.snapshot());
        assert_eq!(endpoint(commands[0]), Vec2::new(2.0, 0.75));
        assert_eq!(endpoint(commands[1]), Vec2::new(1.75, 1.0));
        assert_eq!(endpoint(commands[2]), Vec2::new(-1.5, 1.0));
    }
}
