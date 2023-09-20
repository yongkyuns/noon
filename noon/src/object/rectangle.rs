use super::common::*;

#[derive(Component)]
pub struct Rectangle;

impl Rectangle {
    fn path(size: &Size) -> Path {
        let mut builder = Path::svg_builder();
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
            size: Size::UNIT,
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

crate::angle_builder!(RectangleBuilder);
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
        let scale = Scale::ONE;
        let path = Rectangle::path(&self.size);
        let transform = Transform::identity()
            .scale(scale)
            .rotate(self.angle)
            .translate(self.position.into());
        let screen_transform = self.scene.transform;

        let global_path = PixelPath(
            path.clone()
                .transform(&transform.transform(screen_transform)),
        );

        let id = world
            .spawn_empty()
            .insert(Rectangle)
            .insert(self.size)
            .insert(scale)
            .insert(self.position)
            .insert(self.angle)
            .insert(self.stroke_weight)
            .insert(StrokeColor(self.stroke_color))
            .insert(FillColor(self.fill_color))
            .insert(Opacity(0.0))
            .insert(depth)
            .insert(PathCompletion(0.0))
            .insert(path)
            .insert(global_path)
            .insert(transform)
            .insert(HasFill(true))
            .id();

        id.into()
    }
}

pub fn rectangle(scene: &mut Scene) -> RectangleBuilder {
    RectangleBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct RectangleId(pub(crate) Entity);
crate::into_entity!(RectangleId);

impl WithStroke for RectangleId {}
impl WithFill for RectangleId {}
impl WithColor for RectangleId {}
impl WithPath for RectangleId {}
impl WithPosition for RectangleId {}
impl WithAngle for RectangleId {}
impl WithSize for RectangleId {}
impl WithStrokeWeight for RectangleId {}
