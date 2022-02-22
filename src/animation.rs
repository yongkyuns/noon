use bevy_ecs::{
    entity::Entity,
    prelude::{Component, World},
};

use crate::{Angle, EaseType, FillColor, Interpolate, Position, Scene, Size, StrokeColor, Value};

#[derive(Component)]
pub struct Animations<C: Interpolate + Component>(pub Vec<Animation<C>>);

#[derive(Component)]
pub struct Animation<T> {
    pub(crate) begin: Option<T>,
    pub(crate) end: Value<T>,
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

pub enum AnimationType {
    StrokeColor(Animation<StrokeColor>),
    FillColor(Animation<FillColor>),
    Position(Animation<Position>),
    Angle(Animation<Angle>),
    Size(Animation<Size>),
}

impl Into<AnimationType> for Animation<StrokeColor> {
    fn into(self) -> AnimationType {
        AnimationType::StrokeColor(self)
    }
}

impl Into<AnimationType> for Animation<FillColor> {
    fn into(self) -> AnimationType {
        AnimationType::FillColor(self)
    }
}

impl Into<AnimationType> for Animation<Position> {
    fn into(self) -> AnimationType {
        AnimationType::Position(self)
    }
}

impl Into<AnimationType> for Animation<Angle> {
    fn into(self) -> AnimationType {
        AnimationType::Angle(self)
    }
}

impl Into<AnimationType> for Animation<Size> {
    fn into(self) -> AnimationType {
        AnimationType::Size(self)
    }
}

fn insert_animation<C: Component + Interpolate>(
    animation: Animation<C>,
    world: &mut World,
    id: Entity,
) {
    if let Some(mut animations) = world.get_mut::<Animations<C>>(id) {
        animations.0.push(animation);
    } else {
        world.entity_mut(id).insert(Animations(vec![animation]));
    }
}

pub struct EntityAnimation {
    pub(crate) entity: Entity,
    pub(crate) animation: AnimationType,
}

impl EntityAnimation {
    pub fn insert_animation(self, world: &mut World) {
        let animation = self.animation;
        match animation {
            AnimationType::StrokeColor(animation) => {
                insert_animation(animation, world, self.entity);
            }
            AnimationType::FillColor(animation) => {
                insert_animation(animation, world, self.entity);
            }
            AnimationType::Position(animation) => {
                insert_animation(animation, world, self.entity);
            }
            AnimationType::Angle(animation) => {
                insert_animation(animation, world, self.entity);
            }
            AnimationType::Size(animation) => {
                insert_animation(animation, world, self.entity);
            }
        };
    }
}

impl Into<Vec<EntityAnimation>> for EntityAnimation {
    fn into(self) -> Vec<EntityAnimation> {
        vec![self]
    }
}

pub struct AnimBuilder<'a> {
    scene: &'a mut Scene,
    animations: Vec<EntityAnimation>,
    run_time: f32,
    rate_func: EaseType,
    lag: f32,
    repeat: usize,
}

impl<'a> AnimBuilder<'a> {
    pub fn new(scene: &'a mut Scene, animations: Vec<EntityAnimation>) -> Self {
        let mut rate_func = EaseType::Linear;
        // for ta in animations.iter() {
        //     if ta.action == Action::ShowCreation {
        //         rate_func = EaseType::Quad;
        //         break;
        //     }
        // }
        AnimBuilder {
            scene,
            animations,
            run_time: 1.0,
            rate_func,
            lag: 0.0,
            repeat: 0,
        }
    }
    pub fn run_time(mut self, duration: f32) -> Self {
        self.run_time = duration;
        self
    }
    pub fn rate_func(mut self, rate_func: EaseType) -> Self {
        self.rate_func = rate_func;
        self
    }
    pub fn lag(mut self, lag: f32) -> Self {
        self.lag = lag;
        self
    }
}

// impl<'a> Drop for AnimBuilder<'a> {
//     fn drop(&mut self) {
//         let Self {
//             run_time,
//             animations,
//             rate_func,
//             scene,
//             lag,
//             repeat,
//         } = self;

//         scene.commands.play(
//             animations.iter().fold(Vec::new(), |mut animations, ta| {
//                 animations.push(Animation {
//                     object: ta.target,
//                     action: ta.action,
//                     run_time: *run_time,
//                     rate_func: *rate_func,
//                     status: Status::NotStarted,
//                 });
//                 animations
//             }),
//             *lag,
//             *repeat,
//         );
//     }
// }
