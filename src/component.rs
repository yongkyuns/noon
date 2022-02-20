use std::{marker::PhantomData, ops::Add};

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
pub struct Animations<C: Interpolate + Component>(pub Vec<Animation<C>>);

#[derive(Component)]
pub struct Animation<T> {
    begin: Option<T>,
    end: Value<T>,
    pub(crate) duration: f32,
    pub(crate) start_time: f32,
    pub(crate) ease: EaseType,
}

impl<T> Animation<T>
where
    T: Interpolate + Component + Copy,
{
    pub fn change_to(to: T, start_time: f32) -> Self {
        Self {
            begin: None,
            end: Value::Absolute(to),
            duration: 3.0,
            start_time,
            ease: EaseType::Quint,
        }
    }

    pub fn change_to_target(target: Entity, start_time: f32) -> Self {
        Self {
            begin: None,
            end: Value::From(target),
            duration: 1.0,
            start_time,
            ease: EaseType::Linear,
        }
    }

    pub fn change_by(by: T, start_time: f32) -> Self {
        Self {
            begin: None,
            end: Value::Relative(by),
            duration: 1.0,
            start_time,
            ease: EaseType::Linear,
        }
    }

    pub fn has_target(&self) -> Option<Entity> {
        match self.end {
            Value::From(entity) => Some(entity),
            _ => None,
        }
    }

    pub fn init_from_target(&mut self, end: &T) {
        match &self.end {
            Value::From(entity) => {
                self.end = Value::Absolute(*end);
            }
            _ => (),
        }
    }

    pub fn update(&mut self, property: &mut T, progress: f32) {
        match (&mut self.begin, &mut self.end) {
            (Some(begin), Value::Absolute(to)) => *property = begin.interp(&to, progress),
            (None, Value::Absolute(to)) => {
                self.begin = Some(*property);
            }
            _ => (),
        }
    }
}

impl Animation<Position> {
    pub fn update_position(&mut self, property: &mut Position, progress: f32) {
        match (&mut self.begin, &mut self.end) {
            (Some(begin), Value::Absolute(to)) => *property = begin.interp(&to, progress),
            (Some(begin), Value::Relative(by)) => {
                self.end = Value::Absolute(*begin + *by);
            }
            (None, Value::Absolute(to)) => {
                self.begin = Some(*property);
            }
            _ => (),
        }
    }
}

// impl<E, C> Into<Vec<(E, Animation<C>)>> for (E, Animation<C>)
// where
//     E: Into<Entity>,
//     C: Component + Interpolate,
// {
//     vec![(entity,animation)]
// }

// impl<A, E, C> From<A> for Vec<A>
// where
//     A: Into<Vec<(E, Animation<C>)>>,
//     E: Into<Entity>,
//     C: Component + Interpolate,
// {
//     fn from(animation: A) -> Self {
//         vec![animation]
//     }
// }

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

// #[derive(Component, Debug, Clone, Copy)]
// pub struct Color {
//     r: f32,
//     g: f32,
//     b: f32,
// }

// impl Color {
//     pub const BLACK: Self = Self {
//         r: 0.0,
//         g: 0.0,
//         b: 0.0,
//     };
//     pub const RED: Self = Self {
//         r: 1.0,
//         g: 0.0,
//         b: 0.0,
//     };
//     pub const BLUE: Self = Self {
//         r: 0.0,
//         g: 0.0,
//         b: 1.0,
//     };
//     pub const WHITE: Self = Self {
//         r: 1.0,
//         g: 1.0,
//         b: 1.0,
//     };
// }

// impl std::fmt::Display for Color {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "(r:{:1.2}, g:{:1.2}, b: {:1.2})", self.r, self.g, self.b)
//     }
// }

// impl Interpolate for Color {
//     fn interp(&self, other: &Self, progress: f32) -> Self {
//         Self {
//             r: self.r.interp(&other.r, progress),
//             g: self.g.interp(&other.g, progress),
//             b: self.b.interp(&other.b, progress),
//         }
//     }
// }

// impl Default for Color {
//     fn default() -> Self {
//         Self::BLACK
//     }
// }

#[derive(Debug, Component, Clone, Copy)]
pub struct FillColor(pub(crate) Color);

impl Interpolate for FillColor {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        let progress = progress.min(1.0).max(0.0);
        FillColor(self.0.interp(&other.0, progress))
    }
}

// impl std::fmt::Display for FillColor {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         self.0.fmt(f)
//     }
// }

impl IntoLinSrgba<f32> for FillColor {
    fn into_lin_srgba(self) -> LinSrgba {
        // let c = self.0;
        // LinSrgba::new(c.r, c.g, c.b, 1.0)
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

// impl std::fmt::Display for StrokeColor {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         self.0.fmt(f)
//     }
// }

impl IntoLinSrgba<f32> for StrokeColor {
    fn into_lin_srgba(self) -> LinSrgba {
        // let c = self.0;
        // // LinSrgba::new(c.r, c.g, c.b, 1.0)
        IntoLinSrgba::into_lin_srgba(self.0)
    }
}

#[derive(Component)]
pub enum Value<C> {
    Relative(C),
    Absolute(C),
    From(Entity),
}

// impl<C: Interpolate> Interpolate for Value<C> {
//     fn interp(&self, to: &C, progress: f32) -> Self {
//         match self {
//             Self::Relative(from) => Self::Relative(from.interp(to, progress)),
//             Self::Absolute(from) => Self::Absolute(from.interp(to, progress)),
//         }
//     }
// }

// pub trait Interpolate<T = Self> {
//     fn interp(&self, other: &T, progress: f32) -> Self
//     where
//         T: Into<Self>,
//         Self: Sized;
// }
