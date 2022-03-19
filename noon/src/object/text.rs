use super::common::*;

#[derive(Component)]
pub struct Text;

impl Text {
    fn path(text: &str, font_size: u32) -> (Path, Size) {
        use nannou::text;
        use nannou::{geom::Rect, lyon::geom};

        let mut builder = Path::builder();

        let rect = nannou::geom::Rect::from_w_h(1.0, 1.0);
        let text = text::text(text)
            .font_size(font_size)
            .no_line_wrap()
            .left_justify()
            .build(rect);

        for e in text.path_events() {
            builder.path_event(e);
        }

        let rect = text.bounding_rect();
        let x = -rect.x();
        let y = -rect.y();
        // let scale = 1.0 / rect.w().max(rect.h()) * width;
        let scale = 0.01;

        (
            Path::new(
                builder
                    .build()
                    .transformed(&geom::Transform::translation(x, y).then_scale(scale, scale)),
                true,
            ),
            Size::from(rect.w(), rect.h()),
        )
    }
}

pub struct TextBuilder<'a> {
    text: String,
    font_size: u32,
    stroke_weight: StrokeWeight,
    stroke_color: Color,
    fill_color: Color,
    position: Position,
    angle: Angle,
    scene: &'a mut Scene,
}

impl<'a> TextBuilder<'a> {
    fn new(scene: &'a mut Scene) -> Self {
        let fill_color = Color::random();
        Self {
            text: String::new(),
            font_size: 30,
            stroke_weight: StrokeWeight::THIN,
            fill_color,
            stroke_color: fill_color.brighten(),
            position: Default::default(),
            angle: Default::default(),
            scene,
        }
    }
    pub fn with_text(mut self, text: &str) -> Self {
        self.text = text.to_owned();
        self
    }
    pub fn with_color(mut self, color: Color) -> Self {
        self.fill_color = color;
        self.stroke_color = color.brighten();
        self
    }
    pub fn with_font_size(mut self, size: u32) -> Self {
        self.font_size = size;
        self
    }
}

crate::stroke_builder!(TextBuilder);
crate::position_builder!(TextBuilder);
crate::fill_builder!(TextBuilder);

impl Create<TextId> for TextBuilder<'_> {
    fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }
    fn make(&mut self) -> TextId {
        let depth = self.scene.increment_counter();
        let world = &mut self.scene.world;
        let position = self.position;
        let scale = Scale::ONE;
        let (path, size) = Text::path(&self.text, self.font_size);
        let transform = Transform::identity()
            .scale(scale)
            .rotate(self.angle)
            .translate(position.into());
        let screen_transform = self.scene.transform;

        let global_path = PixelPath(
            path.clone()
                .transform(&transform.transform(screen_transform)),
        );
        let id = world
            .spawn()
            .insert(Text)
            .insert(FontSize(self.font_size))
            .insert(size)
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
            .id();

        id.into()
    }
}

pub fn draw_text(
    draw: NonSend<nannou::Draw>,
    query: Query<
        (
            &StrokeColor,
            &StrokeWeight,
            &FillColor,
            &Opacity,
            &PixelPath,
            &Depth,
            &FontSize,
        ),
        With<Text>,
    >,
) {
    for (stroke_color, stroke_weight, fill_color, alpha, path, depth, font_size) in query.iter() {
        if alpha.is_visible() {
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
                .z(depth.0)
                .color(fill)
                .events(&path.0.raw);

            if !stroke_weight.is_none() {
                let thickness = if stroke_weight.is_auto() {
                    font_size.0 as f32 / 80.0
                } else {
                    stroke_weight.0
                };
                draw.path()
                    .stroke()
                    .z(depth.0)
                    .color(stroke)
                    .stroke_weight(thickness)
                    .events(&path.0.raw);
            }
        }
    }
}

pub fn text(scene: &mut Scene) -> TextBuilder {
    TextBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct TextId(pub(crate) Entity);

impl WithFontSize for TextId {}
impl WithStroke for TextId {}
impl WithFill for TextId {}
impl WithColor for TextId {}
impl WithPath for TextId {}
impl WithPosition for TextId {}
impl WithAngle for TextId {}
impl WithSize for TextId {}

impl WithId for TextId {
    fn id(&self) -> Entity {
        self.0
    }
}

impl From<TextId> for Entity {
    fn from(id: TextId) -> Self {
        id.0
    }
}

impl From<Entity> for TextId {
    fn from(id: Entity) -> Self {
        TextId(id)
    }
}
