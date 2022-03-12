use super::common::*;

#[derive(Component)]
pub struct Rectangle;

impl Rectangle {
    fn path(size: &Size) -> Path {
        let mut builder = Path::svg_builder();
        let start = point(-size.width / 2.0, size.height / 2.0);

        builder.move_to(start);
        builder.line_to(point(start.x + size.width, start.y));
        builder.line_to(point(start.x + size.width, start.y - size.height));
        builder.line_to(point(start.x, start.y - size.height));
        builder.line_to(point(start.x, start.y));
        builder.close();

        Path::new(builder.build())
    }
}

pub struct RectangleBuilder<'a> {
    size: Size,
    stroke_weight: StrokeWeight,
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
            stroke_weight: StrokeWeight::AUTO,
            stroke_color: Default::default(),
            fill_color: Default::default(),
            position: Default::default(),
            angle: Default::default(),
            scene,
        }
    }
    pub fn with_stroke_weight(mut self, weight: f32) -> Self {
        self.stroke_weight = StrokeWeight(weight);
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
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.size = Size::from(width, height);
        self
    }
    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.position = Position { x, y };
        self
    }
}

impl Create<RectangleId> for RectangleBuilder<'_> {
    fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }
    fn make(&mut self) -> RectangleId {
        let world = &mut self.scene.world;
        let id = world
            .spawn()
            .insert(Rectangle)
            .insert(self.size)
            .insert(self.position)
            .insert(self.angle)
            .insert(self.stroke_weight)
            .insert(StrokeColor(self.stroke_color))
            .insert(FillColor(self.fill_color))
            .insert(Opacity(0.0))
            .insert(PathCompletion(0.0))
            .insert(Rectangle::path(&self.size))
            .id();

        id.into()
    }
}

pub fn draw_rectangle(
    draw: NonSend<nannou::Draw>,
    query: Query<
        (
            &PathCompletion,
            &Position,
            &Angle,
            &StrokeColor,
            &StrokeWeight,
            &FillColor,
            &Opacity,
            &Size,
            &Path,
        ),
        With<Rectangle>,
    >,
) {
    for (completion, position, angle, stroke_color, stroke_width, fill_color, alpha, size, path) in
        query.iter()
    {
        if alpha.is_visible() {
            // let path = rectangle_path(size, completion);
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
                .events(&path.clone().upto(completion.0, EPS).raw);

            // Draw stroke on top
            // draw.path()
            //     .stroke()
            //     .x_y(position.x, position.y)
            //     .z_degrees(angle.0)
            //     .join_round()
            //     .color(stroke)
            //     .stroke_weight(size.width.max(size.height) / 100.0)
            //     .events(&path.clone().upto(completion.0, 0.01).raw);

            if !stroke_width.is_none() {
                let thickness = if stroke_width.is_auto() {
                    size.width.max(size.height) / 100.0
                } else {
                    stroke_width.0
                };
                draw.path()
                    .stroke()
                    .x_y(position.x, position.y)
                    .z_degrees(angle.0)
                    .join_round()
                    .color(stroke)
                    .stroke_weight(thickness)
                    .events(&path.clone().upto(completion.0, EPS).raw);
            }

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
