use std::{
    ops::{Add, Mul},
    time::Instant,
};

use bevy_ecs::prelude::*;

use crate::{
    Angle, Animation, Animations, BoundingSize, Bounds, Circle, FillColor, Interpolate, Path,
    Position, Previous, Size,
};

#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemLabel)]
pub enum Label {
    Init,
    Main,
    Post,
}

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

// pub fn update_path_from_size_change(mut query: Query<(&mut Path, &Size), Changed<Size>>) {
//     for (mut path, size) in query.iter_mut() {
//         // let scale = size.before.scale_factor(&size.now);
//         let scale = path.size().scale_factor(&(*size * TO_PXL));
//         // println!(
//         //     "path = {},{}",
//         //     path.size().width / TO_PXL,
//         //     path.size().height / TO_PXL
//         // );
//         // println!("size = {},{}", size.before.width, size.before.height);
//         *path = path.scale(scale.0, scale.1);
//     }
// }

// pub fn update_children(
//     // time: Res<Time>,
//     mut parents_query: Query<(Entity, &Position, &Angle, &Children), With<Empty>>,
//     mut attributes_query: Query<(&mut Position, &mut Angle)>,
// ) {
//     for (parent, position, angle, children) in parents_query.iter_mut() {
//         if let Ok((position, angle)) = attributes_query.get_mut(parent) {
//             // transform.rotate(Quat::from_rotation_z(-angle * time.delta_seconds()));
//         }
//     }
// }

pub fn bbox_angle_update(mut query: Query<(&mut BoundingSize, &Path, &Angle), Changed<Angle>>) {
    for (mut bbox, path, angle) in query.iter_mut() {
        *bbox = BoundingSize::from(path, angle.0);
    }
}

pub fn update_path_from_size_change(
    mut query: Query<
        (
            &mut Path,
            &Size,
            &mut Previous<Size>,
            &mut BoundingSize,
            &Angle,
        ),
        Changed<Size>,
    >,
) {
    for (mut path, size_now, mut size_prev, mut bbox, angle) in query.iter_mut() {
        let scale = size_prev.0.scale_factor(size_now);
        *path = path.scale(scale.0, scale.1);
        *size_prev = Previous(*size_now);
        *bbox = BoundingSize::from(&path, angle.0);
    }
}

// pub fn update_previous<T: Component + Clone>(mut query: Query<(&T, &mut Previous<T>), Changed<T>>) {
//     // println!("previous changed");
//     // for (current, mut prev) in query.iter_mut() {
//     //     prev.0 = current.clone();
//     // }
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

/// [System] for initializing the animation of the current object from
/// another target's current state.
///
/// When animation commands are specified with respect to another object
/// (i.e. target) instead of a specific value, this system takes care of
/// querying the attribute of the target object and initializing the
/// animation. Once initialized, [animate] executes the actual animation
/// for subsequent durations.
pub fn init_from_target<C: Interpolate + Component + Clone>(
    time: Res<Time>,
    mut animation_query: Query<&mut Animations<C>>,
    attribute_query: Query<&C>,
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

#[inline]
fn common_update<C, F>(time: Res<Time>, mut query: Query<(&mut C, &mut Animations<C>)>, updater: F)
where
    C: Interpolate + Component + Clone,
    F: Fn(&mut Animation<C>, &mut Mut<C>, f32),
{
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
                updater(animation, &mut att, progress);
            } else if end < t && t <= end + 0.1 {
                updater(animation, &mut att, 1.0);
            }
        }
    }
}

/// Generic [System] for animation of all regular [Component]s in ECS.
///
/// The way this works is by using [Interpolate] trait on [Component]s.
/// Attributes such as [Position] and [Size](crate::Size) that implements
/// [Interpolate] can be updated here, based on the corresponding [Animations]
/// for that attribute. [Time] is used as a trigger for each
/// [Animation](crate::Animation) contained within [Animations].
///
pub fn animate<C: Interpolate + Component + Clone>(
    time: Res<Time>,
    query: Query<(&mut C, &mut Animations<C>)>,
) {
    let updater =
        |animation: &mut Animation<C>, att: &mut Mut<C>, progress| animation.update(att, progress);
    common_update(time, query, updater);
}

pub fn animate_with_relative<C: Interpolate + Component + Clone + Add<Output = C>>(
    time: Res<Time>,
    query: Query<(&mut C, &mut Animations<C>)>,
) {
    let updater = |animation: &mut Animation<C>, att: &mut Mut<C>, progress| {
        animation.update_with_relative(att, progress)
    };
    common_update(time, query, updater);
}

