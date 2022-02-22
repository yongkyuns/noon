use std::{any::TypeId, marker::PhantomData, ops::Add};

use bevy_ecs::prelude::*;
use nannou::{
    color::{IntoLinSrgba, LinSrgba},
    draw::IntermediaryState,
};

use crate::EaseType;

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

#[derive(Component)]
pub struct Name(String);

#[derive(Debug, Component, Default, Clone, Copy)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl Interpolate for Position {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        let progress = progress.min(1.0).max(0.0);
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

#[derive(Debug, Component, Default, Clone, Copy)]
pub struct Angle(pub(crate) f32);

impl Interpolate for Angle {
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

#[derive(Component)]
pub enum Value<C> {
    Relative(C),
    Absolute(C),
    From(Entity),
}
