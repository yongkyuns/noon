#![allow(unused)]

use bevy_ecs::prelude::*;

mod arrow;
mod circle;
mod dot;
mod line;
mod rectangle;
mod triangle;

pub use circle::{circle, Circle, CircleBuilder};
pub use rectangle::{rectangle, Rectangle, RectangleBuilder};

#[derive(Component)]
pub struct Triangle;

#[derive(Component)]
pub struct Line;

#[derive(Component)]
pub struct Arrow;

#[derive(Component)]
pub struct Dot;
