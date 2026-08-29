//! Shared Rust authoring semantics for analytic circle-derived Manim shapes.
//!
//! `Dot` and `Ellipse` deliberately reuse Noon's existing analytic circle geometry.
//! Their size/placement differences live in retained style and affine transform state,
//! so frontends do not need to rebuild paths and renderers do not need new primitives.

use crate::legacy::{Circle, IntoSnapshot};
use noon_core::{Color, GeometryRef, ObjectSnapshot, Vec2, WHITE};

pub const DEFAULT_DOT_RADIUS: f32 = 0.08;

macro_rules! define_analytic_shape {
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

define_analytic_shape!(Dot);
define_analytic_shape!(Ellipse);

impl Dot {
    /// Build a Manim-compatible filled dot centered at `point`.
    pub fn new(point: Vec2, radius: f32) -> Self {
        let snapshot = Circle::new(radius)
            .color(WHITE)
            .set_fill(Some(WHITE), Some(1.0))
            .set_stroke(Some(WHITE), Some(0.0))
            .move_to(point)
            .into_snapshot();
        Self(snapshot)
    }
}

impl Default for Dot {
    fn default() -> Self {
        Self::new(Vec2::ZERO, DEFAULT_DOT_RADIUS)
    }
}

/// Return ManimCE's observable VMobject layout hull for an Ellipse snapshot.
///
/// Noon deliberately renders Ellipse as an analytic affine circle. ManimCE's Circle,
/// however, stores eight cubic Bézier segments and its generic layout queries inspect
/// the full point/control-point array rather than the true analytic extrema. For a
/// rotated non-uniform ellipse that control hull is slightly larger. This query keeps
/// that observable layout behavior in shared Rust without changing renderer geometry.
pub fn manim_ellipse_layout_bounds(snapshot: &ObjectSnapshot) -> Option<(Vec2, Vec2)> {
    let GeometryRef::Circle { radius } = &snapshot.geometry else {
        return None;
    };

    let handle_factor = (4.0 / 3.0) * (std::f32::consts::PI / 16.0).tan();
    let mut minimum = Vec2::new(f32::INFINITY, f32::INFINITY);
    let mut maximum = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);

    for index in 0..8 {
        let start_angle = index as f32 * std::f32::consts::PI / 4.0;
        let end_angle = (index + 1) as f32 * std::f32::consts::PI / 4.0;
        let (start_sin, start_cos) = start_angle.sin_cos();
        let (end_sin, end_cos) = end_angle.sin_cos();
        let start = Vec2::new(start_cos, start_sin);
        let end = Vec2::new(end_cos, end_sin);
        let start_tangent = Vec2::new(-start_sin, start_cos);
        let end_tangent = Vec2::new(-end_sin, end_cos);
        let control1 = start + start_tangent * handle_factor;
        let control2 = end - end_tangent * handle_factor;

        for point in [start, control1, control2, end] {
            let world = snapshot.transform.transform_point(point * *radius);
            minimum.x = minimum.x.min(world.x);
            minimum.y = minimum.y.min(world.y);
            maximum.x = maximum.x.max(world.x);
            maximum.y = maximum.y.max(world.y);
        }
    }

    Some((minimum, maximum))
}

impl Ellipse {
    /// Build an ellipse by affinely stretching the retained unit circle.
    ///
    /// This mirrors Manim's `Ellipse`, which constructs a `Circle` and then stretches
    /// it to the requested width and height, while retaining Noon's analytic circle
    /// representation instead of eagerly lowering to a vector path.
    pub fn new(width: f32, height: f32) -> Self {
        let snapshot = Circle::new(1.0)
            .scale_xy(Vec2::new(width * 0.5, height * 0.5))
            .into_snapshot();
        Self(snapshot)
    }

    /// Return the Manim-visible VMobject control hull after retained transforms.
    pub fn manim_layout_bounds(&self) -> (Vec2, Vec2) {
        manim_ellipse_layout_bounds(&self.0).expect("Ellipse retains analytic circle geometry")
    }
}

