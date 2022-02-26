use crate::{
    AnimBuilder, Animation, AnimationType, Color, ColorExtension, EaseType, EntityAnimations,
    FillColor, Opacity, Position, Scene, Size, StrokeColor, Value,
};
use bevy_ecs::prelude::*;
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
            .id();

        id.into()
    }
    pub fn show(&mut self) -> CircleId {
        let id = self.make();
        let animation = EntityAnimations {
            entity: id.into(),
            animations: vec![Animation::change_to(Opacity(1.0)).into()],
        };

        AnimBuilder::new(self.scene, animation.into()).run_time(0.0);

        id
    }
}

pub fn draw_circle(
    draw: NonSend<nannou::Draw>,
    query: Query<(&Position, &StrokeColor, &FillColor, &Opacity, &Size), With<Circle>>,
) {
    for (position, stroke_color, fill_color, alpha, size) in query.iter() {
        if alpha.is_visible() {
            // let mut builder = Path::svg_builder();
            // let sweep_angle = Angle::radians(PI * 2.0);
            // let x_rotation = Angle::radians(0.0);
            // let center: nannou::lyon::math::Point = point(0.0, 0.0);
            // let start = point(self.radius(), 0.0);
            // let radii = Vector::new(self.radius(), self.radius());

            // builder.move_to(start);
            // builder.arc(center, radii, sweep_angle, x_rotation);
            // builder.close();

            // let path = builder.build();
            // let path = path.upto(self.completion(), 0.01);

            let stroke = Rgba {
                color: stroke_color.0,
                alpha: alpha.0,
            };
            let fill = Rgba {
                color: fill_color.0,
                alpha: alpha.0,
            };
            draw.ellipse()
                .x_y(position.x, position.y)
                .radius(size.width)
                .stroke_color(stroke)
                .stroke_weight(size.width / 15.0)
                .color(fill);
        }
    }
}

pub fn circle(scene: &mut Scene) -> CircleBuilder {
    CircleBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct CircleId(pub(crate) Entity);

impl CircleId {
    pub fn move_to(&self, x: f32, y: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: vec![Animation::change_to(Position { x, y }).into()],
        }
    }
    pub fn set_color(&self, color: Color) -> EntityAnimations {
        // self.set_fill_color(color.brighten());
        // self.set_stroke_color(color.brighten());
        EntityAnimations {
            entity: self.0,
            animations: vec![
                Animation::change_to(StrokeColor(color.brighten())).into(),
                Animation::change_to(FillColor(color)).into(),
            ],
        }
    }
    pub fn set_color_from(&self, entity: impl Into<Entity>) -> EntityAnimations {
        let entity: Entity = entity.into();
        EntityAnimations {
            entity: self.0,
            animations: vec![
                Animation::<StrokeColor>::change_to_target(entity).into(),
                Animation::<FillColor>::change_to_target(entity).into(),
            ],
        }
    }
    pub fn fade_in(&self) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: vec![Animation::change_to(Opacity(1.0)).into()],
        }
    }
    pub fn fade_out(&self) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: vec![Animation::change_to(Opacity(0.0)).into()],
        }
    }
    pub fn set_fill_color(&self, color: Color) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: vec![Animation::change_to(FillColor(color)).into()],
        }
    }
    pub fn set_fill_color_from(&self, entity: impl Into<Entity>) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: vec![Animation::<FillColor>::change_to_target(entity.into()).into()],
        }
    }
    pub fn set_stroke_color(&self, color: Color) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: vec![Animation::change_to(StrokeColor(color)).into()],
        }
    }
    pub fn set_stroke_color_from(&self, entity: impl Into<Entity>) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: vec![Animation::<StrokeColor>::change_to_target(entity.into()).into()],
        }
    }
    pub fn set_radius(&self, radius: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: vec![Animation::change_to(Size::from_radius(radius)).into()],
        }
    }
    pub fn set_radius_from(&self, entity: impl Into<Entity>) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: vec![Animation::<Size>::change_to_target(entity.into()).into()],
        }
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
