//! Renderer-independent semantic data model for Noon.
//!
//! This crate intentionally contains no renderer, windowing, ECS, or Python
//! dependencies. Frontends build a [`SceneDefinition`]; later compiler/runtime
//! crates consume it without depending on the authoring language.

#![forbid(unsafe_code)]

mod patch;
mod timeline;

pub use patch::*;
pub use timeline::*;

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
    pub const WHITE: Self = Self::rgba(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Self = Self::rgba(0.0, 0.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);

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
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
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

    pub const fn line(start: Vec2, end: Vec2) -> Self {
        Self::Line { start, end }
    }

    pub fn path(path: VectorPath) -> Self {
        Self::VectorPath(path)
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
