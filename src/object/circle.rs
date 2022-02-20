use crate::{Animation, Color, FillColor, Position, Scene, Size, StrokeColor, Value};
use bevy_ecs::prelude::*;

#[derive(Component)]
pub struct Circle;

pub struct CircleBuilder<'a> {
    radius: f32,
    stroke_color: Color,
    fill_color: Color,
    position: Position,
    scene: &'a mut Scene,
}

impl<'a> CircleBuilder<'a> {
    fn new(scene: &'a mut Scene) -> Self {
        Self {
            radius: 1.0,
            stroke_color: Default::default(),
            fill_color: Default::default(),
            position: Default::default(),
            scene,
        }
    }
    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
    pub fn with_stroke_color(mut self, color: Color) -> Self {
        self.stroke_color = color;
        self
    }
    pub fn with_fill_color(mut self, color: Color) -> Self {
        self.fill_color = color;
        self
    }
    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.position = Position { x, y };
        self
    }
    pub fn make(&mut self) -> CircleId {
        let world = &mut self.scene.world;
        let id = world
            .spawn()
            .insert(Circle)
            .insert(Size::from_radius(self.radius))
            .insert(self.position)
            .insert(StrokeColor(self.stroke_color))
            .insert(FillColor(self.fill_color))
            .id();

        id.into()
    }
}

pub fn circle(scene: &mut Scene) -> CircleBuilder {
    CircleBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct CircleId(pub(crate) Entity);

impl CircleId {
    pub fn move_to(
        &self,
        x: f32,
        y: f32,
        start_time: f32,
    ) -> Vec<(impl Into<Entity>, Animation<Position>)> {
        vec![(self.0, Animation::change_to(Position { x, y }, start_time))]
    }
    // pub fn set_color(
    //     &self,
    //     color: Color,
    //     start_time: f32,
    // ) -> Vec<(impl Into<Entity>, Animation<FillColor>)> {
    //     vec![(self.0, Animation::change_to(FillColor(color), start_time))]
    // }
    // pub fn set_color_from(
    //     &self,
    //     entity: impl Into<Entity>,
    //     start_time: f32,
    // ) -> Vec<(impl Into<Entity>, Animation<FillColor>)> {
    //     vec![(
    //         self.0,
    //         Animation::change_to_target(entity.into(), start_time),
    //     )]
    // }
    pub fn set_fill_color(
        &self,
        color: Color,
        start_time: f32,
    ) -> Vec<(impl Into<Entity>, Animation<FillColor>)> {
        vec![(self.0, Animation::change_to(FillColor(color), start_time))]
    }
    pub fn set_fill_color_from(
        &self,
        entity: impl Into<Entity>,
        start_time: f32,
    ) -> Vec<(impl Into<Entity>, Animation<FillColor>)> {
        vec![(
            self.0,
            Animation::change_to_target(entity.into(), start_time),
        )]
    }
    pub fn set_stroke_color(
        &self,
        color: Color,
        start_time: f32,
    ) -> Vec<(impl Into<Entity>, Animation<StrokeColor>)> {
        vec![(self.0, Animation::change_to(StrokeColor(color), start_time))]
    }
    pub fn set_stroke_color_from(
        &self,
        entity: impl Into<Entity>,
        start_time: f32,
    ) -> Vec<(impl Into<Entity>, Animation<StrokeColor>)> {
        vec![(
            self.0,
            Animation::change_to_target(entity.into(), start_time),
        )]
    }
    pub fn set_radius(
        &self,
        radius: f32,
        start_time: f32,
    ) -> Vec<(impl Into<Entity>, Animation<Size>)> {
        vec![(
            self.0,
            Animation::change_to(Size::from_radius(radius), start_time),
        )]
    }
    pub fn set_radius_from(
        &self,
        entity: impl Into<Entity>,
        start_time: f32,
    ) -> Vec<(impl Into<Entity>, Animation<Size>)> {
        vec![(
            self.0,
            Animation::change_to_target(entity.into(), start_time),
        )]
    }
}

impl From<CircleId> for Entity {
    fn from(id: CircleId) -> Self {
        id.0
    }
}

impl From<Entity> for CircleId {
    fn from(id: Entity) -> Self {
        CircleId(id)
    }
}
