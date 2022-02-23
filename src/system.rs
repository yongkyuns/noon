use std::{ops::Add, time::Instant};

use bevy_ecs::prelude::*;

use crate::{
    Angle, Animations, Bounds, Circle, Color, FillColor, Interpolate, Position, Rectangle, Size,
    StrokeColor, Value,
};

pub struct Time {
    pub seconds: f32,
    pub count: u64,
    pub begin: Option<Instant>,
}

impl Default for Time {
    fn default() -> Self {
        Self {
            seconds: 0.0,
            count: 0,
            begin: None,
        }
    }
}

impl Time {
    // #[allow(non_upper_case_globals)]
    // pub const dt: f32 = 0.1;

    // pub fn step(&mut self) {
    //     self.seconds += Time::dt;
    //     self.count += 1;
    //     if self.begin.is_none() {
    //         self.begin = Some(Instant::now());
    //     }
    // }
    pub fn sample_time(&self) -> f64 {
        self.begin.unwrap().elapsed().as_secs_f64() / self.count as f64
    }
    pub fn elapsed_micros(&self) -> u128 {
        self.begin.unwrap().elapsed().as_micros()
    }
}

pub fn animate_from_target<Attribute: Interpolate + Component + Copy>(
    time: Res<Time>,
    mut animation_query: Query<&mut Animations<Attribute>>,
    mut attribute_query: Query<&mut Attribute>,
) {
    for (mut animations) in animation_query.iter_mut() {
        for animation in animations.0.iter_mut() {
            let t = time.seconds;
            let begin = animation.start_time;
            let duration = animation.duration;
            let end = animation.start_time + animation.duration + 0.1;

            if begin < t && t <= end {
                // If animation end state points to another entity, we need to query from that entity
                if let Some(target) = animation.has_target() {
                    // Check if target entity has said attribute
                    for src_attribute in attribute_query.iter() {
                        if let Ok(attribute) = attribute_query.get(target) {
                            animation.init_from_target(attribute);
                        }
                    }
                }
            }
        }
    }
}

pub fn animate<Attribute: Interpolate + Component + Copy>(
    time: Res<Time>,
    mut query: Query<(Entity, &mut Attribute, &mut Animations<Attribute>)>,
) {
    for (entity, mut att, mut animations) in query.iter_mut() {
        for animation in animations.0.iter_mut() {
            let t = time.seconds;
            let begin = animation.start_time;
            let duration = animation.duration;
            let end = animation.start_time + animation.duration + 0.1;

            if begin < t && t <= end {
                let progress = animation.rate_func.calculate((t - begin) / duration);
                animation.update(&mut att, progress);
            }
        }
    }
}

pub fn animate_position(
    time: Res<Time>,
    bounds: Res<Bounds>,
    mut query: Query<(&mut Position, &mut Animations<Position>)>,
) {
    for (mut att, mut animations) in query.iter_mut() {
        for animation in animations.0.iter_mut() {
            let t = time.seconds;
            let begin = animation.start_time;
            let duration = animation.duration;
            let end = animation.start_time + animation.duration + 0.1;

            if begin < t && t <= end {
                let progress = animation.rate_func.calculate((t - begin) / duration);
                animation.update_position(&mut att, progress);
            }
        }
    }
}

pub fn update_time(mut time: ResMut<Time>) {
    // time.step();
    println!("t = {:2.2} sec", time.seconds);
}

pub fn print(res: Res<Time>, query: Query<(Entity, &Position, &FillColor), With<Circle>>) {
    for (entity, position, color) in query.iter() {
        // println!(
        //     "Time = {:2.1} sec, Position = {:2.1}, FillColor = {:1.1}",
        //     res.seconds, &position, &color
        // );
    }
}

pub fn draw_circle(
    draw: NonSend<nannou::Draw>,
    query: Query<(&Position, &StrokeColor, &FillColor, &Size), With<Circle>>,
) {
    for (position, stroke_color, fill_color, size) in query.iter() {
        draw.ellipse()
            .x_y(position.x, position.y)
            .radius(size.width)
            .color(*fill_color)
            .stroke_color(*stroke_color)
            .stroke_weight(size.width / 15.0);
    }
}

pub fn draw_rectangle(
    draw: NonSend<nannou::Draw>,
    query: Query<(&Position, &Angle, &StrokeColor, &FillColor, &Size), With<Rectangle>>,
) {
    for (position, angle, stroke_color, fill_color, size) in query.iter() {
        draw.rect()
            .x_y(position.x, position.y)
            .w(size.width)
            .h(size.height)
            .z_radians(angle.0)
            .color(*fill_color)
            .stroke_color(*stroke_color)
            .stroke_weight(size.width / 15.0);
    }
}
