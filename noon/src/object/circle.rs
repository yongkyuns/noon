use super::common::*;
use crate::Angle;
use core::f32::consts::TAU;
use nannou::lyon::math::Vector;

/// Component indicating a circle. Other [Component]s belonging to a circle
/// is implemented in [CircleBuilder].
#[derive(Component)]
pub struct Circle;

impl Circle {
    /// Returns path for a circle.
    fn path(size: &Size) -> Path {
        let radius = size.width / 2.0;
        let mut builder = Path::svg_builder();
        let sweep_angle = nannou::lyon::math::Angle::radians(TAU);
        let x_rotation = nannou::lyon::math::Angle::radians(0.0);
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
    angle: Angle,
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
            angle: Default::default(),
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

crate::angle_builder!(CircleBuilder);
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
        let position = self.position;
        let scale = Scale::ONE;
        let path = Circle::path(&Size::from_radius(self.radius));
        let transform = Transform::identity()
            .scale(scale)
            .rotate(self.angle)
            .translate(position.into());
        let screen_transform = self.scene.transform;

        let pixel_path = PixelPath(
            path.clone()
                .transform(&transform.transform(screen_transform)),
        );

        let id = world
            .spawn()
            .insert(Circle)
            .insert(Size::from_radius(self.radius))
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
            .insert(pixel_path)
            .insert(transform)
            .insert(HasFill(true))
            .id();

        id.into()
    }
}

pub fn circle(scene: &mut Scene) -> CircleBuilder {
    CircleBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct CircleId(pub(crate) Entity);
crate::into_entity!(CircleId);

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
