use super::*;
use bevy_ecs::prelude::*;

#[derive(Debug, Component, Clone)]
pub struct Arrange {
    pub(crate) alignment: Alignment,
    pub(crate) gap: f32,
    /// Duration of animation in seconds.
    pub(crate) duration: f32,
    /// Time at which animation should begin.
    pub(crate) start_time: f32,
    /// Easing function to be used for animation.
    pub(crate) rate_func: EaseType,
}

impl Arrange {
    pub fn new(alignment: Alignment, gap: f32) -> Self {
        Self {
            alignment,
            gap,
            duration: 0.0,
            start_time: 0.0,
            rate_func: EaseType::Quad,
        }
    }
    pub fn insert(self, world: &mut World, id: Entity)
    // where
    //     T: Component + Interpolate,
    {
        world.entity_mut(id).insert(self);
        // if let Some(mut arrange) = world.get_mut::<Self>(id) {
        //     animations.push(self);

        // } else {
        //     world.entity_mut(id).insert(Animations(vec![self]));
        // }
    }

    pub fn set_properties(&mut self, start_time: f32, duration: f32, rate_func: EaseType) {
        self.start_time = start_time;
        self.duration = duration;
        self.rate_func = rate_func;
    }
}

impl Into<Vec<AnimationType>> for Arrange {
    fn into(self) -> Vec<AnimationType> {
        vec![self.into()]
    }
}

pub trait WithArrange: WithId {
    fn arrange(&self, alignment: Alignment, gap: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Arrange::new(alignment, gap).into(),
        }
    }
}

#[derive(Debug, Component, Clone, Copy)]
pub enum Alignment {
    Vertical,
    Horizontal,
    Tabular(u32, u32),
}

impl Alignment {
    pub fn into_vector(&self, size: &Size, gap: f32) -> Vector {
        match self {
            Self::Vertical => Vector::new(0.0, size.height + gap),
            Self::Horizontal => Vector::new(size.width + gap, 0.0),
            _ => Vector::new(0.0, 0.0),
        }
    }
}
