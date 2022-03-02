use std::array::IntoIter;

use bevy_ecs::prelude::*;
use nannou::geom::Rect;

use crate::component::FillColor;
use crate::system::{
    animate, animate_from_target, animate_position, print, update_path, update_time, Time,
};
use crate::{
    circle, draw_circle, draw_rectangle, rectangle, Angle, AnimBuilder, Animation, AnimationType,
    Animations, Circle, CircleBuilder, EntityAnimations, Interpolate, Opacity, PathCompletion,
    Position, Rectangle, RectangleBuilder, Size, StrokeColor, Value,
};

pub struct Bounds {
    rect: Rect,
}

impl Bounds {
    pub fn new(rect: Rect) -> Self {
        Self { rect }
    }
    pub fn edge_upper(&self) -> f32 {
        self.rect.y.end
    }
    pub fn edge_lower(&self) -> f32 {
        self.rect.y.start
    }
    pub fn edge_left(&self) -> f32 {
        self.rect.x.start
    }
    pub fn edge_right(&self) -> f32 {
        self.rect.x.end
    }
}

pub struct Scene {
    pub(crate) world: World,
    pub(crate) updater: Schedule,
    pub(crate) drawer: Schedule,
    pub(crate) event_time: f32,
}

impl Scene {
    pub fn new(window: Rect) -> Self {
        let mut world = World::new();
        world.insert_resource(Time::default());
        world.insert_resource(Bounds::new(window));

        let mut updater = Schedule::default();
        updater.add_stage(
            "update",
            SystemStage::parallel()
                .with_system(animate_position)
                .with_system(animate_from_target::<Position>)
                .with_system(animate_from_target::<FillColor>)
                .with_system(animate_from_target::<StrokeColor>)
                .with_system(animate_from_target::<Size>)
                .with_system(animate_from_target::<Angle>)
                .with_system(animate_from_target::<Opacity>)
                .with_system(animate::<FillColor>)
                .with_system(animate::<StrokeColor>)
                .with_system(animate::<Size>)
                .with_system(animate::<Angle>)
                .with_system(animate::<Opacity>)
                .with_system(animate::<PathCompletion>)
                .with_system(update_path::<Circle>)
                .with_system(update_path::<Rectangle>)
                .with_system(print),
        );
        let mut drawer = Schedule::default();
        drawer.add_stage(
            "draw",
            SystemStage::single_threaded()
                .with_system(draw_circle)
                .with_system(draw_rectangle),
        );
        Self {
            world,
            updater,
            drawer,
            event_time: 0.1,
        }
    }
    pub fn circle(&mut self) -> CircleBuilder {
        circle(self)
    }
    pub fn rectangle(&mut self) -> RectangleBuilder {
        rectangle(self)
    }
    pub fn update(&mut self, now: f32) {
        self.world
            .get_resource_mut::<Time>()
            .map(|mut t| t.seconds = now);

        self.updater.run(&mut self.world);
    }
    pub fn draw(&mut self, nannou_draw: nannou::Draw) {
        self.world.remove_non_send::<nannou::Draw>();
        self.world.insert_non_send(nannou_draw.clone());
        self.drawer.run(&mut self.world);
    }

    pub fn wait(&mut self) {
        self.event_time += 1.0;
    }

    pub fn wait_for(&mut self, time: f32) {
        self.event_time += time;
    }

    pub fn play(&mut self, animations: impl Into<Vec<EntityAnimations>>) -> AnimBuilder {
        AnimBuilder::new(self, animations.into())
    }
}

pub trait Construct {
    fn construct(&mut self);
}
