use crate::{Angle, Animation, Color, FillColor, Position, Scene, Size, StrokeColor, Value};
use bevy_ecs::prelude::*;

#[derive(Component)]
pub struct Rectangle;

pub struct RectangleBuilder<'a> {
    size: Size,
    stroke_color: Color,
    fill_color: Color,
    position: Position,
    angle: Angle,
    scene: &'a mut Scene,
}

impl<'a> RectangleBuilder<'a> {
    fn new(scene: &'a mut Scene) -> Self {
        Self {
            size: Size {
                width: 1.0,
                height: 1.0,
            },
            stroke_color: Default::default(),
            fill_color: Default::default(),
            position: Default::default(),
            angle: Default::default(),
            scene,
        }
    }
    pub fn with_stroke_color(mut self, color: Color) -> Self {
        self.stroke_color = color;
        self
    }
    pub fn with_fill_color(mut self, color: Color) -> Self {
        self.fill_color = color;
        self
    }
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.size = Size::from(width, height);
        self
    }
    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.position = Position { x, y };
        self
    }
    pub fn make(&mut self) -> RectangleId {
        let world = &mut self.scene.world;
        let id = world
            .spawn()
            .insert(Rectangle)
            .insert(self.size)
            .insert(self.position)
            .insert(self.angle)
            .insert(StrokeColor(self.stroke_color))
            .insert(FillColor(self.fill_color))
            .id();

        id.into()
    }
}

pub fn rectangle(scene: &mut Scene) -> RectangleBuilder {
    RectangleBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct RectangleId(pub(crate) Entity);

impl RectangleId {
    pub fn move_to(
        &self,
        x: f32,
        y: f32,
        start_time: f32,
    ) -> Vec<(impl Into<Entity>, Animation<Position>)> {
        vec![(self.0, Animation::change_to(Position { x, y }, start_time))]
    }
    pub fn set_angle(
        &self,
        angle: f32,
        start_time: f32,
    ) -> Vec<(impl Into<Entity>, Animation<Angle>)> {
        vec![(self.0, Animation::change_to(Angle(angle), start_time))]
    }
    pub fn set_size(
        &self,
        width: f32,
        height: f32,
        start_time: f32,
    ) -> Vec<(impl Into<Entity>, Animation<Size>)> {
        vec![(
            self.0,
            Animation::change_to(Size::from(width, height), start_time),
        )]
    }
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
}

impl From<RectangleId> for Entity {
    fn from(id: RectangleId) -> Self {
        id.0
    }
}

impl From<Entity> for RectangleId {
    fn from(id: Entity) -> Self {
        RectangleId(id)
    }
}
