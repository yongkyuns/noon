//! Shared Rust authoring semantics for Manim-compatible shape matchers.
//!
//! `SurroundingRectangle` and `BackgroundRectangle` derive exclusively from the
//! retained world bounds of their target snapshots and lower through
//! [`crate::RoundedRectangle`]. Frontends do not own grouping/bounds semantics,
//! and renderers do not need matcher-specific primitives.

use crate::legacy::IntoSnapshot;
use crate::{RoundedRectangle, RoundedRectangleAuthoringError};
use noon_core::{Color, ObjectSnapshot, Rect, Vec2, BLACK, SMALL_BUFF};

/// ManimCE's `PURE_YELLOW` default for `SurroundingRectangle`.
pub const SURROUNDING_RECTANGLE_DEFAULT_COLOR: Color = Color::from_hex(0xFFFF00);
pub const BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY: f32 = 0.75;

#[derive(Clone, Debug, PartialEq)]
pub enum ShapeMatcherAuthoringError {
    NoBoundedTargets,
    NonFiniteBuffer(Vec2),
    RoundedRectangle(RoundedRectangleAuthoringError),
}

impl std::fmt::Display for ShapeMatcherAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoBoundedTargets => {
                formatter.write_str("shape matcher requires at least one target with world bounds")
            }
            Self::NonFiniteBuffer(value) => write!(
                formatter,
                "shape matcher buffer must be finite, got ({}, {})",
                value.x, value.y
            ),
            Self::RoundedRectangle(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ShapeMatcherAuthoringError {}

impl From<RoundedRectangleAuthoringError> for ShapeMatcherAuthoringError {
    fn from(value: RoundedRectangleAuthoringError) -> Self {
        Self::RoundedRectangle(value)
    }
}

macro_rules! define_matcher_shape {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name(ObjectSnapshot);

        impl $name {
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

define_matcher_shape!(SurroundingRectangle);
define_matcher_shape!(BackgroundRectangle);

fn group_world_bounds<'a>(targets: impl IntoIterator<Item = &'a ObjectSnapshot>) -> Option<Rect> {
    let mut bounds: Option<Rect> = None;
    for snapshot in targets {
        let Some(snapshot_bounds) = snapshot.world_bounds() else {
            continue;
        };
        match &mut bounds {
            Some(bounds) => {
                bounds.include(snapshot_bounds.min);
                bounds.include(snapshot_bounds.max);
            }
            None => bounds = Some(snapshot_bounds),
        }
    }
    bounds
}

fn validate_buffer(buff: Vec2) -> Result<(), ShapeMatcherAuthoringError> {
    if !buff.x.is_finite() || !buff.y.is_finite() {
        return Err(ShapeMatcherAuthoringError::NonFiniteBuffer(buff));
    }
    Ok(())
}

fn surrounding_snapshot<'a>(
    targets: impl IntoIterator<Item = &'a ObjectSnapshot>,
    buff: Vec2,
    corner_radius: f32,
    color: Color,
) -> Result<ObjectSnapshot, ShapeMatcherAuthoringError> {
    validate_buffer(buff)?;
    let bounds = group_world_bounds(targets).ok_or(ShapeMatcherAuthoringError::NoBoundedTargets)?;
    let width = bounds.width() + 2.0 * buff.x;
    let height = bounds.height() + 2.0 * buff.y;

    // RoundedRectangle's VMobject defaults already carry Manim's screen-space
    // stroke policy. Explicit fill opacity zero is important because changing the
    // matcher color must not make the otherwise transparent fill opaque.
    let snapshot = RoundedRectangle::new(width, height, corner_radius)?
        .set_fill(Some(color), Some(0.0))
        .set_stroke(Some(color), None)
        .move_to(bounds.center())
        .into_snapshot();
    Ok(snapshot)
}

impl SurroundingRectangle {
    pub fn new(target: &ObjectSnapshot) -> Result<Self, ShapeMatcherAuthoringError> {
        Self::around(
            [target],
            Vec2::new(SMALL_BUFF, SMALL_BUFF),
            0.0,
            SURROUNDING_RECTANGLE_DEFAULT_COLOR,
        )
    }

    pub fn around<'a>(
        targets: impl IntoIterator<Item = &'a ObjectSnapshot>,
        buff: Vec2,
        corner_radius: f32,
        color: Color,
    ) -> Result<Self, ShapeMatcherAuthoringError> {
        Ok(Self(surrounding_snapshot(
            targets,
            buff,
            corner_radius,
            color,
        )?))
    }
}

