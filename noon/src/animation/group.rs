use super::*;
// use bevy_ecs::prelude::*;

// pub struct GroupAnimations<T>(pub(crate) Vec<GroupAnimation<T>>);

// #[derive(Debug, Component, Clone)]
// pub struct GroupAnimation<T> {
//     pub(crate) animation: T,
//     /// Duration of animation in seconds.
//     pub(crate) duration: f32,
//     /// Time at which animation should begin.
//     pub(crate) start_time: f32,
//     /// Easing function to be used for animation.
//     pub(crate) rate_func: EaseType,
// }

// impl<T: Component> GroupAnimation<T> {
//     pub fn insert(self, world: &mut World, id: Entity) {
//         world.entity_mut(id).insert(self);
//     }

//     pub fn set_properties(&mut self, start_time: f32, duration: f32, rate_func: EaseType) {
//         self.start_time = start_time;
//         self.duration = duration;
//         self.rate_func = rate_func;
//     }
// }

// impl Into<Vec<AnimationType>> for GroupAnimation {
//     fn into(self) -> Vec<AnimationType> {
//         vec![self.into()]
//     }
// }

#[derive(Debug, Component, Clone)]
pub struct Arrange {
    pub(crate) align: Align,
    pub(crate) gap: f32,
    /// Duration of animation in seconds.
    pub(crate) duration: f32,
    /// Time at which animation should begin.
    pub(crate) start_time: f32,
    /// Easing function to be used for animation.
    pub(crate) rate_func: EaseType,
}

impl Arrange {
    pub fn new(align: Align, gap: f32) -> Self {
        Self {
            align,
            gap,
            duration: 0.0,
            start_time: 0.0,
            rate_func: EaseType::Quad,
        }
    }
    pub fn insert(self, world: &mut World, id: Entity) {
        world.entity_mut(id).insert(self);
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
    fn arrange(&self, align: Align, gap: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Arrange::new(align, gap).into(),
        }
    }
}

#[derive(Debug, Component, Clone, Copy)]
pub enum Align {
    Vertical,
    Horizontal,
    Grid(u32, u32),
}

impl Align {
    pub fn into_vector(&self, size: &Size, gap: f32) -> Vector {
        match self {
            Self::Vertical => Vector::new(0.0, -(size.height + gap)),
            Self::Horizontal => Vector::new(size.width + gap, 0.0),
            _ => Vector::new(0.0, 0.0),
        }
    }
    pub fn starting_vector(&self, total_size: &Size) -> Vector {
        match self {
            Self::Vertical => Vector::new(0.0, total_size.height / 2.0),
            Self::Horizontal => Vector::new(-total_size.width / 2.0, 0.0),
            _ => Vector::new(0.0, 0.0),
        }
    }
}