impl Default for Ellipse {
    fn default() -> Self {
        Self::new(2.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, StrokeWidthMode, RED};

    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    fn assert_close_with_tolerance(actual: f32, expected: f32, tolerance: f32) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual} within {tolerance}"
        );
    }

    fn assert_vec_close(actual: Vec2, expected: Vec2) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
    }

    #[test]
    fn dot_is_a_filled_analytic_circle_with_manim_defaults() {
        let dot = Dot::default();
        match &dot.snapshot().geometry {
            GeometryRef::Circle { radius } => assert_close(*radius, DEFAULT_DOT_RADIUS),
            other => panic!("expected analytic circle geometry, got {other:?}"),
        }
        assert_eq!(dot.snapshot().transform.translation, Vec2::ZERO);
        assert_eq!(dot.snapshot().style.fill, Some(WHITE));
        assert_eq!(dot.snapshot().style.stroke, Some(WHITE));
        assert_close(dot.snapshot().style.stroke_width, 0.0);
        assert_eq!(
            dot.snapshot().style.stroke_width_mode,
            StrokeWidthMode::ScreenSpace
        );
    }

    #[test]
    fn dot_places_circle_center_at_requested_point() {
        let point = Vec2::new(-2.5, 1.25);
        let dot = Dot::new(point, 0.2);
        assert_eq!(dot.snapshot().center(), point);
        assert_close(dot.snapshot().width(), 0.4);
        assert_close(dot.snapshot().height(), 0.4);
    }

    #[test]
    fn ellipse_is_an_affinely_stretched_analytic_circle() {
        let ellipse = Ellipse::new(4.0, 1.5);
        match &ellipse.snapshot().geometry {
            GeometryRef::Circle { radius } => assert_close(*radius, 1.0),
            other => panic!("expected analytic circle geometry, got {other:?}"),
        }
        assert_eq!(ellipse.snapshot().transform.scale, Vec2::new(2.0, 0.75));
        assert_close(ellipse.snapshot().width(), 4.0);
        assert_close(ellipse.snapshot().height(), 1.5);
    }

    #[test]
    fn ellipse_preserves_circle_vmobject_style_and_manim_dimensions() {
        let ellipse = Ellipse::default();
        assert_close(ellipse.snapshot().width(), 2.0);
        assert_close(ellipse.snapshot().height(), 1.0);
        assert_eq!(ellipse.snapshot().style.stroke, Some(RED));
        assert_eq!(
            ellipse.snapshot().style.fill.map(|color| color.alpha),
            Some(0.0)
        );
        assert_eq!(
            ellipse.snapshot().style.stroke_width_mode,
            StrokeWidthMode::ScreenSpace
        );
    }

    #[test]
    fn ellipse_manim_layout_bounds_match_axis_aligned_dimensions() {
        let ellipse = Ellipse::new(4.0, 1.5).shift(Vec2::new(2.0, -1.0));
        let (minimum, maximum) = ellipse.manim_layout_bounds();
        assert_vec_close(minimum, Vec2::new(0.0, -1.75));
        assert_vec_close(maximum, Vec2::new(4.0, -0.25));
    }

    #[test]
    fn rotated_nonuniform_ellipse_uses_manim_control_point_hull() {
        let ellipse = Ellipse::new(4.0, 1.5)
            .rotate(std::f32::consts::PI / 6.0)
            .shift(Vec2::new(2.0, -1.0));
        let (minimum, maximum) = ellipse.manim_layout_bounds();

        assert_close_with_tolerance(minimum.x, 0.16849303, 1e-5);
        assert_close_with_tolerance(minimum.y, -2.232114, 1e-5);
        assert_close_with_tolerance(maximum.x, 3.8315067, 1e-5);
        assert_close_with_tolerance(maximum.y, 0.23211408, 1e-5);
        assert!(maximum.x - minimum.x < ellipse.snapshot().width());
        assert!(maximum.y - minimum.y < ellipse.snapshot().height());
    }

    #[test]
    fn ellipse_layout_query_rejects_non_circle_snapshots() {
        let mut snapshot = Ellipse::default().into_snapshot();
        snapshot.geometry = GeometryRef::rectangle(2.0, 1.0);
        assert_eq!(manim_ellipse_layout_bounds(&snapshot), None);
    }
}