impl BackgroundRectangle {
    pub fn new(target: &ObjectSnapshot) -> Result<Self, ShapeMatcherAuthoringError> {
        Self::around(
            [target],
            Vec2::ZERO,
            0.0,
            BLACK,
            BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY,
        )
    }

    pub fn around<'a>(
        targets: impl IntoIterator<Item = &'a ObjectSnapshot>,
        buff: Vec2,
        corner_radius: f32,
        color: Color,
        fill_opacity: f32,
    ) -> Result<Self, ShapeMatcherAuthoringError> {
        let mut snapshot = surrounding_snapshot(targets, buff, corner_radius, color)?;
        snapshot = snapshot.set_fill(Some(color), Some(fill_opacity));

        // Manim defaults BackgroundRectangle to `stroke_width=0` and
        // `stroke_opacity=0`. Keep the color identity but encode the invisible
        // stroke through both zero width and transparent alpha.
        let mut transparent_stroke = color;
        transparent_stroke.alpha = 0.0;
        snapshot = snapshot.set_stroke(Some(transparent_stroke), Some(0.0));
        Ok(Self(snapshot))
    }
}

#[cfg(test)]
mod tests {
    use crate::legacy::{Circle, IntoSnapshot, Rectangle};
    use noon_core::{GeometryRef, StrokeWidthMode, RED, WHITE};

    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn surrounding_rectangle_adds_default_buffer_and_centers_on_target() {
        let target = Rectangle::new(4.0, 2.0)
            .shift(Vec2::new(3.0, -1.0))
            .into_snapshot();
        let matcher = SurroundingRectangle::new(&target).expect("bounded target");
        let center = matcher.snapshot().center();
        let target_center = target.center();
        assert_close(center.x, target_center.x);
        assert_close(center.y, target_center.y);
        assert_close(matcher.snapshot().width(), 4.0 + 2.0 * SMALL_BUFF);
        assert_close(matcher.snapshot().height(), 2.0 + 2.0 * SMALL_BUFF);
        assert_eq!(
            matcher.snapshot().style.stroke,
            Some(SURROUNDING_RECTANGLE_DEFAULT_COLOR)
        );
        assert_eq!(
            matcher.snapshot().style.fill.map(|color| color.alpha),
            Some(0.0)
        );
        assert_eq!(
            matcher.snapshot().style.stroke_width_mode,
            StrokeWidthMode::ScreenSpace
        );
    }

    #[test]
    fn surrounding_rectangle_uses_union_bounds_and_xy_buffer() {
        let left = Circle::new(1.0).shift(Vec2::new(-2.0, 0.0)).into_snapshot();
        let right = Rectangle::new(2.0, 4.0)
            .shift(Vec2::new(3.0, 1.0))
            .into_snapshot();
        let matcher = SurroundingRectangle::around([&left, &right], Vec2::new(0.25, 0.5), 0.2, RED)
            .expect("bounded targets");

        assert_eq!(matcher.snapshot().center(), Vec2::new(0.5, 1.0));
        assert_close(matcher.snapshot().width(), 7.5);
        assert_close(matcher.snapshot().height(), 5.0);
        assert_eq!(matcher.snapshot().style.stroke, Some(RED));
        assert!(matches!(
            matcher.snapshot().geometry,
            GeometryRef::VectorPath(_)
        ));
    }

    #[test]
    fn background_rectangle_matches_default_geometry_and_style() {
        let target = Circle::new(1.5).into_snapshot();
        let background = BackgroundRectangle::new(&target).expect("bounded target");
        assert_eq!(background.snapshot().center(), target.center());
        assert_close(background.snapshot().width(), 3.0);
        assert_close(background.snapshot().height(), 3.0);
        assert_eq!(
            background.snapshot().style.fill.map(|color| color.alpha),
            Some(BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY)
        );
        assert_eq!(
            background.snapshot().style.fill.map(|color| {
                let mut opaque = color;
                opaque.alpha = 1.0;
                opaque
            }),
            Some(BLACK)
        );
        assert_close(background.snapshot().style.stroke_width, 0.0);
        assert_eq!(
            background.snapshot().style.stroke.map(|color| color.alpha),
            Some(0.0)
        );
    }

    #[test]
    fn explicit_background_color_and_opacity_are_retained() {
        let target = Rectangle::new(2.0, 1.0).into_snapshot();
        let background =
            BackgroundRectangle::around([&target], Vec2::new(0.1, 0.2), 0.1, WHITE, 0.15)
                .expect("bounded target");
        assert_eq!(
            background.snapshot().style.fill.map(|color| color.alpha),
            Some(0.15)
        );
        assert_close(background.snapshot().width(), 2.2);
        assert_close(background.snapshot().height(), 1.4);
    }
}
