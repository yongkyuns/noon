use bevy_ecs::prelude::*;

use crate::{circle, Animation, Animations, CircleBuilder, Interpolate, Position, Time};

pub struct Size {
    width: f32,
    height: f32,
}

pub struct Bounds {
    size: Size,
}

impl Default for Bounds {
    fn default() -> Self {
        Self {
            size: Size {
                width: 100.0,
                height: 50.0,
            },
        }
    }
}

pub struct Scene {
    pub(crate) world: World,
}

impl Scene {
    pub fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(Time::default());
        world.insert_resource(Bounds::default());
        Self { world }
    }
    pub fn circle(&mut self) -> CircleBuilder {
        circle(self)
    }
    pub fn play<C>(&mut self, animation: (impl Into<Entity>, Animation<C>))
    where
        C: Component + Interpolate,
    {
        let id: Entity = animation.0.into();
        if let Some(mut animations) = self.world.get_mut::<Animations<C>>(id) {
            animations.0.push(animation.1);
        } else {
            self.world
                .entity_mut(id)
                .insert(Animations(vec![animation.1]));
        }

        // if let Some(mut p) = self.world.get_mut::<Position>(id) {
        //     p.x += position.x;
        //     p.y += position.y;
        // }
    }
}
