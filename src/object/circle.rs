use crate::{
    AnimBuilder, Animation, AnimationType, Color, EaseType, EntityAnimations, FillColor, Opacity,
    Position, Scene, Size, StrokeColor, Value,
};
use bevy_ecs::prelude::*;
use nannou::color::Rgba;

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
        let mut hsv: nannou::color::Hsv = color.into_linear().into();
        hsv.saturation -= 0.1;
        hsv.value += 0.2;
        self.set_stroke_color(hsv.into());

        EntityAnimations {
            entity: self.0,
            animations: vec![
                Animation::change_to(StrokeColor(hsv.into())).into(),
                Animation::change_to(FillColor(color)).into(),
            ],
        }
    }
    pub fn set_color_from(&self, entity: impl Into<Entity>) -> EntityAnimations {
        let entity: Entity = entity.into();
        EntityAnimations {
            entity: self.0,
            animations: vec![
                Animation::<StrokeColor>::change_to_target(entity.into()).into(),
                Animation::<FillColor>::change_to_target(entity.into()).into(),
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
