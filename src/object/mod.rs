#![allow(unused)]

use bevy_ecs::prelude::*;

mod arrow;
pub mod circle;
mod dot;
mod line;
pub mod rectangle;
mod triangle;

pub use circle::{circle, draw_circle, Circle, CircleBuilder};
pub use rectangle::{draw_rectangle, rectangle, Rectangle, RectangleBuilder};

use crate::{Animation, Color, EntityAnimations, FillColor, Opacity, Position, Size, StrokeColor};

// pub trait BaseObject {
//     fn id(&self) -> Entity;
//     fn fade_in(&self) -> EntityAnimation {
//         EntityAnimation {
//             entity: self.id(),
//             animation: Animation::change_to(Opacity(1.0)).into(),
//         }
//     }
//     fn fade_out(&self) -> EntityAnimation {
//         EntityAnimation {
//             entity: self.id(),
//             animation: Animation::change_to(Opacity(0.0)).into(),
//         }
//     }
//     fn move_to(&self, x: f32, y: f32) -> EntityAnimation {
//         EntityAnimation {
//             entity: self.id(),
//             animation: Animation::change_to(Position { x, y }).into(),
//         }
//     }
//     fn set_fill_color(&self, color: Color) -> EntityAnimation {
//         EntityAnimation {
//             entity: self.id(),
//             animation: Animation::change_to(FillColor(color)).into(),
//         }
//     }
//     fn set_fill_color_from(&self, entity: impl Into<Entity>) -> EntityAnimation {
//         EntityAnimation {
//             entity: self.id(),
//             animation: Animation::<FillColor>::change_to_target(entity.into()).into(),
//         }
//     }
//     fn set_stroke_color(&self, color: Color) -> EntityAnimation {
//         EntityAnimation {
//             entity: self.id(),
//             animation: Animation::change_to(StrokeColor(color)).into(),
//         }
//     }
//     fn set_stroke_color_from(&self, entity: impl Into<Entity>) -> EntityAnimation {
//         EntityAnimation {
//             entity: self.id(),
//             animation: Animation::<StrokeColor>::change_to_target(entity.into()).into(),
//         }
//     }
// }

#[derive(Component)]
pub struct Triangle;

#[derive(Component)]
pub struct Line;

#[derive(Component)]
pub struct Arrow;

#[derive(Component)]
pub struct Dot;
