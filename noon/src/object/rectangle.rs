use super::common::*;

#[derive(Component)]
pub struct Rectangle;

impl Rectangle {
    fn path(size: &Size) -> Path {
        let size = size.into_pxl_scale();
        let mut builder = Path::svg_builder();
        // let start = point(-size.width / 2.0, size.height / 2.0);

        // builder.move_to(start);
        // builder.line_to(point(start.x + size.width, start.y));
        // builder.line_to(point(start.x + size.width, start.y - size.height));
        // builder.line_to(point(start.x, start.y - size.height));
        // builder.line_to(point(start.x, start.y));
        // builder.close();

        let start = point(size.width / 2.0, 0.0);

        builder.move_to(start);
        builder.line_to(point(start.x, start.y + size.height / 2.0));
        builder.line_to(point(start.x - size.width, start.y + size.height / 2.0));
        builder.line_to(point(start.x - size.width, start.y - size.height / 2.0));
        builder.line_to(point(start.x, start.y - size.height / 2.0));
        builder.line_to(point(start.x, 0.0));
        builder.close();

        Path::new(builder.build(), true)
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
        let fill_color = Color::random();
        Self {
            size: Size {
                width: 1.0,
                height: 1.0,
            },
            stroke_weight: StrokeWeight::THICK,
            fill_color,
            stroke_color: fill_color.brighten(),
            position: Default::default(),
            angle: Default::default(),
            scene,
        }
    }
    pub fn with_color(mut self, color: Color) -> Self {
        self.fill_color = color;
        self.stroke_color = color.brighten();
        self
    }
}

crate::stroke_builder!(RectangleBuilder);
crate::position_builder!(RectangleBuilder);
crate::fill_builder!(RectangleBuilder);
crate::size_builder!(RectangleBuilder);

impl Create<RectangleId> for RectangleBuilder<'_> {
    fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }
    fn make(&mut self) -> RectangleId {
        let depth = self.scene.increment_counter();
        let world = &mut self.scene.world;
        let id = world
            .spawn()
            .insert(Rectangle)
            .insert(self.size)
            .insert(Previous(self.size))
            .insert(self.position)
            .insert(self.angle)
            .insert(self.stroke_weight)
            .insert(StrokeColor(self.stroke_color))
            .insert(FillColor(self.fill_color))
            .insert(Opacity(0.0))
            .insert(depth)
            .insert(PathCompletion(0.0))
            .insert(Rectangle::path(&self.size))
            .insert(Transform::identity())
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
            &Depth,
        ),
        With<Rectangle>,
    >,
) {
    for (
        completion,
        position,
        angle,
        stroke_color,
        stroke_weight,
        fill_color,
        alpha,
        size,
        path,
        depth,
    ) in query.iter()
    {
        if alpha.is_visible() {
            let position = position.into_pxl_scale();
            let size = size.into_pxl_scale();

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
                .z(depth.0)
                .z_degrees(angle.0)
                .color(fill)
                .events(&path.clone().upto(completion.0, EPS).raw);

            // Draw stroke on top
            if !stroke_weight.is_none() {
                let thickness = if stroke_weight.is_auto() {
                    size.width.max(size.height) / 100.0
                } else {
                    stroke_weight.0
                };
                draw.path()
                    .stroke()
                    .x_y(position.x, position.y)
                    .z(depth.0)
                    .z_degrees(angle.0)
                    .join_round()
                    .color(stroke)
                    .stroke_weight(thickness)
                    .events(&path.clone().upto(completion.0, EPS).raw);
            }
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
impl WithStrokeWeight for RectangleId {}

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
