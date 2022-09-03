use bevy_ecs::prelude::*;
use nannou::geom::Rect;

use crate::component::FillColor;
use crate::prelude::*;
use crate::system::*;
use crate::Depth;
use crate::Scale;
use crate::Transform;
use crate::{
    circle, empty, line, rectangle, text, Angle, EmptyBuilder, FontSize, LineBuilder, Opacity,
    Path, PathCompletion, Position, RectangleBuilder, Size, StrokeColor,
};

#[derive(Debug)]
pub struct Bounds(pub(crate) Rect);

impl Bounds {
    pub fn new(rect: Rect) -> Self {
        let x = rect.x() / ZOOM;
        let y = rect.y() / ZOOM;
        let w = rect.w() / ZOOM;
        let h = rect.h() / ZOOM;
        Self(Rect::from_x_y_w_h(x, y, w, h))
    }
    pub fn none() -> Self {
        Self(Rect::from_w_h(0.0, 0.0))
    }
    pub fn edge_upper(&self) -> f32 {
        self.0.y.end
    }
    pub fn edge_lower(&self) -> f32 {
        self.0.y.start
    }
    pub fn edge_left(&self) -> f32 {
        self.0.x.start
    }
    pub fn edge_right(&self) -> f32 {
        self.0.x.end
    }
    pub fn get_edge(&self, now: Position, direction: Direction) -> Position {
        let x = now.x.min(self.edge_right()).max(self.edge_left());
        let y = now.y.min(self.edge_upper()).max(self.edge_lower());

        match direction {
            Direction::Up => Position {
                x,
                y: self.edge_upper(),
            },
            Direction::Down => Position {
                x,
                y: self.edge_lower(),
            },
            Direction::Left => Position {
                x: self.edge_left(),
                y,
            },
            Direction::Right => Position {
                x: self.edge_right(),
                y,
            },
        }
    }
    /// Provide a reduced [Bounds] from given [Size]
    pub fn reduced_by(&self, size: &Size) -> Self {
        let x_pad = size.width / 2.0;
        let y_pad = size.height / 2.0;
        Self(
            self.0
                .clone()
                .pad_bottom(y_pad)
                .pad_top(y_pad)
                .pad_left(x_pad)
                .pad_right(x_pad),
        )
    }
}

pub struct Scene {
    pub(crate) world: World,
    pub(crate) updater: Schedule,
    pub(crate) drawer: Schedule,
    pub(crate) event_time: f32,
    pub(crate) clock_time: f32,
    pub(crate) creation_count: u32,
    pub(crate) transform: Transform,
}

