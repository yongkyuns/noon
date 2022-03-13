use super::common::*;
use nannou::lyon::path::traits::PathBuilder;

#[derive(Component)]
pub struct Line;

impl Line {
    fn path(points: &[Point]) -> Path {
        let mut builder = Path::builder();

        builder.begin(points.get(0).unwrap().into_pxl_scale());
        for &p in points.iter() {
            builder.line_to(p.into_pxl_scale());
        }
        builder.end(false);
        Path {
            raw: builder.build(),
            closed: false,
        }
    }
}

pub struct LineBuilder<'a> {
    points: Vec<Point>,
    stroke_color: Color,
    stroke_weight: StrokeWeight,
    angle: Angle,
    scene: &'a mut Scene,
}

impl<'a> LineBuilder<'a> {
    fn new(scene: &'a mut Scene) -> Self {
        Self {
            points: Vec::new(),
            stroke_weight: StrokeWeight::THICK,
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
}

impl Create<LineId> for LineBuilder<'_> {
    fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }
    fn make(&mut self) -> LineId {
        let world = &mut self.scene.world;
        let id = world
            .spawn()
            .insert(Line)
            .insert(Position::from_points(&self.points))
            .insert(Size::from_points(&self.points))
            .insert(self.angle)
            .insert(self.stroke_weight)
            .insert(StrokeColor(self.stroke_color))
            .insert(Opacity(0.0))
            .insert(PathCompletion(0.0))
            .insert(Line::path(&self.points))
            .id();

        id.into()
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
            &StrokeWeight,
            &Opacity,
            &Size,
            &Path,
        ),
        With<Line>,
    >,
) {
    for (completion, position, angle, stroke_color, stroke_weight, alpha, size, path) in
        query.iter()
    {
        if alpha.is_visible() {
            let position = position.into_pxl_scale();
            let size = size.into_pxl_scale();

            // let path = rectangle_path(size, completion);
            let stroke = Rgba {
                color: stroke_color.0,
                alpha: alpha.0,
            };

            // Draw stroke
            if !stroke_weight.is_none() {
                let thickness = if stroke_weight.is_auto() {
                    (size.width.max(size.height) / 100.0).min(3.0)
                } else {
                    stroke_weight.0
                };
                draw.path()
                    .stroke()
                    .x_y(position.x, position.y)
                    .z_degrees(angle.0)
                    .color(stroke)
                    .caps_round()
                    .stroke_weight(thickness)
                    .events(&path.clone().upto(completion.0, 0.01).raw);
            }
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
impl WithStrokeWeight for LineId {}

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
