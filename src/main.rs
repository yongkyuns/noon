#![allow(unused)]

use std::marker::PhantomData;

use bevy_ecs::prelude::*;

mod component;
mod object;
mod scene;
mod system;

pub use crate::component::{
    Animation, Animations, Color, FillColor, Interpolate, Name, Orientation, Position, Size,
    StrokeColor, Value,
};
pub use object::*;
pub use scene::{Bounds, Scene};
pub use system::{animate, animate_from_target, animate_position, print, update_time, Time};

fn main() {
    let mut scene = Scene::new();

    // for _ in (0..1) {
    //     let circle = scene
    //         .circle()
    //         .at(3.0, 2.0)
    //         .with_fill_color(Color::BLACK)
    //         .with_radius(2.3)
    //         .make();
    //     }

    let circle = scene
        .circle()
        .at(3.0, 2.0)
        .with_fill_color(Color::WHITE)
        .with_radius(2.3)
        .make();

    let rect = scene.rectangle().with_fill_color(Color::RED).make();

    // scene.play(circle.move_to(10.0, 15.0, 1.0));
    scene.play(circle.set_fill_color(Color::BLACK, 1.0));
    // scene.play(circle.set_fill_color_from(rect, 2.0));
    // scene.play(circle.move_to(0.0, 0.0, 3.0));

    let mut update = Schedule::default();
    update.add_stage(
        "update",
        SystemStage::parallel()
            .with_system(update_time)
            .with_system(animate_position)
            .with_system(animate_from_target::<FillColor>)
            .with_system(animate::<FillColor>)
            .with_system(print),
    );

    for _ in (0..35) {
        update.run(&mut scene.world);
    }

    if let Some(t) = scene.world.get_resource::<Time>() {
        println!(
            "time elapsed = {} us, sample time = {} sec",
            t.elapsed_micros(),
            t.sample_time()
        );
    }
}