impl Scene {
    pub fn new(window: Rect) -> Self {
        let mut world = World::new();
        let transform = Transform::identity().scale(Scale::new(ZOOM, ZOOM));
        let bounds = Bounds::new(window);
        world.insert_resource(Time::default());
        world.insert_resource(bounds);
        world.insert_resource(transform);

        // world.init_component::<Animation<Position>>();

        let mut updater = Schedule::default();
        updater.add_stage(
            "update",
            SystemStage::parallel()
                // .with_system(update_previous::<Size>.before(Label::Init))
                .with_system(group.before(Label::Init))
                .with_system(arrange.label(Label::Init))
                // Beginnning of Main systems
                .with_system(init_from_target::<Position>.label(Label::Init))
                .with_system(init_from_target::<FillColor>.label(Label::Init))
                .with_system(init_from_target::<StrokeColor>.label(Label::Init))
                .with_system(init_from_target::<StrokeWeight>.label(Label::Init))
                .with_system(init_from_target::<Size>.label(Label::Init))
                .with_system(init_from_target::<Scale>.label(Label::Init))
                .with_system(init_from_target::<Angle>.label(Label::Init))
                .with_system(init_from_target::<Opacity>.label(Label::Init))
                .with_system(init_from_target::<PathCompletion>.label(Label::Init))
                .with_system(init_from_target::<FontSize>.label(Label::Init))
                // Begin main animation updates
                .with_system(animate_position.after(Label::Init).label(Label::Main))
                .with_system(animate::<FillColor>.after(Label::Init).label(Label::Main))
                .with_system(animate::<StrokeColor>.after(Label::Init).label(Label::Main))
                .with_system(
                    animate::<StrokeWeight>
                        .after(Label::Init)
                        .label(Label::Main),
                )
                .with_system(
                    animate_with_multiply::<Size>
                        .after(Label::Init)
                        .label(Label::Main),
                )
                .with_system(
                    animate_with_multiply::<Scale>
                        .after(Label::Init)
                        .label(Label::Main),
                )
                .with_system(
                    animate_with_relative::<Angle>
                        .after(Label::Init)
                        .label(Label::Main),
                )
                .with_system(
                    animate_with_relative::<Opacity>
                        .after(Label::Init)
                        .label(Label::Main),
                )
                .with_system(
                    animate_with_relative::<PathCompletion>
                        .after(Label::Init)
                        .label(Label::Main),
                )
                .with_system(animate_with_relative::<FontSize>)
                // Post-processing
                .with_system(
                    init_from_target::<Path>
                        .after(Label::Main)
                        .label(Label::Post),
                )
                .with_system(animate::<Path>.after(Label::Main).label(Label::Post))
                .with_system(update_origin.after(Label::Post))
                .with_system(update_transform.after(Label::Post))
                .with_system(update_screen_paths.after(Label::Post))
                .with_system(print),
        );

        let mut drawer = Schedule::default();
        drawer.add_stage("draw", SystemStage::single_threaded().with_system(draw));

        Self {
            world,
            updater,
            drawer,
            event_time: 0.5,
            clock_time: 0.0,
            creation_count: 0,
            transform,
        }
    }
    /// All objects added to [Scene] has a depth value (i.e. z value)
    /// associated with it in order to identify the order of occlusion.
    /// Therefore, we keep a running counter of objects added and derive
    /// depth value from it at creation.
    pub fn increment_counter(&mut self) -> Depth {
        self.creation_count += 1;
        Depth(self.creation_count as f32 / 10.0)
    }
    pub fn circle(&mut self) -> CircleBuilder {
        circle(self)
    }
    pub fn rectangle(&mut self) -> RectangleBuilder {
        rectangle(self)
    }
    pub fn line(&mut self) -> LineBuilder {
        line(self)
    }
    pub fn text(&mut self) -> TextBuilder {
        text(self)
    }
    // pub fn group(&mut self, entities: impl Into<Vec<EntityAnimations>>) -> EmptyBuilder {
    //     let builder = empty(self);
    //     let entities:Vec<EntityAnimations>
    //     for entity in entities.into().iter() {}
    // }
    pub fn group(&mut self) -> EmptyBuilder {
        empty(self)
    }

    // pub fn group(&mut self, objects: impl Into<Vec<Entity>>) -> EmptyBuilder {
    //     let objects: Vec<Entity> = objects.into();
    //     let mut builder = empty(self);

    //     for object in objects.iter() {
    //         builder = builder.add(*object);
    //     }
    //     builder
    // }

    pub fn add_circle(&mut self, x: f32, y: f32) {
        let c = circle(self)
            .with_position(x, y)
            .with_radius(0.2)
            .with_color(Color::random())
            .make();
        let t = self.clock_time;
        self.play(c.show_creation()).start_time(t).run_time(0.1);
    }

    pub fn update(&mut self, now: f32, win_rect: Rect) {
        // let now = self.clock_time;
        self.world
            .get_resource_mut::<Time>()
            .map(|mut t| t.seconds = now);
        self.world
            .get_resource_mut::<Bounds>()
            .map(|mut bounds| *bounds = Bounds::new(win_rect));

        self.updater.run(&mut self.world);
        // self.clock_time += 1. / 60.;
        self.clock_time = now;
    }

    pub fn draw(&mut self, nannou_draw: nannou::Draw) {
        // use nannou::glam::{Mat4, Vec3};
        self.world.remove_non_send::<nannou::Draw>();
        self.world.insert_non_send(
            nannou_draw
                // .transform(Mat4::from_scale(Vec3::new(TO_PXL, TO_PXL, 1.0)))
                .clone(),
        );
        self.drawer.run(&mut self.world);
    }

    pub fn wait(&mut self) {
        self.event_time += 1.0;
    }

    pub fn wait_for(&mut self, time: f32) {
        self.event_time += time;
    }

    pub fn play(&mut self, animations: impl Into<Vec<EntityAnimations>>) -> AnimBuilder {
        AnimBuilder::new(self, animations.into())
    }
}
