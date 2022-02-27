use crate::path::GetPartial;
use crate::{
    Angle, AnimBuilder, Animation, Color, ColorExtension, EaseType, EntityAnimations, FillColor,
    Opacity, PathCompletion, Position, Scene, Size, StrokeColor, Value, WithColor, WithFill,
    WithId, WithPath, WithPosition, WithStroke, WithAngle, WithSize,
};
use bevy_ecs::prelude::*;
use nannou::color::Rgba;
use nannou::lyon::math::point;
use nannou::lyon::path::Path;

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
    pub fn with_color(mut self, color: Color) -> Self {
        self.fill_color = color;
        self.stroke_color = color.brighten();
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
            .insert(PathCompletion(0.0))
            .id();

        id.into()
    }
    pub fn show(&mut self) -> RectangleId {
        let id = self.make();
        let animations = EntityAnimations {
            entity: id.into(),
            animations: vec![Animation::to(Opacity(1.0)).into()],
        };

        AnimBuilder::new(self.scene, animations.into()).run_time(0.0);

        id
    }
}

pub fn draw_rectangle(
    draw: NonSend<nannou::Draw>,
    query: Query<
        (
            &Position,
            &Angle,
            &StrokeColor,
            &FillColor,
            &Opacity,
            &Size,
            &PathCompletion,
        ),
        With<Rectangle>,
    >,
) {
    for (position, angle, stroke_color, fill_color, alpha, size, completion) in query.iter() {
        if alpha.is_visible() {
            let mut builder = Path::svg_builder();
            let start = point(-size.width / 2.0, size.height / 2.0);

            builder.move_to(start);
            builder.line_to(point(start.x + size.width, start.y));
            builder.line_to(point(start.x + size.width, start.y - size.height));
            builder.line_to(point(start.x, start.y - size.height));
            builder.line_to(point(start.x, start.y));
            builder.close();

            let path = builder.build();
            let path = path.upto(completion.0, 0.01);

            let stroke = Rgba {
                color: stroke_color.0,
                alpha: alpha.0,
            };
            let fill = Rgba {
                color: fill_color.0,
                alpha: alpha.0,
            };

            // Draw fill first
            draw.path()
                .fill()
                .x_y(position.x, position.y)
                .z_degrees(angle.0)
                .color(fill)
                .events(&path);

            // Draw stroke on top
            draw.path()
                .stroke()
                .x_y(position.x, position.y)
                .z_degrees(angle.0)
                .color(stroke)
                .stroke_weight(size.width.min(size.height) / 25.0)
                .events(&path);
        }
    }
}

pub fn rectangle(scene: &mut Scene) -> RectangleBuilder {
    RectangleBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct RectangleId(pub(crate) Entity);

impl WithStroke for RectangleId {}
impl WithFill for RectangleId {}
impl WithColor for RectangleId {}
impl WithPath for RectangleId {}
impl WithPosition for RectangleId {}
impl WithAngle for RectangleId {}
impl WithSize for RectangleId {}

impl WithId for RectangleId {
    fn id(&self) -> Entity {
        self.0
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
