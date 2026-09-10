//! Renderer-independent semantic data model for Noon.
//!
//! This crate intentionally contains no renderer, windowing, ECS, or Python
//! dependencies. Shared authoring operations mutate the [`SemanticStore`];
//! compiler/runtime crates consume typed semantic and execution contracts without
//! depending on the authoring language.

#![forbid(unsafe_code)]

mod animation;
mod graph_topology;
mod object_state;
mod publication;
mod reactive;
mod resources;
mod semantic_store;

pub use animation::*;
pub use graph_topology::*;
pub use object_state::*;
pub use publication::*;
pub use reactive::*;
pub use resources::*;
pub use semantic_store::*;

use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

macro_rules! define_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        pub struct $name(u64);

        impl $name {
            pub const fn new(raw: u64) -> Self {
                Self(raw)
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

define_id!(ObjectId);
define_id!(GeometryId);
define_id!(TrackId);
define_id!(SignalId);

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self::new(0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0);

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }

    pub fn normalized(self) -> Option<Self> {
        let length = self.length();
        (length > 0.0 && length.is_finite()).then(|| self / length)
    }

    pub fn component_mul(self, rhs: Self) -> Self {
        Self::new(self.x * rhs.x, self.y * rhs.y)
    }

    pub fn rotate(self, angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self::new(self.x * cos - self.y * sin, self.x * sin + self.y * cos)
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Neg for Vec2 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl Mul<Vec2> for f32 {
    type Output = Vec2;

    fn mul(self, rhs: Vec2) -> Self::Output {
        rhs * self
    }
}

impl Div<f32> for Vec2 {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

pub const ORIGIN: Vec2 = Vec2::ZERO;
pub const UP: Vec2 = Vec2::new(0.0, 1.0);
pub const DOWN: Vec2 = Vec2::new(0.0, -1.0);
pub const LEFT: Vec2 = Vec2::new(-1.0, 0.0);
pub const RIGHT: Vec2 = Vec2::new(1.0, 0.0);
pub const UL: Vec2 = Vec2::new(-1.0, 1.0);
pub const UR: Vec2 = Vec2::new(1.0, 1.0);
pub const DL: Vec2 = Vec2::new(-1.0, -1.0);
pub const DR: Vec2 = Vec2::new(1.0, -1.0);

pub const PI: f32 = std::f32::consts::PI;
pub const TAU: f32 = std::f32::consts::TAU;
pub const DEGREES: f32 = TAU / 360.0;

pub const SMALL_BUFF: f32 = 0.1;
pub const MED_SMALL_BUFF: f32 = 0.25;
pub const MED_LARGE_BUFF: f32 = 0.5;
pub const LARGE_BUFF: f32 = 1.0;
pub const DEFAULT_MOBJECT_TO_EDGE_BUFFER: f32 = MED_LARGE_BUFF;
pub const DEFAULT_MOBJECT_TO_MOBJECT_BUFFER: f32 = MED_SMALL_BUFF;
pub const DEFAULT_FRAME_HEIGHT: f32 = 8.0;
pub const DEFAULT_FRAME_WIDTH: f32 = DEFAULT_FRAME_HEIGHT * 16.0 / 9.0;

/// Runtime-facing 2D camera state. Camera authoring may use an ordinary semantic
/// frame object, but renderers consume only this normalized center/height contract.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera2DState {
    pub center: Vec2,
    pub height: f32,
}

impl Default for Camera2DState {
    fn default() -> Self {
        Self {
            center: ORIGIN,
            height: DEFAULT_FRAME_HEIGHT,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform2D {
    pub translation: Vec2,
    pub rotation: f32,
    pub scale: Vec2,
}

impl Transform2D {
    pub const IDENTITY: Self = Self {
        translation: Vec2::ZERO,
        rotation: 0.0,
        scale: Vec2::ONE,
    };

    pub fn transform_point(self, point: Vec2) -> Vec2 {
        point.component_mul(self.scale).rotate(self.rotation) + self.translation
    }
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl Color {
    pub const WHITE: Self = Self::from_hex(0xFFFFFF);
    pub const BLACK: Self = Self::from_hex(0x000000);
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);

    // Manim Community default palette (base names alias their C shade).
    pub const BLUE_A: Self = Self::from_hex(0xC7E9F1);
    pub const BLUE_B: Self = Self::from_hex(0x9CDCEB);
    pub const BLUE_C: Self = Self::from_hex(0x58C4DD);
    pub const BLUE_D: Self = Self::from_hex(0x29ABCA);
    pub const BLUE_E: Self = Self::from_hex(0x236B8E);
    pub const BLUE: Self = Self::BLUE_C;

    pub const TEAL_A: Self = Self::from_hex(0xACEAD7);
    pub const TEAL_B: Self = Self::from_hex(0x76DDC0);
    pub const TEAL_C: Self = Self::from_hex(0x5CD0B3);
    pub const TEAL_D: Self = Self::from_hex(0x55C1A7);
    pub const TEAL_E: Self = Self::from_hex(0x49A88F);
    pub const TEAL: Self = Self::TEAL_C;

    pub const GREEN_A: Self = Self::from_hex(0xC9E2AE);
    pub const GREEN_B: Self = Self::from_hex(0xA6CF8C);
    pub const GREEN_C: Self = Self::from_hex(0x83C167);
    pub const GREEN_D: Self = Self::from_hex(0x77B05D);
    pub const GREEN_E: Self = Self::from_hex(0x699C52);
    pub const GREEN: Self = Self::GREEN_C;

    pub const YELLOW_A: Self = Self::from_hex(0xFFF1B6);
    pub const YELLOW_B: Self = Self::from_hex(0xFFEA94);
    pub const YELLOW_C: Self = Self::from_hex(0xF7D96F);
    pub const YELLOW_D: Self = Self::from_hex(0xF4D345);
    pub const YELLOW_E: Self = Self::from_hex(0xE8C11C);
    pub const YELLOW: Self = Self::YELLOW_C;

    pub const GOLD_A: Self = Self::from_hex(0xF7C797);
    pub const GOLD_B: Self = Self::from_hex(0xF9B775);
    pub const GOLD_C: Self = Self::from_hex(0xF0AC5F);
    pub const GOLD_D: Self = Self::from_hex(0xE1A158);
    pub const GOLD_E: Self = Self::from_hex(0xC78D46);
    pub const GOLD: Self = Self::GOLD_C;

    pub const RED_A: Self = Self::from_hex(0xF7A1A3);
    pub const RED_B: Self = Self::from_hex(0xFF8080);
    pub const RED_C: Self = Self::from_hex(0xFC6255);
    pub const RED_D: Self = Self::from_hex(0xE65A4C);
    pub const RED_E: Self = Self::from_hex(0xCF5044);
    pub const RED: Self = Self::RED_C;

    pub const MAROON_A: Self = Self::from_hex(0xECABC1);
    pub const MAROON_B: Self = Self::from_hex(0xEC92AB);
    pub const MAROON_C: Self = Self::from_hex(0xC55F73);
    pub const MAROON_D: Self = Self::from_hex(0xA24D61);
    pub const MAROON_E: Self = Self::from_hex(0x94424F);
    pub const MAROON: Self = Self::MAROON_C;

    pub const PURPLE_A: Self = Self::from_hex(0xCAA3E8);
    pub const PURPLE_B: Self = Self::from_hex(0xB189C6);
    pub const PURPLE_C: Self = Self::from_hex(0x9A72AC);
    pub const PURPLE_D: Self = Self::from_hex(0x715582);
    pub const PURPLE_E: Self = Self::from_hex(0x644172);
    pub const PURPLE: Self = Self::PURPLE_C;

    pub const ORANGE: Self = Self::from_hex(0xFF862F);
    pub const PINK: Self = Self::from_hex(0xD147BD);
    pub const LIGHT_PINK: Self = Self::from_hex(0xDC75CD);

    pub const GRAY_A: Self = Self::from_hex(0xDDDDDD);
    pub const GRAY_B: Self = Self::from_hex(0xBBBBBB);
    pub const GRAY_C: Self = Self::from_hex(0x888888);
    pub const GRAY_D: Self = Self::from_hex(0x444444);
    pub const GRAY_E: Self = Self::from_hex(0x222222);
    pub const GRAY: Self = Self::GRAY_C;
    pub const GREY_A: Self = Self::GRAY_A;
    pub const GREY_B: Self = Self::GRAY_B;
    pub const GREY_C: Self = Self::GRAY_C;
    pub const GREY_D: Self = Self::GRAY_D;
    pub const GREY_E: Self = Self::GRAY_E;
    pub const GREY: Self = Self::GRAY;

    pub const fn rgb(red: f32, green: f32, blue: f32) -> Self {
        Self::rgba(red, green, blue, 1.0)
    }

    pub const fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub const fn from_hex(hex: u32) -> Self {
        Self::rgb(
            ((hex >> 16) & 0xFF) as f32 / 255.0,
            ((hex >> 8) & 0xFF) as f32 / 255.0,
            (hex & 0xFF) as f32 / 255.0,
        )
    }
}

pub const WHITE: Color = Color::WHITE;
pub const BLACK: Color = Color::BLACK;
pub const BLUE: Color = Color::BLUE;
pub const BLUE_A: Color = Color::BLUE_A;
pub const BLUE_B: Color = Color::BLUE_B;
pub const BLUE_C: Color = Color::BLUE_C;
pub const BLUE_D: Color = Color::BLUE_D;
pub const BLUE_E: Color = Color::BLUE_E;
pub const TEAL: Color = Color::TEAL;
pub const TEAL_A: Color = Color::TEAL_A;
pub const TEAL_B: Color = Color::TEAL_B;
pub const TEAL_C: Color = Color::TEAL_C;
pub const TEAL_D: Color = Color::TEAL_D;
pub const TEAL_E: Color = Color::TEAL_E;
pub const GREEN: Color = Color::GREEN;
pub const GREEN_A: Color = Color::GREEN_A;
pub const GREEN_B: Color = Color::GREEN_B;
pub const GREEN_C: Color = Color::GREEN_C;
pub const GREEN_D: Color = Color::GREEN_D;
pub const GREEN_E: Color = Color::GREEN_E;
pub const YELLOW: Color = Color::YELLOW;
pub const YELLOW_A: Color = Color::YELLOW_A;
pub const YELLOW_B: Color = Color::YELLOW_B;
pub const YELLOW_C: Color = Color::YELLOW_C;
pub const YELLOW_D: Color = Color::YELLOW_D;
pub const YELLOW_E: Color = Color::YELLOW_E;
pub const GOLD: Color = Color::GOLD;
pub const RED: Color = Color::RED;
pub const RED_A: Color = Color::RED_A;
pub const RED_B: Color = Color::RED_B;
pub const RED_C: Color = Color::RED_C;
pub const RED_D: Color = Color::RED_D;
pub const RED_E: Color = Color::RED_E;
pub const MAROON: Color = Color::MAROON;
pub const PURPLE: Color = Color::PURPLE;
pub const PURPLE_A: Color = Color::PURPLE_A;
pub const PURPLE_B: Color = Color::PURPLE_B;
pub const PURPLE_C: Color = Color::PURPLE_C;
pub const PURPLE_D: Color = Color::PURPLE_D;
pub const PURPLE_E: Color = Color::PURPLE_E;
pub const ORANGE: Color = Color::ORANGE;
pub const PINK: Color = Color::PINK;
pub const LIGHT_PINK: Color = Color::LIGHT_PINK;
pub const GRAY: Color = Color::GRAY;
pub const GREY: Color = Color::GREY;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    pub fn from_points(points: impl IntoIterator<Item = Vec2>) -> Option<Self> {
        let mut points = points.into_iter();
        let first = points.next()?;
        let mut result = Self::new(first, first);
        for point in points {
            result.include(point);
        }
        Some(result)
    }

    pub fn include(&mut self, point: Vec2) {
        self.min.x = self.min.x.min(point.x);
        self.min.y = self.min.y.min(point.y);
        self.max.x = self.max.x.max(point.x);
        self.max.y = self.max.y.max(point.y);
    }

    pub fn union(self, other: Self) -> Self {
        Self::new(
            Vec2::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            Vec2::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        )
    }

    pub fn center(self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    pub fn size(self) -> Vec2 {
        self.max - self.min
    }

    pub fn width(self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn critical_point(self, direction: Vec2) -> Vec2 {
        Vec2::new(
            if direction.x < 0.0 {
                self.min.x
            } else if direction.x > 0.0 {
                self.max.x
            } else {
                self.center().x
            },
            if direction.y < 0.0 {
                self.min.y
            } else if direction.y > 0.0 {
                self.max.y
            } else {
                self.center().y
            },
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeJoin {
    #[default]
    Round,
    Miter,
    Bevel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeCap {
    #[default]
    Round,
    Butt,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_width: f32,
    #[serde(default, skip_serializing_if = "StrokeWidthMode::is_scale_with_object")]
    pub stroke_width_mode: StrokeWidthMode,
    #[serde(default)]
    pub stroke_join: StrokeJoin,
    #[serde(default)]
    pub stroke_cap: StrokeCap,
    pub opacity: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fill: Some(Color::WHITE),
            stroke: None,
            stroke_width: 1.0,
            stroke_width_mode: StrokeWidthMode::ScaleWithObject,
            stroke_join: StrokeJoin::Round,
            stroke_cap: StrokeCap::Round,
            opacity: 1.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VectorPath {
    commands: Vec<PathCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    morph_target: Option<Box<VectorPath>>,
}

impl VectorPath {
    /// Transform every endpoint/control point, including any morph target.
    pub fn transformed(&self, transform: Transform2D) -> Self {
        let mut result = Self::new();
        for command in self.commands() {
            result = match *command {
                PathCommand::MoveTo { to } => result.move_to(transform.transform_point(to)),
                PathCommand::LineTo { to } => result.line_to(transform.transform_point(to)),
                PathCommand::QuadraticTo { control, to } => result.quadratic_to(
                    transform.transform_point(control),
                    transform.transform_point(to),
                ),
                PathCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => result.cubic_to(
                    transform.transform_point(control1),
                    transform.transform_point(control2),
                    transform.transform_point(to),
                ),
                PathCommand::Close => result.close(),
            };
        }
        if let Some(target) = self.morph_target() {
            result = result.with_morph_target(target.transformed(transform));
        }
        result
    }

    /// Whether every point, including morph targets, is finite.
    pub fn is_finite(&self) -> bool {
        let vec2_is_finite = |value: Vec2| value.x.is_finite() && value.y.is_finite();
        self.commands().iter().all(|command| match *command {
            PathCommand::MoveTo { to } | PathCommand::LineTo { to } => vec2_is_finite(to),
            PathCommand::QuadraticTo { control, to } => {
                vec2_is_finite(control) && vec2_is_finite(to)
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => vec2_is_finite(control1) && vec2_is_finite(control2) && vec2_is_finite(to),
            PathCommand::Close => true,
        }) && self.morph_target().is_none_or(VectorPath::is_finite)
    }

    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
            morph_target: None,
        }
    }

    pub fn move_to(mut self, to: Vec2) -> Self {
        self.commands.push(PathCommand::MoveTo { to });
        self
    }

    pub fn line_to(mut self, to: Vec2) -> Self {
        self.commands.push(PathCommand::LineTo { to });
        self
    }

    pub fn quadratic_to(mut self, control: Vec2, to: Vec2) -> Self {
        self.commands.push(PathCommand::QuadraticTo { control, to });
        self
    }

    pub fn cubic_to(mut self, control1: Vec2, control2: Vec2, to: Vec2) -> Self {
        self.commands.push(PathCommand::CubicTo {
            control1,
            control2,
            to,
        });
        self
    }

    pub fn close(mut self) -> Self {
        self.commands.push(PathCommand::Close);
        self
    }

    /// First and last anchors, including an unfinished subpath's starting point.
    pub fn endpoints(&self) -> Option<(Vec2, Vec2)> {
        let mut first = None;
        let mut last = None;
        let mut subpath_start = None;
        for command in self.commands() {
            match *command {
                PathCommand::MoveTo { to } => {
                    first.get_or_insert(to);
                    subpath_start = Some(to);
                    last = Some(to);
                }
                PathCommand::LineTo { to }
                | PathCommand::QuadraticTo { to, .. }
                | PathCommand::CubicTo { to, .. } => last = Some(to),
                PathCommand::Close => last = subpath_start,
            }
        }
        first.zip(last)
    }

    /// Reopen the final closed contour for extension, retaining its closing edge.
    /// Earlier subpaths and their closure flags are unchanged.
    pub fn open_last_subpath(mut self) -> Self {
        if !matches!(self.commands.last(), Some(PathCommand::Close)) {
            return self;
        }
        let start = self
            .commands
            .iter()
            .rev()
            .find_map(|command| match command {
                PathCommand::MoveTo { to } => Some(*to),
                _ => None,
            });
        self.commands.pop();
        if let (Some(start), Some((_, last))) = (start, self.endpoints()) {
            if last != start {
                self.commands.push(PathCommand::LineTo { to: start });
            }
        }
        self
    }

    pub fn commands(&self) -> &[PathCommand] {
        &self.commands
    }

    pub fn with_morph_target(mut self, target: VectorPath) -> Self {
        self.morph_target = Some(Box::new(target));
        self
    }

    pub fn morph_target(&self) -> Option<&VectorPath> {
        self.morph_target.as_deref()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn conservative_bounds(&self) -> Option<Rect> {
        self.transformed_conservative_bounds(Transform2D::IDENTITY)
    }

    fn transformed_conservative_bounds(&self, transform: Transform2D) -> Option<Rect> {
        let mut bounds: Option<Rect> = None;
        let mut include = |point: Vec2| {
            let point = transform.transform_point(point);
            match &mut bounds {
                Some(bounds) => bounds.include(point),
                None => bounds = Some(Rect::new(point, point)),
            }
        };

        for command in &self.commands {
            match *command {
                PathCommand::MoveTo { to } | PathCommand::LineTo { to } => include(to),
                PathCommand::QuadraticTo { control, to } => {
                    include(control);
                    include(to);
                }
                PathCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => {
                    include(control1);
                    include(control2);
                    include(to);
                }
                PathCommand::Close => {}
            }
        }
        bounds
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathCommand {
    MoveTo {
        to: Vec2,
    },
    LineTo {
        to: Vec2,
    },
    QuadraticTo {
        control: Vec2,
        to: Vec2,
    },
    CubicTo {
        control1: Vec2,
        control2: Vec2,
        to: Vec2,
    },
    Close,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryRef {
    Circle { radius: f32 },
    Rectangle { size: Vec2 },
    Line { start: Vec2, end: Vec2 },
    VectorPath(VectorPath),
    External(GeometryId),
}

impl GeometryRef {
    /// Check the complete inline/path payload before resource admission.
    pub fn is_finite(&self) -> bool {
        let point = |p: Vec2| p.x.is_finite() && p.y.is_finite();
        match self {
            Self::Circle { radius } => radius.is_finite(),
            Self::Rectangle { size } => point(*size),
            Self::Line { start, end } => point(*start) && point(*end),
            Self::VectorPath(path) => path.is_finite(),
            Self::External(_) => true,
        }
    }

    pub const fn circle(radius: f32) -> Self {
        Self::Circle { radius }
    }

    pub const fn rectangle(width: f32, height: f32) -> Self {
        Self::Rectangle {
            size: Vec2::new(width, height),
        }
    }

    pub const fn square(side_length: f32) -> Self {
        Self::rectangle(side_length, side_length)
    }

    pub const fn line(start: Vec2, end: Vec2) -> Self {
        Self::Line { start, end }
    }

    pub fn path(path: VectorPath) -> Self {
        Self::VectorPath(path)
    }

    pub fn local_bounds(&self) -> Option<Rect> {
        match self {
            Self::Circle { radius } => Some(Rect::new(
                Vec2::new(-radius, -radius),
                Vec2::new(*radius, *radius),
            )),
            Self::Rectangle { size } => {
                let half = *size * 0.5;
                Some(Rect::new(-half, half))
            }
            Self::Line { start, end } => Rect::from_points([*start, *end]),
            Self::VectorPath(path) => path.conservative_bounds(),
            Self::External(_) => None,
        }
    }

    pub fn world_bounds(&self, transform: Transform2D) -> Option<Rect> {
        match self {
            Self::Line { start, end } => Rect::from_points([
                transform.transform_point(*start),
                transform.transform_point(*end),
            ]),
            Self::VectorPath(path) => path.transformed_conservative_bounds(transform),
            Self::Circle { .. } | Self::Rectangle { .. } => {
                let bounds = self.local_bounds()?;
                let corners = [
                    Vec2::new(bounds.min.x, bounds.min.y),
                    Vec2::new(bounds.min.x, bounds.max.y),
                    Vec2::new(bounds.max.x, bounds.min.y),
                    Vec2::new(bounds.max.x, bounds.max.y),
                ];
                Rect::from_points(corners.map(|point| transform.transform_point(point)))
            }
            Self::External(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_and_execution_state_defaults_are_renderer_independent() {
        assert_eq!(Camera2DState::default().center, ORIGIN);
        assert_eq!(Camera2DState::default().height, DEFAULT_FRAME_HEIGHT);
        assert_eq!(Transform2D::default(), Transform2D::IDENTITY);
    }

    #[test]
    fn line_endpoints_remain_renderer_independent() {
        let start = Vec2::new(-2.0, 1.0);
        let end = Vec2::new(3.0, -4.0);

        assert_eq!(
            GeometryRef::line(start, end),
            GeometryRef::Line { start, end }
        );
    }

    #[test]
    fn vector_path_preserves_semantic_curve_commands() {
        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .quadratic_to(Vec2::new(0.0, 2.0), Vec2::new(1.0, 0.0))
            .cubic_to(
                Vec2::new(1.5, -0.5),
                Vec2::new(-1.5, -0.5),
                Vec2::new(-1.0, 0.0),
            )
            .close();

        assert_eq!(path.commands().len(), 4);
        assert!(matches!(path.commands()[0], PathCommand::MoveTo { .. }));
        assert_eq!(
            GeometryRef::path(path.clone()),
            GeometryRef::VectorPath(path)
        );
    }

    #[test]
    fn vector_path_can_carry_a_semantic_morph_target() {
        let source = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 0.0));
        let target = VectorPath::new()
            .move_to(Vec2::new(0.0, -1.0))
            .line_to(Vec2::new(0.0, 1.0));
        let morph = source.clone().with_morph_target(target.clone());

        assert_eq!(morph.commands(), source.commands());
        assert_eq!(morph.morph_target(), Some(&target));
        assert_eq!(source.morph_target(), None);
    }

    #[test]
    fn named_palette_matches_manim_defaults() {
        assert_eq!(BLUE, Color::from_hex(0x58C4DD));
        assert_eq!(RED, Color::from_hex(0xFC6255));
        assert_eq!(GREEN, Color::from_hex(0x83C167));
        assert_eq!(PURPLE, Color::from_hex(0x9A72AC));
        assert_eq!(PINK, Color::from_hex(0xD147BD));
    }

    #[test]
    fn vector_vocabulary_is_composable() {
        assert_eq!(UP + LEFT, UL);
        assert_eq!(DOWN + RIGHT, DR);
        assert_eq!(RIGHT * 2.0 + UP, Vec2::new(2.0, 1.0));
        assert!((90.0 * DEGREES - PI / 2.0).abs() < 1e-6);
    }

    #[test]
    fn primitive_world_bounds_include_rotation_scale_and_translation() {
        let bounds = GeometryRef::rectangle(2.0, 1.0)
            .world_bounds(Transform2D {
                translation: RIGHT * 3.0,
                rotation: PI / 2.0,
                scale: Vec2::new(2.0, 1.0),
            })
            .expect("rectangle has bounds");
        assert!((bounds.width() - 1.0).abs() < 1e-5);
        assert!((bounds.height() - 4.0).abs() < 1e-5);
        assert!((bounds.center().x - 3.0).abs() < 1e-5);
    }

    #[test]
    fn line_world_bounds_follow_transformed_endpoints() {
        let bounds = GeometryRef::line(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0))
            .world_bounds(Transform2D {
                translation: RIGHT * 3.0,
                rotation: PI / 4.0,
                ..Transform2D::IDENTITY
            })
            .expect("line has bounds");
        assert!(bounds.width().abs() < 1e-5);
        assert!((bounds.height() - 2.0_f32.sqrt() * 2.0).abs() < 1e-5);
        assert!((bounds.center().x - 3.0).abs() < 1e-5);
        assert!(bounds.center().y.abs() < 1e-5);
    }

    #[test]
    fn vector_path_world_bounds_transform_retained_points_directly() {
        let path = VectorPath::new()
            .move_to(Vec2::new(0.0, 1.0))
            .line_to(Vec2::new(1.0, 1.0))
            .line_to(Vec2::new(1.0, 0.0));
        let bounds = GeometryRef::path(path)
            .world_bounds(Transform2D {
                rotation: PI / 4.0,
                ..Transform2D::IDENTITY
            })
            .expect("path has bounds");
        let root_two = 2.0_f32.sqrt();
        assert!((bounds.width() - root_two).abs() < 1e-5);
        assert!((bounds.height() - root_two * 0.5).abs() < 1e-5);
        assert!(bounds.center().x.abs() < 1e-5);
        assert!((bounds.center().y - 3.0 * root_two * 0.25).abs() < 1e-5);
    }

    #[test]
    fn vector_path_world_bounds_keep_curve_controls_conservative() {
        let path = VectorPath::new()
            .move_to(ORIGIN)
            .quadratic_to(Vec2::new(0.0, 2.0), Vec2::new(2.0, 0.0));
        let bounds = GeometryRef::path(path)
            .world_bounds(Transform2D {
                rotation: PI / 2.0,
                ..Transform2D::IDENTITY
            })
            .expect("path has bounds");
        assert!((bounds.min.x + 2.0).abs() < 1e-5);
        assert!(bounds.max.x.abs() < 1e-5);
        assert!(bounds.min.y.abs() < 1e-5);
        assert!((bounds.max.y - 2.0).abs() < 1e-5);
    }

    #[test]
    fn id_namespaces_are_distinct_types() {
        let object = ObjectId::new(7);
        let geometry = GeometryId::new(7);
        let track = TrackId::new(7);
        let signal = SignalId::new(7);

        assert_eq!(object.get(), 7);
        assert_eq!(geometry.get(), 7);
        assert_eq!(track.get(), 7);
        assert_eq!(signal.get(), 7);
    }
}
