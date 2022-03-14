use std::time::Instant;

use bevy_ecs::prelude::*;

use crate::{Animations, Bounds, Circle, FillColor, Interpolate, Position};

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

// pub fn update_path<E>(mut query: Query<(&E, &mut Path), ChangeTrackers<PathCompletion>>)
// pub fn update_path<E>(mut query: Query<(&PathCompletion, &Opacity, &Size, &mut Path), With<E>>)
// where
//     E: Component + PathComponent,
// {
//     for (completion, alpha, size, mut path) in query.iter_mut() {
//         if alpha.is_visible() {
//             *path = E::path(size, completion);
//         }
//     }
// }

// pub fn update_path<E>(mut query: Query<(&PathCompletion, &Opacity, &Size, &mut Path), With<E>>)
// where
//     E: Component,
// {
//     for (completion, alpha, size, mut path) in query.iter_mut() {
//         println!("alpha = {}, completion = {}", alpha.0, completion.0);
//         *path = path.clone().upto(completion.0, 0.01);

//         // *path = path.clone().upto(completion.0, 0.01);
//         // if alpha.is_visible() {
//         //     // println!("completion = {}", completion.0);
//         // }
//     }
// }

/// Initialize animation from another target's current state.
///
/// When animation commands are specified with respect to another
/// object instead of a specific value, this system takes care of
/// querying the attribute of the target object and initializing the
/// animation. Once initialized, [animate] executes the animation
/// for subsequent durations.
pub fn init_from_target<Attribute: Interpolate + Component + Clone>(
    time: Res<Time>,
    mut animation_query: Query<&mut Animations<Attribute>>,
    attribute_query: Query<&mut Attribute>,
) {
    for mut animations in animation_query.iter_mut() {
        for animation in animations.0.iter_mut() {
            let t = time.seconds;
            let begin = animation.start_time;
            let _duration = animation.duration;
            let end = animation.start_time + animation.duration + 0.0;

            if begin < t && t <= end {
                // If animation end state points to another entity, we need to query from that entity
                if let Some(target) = animation.has_target() {
                    // Check if target entity has said attribute
                    for _ in attribute_query.iter() {
                        if let Ok(attribute) = attribute_query.get(target) {
                            animation.init_from_target(attribute);
                        }
                    }
                }
            }
        }
    }
}

/// Generic system for animation of all contained attributes in `Bevy` ECS.
///
/// The way this works is by using [Interpolate] trait on [Component]s.
/// Attributes such as [Position] and [Size](crate::Size) that implements
/// [Interpolate] can be updated here, based on the corresponding [Animations]
/// for that attribute. [Time] is used as a trigger for each
/// [Animation](crate::Animation) contained within [Animations].
pub fn animate<Attribute: Interpolate + Component + Clone>(
    time: Res<Time>,
    mut query: Query<(Entity, &mut Attribute, &mut Animations<Attribute>)>,
) {
    for (_entity, mut att, mut animations) in query.iter_mut() {
        for animation in animations.0.iter_mut() {
            let t = time.seconds;
            let begin = animation.start_time;
            let duration = animation.duration;
            let end = animation.start_time + animation.duration + 0.0;

            if begin < t && t <= end {
                let progress = {
                    if duration > 0.0 {
                        animation.rate_func.calculate((t - begin) / duration)
                    } else {
                        1.0
                    }
                };
                animation.update(&mut att, progress);
            } else if end < t && t <= end + 0.1 {
                animation.update(&mut att, 1.0);
            }
        }
    }
}

/// A one-off implementation of [Position] animation system from [animate] system.
/// There are two reasons for not using the generic [animate] system:
///
/// 1. [Position] needs additional [Bounds] information for certain commands such
/// as moving to the edges of a window frame.
/// 2. [Position] supports relative movement commands, i.e. shift for specified
/// amount from the current position.
///
pub fn animate_position(
    time: Res<Time>,
    _bounds: Res<Bounds>,
    mut query: Query<(&mut Position, &mut Animations<Position>)>,
) {
    for (mut att, mut animations) in query.iter_mut() {
        for animation in animations.0.iter_mut() {
            let t = time.seconds;
            let begin = animation.start_time;
            let duration = animation.duration;
            let end = animation.start_time + animation.duration + 0.0;

            if begin < t && t <= end {
                let progress = {
                    if duration > 0.0 {
                        animation.rate_func.calculate((t - begin) / duration)
                    } else {
                        1.0
                    }
                };
                animation.update_position(&mut att, progress);
            } else if end < t && t <= end + 0.1 {
                animation.update(&mut att, 1.0);
            }
        }
    }
}

pub fn update_time(time: ResMut<Time>) {
    // time.step();
    println!("t = {:2.2} sec", time.seconds);
}

pub fn print(_res: Res<Time>, _query: Query<(Entity, &Position, &FillColor), With<Circle>>) {
    // for (entity, position, color) in query.iter() {
    //     // println!(
    //     //     "Time = {:2.1} sec, Position = {:2.1}, FillColor = {:1.1}",
    //     //     res.seconds, &position, &color
    //     // );
    // }
}
