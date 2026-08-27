//! Shared Rust authoring semantics for Manim-compatible line-based shape matchers.
//!
//! `Underline` retains Noon's analytic `GeometryRef::Line` representation.
//! `Cross` uses one retained [`VectorPath`] with two line subpaths because the
//! current deterministic authoring facade accepts one semantic snapshot at a
//! time. This preserves Manim's geometry, bounds, stretch-to-target behavior,
//! stroke style, and animation identity without adding frontend-owned grouping.

use crate::legacy::{IntoSnapshot, Line, Path};
use noon_core::{Color, ObjectSnapshot, Vec2, VectorPath, RED, SMALL_BUFF};

pub const DEFAULT_CROSS_STROKE_WIDTH: f32 = 6.0;
pub const DEFAULT_CROSS_SCALE_FACTOR: f32 = 1.0;
pub const DEFAULT_UNDERLINE_BUFF: f32 = SMALL_BUFF;

#[derive(Clone, Debug, PartialEq)]
pub enum LineMatcherAuthoringError {
    TargetHasNoBounds,
    NonFiniteStrokeWidth(f32),
    NonFiniteScaleFactor(f32),
    NonFiniteBuff(f32),
}

impl std::fmt::Display for LineMatcherAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetHasNoBounds => {
                write!(formatter, "shape matcher target has no finite bounds")
            }
            Self::NonFiniteStrokeWidth(value) => {
                write!(formatter, "cross stroke width must be finite, got {value}")
            }
            Self::NonFiniteScaleFactor(value) => {
                write!(formatter, "cross scale factor must be finite, got {value}")
            }
            Self::NonFiniteBuff(value) => {
                write!(formatter, "underline buffer must be finite, got {value}")
            }
        }
    }
}

impl std::error::Error for LineMatcherAuthoringError {}

