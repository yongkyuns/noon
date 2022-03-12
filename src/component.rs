use crate::{point, Point};
use bevy_ecs::prelude::*;
use nannou::color::{IntoLinSrgba, LinSrgba};
use std::{marker::PhantomData, ops::Add};

pub trait Interpolate<T = Self> {
    fn interp(&self, other: &T, progress: f32) -> Self
    where
        T: Into<Self>,
        Self: Sized;
}

impl Interpolate for f32 {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        self + (other - self) * progress
    }
}

impl Interpolate for u32 {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        self + ((other - self) as f32 * progress) as u32
    }
}

#[derive(Component)]
pub struct Name(String);

#[derive(Debug, Component, Default, Clone, Copy)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl Position {
    pub fn from_points(points: &[Point]) -> Self {
        let sum = points
            .iter()
            .fold(point(0.0, 0.0), |sum, &p| point(sum.x + p.x, sum.y + p.y));
        Position {
            x: sum.x / points.len() as f32,
            y: sum.y / points.len() as f32,
        }
    }
}

impl Interpolate for Position {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        Self {
            x: self.x.interp(&other.x, progress),
            y: self.y.interp(&other.y, progress),
        }
    }
}

impl Add for Position {
    type Output = Self;
    fn add(self, other: Self) -> Self::Output {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(x:{:3.2}, y:{:3.2})", self.x, self.y)
    }
}

impl Into<Position> for Point {
    fn into(self) -> Position {
        Position {
            x: self.x,
            y: self.y,
        }
    }
}

impl Interpolate for Point {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        point(
            self.x.interp(&other.x, progress),
            self.y.interp(&other.y, progress),
        )
    }
}

#[derive(Debug, Component, Default, Clone, Copy)]
pub struct Angle(pub(crate) f32);

impl Interpolate for Angle {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        Self(self.0.interp(&other.0, progress))
    }
}

#[derive(Debug, Component, Clone, Copy)]
pub struct FontSize(pub(crate) u32);

impl Interpolate for FontSize {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        Self(self.0.interp(&other.0, progress))
    }
}

#[derive(Debug, Component, Clone, Copy)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub fn from_radius(radius: f32) -> Self {
        Self {
            width: radius * 2.0,
            height: radius * 2.0,
        }
    }

    pub fn from(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn from_points(points: &[Point]) -> Self {
        let mut min = *points.first().unwrap();
        let mut max = *points.first().unwrap();

        for &p in points.iter() {
            if p.x < min.x {
                min.x = p.x;
            }
            if p.y < min.y {
                min.y = p.y;
            }
            if p.x > max.x {
                max.x = p.x;
            }
            if p.y > max.y {
                max.y = p.y;
            }
        }
        Size {
            width: (max.x - min.x).abs(),
            height: (max.y - min.y).abs(),
        }
    }
}

impl std::fmt::Display for Size {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(width:{:3.2}, height:{:3.2})", self.width, self.height)
    }
}

impl Interpolate for Size {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        Self {
            width: self.width.interp(&other.width, progress),
            height: self.height.interp(&other.height, progress),
        }
    }
}

#[derive(Debug, Component, Default, Clone, Copy)]
pub struct StrokeWeight(pub(crate) f32);

impl StrokeWeight {
    /// Think stroke. Normal default for shapes.
    pub const THICK: Self = Self(3.0);
    /// Thin stroke. Use it for very thin shape outline.
    pub const THIN: Self = Self(1.0);
    /// No stroke
    pub const NONE: Self = Self(0.0);
    /// Let the shape determine it's stroke width based on its size.
    pub const AUTO: Self = Self(-1.0);
    /// Determines if stroke should be drawn
    pub fn is_none(&self) -> bool {
        self.0.abs() < std::f32::EPSILON
    }
    /// Determines the stroke mode between auto or manual.
    pub fn is_auto(&self) -> bool {
        self.0 < 0.0
    }
}

impl Interpolate for StrokeWeight {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        if self.is_auto() {
            Self::AUTO
        } else {
            let progress = progress.min(1.0).max(0.0);
            Self(self.0.interp(&other.0, progress))
        }
    }
}

#[derive(Debug, Component, Default, Clone, Copy)]
pub struct Opacity(pub(crate) f32);

impl Opacity {
    pub const FULL: Self = Self(1.0);
    pub const HALF: Self = Self(0.5);
    pub const CLEAR: Self = Self(0.0);
    pub fn is_visible(&self) -> bool {
        self.0 > 0.0
    }
}

impl Interpolate for Opacity {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        let progress = progress.min(1.0).max(0.0);
        Self(self.0.interp(&other.0, progress))
    }
}

#[derive(Debug, Component, Default, Clone, Copy)]
pub struct PathCompletion(pub(crate) f32);

impl Interpolate for PathCompletion {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        let progress = progress.min(1.0).max(0.0);
        Self(self.0.interp(&other.0, progress))
    }
}

pub type Color = nannou::color::Rgb;

impl Interpolate for Color {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        let progress = progress.min(1.0).max(0.0);
        Self {
            red: self.red.interp(&other.red, progress),
            green: self.green.interp(&other.green, progress),
            blue: self.blue.interp(&other.blue, progress),
            standard: PhantomData,
        }
    }
}

impl ColorExtension for Color {
    fn get_color(&self) -> Color {
        *self
    }
}

pub trait ColorExtension {
    fn get_color(&self) -> Color;
    fn brighten(&self) -> Color {
        let mut hsv: nannou::color::Hsv = self.get_color().into_linear().into();
        hsv.saturation -= 0.1;
        hsv.value += 0.2;
        hsv.into()
    }
}

#[derive(Debug, Component, Clone, Copy)]
pub struct FillColor(pub(crate) Color);

impl Interpolate for FillColor {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        let progress = progress.min(1.0).max(0.0);
        FillColor(self.0.interp(&other.0, progress))
    }
}

impl IntoLinSrgba<f32> for FillColor {
    fn into_lin_srgba(self) -> LinSrgba {
        IntoLinSrgba::into_lin_srgba(self.0)
    }
}

#[derive(Debug, Component, Clone, Copy)]
pub struct StrokeColor(pub(crate) Color);

impl Interpolate for StrokeColor {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        let progress = progress.min(1.0).max(0.0);
        StrokeColor(self.0.interp(&other.0, progress))
    }
}

impl IntoLinSrgba<f32> for StrokeColor {
    fn into_lin_srgba(self) -> LinSrgba {
        IntoLinSrgba::into_lin_srgba(self.0)
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub enum Value<C> {
    Relative(C),
    Absolute(C),
    From(Entity),
}
