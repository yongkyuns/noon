use crate::{
    Angle, AnimBuilder, Animation, Color, EaseType, EntityAnimations, FillColor, Opacity, Position,
    Scene, Size, StrokeColor, Value,
};
use bevy_ecs::prelude::*;
use nannou::color::Rgba;

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
    pub fn with_position(mut self, x: f32, y: f32) -> Self {
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
            .insert(Opacity(0.0))
            .id();

        id.into()
    }
    pub fn show(&mut self) -> RectangleId {
        let id = self.make();
        let animations = EntityAnimations {
            entity: id.into(),
            animations: vec![Animation::change_to(Opacity(1.0)).into()],
        };

        AnimBuilder::new(self.scene, animations.into()).run_time(0.0);

        id
    }
}

pub fn draw_rectangle(
    draw: NonSend<nannou::Draw>,
    query: Query<(&Position, &Angle, &StrokeColor, &FillColor, &Opacity, &Size), With<Rectangle>>,
) {
    for (position, angle, stroke_color, fill_color, alpha, size) in query.iter() {
        if alpha.is_visible() {
            let stroke = Rgba {
                color: stroke_color.0,
                alpha: alpha.0,
            };
            let fill = Rgba {
                color: fill_color.0,
                alpha: alpha.0,
            };
            draw.rect()
                .x_y(position.x, position.y)
                .w(size.width)
                .h(size.height)
                .z_radians(angle.0)
                .color(fill)
                .stroke_color(stroke)
                .stroke_weight(size.width / 15.0);
        }
    }
}

pub fn rectangle(scene: &mut Scene) -> RectangleBuilder {
    RectangleBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct RectangleId(pub(crate) Entity);

impl RectangleId {
    pub fn set_angle(&self, angle: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: vec![Animation::change_to(Angle(angle)).into()],
        }
    }
    pub fn set_size(&self, width: f32, height: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: vec![Animation::change_to(Size::from(width, height)).into()],
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
    pub fn move_to(&self, x: f32, y: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: vec![Animation::change_to(Position { x, y }).into()],
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
