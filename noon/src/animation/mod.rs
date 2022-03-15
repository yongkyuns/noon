use bevy_ecs::{
    entity::Entity,
    prelude::{Component, World},
};

use crate::{
    Angle, Cached, EaseType, FillColor, FontSize, Interpolate, Opacity, Path, PathCompletion,
    Position, Scene, Size, StrokeColor, StrokeWeight, Value,
};

mod builder;
mod color;
mod path;
mod spatial;

pub use builder::AnimBuilder;
pub use color::*;
pub use path::*;
pub use spatial::*;

/// Trait to indicate whether an object contains [Entity]. If it does,
/// the said object qualifies as a valid object to be inserted to the
/// [bevy_ecs].
pub trait WithId {
    fn id(&self) -> Entity;
}

/// Convenience struct to contain more than one [Animation] and implement
/// any related functionalities.
#[derive(Component)]
pub struct Animations<C: Interpolate + Component>(pub Vec<Animation<C>>);

/// Basic structure to describe an animation.
#[derive(Component, Debug, Clone)]
pub struct Animation<T> {
    /// Initial state of the animation. If `None`, will be initialized
    /// with current state when the time reaches `start_time`.
    pub(crate) begin: Option<T>,
    /// Final state of the animation. The final state may contain an
    /// absolute value, or a relative value with respect to the
    /// initialized `begin` state
    pub(crate) end: Value<T>,
    /// Duration of animation in seconds.
    pub(crate) duration: f32,
    /// Time at which animation should begin.
    pub(crate) start_time: f32,
    /// Easing function to be used for animation.
    pub(crate) rate_func: EaseType,
    /// If set to `false`, `duration` will be assigned by user
    /// through [Scene](crate::Scene)'s `play` function
    pub(crate) init_duration: bool,
    /// If set to `false`, `start_time` will be assigned by user
    /// through [Scene](crate::Scene)'s `play` function
    pub(crate) init_start_time: bool,
    /// If set to `false`, `rate_func` will be assigned by user
    /// through [Scene](crate::Scene)'s `play` function
    pub(crate) init_rate_func: bool,
}

