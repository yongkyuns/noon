use super::common::*;
use core::f32::consts::TAU;
use nannou::lyon::math::{Angle, Vector};

/// Component indicating a circle. Other [Component]s belonging to a circle
/// is implemented in [CircleBuilder].
#[derive(Component)]
pub struct Circle;

impl Circle {
    /// Returns path for a circle.
    fn path(size: &Size) -> Path {
        let size = size.into_pxl_scale();
        let radius = size.width / 2.0;
        let mut builder = Path::svg_builder();
        let sweep_angle = Angle::radians(TAU);
        let x_rotation = Angle::radians(0.0);
        let center = point(0.0, 0.0);
        let start = point(radius, 0.0);
        let radii = Vector::new(radius, radius);

        builder.move_to(start);
        builder.arc(center, radii, sweep_angle, x_rotation);
        builder.close();

        Path::new(builder.build(), true)
    }
}

pub struct CircleBuilder<'a> {
    radius: f32,
    stroke_weight: StrokeWeight,
    stroke_color: Color,
    fill_color: Color,
    position: Position,
    scene: &'a mut Scene,
}

impl<'a> CircleBuilder<'a> {
    fn new(scene: &'a mut Scene) -> Self {
        let fill_color = Color::random();
        Self {
            radius: 0.5,
            stroke_weight: StrokeWeight::THICK,
            fill_color,
            stroke_color: fill_color.brighten(),
            position: Default::default(),
            scene,
        }
    }
    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
    pub fn with_color(mut self, color: Color) -> Self {
        self.fill_color = color;
        self.stroke_color = color.brighten();
        self
    }
}

crate::stroke_builder!(CircleBuilder);
crate::position_builder!(CircleBuilder);
crate::fill_builder!(CircleBuilder);

impl Create<CircleId> for CircleBuilder<'_> {
    fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }
    fn make(&mut self) -> CircleId {
        let depth = self.scene.increment_counter();
        let world = &mut self.scene.world;
        let id = world
            .spawn()
            .insert(Circle)
            .insert(BoundingSize(Size::from_radius(self.radius)))
            .insert(Size::from_radius(self.radius))
            .insert(Previous(Size::from_radius(self.radius)))
            .insert(self.position)
            .insert(FillColor(self.fill_color))
            .insert(StrokeColor(self.stroke_color))
            .insert(self.stroke_weight)
            .insert(Opacity(0.0))
            .insert(depth)
            .insert(PathCompletion(0.0))
            .insert(Circle::path(&Size::from_radius(self.radius)))
            .id();

        id.into()
    }
}

/// [System] for rendering circles.
pub fn draw_circle(
    draw: NonSend<nannou::Draw>,
    query: Query<
        (
            &PathCompletion,
            &Position,
            &StrokeColor,
            &StrokeWeight,
            &FillColor,
            &Opacity,
            &Size,
            &Path,
            &Depth,
        ),
        With<Circle>,
    >,
) {
    for (completion, position, stroke_color, stroke_weight, fill_color, alpha, size, path, depth) in
        query.iter()
    {
        if alpha.is_visible() {
            // println!("Circle size = {}, {}", size.width, size.height);
            let size = size.into_pxl_scale();
            let position = position.into_pxl_scale();

            let radius = size.width / 2.0;
            // let path = circle_path(size, completion);

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
                .color(fill)
                .events(&path.clone().upto(completion.0, EPS).raw);

            // Draw stroke on top
            if !stroke_weight.is_none() {
                let thickness = if stroke_weight.is_auto() {
                    radius / 30.0
                } else {
                    stroke_weight.0
                };
                draw.path()
                    .stroke()
                    .x_y(position.x, position.y)
                    .z(depth.0)
                    .color(stroke)
                    .stroke_weight(thickness)
                    .events(&path.clone().upto(completion.0, EPS).raw);
            }
        }
    }
}

pub fn circle(scene: &mut Scene) -> CircleBuilder {
    CircleBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct CircleId(pub(crate) Entity);

impl WithStroke for CircleId {}
impl WithSize for CircleId {}
impl WithFill for CircleId {}
impl WithColor for CircleId {}
impl WithPath for CircleId {}
impl WithPosition for CircleId {}
impl WithStrokeWeight for CircleId {}

impl CircleId {
    pub fn set_radius(&self, radius: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: Animation::to(Size::from_radius(radius)).into(),
        }
    }
    pub fn set_radius_from(&self, entity: impl Into<Entity>) -> EntityAnimations {
        EntityAnimations {
            entity: self.0,
            animations: Animation::<Size>::to_target(entity.into()).into(),
        }
    }
}

impl WithId for CircleId {
    fn id(&self) -> Entity {
        self.0
    }
}

impl Into<Entity> for CircleId {
    fn into(self) -> Entity {
        self.0
    }
}

impl From<Entity> for CircleId {
    fn from(id: Entity) -> Self {
        CircleId(id)
    }
}
