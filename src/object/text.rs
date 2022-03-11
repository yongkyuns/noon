use super::common::*;

#[derive(Component)]
pub struct Text;

impl Text {
    fn path(text: &str, font_size: FontSize) -> Path {
        let mut builder = Path::svg_builder();
        // let start = point(-size.width / 2.0, size.height / 2.0);

        let rect = nannou::geom::Rect::from_w_h(10.0, 10.0);
        let text = nannou::text::text(text)
            .font_size(font_size.0)
            .no_line_wrap()
            .left_justify()
            .build(rect);

        let mut builder = Path::builder();
        for e in text.path_events() {
            builder.path_event(e);
        }
        // builder.close();

        // let bbox = text.bounding_rect();
        // draw.rect()
        //     .x_y(bbox.x() + self.position().x, bbox.y() + self.position().y)
        //     .z_degrees(self.orientation)
        //     .w_h(self.width(), self.height())
        //     .color(RED_D);

        // for (_glyph, rect) in text.glyphs() {
        //     draw.rect()
        //         .x_y(rect.x() + self.position.x, rect.y() + self.position.y)
        //         .wh(rect.wh())
        //         .hsla(0.5, 1.0, 0.5, 0.5);
        // }

        // builder.move_to(start);
        // builder.line_to(point(start.x + size.width, start.y));
        // builder.line_to(point(start.x + size.width, start.y - size.height));
        // builder.line_to(point(start.x, start.y - size.height));
        // builder.line_to(point(start.x, start.y));
        // builder.close();

        Path::new(builder.build())
    }
}

pub struct TextBuilder<'a> {
    text: String,
    font_size: FontSize,
    stroke_color: Color,
    fill_color: Color,
    position: Position,
    angle: Angle,
    scene: &'a mut Scene,
}

impl<'a> TextBuilder<'a> {
    fn new(scene: &'a mut Scene) -> Self {
        Self {
            text: String::new(),
            font_size: FontSize(90),
            stroke_color: Default::default(),
            fill_color: Default::default(),
            position: Default::default(),
            angle: Default::default(),
            scene,
        }
    }
    pub fn with_text(mut self, text: &str) -> Self {
        self.text = text.to_owned();
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
    pub fn with_font_size(mut self, size: u32) -> Self {
        self.font_size = FontSize(size);
        self
    }
    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.position = Position { x, y };
        self
    }
    pub fn make(&mut self) -> TextId {
        let world = &mut self.scene.world;
        let id = world
            .spawn()
            .insert(Text)
            .insert(self.font_size)
            .insert(self.position)
            .insert(self.angle)
            .insert(StrokeColor(self.stroke_color))
            .insert(FillColor(self.fill_color))
            .insert(Opacity(0.0))
            .insert(PathCompletion(0.0))
            .insert(Text::path(&self.text, self.font_size))
            .id();

        id.into()
    }
    pub fn show(&mut self) -> TextId {
        let id = self.make();
        let animations = EntityAnimations {
            entity: id.into(),
            animations: vec![Animation::to(Opacity(1.0)).into()],
        };

        AnimBuilder::new(self.scene, animations.into()).run_time(0.0);

        id
    }
}

fn size_from_text(text: &str, font_size: FontSize) -> Size {
    let rect = nannou::geom::Rect::from_w_h(10.0, 10.0);
    let text = nannou::text::text(text)
        .font_size(font_size.0)
        .left_justify()
        .no_line_wrap()
        .build(rect);
    let bbox = text.bounding_rect();
    Size {
        width: bbox.w(),
        height: bbox.h(),
    }
}

pub fn draw_text(
    draw: NonSend<nannou::Draw>,
    query: Query<
        (
            &FontSize,
            &PathCompletion,
            &Position,
            &Angle,
            &StrokeColor,
            &FillColor,
            &Opacity,
            &Path,
        ),
        With<Text>,
    >,
) {
    for (font_size, completion, position, angle, stroke_color, fill_color, alpha, path) in
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

            // // Draw fill first
            // draw.path()
            //     .fill()
            //     .x_y(position.x, position.y)
            //     .z_degrees(angle.0)
            //     .color(fill)
            //     .events(&path.clone().upto(completion.0, 0.01).raw);

            // // Draw stroke on top
            // draw.path()
            //     .stroke()
            //     .x_y(position.x, position.y)
            //     .z_degrees(angle.0)
            //     .join_round()
            //     .color(stroke)
            //     .stroke_weight(size.width.max(size.height) / 100.0)
            //     .events(&path.clone().upto(completion.0, 0.01).raw);

            // let draw = draw
            //     .rotate(self.orientation)
            //     .x_y(self.position.x, self.position.y);

            draw.path()
                .stroke()
                .x_y(position.x, position.y)
                .z_degrees(angle.0)
                .color(stroke)
                .stroke_weight(1.0)
                .events(&path.clone().upto(completion.0, 0.01).raw);
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
