use super::*;
// use bevy_ecs::prelude::*;

#[derive(Component)]
pub struct GroupActions<T>(pub(crate) Vec<GroupAction<T>>);

#[derive(Debug, Component, Clone)]
pub struct GroupAction<T> {
    pub(crate) action: T,
    /// Duration of animation in seconds.
    pub(crate) duration: f32,
    /// Time at which animation should begin.
    pub(crate) start_time: f32,
    /// Easing function to be used for animation.
    pub(crate) rate_func: EaseType,
    pub(crate) done: bool,
}

impl<T: Component> GroupAction<T> {
    pub fn new(action: T) -> Self {
        Self {
            action,
            duration: 1.0,
            start_time: 0.0,
            rate_func: Default::default(),
            done: false,
        }
    }
    pub fn insert(self, world: &mut World, id: Entity) {
        if let Some(mut actions) = world.get_mut::<GroupActions<T>>(id) {
            actions.0.push(self);
        } else {
            world.entity_mut(id).insert(GroupActions(vec![self]));
        }
    }

    pub fn set_properties(&mut self, start_time: f32, duration: f32, rate_func: EaseType) {
        self.start_time = start_time;
        self.duration = duration;
        self.rate_func = rate_func;
    }
}

impl GroupAction<Arrange> {
    pub fn gap(&self) -> f32 {
        self.action.gap
    }
    pub fn align(&self) -> Align {
        self.action.align
    }
}

impl<T> Into<Vec<AnimationType>> for GroupAction<T>
where
    GroupAction<T>: Into<AnimationType>,
{
    fn into(self) -> Vec<AnimationType> {
        vec![self.into()]
    }
}

#[derive(Debug, Component, Clone)]
pub struct Arrange {
    pub(crate) align: Align,
    pub(crate) gap: f32,
}

impl Arrange {
    pub fn new(align: Align, gap: f32) -> Self {
        Self { align, gap }
    }
}

pub trait WithArrange: WithId {
    fn arrange(&self, align: Align, gap: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: GroupAction::new(Arrange::new(align, gap)).into(),
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

#[derive(Debug, Component, Clone)]
pub struct Group;

#[derive(Debug, Component, Clone)]
pub struct Ungroup;
