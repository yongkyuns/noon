use crate::path::GetPartial;
use crate::{
    AnimBuilder, Animation, AnimationType, Color, ColorExtension, EaseType, EntityAnimations,
    FillColor, Opacity, PathCompletion, Position, Scene, Size, StrokeColor, Value, WithColor,
    WithFill, WithId, WithPath, WithPosition, WithStroke,
};
use bevy_ecs::prelude::*;
use core::f32::consts::TAU;
use nannou::color::Rgba;
use nannou::lyon::math::{point, Angle, Vector};
use nannou::lyon::path::Path;

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
    pub fn with_color(mut self, color: Color) -> Self {
        self.fill_color = color;
        self.stroke_color = color.brighten();
        self
    }
    pub fn with_position(mut self, x: f32, y: f32) -> Self {
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
            .insert(FillColor(self.fill_color))
            .insert(StrokeColor(self.stroke_color))
            .insert(Opacity(0.0))
            .insert(PathCompletion(0.0))
            .id();

        id.into()
    }
    pub fn show(&mut self) -> CircleId {
        let id = self.make();
        let animation = EntityAnimations {
            entity: id.into(),
            animations: vec![Animation::to(Opacity(1.0)).into()],
        };

        AnimBuilder::new(self.scene, animation.into()).run_time(0.0);

        id
    }
}

pub fn draw_circle(
    draw: NonSend<nannou::Draw>,
    query: Query<
        (
            &Position,
            &StrokeColor,
            &FillColor,
            &Opacity,
            &Size,
            &PathCompletion,
        ),
        With<Circle>,
    >,
) {
    for (position, stroke_color, fill_color, alpha, size, completion) in query.iter() {
        if alpha.is_visible() {
            let radius = size.width / 2.0;
            let mut builder = Path::svg_builder();
            let sweep_angle = Angle::radians(-TAU);
            let x_rotation = Angle::radians(0.0);
            let center = point(0.0, 0.0);
            let start = point(radius, 0.0);
            let radii = Vector::new(radius, radius);

            builder.move_to(start);
            builder.arc(center, radii, sweep_angle, x_rotation);
            builder.close();

            let mut path = builder.build();
            if completion.0 < 1.0 {
                path = path.upto(completion.0, 0.01);
            }

            let stroke = Rgba {
                color: stroke_color.0,
                alpha: alpha.0,
            };
            let fill = Rgba {
                color: fill_color.0,
                alpha: alpha.0,
            };
            draw.path()
                .fill()
                .x_y(position.x, position.y)
                .color(fill)
                .events(&path);
            draw.path()
                .stroke()
                .x_y(position.x, position.y)
                .color(stroke)
                .stroke_weight(radius / 15.0)
                .events(&path);

            // draw.ellipse()
            //     .x_y(position.x, position.y)
            //     .color(fill)
            //     .radius(size.width / 2.0)
            //     .stroke_color(stroke)
            //     .stroke_weight(radius / 25.0);
        }
    }
}

pub fn circle(scene: &mut Scene) -> CircleBuilder {
    CircleBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct CircleId(pub(crate) Entity);

impl WithStroke for CircleId {}
impl WithFill for CircleId {}
impl WithColor for CircleId {}
impl WithPath for CircleId {}
impl WithPosition for CircleId {}

impl CircleId {
    pub fn set_radius(&self, radius: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: Animation::to(Size::from_radius(radius)).into(),
        }
    }
    pub fn set_radius_from(&self, entity: impl Into<Entity>) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: Animation::<Size>::to_target(entity.into()).into(),
        }
    }
}

impl WithId for CircleId {
    fn id(&self) -> Entity {
        self.0
    }
}

impl Into<Entity> for CircleId {
    fn into(self) -> Entity {
        self.0
    }
}

impl From<Entity> for CircleId {
    fn from(id: Entity) -> Self {
        CircleId(id)
    }
}
