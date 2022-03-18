use crate::prelude::Direction;
use crate::{point, Color, PixelFrame, Point, TO_PXL};
use bevy_ecs::prelude::*;
use nannou::color::{IntoLinSrgba, LinSrgba};
use nannou::lyon::math as euclid;
use std::ops::Add;

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
pub struct Transform(pub(crate) euclid::Transform);

impl Transform {
    pub fn new() -> Self {
        Self(euclid::Transform::identity())
    }
    pub fn identity() -> Self {
        Self(euclid::Transform::identity())
    }
    /// Translation. Untested
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.translate_mut(x, y);
        self
    }
    /// Translation. Untested
    pub fn translate_mut(&mut self, x: f32, y: f32) {
        *self = Self(self.0.then_translate(euclid::Vector::new(x, y)));
    }
    /// Rotation. Untested
    pub fn rotate(mut self, radians: f32) -> Self {
        self.rotate_mut(radians);
        self
    }
    /// Rotation. Untested
    pub fn rotate_mut(&mut self, radians: f32) {
        *self = Self(self.0.then_rotate(euclid::Angle::radians(radians)));
    }
    /// Scale. Untested
    pub fn scale(mut self, x: f32, y: f32) -> Self {
        self.scale_mut(x, y);
        self
    }
    /// Scale. Untested
    pub fn scale_mut(&mut self, x: f32, y: f32) {
        *self = Self(self.0.then_scale(x, y));
    }
}

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

impl PixelFrame for Position {
    fn into_pxl_scale(&self) -> Self {
        Self {
            x: self.x * TO_PXL,
            y: self.y * TO_PXL,
        }
    }
    fn into_natural_scale(&self) -> Self {
        Self {
            x: self.x / TO_PXL,
            y: self.y / TO_PXL,
        }
    }
}

#[derive(Debug, Component, Default, Clone, Copy)]
pub struct Angle(pub(crate) f32);

impl Interpolate for Angle {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        Self(self.0.interp(&other.0, progress))
    }
}

impl Add for Angle {
    type Output = Self;
    fn add(self, other: Self) -> Self::Output {
        Self(self.0 + other.0)
    }
}

#[derive(Debug, Component, Default, Clone, Copy)]
pub struct Depth(pub(crate) f32);

impl Interpolate for Depth {
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

impl Add for FontSize {
    type Output = Self;
    fn add(self, other: Self) -> Self::Output {
        Self(self.0 + other.0)
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

impl Add for Opacity {
    type Output = Self;
    fn add(self, other: Self) -> Self::Output {
        Self(self.0 + other.0)
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

impl Add for PathCompletion {
    type Output = Self;
    fn add(self, other: Self) -> Self::Output {
        Self(self.0 + other.0)
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

/// This is used as a means of storing the same [Component],
/// but with a different interpretation of the contained
/// value for [Animation](crate::Animation).
///
/// There are 3 cases of animation for most [Component]s:
/// 1. Animate to an absolute value (e.g. move to absolute position)
/// 2. Animate with respect to the specified change (i.e. relative to current)
/// 3. Use another object's current state as the final value
#[derive(Component, Clone, Copy, Debug)]
pub enum Value<C> {
    /// Indicates an absolute final state for animation
    Absolute(C),
    /// Indicates a relative change to apply in animation
    Relative(C),
    /// Indicates a multiplicative change to apply in animation
    Multiply(C),
    /// Used for moving object to edges
    Edge(Direction),
    /// Contains another object's ID to query for it's information
    From(Entity),
}

#[derive(Component, Clone, Copy)]
pub struct Previous<T>(pub(crate) T);

// /// Cache is used to wrap around any [Component] and keep track of
// /// it's changes.
// ///
// /// Since `Bevy`'s ECS system does not specifically provide mechanisms
// /// for querying the amount of change that occured, we need to handle
// /// it via this data type. It means we need to add [Cache] for any
// /// component which we intend to keep track of it's change amount,
// /// but it gets the job done without too much work for now.
// ///
// /// Where is this actually used? Since [Path](crate::Path) and
// /// [Size](crate::Size) are both [Component]s of ECS and go through
// /// different animations (e.g. path changes when morphing, and size
// /// changes when scaling), we need to sync them when change occurs in
// /// one of them. Therefore, when size changes, scale is computed by
// /// observing their delta, and applies respective changes to the path.
// #[derive(Component, Clone, Copy)]
// pub struct Cached<T> {
//     pub(crate) before: T,
//     pub(crate) now: T,
// }

// // use nannou::math::num_traits::Float;
// impl<T> Cached<T>
// where
//     T: Component + Clone + Copy,
// {
//     pub fn new(value: T) -> Self {
//         Self {
//             before: value,
//             now: value,
//         }
//     }
//     pub fn update(&mut self, value: T) {
//         self.before = self.now;
//         self.now = value;
//     }
//     // pub fn has_changed(&self) {
//     //     self.before Float::epsilon();
//     // }
// }

// impl Cached<Size> {
//     pub fn has_changed(&self) -> bool {
//         if ((self.before.width - self.now.width).abs() > 1.0e-4)
//             || ((self.before.height - self.now.height).abs() > 1.0e-4)
//         {
//             true
//         } else {
//             false
//         }
//     }
// }

// impl<T> Interpolate for Cached<T>
// where
//     T: Interpolate + Component + Clone + Copy,
// {
//     fn interp(&self, other: &Self, progress: f32) -> Self {
//         let before = self.now;
//         Self {
//             before,
//             now: Interpolate::interp(&self.now, &other.now, progress),
//         }
//     }
// }
