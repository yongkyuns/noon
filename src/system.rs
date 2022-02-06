use std::{ops::Add, time::Instant};

use bevy_ecs::prelude::*;

use crate::{Animations, Bounds, Circle, Color, FillColor, Interpolate, Position, Value};

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
    #[allow(non_upper_case_globals)]
    pub const dt: f32 = 0.1;

    pub fn step(&mut self) {
        self.seconds += Time::dt;
        self.count += 1;
        if self.begin.is_none() {
            self.begin = Some(Instant::now());
        }
    }
    pub fn sample_time(&self) -> f64 {
        self.begin.unwrap().elapsed().as_secs_f64() / self.count as f64
    }
    pub fn elapsed_micros(&self) -> u128 {
        self.begin.unwrap().elapsed().as_micros()
    }
}

pub fn animate<Attribute: Interpolate + Component + Copy>(
    time: Res<Time>,
    mut query: Query<(&mut Attribute, &mut Animations<Attribute>)>,
) {
    for (mut att, mut animations) in query.iter_mut() {
        for animation in animations.0.iter_mut() {
            let t = time.seconds;
            let begin = animation.start_time;
            let duration = animation.duration;
            let end = animation.start_time + animation.duration + Time::dt;

            if begin < t && t <= end {
                let progress = ((t - begin) / duration).max(0.0).min(1.0);
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
            let end = animation.start_time + animation.duration + Time::dt;

            if begin < t && t <= end {
                let progress = ((t - begin) / duration).max(0.0).min(1.0);
                animation.update_position(&mut att, progress);
            }
        }
    }
}

pub fn update_time(mut time: ResMut<Time>) {
    time.step();
    // println!("t = {:2.2} sec", time.seconds);
}

pub fn print(res: Res<Time>, query: Query<(Entity, &Position, &FillColor), With<Circle>>) {
    for (entity, position, color) in query.iter() {
        println!(
            "Time = {} sec, Position = {}, FillColor = {}",
            res.seconds, &position, &color
        );
    }
}

// MoveTo(position)
// MoveTo(target)
// MoveBy(offset)
// ToEdge(direction)