impl<T> Animation<T>
where
    T: Interpolate + Component + Clone,
{
    pub fn to(to: T) -> Self {
        Self {
            begin: None,
            end: Value::Absolute(to),
            duration: 1.0,
            start_time: 0.0,
            rate_func: EaseType::Quint,
            init_duration: true,
            init_start_time: true,
            init_rate_func: true,
        }
    }

    pub fn to_target(target: Entity) -> Self {
        Self {
            begin: None,
            end: Value::From(target),
            duration: 1.0,
            start_time: 0.0,
            rate_func: EaseType::Linear,
            init_duration: true,
            init_start_time: true,
            init_rate_func: true,
        }
    }

    pub fn by(by: T) -> Self {
        Self {
            begin: None,
            end: Value::Relative(by),
            duration: 1.0,
            start_time: 0.0,
            rate_func: EaseType::Linear,
            init_duration: true,
            init_start_time: true,
            init_rate_func: true,
        }
    }

    pub fn with_duration(mut self, duration: f32) -> Self {
        self.duration = duration;
        self.init_duration = false;
        self
    }

    pub fn with_start_time(mut self, start_time: f32) -> Self {
        self.start_time = start_time;
        self.init_start_time = false;
        self
    }

    pub fn with_rate_func(mut self, rate_func: EaseType) -> Self {
        self.rate_func = rate_func;
        self.init_rate_func = false;
        self
    }

    pub fn has_target(&self) -> Option<Entity> {
        match self.end {
            Value::From(entity) => Some(entity),
            _ => None,
        }
    }

    pub fn init_from_target(&mut self, end: &T) {
        match &self.end {
            Value::From(_entity) => {
                self.end = Value::Absolute(end.clone());
            }
            _ => (),
        }
    }

    pub fn update(&mut self, property: &mut T, progress: f32) {
        match (&mut self.begin, &mut self.end) {
            (Some(begin), Value::Absolute(to)) => *property = begin.interp(&to, progress),
            (None, Value::Absolute(_to)) => {
                self.begin = Some(property.clone());
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
            (None, Value::Absolute(_to)) => {
                self.begin = Some(*property);
            }
            (None, Value::Relative(by)) => {
                self.begin = Some(*property);
                self.end = Value::Absolute(*property + *by);
            }
            _ => (),
        }
    }
}

impl<T> Into<Vec<AnimationType>> for Animation<T>
where
    Animation<T>: Into<AnimationType>,
{
    fn into(self) -> Vec<AnimationType> {
        vec![self.into()]
    }
}

#[derive(Clone)]
pub enum AnimationType {
    StrokeColor(Animation<StrokeColor>),
    StrokeWeight(Animation<StrokeWeight>),
    FillColor(Animation<FillColor>),
    Position(Animation<Position>),
    Angle(Animation<Angle>),
    Size(Animation<Size>),
    CachedSize(Animation<Cached<Size>>),
    FontSize(Animation<FontSize>),
    Opacity(Animation<Opacity>),
    PathCompletion(Animation<PathCompletion>),
    Path(Animation<Path>),
}

impl Into<AnimationType> for Animation<StrokeColor> {
    fn into(self) -> AnimationType {
        AnimationType::StrokeColor(self)
    }
}

impl Into<AnimationType> for Animation<StrokeWeight> {
    fn into(self) -> AnimationType {
        AnimationType::StrokeWeight(self)
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

impl Into<AnimationType> for Animation<Cached<Size>> {
    fn into(self) -> AnimationType {
        AnimationType::CachedSize(self)
    }
}

impl Into<AnimationType> for Animation<FontSize> {
    fn into(self) -> AnimationType {
        AnimationType::FontSize(self)
    }
}

impl Into<AnimationType> for Animation<Opacity> {
    fn into(self) -> AnimationType {
        AnimationType::Opacity(self)
    }
}

impl Into<AnimationType> for Animation<PathCompletion> {
    fn into(self) -> AnimationType {
        AnimationType::PathCompletion(self)
    }
}

impl Into<AnimationType> for Animation<Path> {
    fn into(self) -> AnimationType {
        AnimationType::Path(self)
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

fn set_properties<T: Component + Interpolate>(
    animation: &mut Animation<T>,
    start_time: f32,
    duration: f32,
    rate_func: EaseType,
) {
    if animation.init_start_time {
        animation.start_time = start_time;
    }
    if animation.init_duration {
        animation.duration = duration;
    }
    if animation.init_rate_func {
        animation.rate_func = rate_func;
    }
}

#[derive(Clone)]
pub struct EntityAnimations {
    pub(crate) entity: Entity,
    pub(crate) animations: Vec<AnimationType>,
}

impl EntityAnimations {
    pub fn insert_animation(self, world: &mut World) {
        for animation in self.animations.into_iter() {
            match animation {
                AnimationType::StrokeColor(animation) => {
                    insert_animation(animation, world, self.entity);
                }
                AnimationType::StrokeWeight(animation) => {
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
                AnimationType::CachedSize(animation) => {
                    insert_animation(animation, world, self.entity);
                }
                AnimationType::FontSize(animation) => {
                    insert_animation(animation, world, self.entity);
                }
                AnimationType::Opacity(animation) => {
                    insert_animation(animation, world, self.entity);
                }
                AnimationType::PathCompletion(animation) => {
                    insert_animation(animation, world, self.entity);
                }
                AnimationType::Path(animation) => {
                    insert_animation(animation, world, self.entity);
                }
            };
        }
    }
    pub fn start_time(&self) -> f32 {
        match self.animations.get(0).unwrap() {
            AnimationType::StrokeColor(animation) => animation.start_time,
            AnimationType::StrokeWeight(animation) => animation.start_time,
            AnimationType::FillColor(animation) => animation.start_time,
            AnimationType::Position(animation) => animation.start_time,
            AnimationType::Angle(animation) => animation.start_time,
            AnimationType::Size(animation) => animation.start_time,
            AnimationType::CachedSize(animation) => animation.start_time,
            AnimationType::FontSize(animation) => animation.start_time,
            AnimationType::Opacity(animation) => animation.start_time,
            AnimationType::PathCompletion(animation) => animation.start_time,
            AnimationType::Path(animation) => animation.start_time,
        }
    }
    pub fn set_properties(&mut self, start_time: f32, duration: f32, rate_func: EaseType) {
        for animation in self.animations.iter_mut() {
            match animation {
                AnimationType::StrokeColor(ref mut animation) => {
                    set_properties(animation, start_time, duration, rate_func);
                }
                AnimationType::StrokeWeight(ref mut animation) => {
                    set_properties(animation, start_time, duration, rate_func);
                }
                AnimationType::FillColor(ref mut animation) => {
                    set_properties(animation, start_time, duration, rate_func);
                }
                AnimationType::Position(ref mut animation) => {
                    set_properties(animation, start_time, duration, rate_func);
                }
                AnimationType::Angle(ref mut animation) => {
                    set_properties(animation, start_time, duration, rate_func);
                }
                AnimationType::Size(ref mut animation) => {
                    set_properties(animation, start_time, duration, rate_func);
                }
                AnimationType::CachedSize(ref mut animation) => {
                    set_properties(animation, start_time, duration, rate_func);
                }
                AnimationType::FontSize(ref mut animation) => {
                    set_properties(animation, start_time, duration, rate_func);
                }
                AnimationType::Opacity(ref mut animation) => {
                    set_properties(animation, start_time, duration, rate_func);
                }
                AnimationType::PathCompletion(ref mut animation) => {
                    set_properties(animation, start_time, duration, rate_func);
                }
                AnimationType::Path(ref mut animation) => {
                    set_properties(animation, start_time, duration, rate_func);
                }
            }
        }
    }
}

impl Into<Vec<EntityAnimations>> for EntityAnimations {
    fn into(self) -> Vec<EntityAnimations> {
        vec![self]
    }
}
