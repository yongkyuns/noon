use bevy_ecs::prelude::*;
use nannou::geom::Rect;
use std::cell::RefCell;

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

#[derive(Debug, Resource)]
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
    pub(crate) world: RefCell<World>,
    pub(crate) updater: RefCell<Schedule>,
    pub(crate) drawer: RefCell<Schedule>,
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

        let mut updater = Schedule::default();

        updater.configure_sets((
            Label::Init,
            Label::Main.after(Label::Init),
            Label::Post.after(Label::Main),
        ));

        updater.add_systems((
            (
                init_from_target::<Position>,
                init_from_target::<FillColor>,
                init_from_target::<StrokeColor>,
                init_from_target::<StrokeWeight>,
                init_from_target::<Size>,
                init_from_target::<Scale>,
                init_from_target::<Angle>,
                init_from_target::<Opacity>,
                init_from_target::<PathCompletion>,
                init_from_target::<FontSize>,
            )
                .in_set(Label::Init),
            (
                animate_position,
                animate::<FillColor>,
                animate::<StrokeColor>,
                animate::<StrokeWeight>,
                animate_with_multiply::<Size>,
                animate_with_multiply::<Scale>,
                animate_with_relative::<Angle>,
                animate_with_relative::<Opacity>,
                animate_with_relative::<PathCompletion>,
                animate_with_relative::<FontSize>,
            )
                .in_set(Label::Main),
            (init_from_target::<Path>, animate::<Path>).in_set(Label::Post),
            update_screen_paths.after(Label::Post),
            print,
        ));

        let mut drawer = Schedule::default();
        drawer.add_systems(draw);

        Self {
            world: RefCell::new(world),
            updater: RefCell::new(updater),
            drawer: RefCell::new(drawer),
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
        let mut world = self.world.borrow_mut();
        world
            .get_resource_mut::<Time>()
            .map(|mut t| t.seconds = now);
        world
            .get_resource_mut::<Bounds>()
            .map(|mut bounds| *bounds = Bounds::new(win_rect));

        self.updater.borrow_mut().run(&mut world);
        // self.clock_time += 1. / 60.;
        self.clock_time = now;
    }

    pub fn draw(&self, nannou_draw: nannou::Draw) {
        // use nannou::glam::{Mat4, Vec3};
        let mut world = self.world.borrow_mut();
        world.insert_non_send_resource(
            nannou_draw
                // .transform(Mat4::from_scale(Vec3::new(TO_PXL, TO_PXL, 1.0)))
                .clone(),
        );
        self.drawer.borrow_mut().run(&mut world);
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
