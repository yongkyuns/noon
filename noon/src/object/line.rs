use super::common::*;
use nannou::lyon::path::traits::PathBuilder;

#[derive(Component)]
pub struct Line;

impl Line {
    fn path(points: &[Point]) -> (Path, Position) {
        let centroid = Position::from_points(&points).into_pxl_scale();

        let mut builder = Path::builder();

        builder.begin(
            points
                .get(0)
                .expect("Attempted to create a line with 0 points")
                .into_pxl_scale(),
        );

        for &p in points.iter().skip(1) {
            builder.line_to(p.into_pxl_scale());
        }
        builder.end(false);

        // Translate the points so that they are centered around point (0,0)
        let path = builder
            .build()
            .transformed(&nannou::lyon::geom::Translation::new(
                -centroid.x,
                -centroid.y,
            ));

        (
            Path {
                raw: path,
                closed: false,
            },
            centroid.into_natural_scale(),
        )
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
            stroke_color: Color::random(),
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
    pub fn with_color(mut self, color: Color) -> Self {
        self.stroke_color = color;
        self
    }
}

crate::stroke_builder!(LineBuilder);

impl Create<LineId> for LineBuilder<'_> {
    fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }
    fn make(&mut self) -> LineId {
        let depth = self.scene.increment_counter();
        let world = &mut self.scene.world;

        let (path, position) = Line::path(&self.points);

        // for e in path.raw.iter() {
        //     println!("{:?}", &e);
        // }

        let id = world
            .spawn()
            .insert(Line)
            .insert(position)
            .insert(Size::from_points(&self.points))
            .insert(Previous(Size::from_points(&self.points)))
            .insert(BoundingSize(Size::from_points(&self.points)))
            .insert(self.angle)
            .insert(self.stroke_weight)
            .insert(StrokeColor(self.stroke_color))
            .insert(Opacity(0.0))
            .insert(depth)
            .insert(PathCompletion(0.0))
            .insert(path)
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
            &Depth,
        ),
        With<Line>,
    >,
) {
    for (completion, position, angle, stroke_color, stroke_weight, alpha, size, path, depth) in
        query.iter()
    {
        if alpha.is_visible() {
            // println!("{:?}", size);

            let position = position.into_pxl_scale();
            let size = size.into_pxl_scale();

            // let path = rectangle_path(size, completion);
            let stroke = Rgba {
                color: stroke_color.0,
                alpha: alpha.0,
            };

            // for e in path.raw.iter() {
            //     println!("{:?}", &e);
            // }
            // println!("\n");

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
                    .z(depth.0)
                    .z_radians(angle.0)
                    .color(stroke)
                    .caps_round()
                    .stroke_weight(thickness)
                    .events(&path.clone().upto(completion.0, 0.01).raw);
                // .events(&path.raw);
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