macro_rules! define_line_matcher_shape {
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

define_line_matcher_shape!(Cross);
define_line_matcher_shape!(Underline);

impl Cross {
    /// Construct Manim's default two-line cross, centered at the origin.
    pub fn new() -> Self {
        Self::with_options(
            None,
            RED,
            DEFAULT_CROSS_STROKE_WIDTH,
            DEFAULT_CROSS_SCALE_FACTOR,
        )
        .expect("the built-in Cross default is valid")
    }

    /// Construct a cross stretched to a target's axis-aligned world bounds.
    pub fn with_target(target: &ObjectSnapshot) -> Result<Self, LineMatcherAuthoringError> {
        Self::with_options(
            Some(target),
            RED,
            DEFAULT_CROSS_STROKE_WIDTH,
            DEFAULT_CROSS_SCALE_FACTOR,
        )
    }

    /// Match Manim `Cross(mobject, stroke_color, stroke_width, scale_factor)`.
    pub fn with_options(
        target: Option<&ObjectSnapshot>,
        stroke_color: Color,
        stroke_width: f32,
        scale_factor: f32,
    ) -> Result<Self, LineMatcherAuthoringError> {
        if !stroke_width.is_finite() {
            return Err(LineMatcherAuthoringError::NonFiniteStrokeWidth(
                stroke_width,
            ));
        }
        if !scale_factor.is_finite() {
            return Err(LineMatcherAuthoringError::NonFiniteScaleFactor(
                scale_factor,
            ));
        }

        let (center, half_width, half_height) = if let Some(target) = target {
            let bounds = target
                .world_bounds()
                .ok_or(LineMatcherAuthoringError::TargetHasNoBounds)?;
            (
                bounds.center(),
                bounds.width() * 0.5 * scale_factor,
                bounds.height() * 0.5 * scale_factor,
            )
        } else {
            (Vec2::ZERO, scale_factor, scale_factor)
        };

        let upper_left = center + Vec2::new(-half_width, half_height);
        let upper_right = center + Vec2::new(half_width, half_height);
        let lower_left = center + Vec2::new(-half_width, -half_height);
        let lower_right = center + Vec2::new(half_width, -half_height);
        let path = VectorPath::new()
            .move_to(upper_left)
            .line_to(lower_right)
            .move_to(upper_right)
            .line_to(lower_left);
        let snapshot = Path::new(path)
            .set_stroke(Some(stroke_color), Some(stroke_width))
            .into_snapshot();
        Ok(Self(snapshot))
    }
}

impl Default for Cross {
    fn default() -> Self {
        Self::new()
    }
}

impl Underline {
    pub fn new(target: &ObjectSnapshot) -> Result<Self, LineMatcherAuthoringError> {
        Self::with_buff(target, DEFAULT_UNDERLINE_BUFF)
    }

    /// Match `Line(LEFT, RIGHT).match_width(target).next_to(target, DOWN, buff)`.
    pub fn with_buff(
        target: &ObjectSnapshot,
        buff: f32,
    ) -> Result<Self, LineMatcherAuthoringError> {
        if !buff.is_finite() {
            return Err(LineMatcherAuthoringError::NonFiniteBuff(buff));
        }
        let bounds = target
            .world_bounds()
            .ok_or(LineMatcherAuthoringError::TargetHasNoBounds)?;
        let center_x = bounds.center().x;
        let y = bounds.min.y - buff;
        let half_width = bounds.width() * 0.5;
        let start = Vec2::new(center_x - half_width, y);
        let end = Vec2::new(center_x + half_width, y);
        Ok(Self(Line::new(start, end).into_snapshot()))
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, PathCommand, StrokeWidthMode, WHITE};

    use crate::legacy::Rectangle;

    use super::*;

    fn commands(snapshot: &ObjectSnapshot) -> &[PathCommand] {
        match &snapshot.geometry {
            GeometryRef::VectorPath(path) => path.commands(),
            other => panic!("expected retained VectorPath geometry, got {other:?}"),
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn default_cross_matches_manim_geometry_and_style() {
        let cross = Cross::new();
        let commands = commands(cross.snapshot());
        assert_eq!(commands.len(), 4);
        assert_eq!(
            commands[0],
            PathCommand::MoveTo {
                to: Vec2::new(-1.0, 1.0),
            }
        );
        assert_eq!(
            commands[1],
            PathCommand::LineTo {
                to: Vec2::new(1.0, -1.0),
            }
        );
        assert_eq!(
            commands[2],
            PathCommand::MoveTo {
                to: Vec2::new(1.0, 1.0),
            }
        );
        assert_eq!(
            commands[3],
            PathCommand::LineTo {
                to: Vec2::new(-1.0, -1.0),
            }
        );
        assert_eq!(cross.snapshot().style.stroke, Some(RED));
        assert_close(cross.snapshot().style.stroke_width, 6.0);
        assert_eq!(
            cross.snapshot().style.stroke_width_mode,
            StrokeWidthMode::ScreenSpace
        );
        assert_eq!(
            cross.snapshot().style.fill.map(|color| color.alpha),
            Some(0.0)
        );
    }

    #[test]
    fn cross_stretches_to_target_bounds_before_scaling() {
        let target = Rectangle::new(4.0, 2.0)
            .shift(Vec2::new(1.0, 2.0))
            .into_snapshot();
        let cross = Cross::with_options(Some(&target), RED, 6.0, 1.5).expect("valid cross");
        let bounds = cross.snapshot().world_bounds().expect("cross has bounds");
        assert_close(bounds.width(), 6.0);
        assert_close(bounds.height(), 3.0);
        assert_eq!(bounds.center(), Vec2::new(1.0, 2.0));
    }

    #[test]
    fn zero_cross_scale_collapses_at_target_center() {
        let target = Rectangle::new(4.0, 2.0)
            .shift(Vec2::new(-2.0, 3.0))
            .into_snapshot();
        let cross = Cross::with_options(Some(&target), RED, 6.0, 0.0).expect("valid cross");
        let bounds = cross.snapshot().world_bounds().expect("cross has bounds");
        assert_eq!(bounds.center(), Vec2::new(-2.0, 3.0));
        assert_close(bounds.width(), 0.0);
        assert_close(bounds.height(), 0.0);
    }

    #[test]
    fn underline_matches_target_width_and_sits_below_by_buff() {
        let target = Rectangle::new(4.0, 2.0)
            .shift(Vec2::new(1.0, 2.0))
            .into_snapshot();
        let underline = Underline::with_buff(&target, 0.25).expect("valid underline");
        match &underline.snapshot().geometry {
            GeometryRef::Line { start, end } => {
                assert_eq!(*start, Vec2::new(-1.0, 0.75));
                assert_eq!(*end, Vec2::new(3.0, 0.75));
            }
            other => panic!("expected retained Line geometry, got {other:?}"),
        }
        assert_eq!(underline.snapshot().style.stroke, Some(WHITE));
        assert_eq!(
            underline.snapshot().style.fill.map(|color| color.alpha),
            Some(0.0)
        );
    }

    #[test]
    fn default_underline_buffer_is_small_buff() {
        let target = Rectangle::new(2.0, 2.0).into_snapshot();
        let underline = Underline::new(&target).expect("valid underline");
        let bounds = underline
            .snapshot()
            .world_bounds()
            .expect("underline has bounds");
        assert_close(bounds.max.y, -1.0 - SMALL_BUFF);
        assert_close(bounds.width(), 2.0);
    }

    #[test]
    fn rejects_non_finite_options() {
        assert!(matches!(
            Cross::with_options(None, RED, f32::NAN, 1.0),
            Err(LineMatcherAuthoringError::NonFiniteStrokeWidth(value)) if value.is_nan()
        ));
        assert!(matches!(
            Cross::with_options(None, RED, 6.0, f32::INFINITY),
            Err(LineMatcherAuthoringError::NonFiniteScaleFactor(value)) if value.is_infinite()
        ));
        let target = Rectangle::new(2.0, 2.0).into_snapshot();
        assert!(matches!(
            Underline::with_buff(&target, f32::NEG_INFINITY),
            Err(LineMatcherAuthoringError::NonFiniteBuff(value)) if value.is_infinite() && value.is_sign_negative()
        ));
    }
}
