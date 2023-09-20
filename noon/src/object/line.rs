use super::common::*;
use nannou::lyon::path::traits::PathBuilder;

#[derive(Component)]
pub struct Line;

impl Line {
    fn path(points: &[Point]) -> (Path, Position) {
        let centroid = Position::from_points(&points);

        let mut builder = Path::builder();
        builder.begin(
            *points
                .get(0)
                .expect("Attempted to create a line with 0 points"),
        );
        for &p in points.iter().skip(1) {
            builder.line_to(p);
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
            centroid,
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

        let scale = Scale::ONE;
        let (path, position) = Line::path(&self.points);
        let transform = Transform::identity()
            .scale(scale)
            .rotate(self.angle)
            .translate(position.into());
        let screen_transform = self.scene.transform;
        let size = Size::from_points(&self.points);

        let global_path = PixelPath(
            path.clone()
                .transform(&transform.transform(screen_transform)),
        );

        let id = world
            .spawn_empty()
            .insert(Line)
            .insert(size)
            .insert(scale)
            .insert(position)
            .insert(self.angle)
            .insert(self.stroke_weight)
            .insert(StrokeColor(self.stroke_color))
            .insert(Opacity(0.0))
            .insert(depth)
            .insert(PathCompletion(0.0))
            .insert(path)
            .insert(global_path)
            .insert(transform)
            .insert(FillColor(Color::BLACK))
            .insert(HasFill(false))
            .id();

        id.into()
    }
}

pub fn line(scene: &mut Scene) -> LineBuilder {
    LineBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct LineId(pub(crate) Entity);
crate::into_entity!(LineId);

impl WithColor for LineId {}
impl WithPath for LineId {}
impl WithPosition for LineId {}
impl WithAngle for LineId {}
impl WithStrokeWeight for LineId {}
