//! Renderer-independent semantic data model for Noon.
//!
//! This crate intentionally contains no renderer, windowing, ECS, or Python
//! dependencies. Frontends build a [`SceneDefinition`]; later compiler/runtime
//! crates consume it without depending on the authoring language.

#![forbid(unsafe_code)]

mod patch;
mod reactive;
mod timeline;

pub use patch::*;
pub use reactive::*;
pub use timeline::*;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeWidthMode {
    /// Geometry/object scaling also scales stroke width.
    #[default]
    ScaleWithObject,
    /// Stroke width is authored in physical canvas pixels and does not scale with the object.
    ScreenSpace,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_width: f32,
    #[serde(default)]
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
        let mut points = Vec::new();
        for command in &self.commands {
            match *command {
                PathCommand::MoveTo { to } | PathCommand::LineTo { to } => points.push(to),
                PathCommand::QuadraticTo { control, to } => {
                    points.push(control);
                    points.push(to);
                }
                PathCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => {
                    points.push(control1);
                    points.push(control2);
                    points.push(to);
                }
                PathCommand::Close => {}
            }
        }
        Rect::from_points(points)
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectDefinition {
    pub id: ObjectId,
    pub geometry: GeometryRef,
    pub transform: Transform2D,
    pub style: Style,
}

impl ObjectDefinition {
    pub fn new(id: ObjectId, geometry: GeometryRef) -> Self {
        Self {
            id,
            geometry,
            transform: Transform2D::default(),
            style: Style::default(),
        }
    }

    pub fn snapshot(&self) -> ObjectSnapshot {
        ObjectSnapshot::from(self)
    }

    pub fn world_bounds(&self) -> Option<Rect> {
        self.snapshot().world_bounds()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectSnapshot {
    pub geometry: GeometryRef,
    pub transform: Transform2D,
    pub style: Style,
}

impl ObjectSnapshot {
    pub fn new(geometry: GeometryRef) -> Self {
        Self {
            geometry,
            transform: Transform2D::default(),
            style: Style::default(),
        }
    }

    pub fn shift(mut self, offset: Vec2) -> Self {
        self.transform.translation += offset;
        self
    }

    pub fn move_to(mut self, point: Vec2) -> Self {
        let center = self.center();
        self.transform.translation += point - center;
        self
    }

    pub fn center(&self) -> Vec2 {
        self.world_bounds()
            .map(Rect::center)
            .unwrap_or(self.transform.translation)
    }

    pub fn scale_by(mut self, factor: f32) -> Self {
        self.transform.scale = self.transform.scale * factor;
        self
    }

    pub fn scale_xy(mut self, factor: Vec2) -> Self {
        self.transform.scale = self.transform.scale.component_mul(factor);
        self
    }

    pub fn rotate_by(mut self, angle: f32) -> Self {
        self.transform.rotation += angle;
        self
    }

    pub fn set_color(mut self, color: Color) -> Self {
        if self.style.fill.is_some() {
            self.style.fill = Some(color);
        }
        if self.style.stroke.is_some() {
            self.style.stroke = Some(color);
        }
        if self.style.fill.is_none() && self.style.stroke.is_none() {
            self.style.fill = Some(color);
        }
        self
    }

    pub fn set_fill(mut self, color: Option<Color>, opacity: Option<f32>) -> Self {
        self.style.fill = color;
        if let Some(opacity) = opacity {
            self.style.opacity = opacity;
        }
        self
    }

    pub fn set_stroke(mut self, color: Option<Color>, width: Option<f32>) -> Self {
        self.style.stroke = color;
        if let Some(width) = width {
            self.style.stroke_width = width;
        }
        self
    }

    pub fn set_opacity(mut self, opacity: f32) -> Self {
        self.style.opacity = opacity;
        self
    }

    pub fn local_bounds(&self) -> Option<Rect> {
        self.geometry.local_bounds()
    }

    pub fn world_bounds(&self) -> Option<Rect> {
        let bounds = self.local_bounds()?;
        let corners = [
            Vec2::new(bounds.min.x, bounds.min.y),
            Vec2::new(bounds.min.x, bounds.max.y),
            Vec2::new(bounds.max.x, bounds.min.y),
            Vec2::new(bounds.max.x, bounds.max.y),
        ];
        Rect::from_points(corners.map(|point| self.transform.transform_point(point)))
    }

    pub fn width(&self) -> f32 {
        self.world_bounds().map_or(0.0, Rect::width)
    }

    pub fn height(&self) -> f32 {
        self.world_bounds().map_or(0.0, Rect::height)
    }

    pub fn next_to(mut self, target: &ObjectSnapshot, direction: Vec2, buff: f32) -> Self {
        let Some(axis) = direction.normalized() else {
            return self;
        };
        let Some(target_bounds) = target.world_bounds() else {
            return self;
        };
        let Some(self_bounds) = self.world_bounds() else {
            return self;
        };
        let target_point = target_bounds.critical_point(axis);
        let point_to_align = self_bounds.critical_point(-axis);
        self.transform.translation += target_point - point_to_align + axis * buff;
        self
    }

    pub fn align_to(mut self, target: &ObjectSnapshot, direction: Vec2) -> Self {
        let Some(target_bounds) = target.world_bounds() else {
            return self;
        };
        let Some(self_bounds) = self.world_bounds() else {
            return self;
        };
        let target_point = target_bounds.critical_point(direction);
        let point_to_align = self_bounds.critical_point(direction);
        let mask = Vec2::new(direction.x.signum().abs(), direction.y.signum().abs());
        self.transform.translation += (target_point - point_to_align).component_mul(mask);
        self
    }

    pub fn to_edge(self, direction: Vec2, buff: f32) -> Self {
        self.align_on_frame(direction, buff)
    }

    pub fn to_corner(self, direction: Vec2, buff: f32) -> Self {
        self.align_on_frame(direction, buff)
    }

    fn align_on_frame(mut self, direction: Vec2, buff: f32) -> Self {
        let Some(bounds) = self.world_bounds() else {
            return self;
        };
        let frame_target = Vec2::new(
            direction.x.signum() * DEFAULT_FRAME_WIDTH * 0.5,
            direction.y.signum() * DEFAULT_FRAME_HEIGHT * 0.5,
        );
        let point_to_align = bounds.critical_point(direction);
        let shift = frame_target - point_to_align - direction * buff;
        let mask = Vec2::new(direction.x.signum().abs(), direction.y.signum().abs());
        self.transform.translation += shift.component_mul(mask);
        self
    }
}

impl From<&ObjectDefinition> for ObjectSnapshot {
    fn from(value: &ObjectDefinition) -> Self {
        Self {
            geometry: value.geometry.clone(),
            transform: value.transform,
            style: value.style,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneDefinition {
    pub(crate) objects: Vec<ObjectDefinition>,
    pub(crate) next_object_id: u64,
    pub(crate) tracks: Vec<TrackDefinition>,
    pub(crate) next_track_id: u64,
}

impl SceneDefinition {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, geometry: GeometryRef) -> ObjectId {
        let id = ObjectId::new(self.next_object_id);
        self.next_object_id = self
            .next_object_id
            .checked_add(1)
            .expect("Noon object ID space exhausted");
        self.objects.push(ObjectDefinition::new(id, geometry));
        id
    }

    pub fn add_snapshot(&mut self, snapshot: ObjectSnapshot) -> ObjectId {
        let id = self.add(snapshot.geometry.clone());
        let object = self.object_mut(id).expect("newly added object exists");
        object.transform = snapshot.transform;
        object.style = snapshot.style;
        id
    }

    pub fn snapshot(&self, id: ObjectId) -> Option<ObjectSnapshot> {
        self.object(id).map(ObjectSnapshot::from)
    }

    pub fn set_snapshot(&mut self, id: ObjectId, snapshot: ObjectSnapshot) -> bool {
        let Some(object) = self.object_mut(id) else {
            return false;
        };
        object.geometry = snapshot.geometry;
        object.transform = snapshot.transform;
        object.style = snapshot.style;
        true
    }

    pub fn objects(&self) -> &[ObjectDefinition] {
        &self.objects
    }

    pub fn object(&self, id: ObjectId) -> Option<&ObjectDefinition> {
        self.objects.iter().find(|object| object.id == id)
    }

    pub fn object_mut(&mut self, id: ObjectId) -> Option<&mut ObjectDefinition> {
        self.objects.iter_mut().find(|object| object.id == id)
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_ids_are_deterministic_for_identical_insertion() {
        let mut first = SceneDefinition::new();
        let mut second = SceneDefinition::new();

        let first_circle = first.add(GeometryRef::circle(1.0));
        let first_rect = first.add(GeometryRef::rectangle(2.0, 3.0));
        let second_circle = second.add(GeometryRef::circle(1.0));
        let second_rect = second.add(GeometryRef::rectangle(2.0, 3.0));

        assert_eq!(first_circle, ObjectId::new(0));
        assert_eq!(first_rect, ObjectId::new(1));
        assert_eq!(first_circle, second_circle);
        assert_eq!(first_rect, second_rect);
    }

    #[test]
    fn insertion_identity_survives_property_mutation() {
        let mut scene = SceneDefinition::new();
        let circle = scene.add(GeometryRef::circle(2.0));

        let object = scene.object_mut(circle).expect("object must exist");
        object.transform.translation = Vec2::new(4.0, -2.0);
        object.style.opacity = 0.5;

        let object = scene.object(circle).expect("object must still exist");
        assert_eq!(object.id, circle);
        assert_eq!(object.transform.translation, Vec2::new(4.0, -2.0));
        assert_eq!(object.style.opacity, 0.5);
    }

    #[test]
    fn objects_start_with_renderer_independent_defaults() {
        let mut scene = SceneDefinition::new();
        let rectangle = scene.add(GeometryRef::rectangle(4.0, 2.0));
        let object = scene.object(rectangle).expect("object must exist");

        assert_eq!(object.transform, Transform2D::IDENTITY);
        assert_eq!(object.style, Style::default());
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
        let snapshot = ObjectSnapshot::new(GeometryRef::rectangle(2.0, 1.0))
            .scale_xy(Vec2::new(2.0, 1.0))
            .rotate_by(PI / 2.0)
            .shift(RIGHT * 3.0);
        let bounds = snapshot.world_bounds().expect("rectangle has bounds");
        assert!((bounds.width() - 1.0).abs() < 1e-5);
        assert!((bounds.height() - 4.0).abs() < 1e-5);
        assert!((bounds.center().x - 3.0).abs() < 1e-5);
    }

    #[test]
    fn next_to_uses_semantic_bounds_and_buffer() {
        let left = ObjectSnapshot::new(GeometryRef::circle(1.0)).shift(LEFT * 2.0);
        let right = ObjectSnapshot::new(GeometryRef::square(1.0)).next_to(
            &left,
            RIGHT,
            DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
        );
        let gap = right.world_bounds().unwrap().min.x - left.world_bounds().unwrap().max.x;
        assert!((gap - DEFAULT_MOBJECT_TO_MOBJECT_BUFFER).abs() < 1e-6);
    }

    #[test]
    fn to_corner_respects_default_frame_and_buffer() {
        let snapshot = ObjectSnapshot::new(GeometryRef::square(1.0))
            .to_corner(UR, DEFAULT_MOBJECT_TO_EDGE_BUFFER);
        let bounds = snapshot.world_bounds().unwrap();
        assert!(
            (bounds.max.x - (DEFAULT_FRAME_WIDTH * 0.5 - DEFAULT_MOBJECT_TO_EDGE_BUFFER)).abs()
                < 1e-5
        );
        assert!(
            (bounds.max.y - (DEFAULT_FRAME_HEIGHT * 0.5 - DEFAULT_MOBJECT_TO_EDGE_BUFFER)).abs()
                < 1e-5
        );
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
