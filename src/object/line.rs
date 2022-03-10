use crate::path::GetPartial;
use crate::{
    Angle, AnimBuilder, Animation, Color, ColorExtension, EaseType, EntityAnimations, FillColor,
    Opacity, Path, PathCompletion, PathComponent, Point, Position, Scene, Size, StrokeColor, Value,
    WithAngle, WithColor, WithFill, WithId, WithPath, WithPosition, WithSize, WithStroke,
};
use bevy_ecs::prelude::*;
use nannou::color::Rgba;
use nannou::lyon::math::point;

#[derive(Component)]
pub struct Line;

impl Line {
    fn path(points: &[Point]) -> Path {
        let mut builder = Path::builder();

        if !points.is_empty() {
            // builder.move_to(*points.get(0).unwrap());
            builder.begin(*points.get(0).unwrap());
            for &p in points.iter() {
                builder.line_to(p);
            }
        }
        builder.end(false);
        // builder.line_to(to);
        // builder.close();
        Path(builder.build())
    }
}

pub struct LineBuilder<'a> {
    points: Vec<Point>,
    stroke_color: Color,
    angle: Angle,
    scene: &'a mut Scene,
}

impl<'a> LineBuilder<'a> {
    fn new(scene: &'a mut Scene) -> Self {
        Self {
            points: Vec::new(),
            stroke_color: Default::default(),
            angle: Default::default(),
            scene,
        }
    }
    pub fn from(mut self, x: f32, y: f32) -> Self {
        self.points = vec![point(x, y)];
        self
    }
    pub fn to(mut self, x: f32, y: f32) -> Self {
        if !self.points.is_empty() {
            self.points.push(point(x, y));
        }
        self
    }
    pub fn add(mut self, point: Point) -> Self {
        self.points.push(point);
        self
    }
    pub fn with_stroke_color(mut self, color: Color) -> Self {
        self.stroke_color = color;
        self
    }
    pub fn with_color(mut self, color: Color) -> Self {
        self.stroke_color = color;
        self
    }
    pub fn make(&mut self) -> LineId {
        let world = &mut self.scene.world;
        let id = world
            .spawn()
            .insert(Line)
            .insert(Position::from_points(&self.points))
            .insert(Size::from_points(&self.points))
            .insert(self.angle)
            .insert(StrokeColor(self.stroke_color))
            .insert(Opacity(0.0))
            .insert(PathCompletion(0.0))
            .insert(Line::path(&self.points))
            .id();

        id.into()
    }
    pub fn show(&mut self) -> LineId {
        let id = self.make();
        let animations = EntityAnimations {
            entity: id.into(),
            animations: vec![Animation::to(Opacity(1.0)).into()],
        };

        AnimBuilder::new(self.scene, animations.into()).run_time(0.0);

        id
    }
}

pub fn draw_line(
    draw: NonSend<nannou::Draw>,
    query: Query<
        (
            &PathCompletion,
            &Position,
            &Angle,
            &StrokeColor,
            &Opacity,
            &Size,
            &Path,
        ),
        With<Line>,
    >,
) {
    for (completion, position, angle, stroke_color, alpha, size, path) in query.iter() {
        if alpha.is_visible() {
            // let path = rectangle_path(size, completion);
            let stroke = Rgba {
                color: stroke_color.0,
                alpha: alpha.0,
            };

            // Draw fill first
            // draw.path()
            //     .fill()
            //     .x_y(position.x, position.y)
            //     .z_degrees(angle.0)
            //     .color(fill)
            //     .events(&path.clone().upto(completion.0, 0.01).0);

            // Draw stroke on top
            draw.path()
                .stroke()
                .x_y(position.x, position.y)
                .z_degrees(angle.0)
                .color(stroke)
                .caps_round()
                .stroke_weight((size.width.max(size.height) / 100.0).min(3.0))
                .events(&path.clone().upto(completion.0, 0.01).0);

            // draw.rect()
            //     .x_y(position.x, position.y)
            //     .w_h(size.width, size.height)
            //     .z_degrees(angle.0)
            //     .color(fill)
            //     .stroke_color(stroke)
            //     .stroke_weight(size.width.min(size.height) / 35.0);
        }
    }
}

pub fn line(scene: &mut Scene) -> LineBuilder {
    LineBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct LineId(pub(crate) Entity);

impl WithColor for LineId {}
impl WithPath for LineId {}
impl WithPosition for LineId {}
impl WithAngle for LineId {}

impl WithId for LineId {
    fn id(&self) -> Entity {
        self.0
    }
}

impl From<LineId> for Entity {
    fn from(id: LineId) -> Self {
        id.0
    }
}

impl From<Entity> for LineId {
    fn from(id: Entity) -> Self {
        LineId(id)
    }
}
