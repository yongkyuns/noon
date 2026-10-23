//! Renderer-independent semantic data model for Noon.
//!
//! This crate intentionally contains no renderer, windowing, ECS, or Python
//! dependencies. Shared authoring operations mutate the [`SemanticStore`];
//! compiler/runtime crates consume typed semantic and execution contracts without
//! depending on the authoring language.

#![forbid(unsafe_code)]

mod animation;
mod camera_motion;
mod graph_topology;
mod manim_camera3d;
mod numeric_format;
mod object_state;
mod publication;
mod reactive;
mod resources;
mod semantic_store;
mod spatial3d;
mod vector_path;
pub use vector_path::VectorPath;
mod planar_math;
pub use planar_math::{DRect, DVec2, Rect, Vec2};
mod point_array_shape;
pub use point_array_shape::{PointArrayShape, PointArrayShapeError};

pub use animation::*;
pub use camera_motion::*;
pub use graph_topology::*;
pub use manim_camera3d::*;
pub use numeric_format::*;
pub use object_state::*;
pub use publication::*;
pub use reactive::*;
pub use resources::*;
pub use semantic_store::*;
pub use spatial3d::*;

use serde::{Deserialize, Serialize};

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

/// One effective retained inset view derived from two ordinary runtime rows in
/// the same frame epoch.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Inset2DViewState {
    pub camera_frame: ObjectId,
    pub display: ObjectId,
    pub camera: Camera2DState,
    pub display_center: Vec2,
    pub display_size: Vec2,
    pub display_stroke_width: f32,
    pub capture_own_display: bool,
}

impl Inset2DViewState {
    pub fn zoom_factor(self) -> f32 {
        self.camera.height / self.display_size.y
    }

    /// World-space source bounds captured by this inset's effective camera.
    pub fn camera_bounds(self) -> Option<Rect> {
        let aspect = self.display_size.x / self.display_size.y;
        let size = Vec2::new(self.camera.height * aspect, self.camera.height);
        if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
            return None;
        }
        let half = size * 0.5;
        Some(Rect::new(
            self.camera.center - half,
            self.camera.center + half,
        ))
    }
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
    /// Preserve the renderer's default cap (Cairo's default is butt).
    Auto,
}

impl StrokeCap {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Round => "round",
            Self::Butt => "butt",
            Self::Square => "square",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_width: f32,
    /// Solid stroke painted beneath the fill and foreground stroke.
    #[serde(default)]
    pub background_stroke: Option<Color>,
    /// Background width uses `stroke_width_mode`, like the foreground stroke.
    #[serde(default)]
    pub background_stroke_width: f32,
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
            background_stroke: Some(Color::BLACK),
            background_stroke_width: 0.0,
            stroke_width_mode: StrokeWidthMode::ScaleWithObject,
            stroke_join: StrokeJoin::Round,
            stroke_cap: StrokeCap::Round,
            opacity: 1.0,
        }
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
    fn raw_manim_points_keep_incomplete_tail_through_path_transforms() {
        let points = (0..7)
            .map(|index| Vec2::new(index as f32, (index * 2) as f32))
            .collect::<Vec<_>>();
        let path = VectorPath::from_manim_points(points.clone());

        let expected = points
            .iter()
            .copied()
            .map(SemanticVec3::from_vec2)
            .collect::<Vec<_>>();
        assert_eq!(path.manim_points(), Some(expected.as_slice()));
        assert_eq!(
            path.commands().len(),
            3,
            "one complete cubic and one pending subpath are retained"
        );
        assert_eq!(path.conservative_bounds().unwrap().max, points[6]);

        let transformed = path.transformed(Transform2D {
            translation: Vec2::new(5.0, -3.0),
            ..Transform2D::IDENTITY
        });
        assert_eq!(
            transformed.manim_points().unwrap()[6],
            SemanticVec3::new(11.0, 9.0, 0.0)
        );
        assert!(transformed.is_finite());
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
