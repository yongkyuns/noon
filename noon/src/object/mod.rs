#![allow(unused)]

use bevy_ecs::prelude::*;

pub mod arrow;
pub mod circle;
pub mod dot;
pub mod empty;
pub mod line;
pub mod rectangle;
pub mod text;
pub mod triangle;

pub use circle::*;
pub use empty::*;
pub use line::*;
pub use rectangle::*;
pub use text::*;

use crate::{Animation, Color, EntityAnimations, FillColor, Opacity, Position, Size, StrokeColor};

mod common {
    pub use crate::path::GetPartial;
    pub use crate::PixelFrame;
    pub use crate::{
        Angle, AnimBuilder, Animation, BoundingSize, Color, ColorExtension, Create, Depth,
        EaseType, EntityAnimations, FillColor, FontSize, HasFill, Opacity, Origin, Path,
        PathCompletion, PathComponent, PixelPath, Point, Position, Previous, Scale, Scene, Size,
        StrokeColor, StrokeWeight, Transform, Value, WithAngle, WithColor, WithFill, WithFontSize,
        WithId, WithPath, WithPosition, WithSize, WithStroke, WithStrokeWeight, EPS, TO_PXL,
    };
    pub use bevy_ecs::prelude::*;
    pub use nannou::color::Rgba;
    pub use nannou::lyon::math::point;
}

#[derive(Component)]
pub struct Triangle;

#[derive(Component)]
pub struct Arrow;

#[derive(Component)]
pub struct Dot;

#[macro_export]
macro_rules! stroke_builder {
    ($name:ident) => {
        impl<'a> $name<'a> {
            pub fn with_stroke_weight(mut self, weight: f32) -> Self {
                self.stroke_weight = StrokeWeight(weight);
                self
            }
            pub fn with_thin_stroke(mut self) -> Self {
                self.stroke_weight = StrokeWeight::THIN;
                self
            }
            pub fn with_thick_stroke(mut self) -> Self {
                self.stroke_weight = StrokeWeight::THICK;
                self
            }
            pub fn with_stroke_color(mut self, color: Color) -> Self {
                self.stroke_color = color;
                self
            }
        }
    };
}

#[macro_export]
macro_rules! position_builder {
    ($name:ident) => {
        impl<'a> $name<'a> {
            pub fn with_position(mut self, x: f32, y: f32) -> Self {
                self.position = Position { x, y };
                self
            }
        }
    };
}

#[macro_export]
macro_rules! size_builder {
    ($name:ident) => {
        impl<'a> $name<'a> {
            pub fn with_size(mut self, width: f32, height: f32) -> Self {
                self.size = Size::from(width, height);
                self
            }
        }
    };
}

#[macro_export]
macro_rules! angle_builder {
    ($name:ident) => {
        impl<'a> $name<'a> {
            pub fn with_angle(mut self, radians: f32) -> Self {
                self.angle = Angle(radians);
                self
            }
        }
    };
}

#[macro_export]
macro_rules! fill_builder {
    ($name:ident) => {
        impl<'a> $name<'a> {
            pub fn with_fill_color(mut self, color: Color) -> Self {
                self.fill_color = color;
                self
            }
        }
    };
}

#[macro_export]
macro_rules! into_entity {
    ($name:ident) => {
        impl WithId for $name {
            fn id(&self) -> Entity {
                self.0
            }
        }

        impl Into<Entity> for $name {
            fn into(self) -> Entity {
                self.0
            }
        }

        impl From<Entity> for $name {
            fn from(id: Entity) -> Self {
                $name(id)
            }
        }
    };
}
