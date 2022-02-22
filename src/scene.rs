use std::array::IntoIter;

use bevy_ecs::prelude::*;
use nannou::geom::Rect;

use crate::component::FillColor;
use crate::system::{
    animate, animate_from_target, animate_position, draw_circle, draw_rectangle, print,
    update_time, Time,
};
use crate::{
    circle, rectangle, Angle, Animation, AnimationType, Animations, CircleBuilder, EntityAnimation,
    Interpolate, Position, RectangleBuilder, Size, StrokeColor, Value,
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
                .with_system(animate_from_target::<FillColor>)
                .with_system(animate::<FillColor>)
                .with_system(animate_from_target::<StrokeColor>)
                .with_system(animate::<StrokeColor>)
                .with_system(animate_from_target::<Size>)
                .with_system(animate::<Size>)
                .with_system(animate_from_target::<Angle>)
                .with_system(animate::<Angle>)
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

    pub fn play(&mut self, animations: impl Into<Vec<EntityAnimation>>) {
        let animations: Vec<EntityAnimation> = animations.into();
        for animation in animations.into_iter() {
            animation.insert_animation(&mut self.world);
        }
    }
}

pub trait Construct {
    fn construct(&mut self);
}
