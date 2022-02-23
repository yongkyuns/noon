#![allow(unused)]

use bevy_ecs::prelude::*;

mod animation;
mod app;
mod component;
mod consts;
mod ease;
mod object;
mod scene;
mod system;

pub use crate::animation::{AnimBuilder, Animation, AnimationType, Animations, EntityAnimation};
pub use crate::component::{
    Angle, Color, FillColor, Interpolate, Name, Position, Size, StrokeColor, Value,
};
pub use consts::*;
pub use ease::EaseType;
use nannou::{
    color::{rgb_u32, rgba, Rgb},
    rand::{prelude::SliceRandom, random_range, thread_rng},
};
pub use object::*;
pub use scene::{Bounds, Construct, Scene};
pub use system::{animate, animate_from_target, animate_position, print, update_time, Time};

impl Construct for Scene {
    fn construct(&mut self) {
        let mut animations = Vec::new();
        for _ in (0..2000) {
            let (x, y, w, h, ang, color) = gen_random_values();

            if nannou::rand::random::<bool>() {
                let circle = self
                    .circle()
                    .at(x, y)
                    .with_fill_color(color)
                    .with_stroke_color(color)
                    .with_radius(w / 2.0)
                    .make();

                let (x, y, w, h, ang, color) = gen_random_values();

                animations.extend(vec![
                    circle.set_fill_color(color),
                    circle.set_stroke_color(color),
                    circle.move_to(x, y),
                    circle.set_radius(w / 2.0),
                ]);
            } else {
                let rect = self
                    .rectangle()
                    .at(x, y)
                    .with_fill_color(color)
                    .with_stroke_color(color)
                    .with_size(w, h)
                    .make();

                let (x, y, w, h, ang, color) = gen_random_values();

                animations.extend(vec![
                    rect.set_fill_color(color),
                    rect.set_stroke_color(color),
                    rect.move_to(x, y),
                    rect.set_size(w, h),
                    rect.set_angle(ang),
                ]);
            }
        }
        self.wait();
        self.play(animations)
            .run_time(5.0)
            .lag(0.001)
            .rate_func(EaseType::Quint);
    }
}

fn main() {
    app::run();
}

fn gen_random_values() -> (f32, f32, f32, f32, f32, Color) {
    let colors = [
        rgb_from_hex(0x264653),
        rgb_from_hex(0x2a9d8f),
        rgb_from_hex(0xe9c46a),
        rgb_from_hex(0xf4a261),
        rgb_from_hex(0xe76f51),
    ];
    let mut rng = thread_rng();

    let x_lim = 1920.0 / 2.0;
    let y_lim = 1080.0 / 2.0;

    let x = random_range::<f32>(-x_lim, x_lim);
    let y = random_range::<f32>(-y_lim, y_lim);
    let w = random_range::<f32>(2.0, 30.0);
    let h = random_range::<f32>(2.0, 30.0);
    let ang = random_range::<f32>(2.0, 30.0);
    let color = *colors.choose(&mut rng).unwrap();

    (x, y, w, h, ang, color)
}

fn rgb_from_hex(color: u32) -> Rgb {
    let color = rgb_u32(color);
    rgba(
        color.red as f32 / 255.0,
        color.green as f32 / 255.0,
        color.blue as f32 / 255.0,
        1.0,
    )
    .into()
}