pub fn animate_with_multiply<C: Interpolate + Component + Clone + Mul<Output = C>>(
    time: Res<Time>,
    query: Query<(&mut C, &mut Animations<C>)>,
) {
    let updater = |animation: &mut Animation<C>, att: &mut Mut<C>, progress| {
        animation.update_with_multiply(att, progress)
    };
    common_update(time, query, updater);
}

pub fn animate_position(
    time: Res<Time>,
    bounds: Res<Bounds>,
    mut query: Query<(&mut Position, &BoundingSize, &mut Animations<Position>)>,
) {
    for (mut position, size, mut animations) in query.iter_mut() {
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
                // println!("{:?}", size);
                animation.update_position(&mut position, progress, &bounds, &size.0);
            } else if end < t && t <= end + 0.1 {
                animation.update_position(&mut position, 1.0, &bounds, &size.0);
            }
        }
    }
}

// /// Generic [System] for animation of all [Component]s in ECS.
// ///
// /// The way this works is by using [Interpolate] trait on [Component]s.
// /// Attributes such as [Position] and [Size](crate::Size) that implements
// /// [Interpolate] can be updated here, based on the corresponding [Animations]
// /// for that attribute. [Time] is used as a trigger for each
// /// [Animation](crate::Animation) contained within [Animations].
// pub fn animate_<C: Interpolate + Component + Clone>(
//     time: Res<Time>,
//     mut query: Query<(Entity, &mut C, &mut Animations<C>)>,
// ) {
//     for (_entity, mut att, mut animations) in query.iter_mut() {
//         for animation in animations.0.iter_mut() {
//             let t = time.seconds;
//             let begin = animation.start_time;
//             let duration = animation.duration;
//             let end = animation.start_time + animation.duration + 0.0;

//             if begin < t && t <= end {
//                 let progress = {
//                     if duration > 0.0 {
//                         animation.rate_func.calculate((t - begin) / duration)
//                     } else {
//                         1.0
//                     }
//                 };
//                 animation.update(&mut att, progress);
//             } else if end < t && t <= end + 0.1 {
//                 animation.update(&mut att, 1.0);
//             }
//         }
//     }
// }

// /// A one-off implementation of [Size] animation system from [animate] system.
// /// There are two reasons for not using the generic [animate] system:
// pub fn animate_size(time: Res<Time>, mut query: Query<(&mut Size, &mut Animations<Size>)>) {
//     for (mut att, mut animations) in query.iter_mut() {
//         for animation in animations.0.iter_mut() {
//             let t = time.seconds;
//             let begin = animation.start_time;
//             let duration = animation.duration;
//             let end = animation.start_time + animation.duration + 0.0;

//             if begin < t && t <= end {
//                 let progress = {
//                     if duration > 0.0 {
//                         animation.rate_func.calculate((t - begin) / duration)
//                     } else {
//                         1.0
//                     }
//                 };
//                 animation.update_size(&mut att, progress);
//             } else if end < t && t <= end + 0.1 {
//                 animation.update_size(&mut att, 1.0);
//             }
//         }
//     }
// }

// /// A one-off implementation of [Position] animation system from [animate] system.
// /// There are two reasons for not using the generic [animate] system:
// ///
// /// 1. [Position] needs additional [Bounds] information for certain commands such
// /// as moving to the edges of a window frame.
// /// 2. [Position] supports relative movement commands, i.e. shift for specified
// /// amount from the current position.
// ///
// pub fn animate_position(
//     time: Res<Time>,
//     _bounds: Res<Bounds>,
//     mut query: Query<(&mut Position, &mut Animations<Position>)>,
// ) {
//     for (mut att, mut animations) in query.iter_mut() {
//         for animation in animations.0.iter_mut() {
//             let t = time.seconds;
//             let begin = animation.start_time;
//             let duration = animation.duration;
//             let end = animation.start_time + animation.duration + 0.0;

//             if begin < t && t <= end {
//                 let progress = {
//                     if duration > 0.0 {
//                         animation.rate_func.calculate((t - begin) / duration)
//                     } else {
//                         1.0
//                     }
//                 };
//                 animation.update_position(&mut att, progress);
//             } else if end < t && t <= end + 0.1 {
//                 animation.update_position(&mut att, 1.0);
//             }
//         }
//     }
// }

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

// 정은, 은정 가족에게,

// 새로운 가족 태희의 탄생을 진심으로 축하합니다! 태희와 모두의 건강과 행복을 항상 기원합니다.

// 용균,혜림 가족 드림
