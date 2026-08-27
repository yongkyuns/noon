//! Shared Rust authoring semantics for analytic circle-derived Manim shapes.
//!
//! `Dot` and `Ellipse` deliberately reuse Noon's existing analytic circle geometry.
//! Their size/placement differences live in retained style and affine transform state,
//! so frontends do not need to rebuild paths and renderers do not need new primitives.

use crate::legacy::{Circle, IntoSnapshot};
use noon_core::{Color, ObjectSnapshot, Vec2, WHITE};

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
}
