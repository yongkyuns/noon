//! Ergonomic Rust authoring facade for Noon.
//!
//! This crate deliberately does not introduce another scene representation.
//! [`Scene`] owns one canonical [`noon_core::SceneDefinition`]; the extra maps
//! here are transient authoring state used only to resolve fluent operations
//! such as `mobject.animate().shift(...)` into deterministic core tracks.

#![forbid(unsafe_code)]

mod semantic_snapshot;
pub use semantic_snapshot::{
    export_mobject_snapshot, import_mobject_snapshot, replace_mobject_snapshot,
};

use std::collections::BTreeMap;

pub use noon_core;
pub use noon_core::*;

mod composition_authoring;
pub use composition_authoring::{AnimationGroup, LaggedStart, Succession};

/// Common imports for normal Noon authoring.
pub mod prelude {
    pub use super::{
        Animate, AnimationGroup, AuthoringError, Circle, Create, FadeIn, FadeOut, LaggedStart,
        Line, Mobject, MobjectEditor, Path, Rectangle, Rotate, Scene, Square, Succession,
        Transform,
    };
    pub use crate::legacy::*;
    pub use crate::{ExecutionSession, RetainedScene};
    pub use noon_core::{
        Color, Easing, GeometryRef, ObjectId, ObjectSnapshot, RateFunction, Style, Vec2,
        VectorPath, BLACK, BLUE, BLUE_A, BLUE_B, BLUE_C, BLUE_D, BLUE_E,
        DEFAULT_MOBJECT_TO_EDGE_BUFFER, DEFAULT_MOBJECT_TO_MOBJECT_BUFFER, DEGREES, DL, DOWN, DR,
        GOLD, GRAY, GREEN, GREY, LARGE_BUFF, LEFT, LIGHT_PINK, MAROON, MED_LARGE_BUFF,
        MED_SMALL_BUFF, ORANGE, ORIGIN, PI, PINK, PURPLE, PURPLE_A, PURPLE_B, PURPLE_C, PURPLE_D,
        PURPLE_E, RED, RED_A, RED_B, RED_C, RED_D, RED_E, RIGHT, SMALL_BUFF, TAU, TEAL, TEAL_A,
        TEAL_B, TEAL_C, TEAL_D, TEAL_E, UL, UP, UR, WHITE, YELLOW, YELLOW_A, YELLOW_B, YELLOW_C,
        YELLOW_D, YELLOW_E,
    };
}

/// Something that can be inserted into the canonical semantic scene.
pub trait IntoSnapshot {
    fn into_snapshot(self) -> ObjectSnapshot;
}

impl IntoSnapshot for ObjectSnapshot {
    fn into_snapshot(self) -> ObjectSnapshot {
        self
    }
}

macro_rules! define_shape {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name(ObjectSnapshot);

        impl $name {
            pub fn color(mut self, color: Color) -> Self {
                self.0 = self.0.set_color(color);
                self
            }

            pub fn shift(mut self, offset: Vec2) -> Self {
                self.0 = self.0.shift(offset);
                self
            }

            pub fn move_to(mut self, point: Vec2) -> Self {
                self.0 = self.0.move_to(point);
                self
            }

            pub fn scale(mut self, factor: f32) -> Self {
                self.0 = self.0.scale_by(factor);
                self
            }

            pub fn scale_xy(mut self, factor: Vec2) -> Self {
                self.0 = self.0.scale_xy(factor);
                self
            }

            pub fn rotate(mut self, angle: f32) -> Self {
                self.0 = self.0.rotate_by(angle);
                self
            }

            pub fn set_fill(mut self, color: Option<Color>, opacity: Option<f32>) -> Self {
                self.0 = self.0.set_fill(color, opacity);
                self
            }

            pub fn set_stroke(mut self, color: Option<Color>, width: Option<f32>) -> Self {
                self.0 = self.0.set_stroke(color, width);
                self
            }

            pub fn set_opacity(mut self, opacity: f32) -> Self {
                self.0 = self.0.set_opacity(opacity);
                self
            }

            pub fn snapshot(&self) -> &ObjectSnapshot {
                &self.0
            }
        }

        impl IntoSnapshot for $name {
            fn into_snapshot(self) -> ObjectSnapshot {
                self.0
            }
        }
    };
}

define_shape!(Circle);
define_shape!(Rectangle);
define_shape!(Square);
define_shape!(Line);
define_shape!(Path);

const MANIM_CAIRO_DEFAULT_STROKE_WIDTH: f32 = 0.04;

fn manim_vmobject_snapshot(geometry: GeometryRef, default_color: Color) -> ObjectSnapshot {
    let mut snapshot = ObjectSnapshot::new(geometry);
    let mut transparent_fill = default_color;
    transparent_fill.alpha = 0.0;
    snapshot.style.fill = Some(transparent_fill);
    snapshot.style.stroke = Some(default_color);
    snapshot.style.stroke_width = MANIM_CAIRO_DEFAULT_STROKE_WIDTH;
    snapshot.style.stroke_width_mode = StrokeWidthMode::ScreenSpace;
    snapshot.style.stroke_join = StrokeJoin::Miter;
    snapshot.style.stroke_cap = StrokeCap::Butt;
    snapshot
}

impl Circle {
    pub fn new(radius: f32) -> Self {
        Self(manim_vmobject_snapshot(GeometryRef::circle(radius), RED))
    }
}

impl Default for Circle {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl Rectangle {
    pub fn new(width: f32, height: f32) -> Self {
        Self(manim_vmobject_snapshot(
            GeometryRef::rectangle(width, height),
            WHITE,
        ))
    }
}

impl Square {
    pub fn new(side_length: f32) -> Self {
        Self(manim_vmobject_snapshot(
            GeometryRef::square(side_length),
            WHITE,
        ))
    }
}

impl Default for Square {
    fn default() -> Self {
        Self::new(2.0)
    }
}

impl Line {
    pub fn new(start: Vec2, end: Vec2) -> Self {
        Self(manim_vmobject_snapshot(
            GeometryRef::line(start, end),
            WHITE,
        ))
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new(LEFT, RIGHT)
    }
}

impl Path {
    pub fn new(path: VectorPath) -> Self {
        Self(manim_vmobject_snapshot(GeometryRef::path(path), WHITE))
    }
}

/// Stable handle to a semantic object in a [`Scene`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mobject {
    id: ObjectId,
}

impl Mobject {
    pub const fn id(self) -> ObjectId {
        self.id
    }

    /// Build a transient target-state animation. No runtime callback is created.
    pub fn animate(self) -> Animate {
        Animate {
            object: self,
            operations: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Operation {
    Shift(Vec2),
    MoveTo(Vec2),
    Scale(f32),
    ScaleXY(Vec2),
    Rotate(f32),
    SetColor(Color),
    SetFill(Option<Color>, Option<f32>),
    SetStroke(Option<Color>, Option<f32>),
    SetOpacity(f32),
}

/// Transient target-state builder returned by [`Mobject::animate`].
#[derive(Clone, Debug, PartialEq)]
pub struct Animate {
    object: Mobject,
    operations: Vec<Operation>,
}

impl Animate {
    pub fn shift(mut self, offset: Vec2) -> Self {
        self.operations.push(Operation::Shift(offset));
        self
    }

    pub fn move_to(mut self, point: Vec2) -> Self {
        self.operations.push(Operation::MoveTo(point));
        self
    }

    pub fn scale(mut self, factor: f32) -> Self {
        self.operations.push(Operation::Scale(factor));
        self
    }

    pub fn scale_xy(mut self, factor: Vec2) -> Self {
        self.operations.push(Operation::ScaleXY(factor));
        self
    }

    pub fn rotate(mut self, angle: f32) -> Self {
        self.operations.push(Operation::Rotate(angle));
        self
    }

    pub fn set_color(mut self, color: Color) -> Self {
        self.operations.push(Operation::SetColor(color));
        self
    }

    pub fn set_fill(mut self, color: Option<Color>, opacity: Option<f32>) -> Self {
        self.operations.push(Operation::SetFill(color, opacity));
        self
    }

    pub fn set_stroke(mut self, color: Option<Color>, width: Option<f32>) -> Self {
        self.operations.push(Operation::SetStroke(color, width));
        self
    }

    pub fn set_opacity(mut self, opacity: f32) -> Self {
        self.operations.push(Operation::SetOpacity(opacity));
        self
    }
}

/// Explicit transformation to a detached target snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct Transform {
    source: Mobject,
    target: ObjectSnapshot,
}

impl Transform {
    pub fn new<T: IntoSnapshot>(source: Mobject, target: T) -> Self {
        Self {
            source,
            target: target.into_snapshot(),
        }
    }
}

/// Procedurally rotate a centered 2D mobject around its current center.
///
/// Unlike [`Animate::rotate`], which builds a target-state transform, `Rotate`
/// lowers to the scalar rotation channel. This preserves Manim's distinction
/// between target-state point interpolation and procedural angular motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotate {
    object: Mobject,
    angle: f32,
}

impl Rotate {
    pub const fn new(object: Mobject, angle: f32) -> Self {
        Self { object, angle }
    }
}

/// Progressively draw a shape while preserving its steady-state semantic geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Create(pub Mobject);

impl Create {
    pub const fn new(object: Mobject) -> Self {
        Self(object)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FadeOut(pub Mobject);

impl FadeOut {
    pub const fn new(object: Mobject) -> Self {
        Self(object)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FadeIn(pub Mobject);

impl FadeIn {
    pub const fn new(object: Mobject) -> Self {
        Self(object)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Animation {
    Animate(Animate),
    Transform(Transform),
    Rotate(Rotate),
    Create(Create),
    FadeOut(FadeOut),
    FadeIn(FadeIn),
    Group(AnimationGroup),
}

impl From<Animate> for Animation {
    fn from(value: Animate) -> Self {
        Self::Animate(value)
    }
}

impl From<Transform> for Animation {
    fn from(value: Transform) -> Self {
        Self::Transform(value)
    }
}

impl From<Rotate> for Animation {
    fn from(value: Rotate) -> Self {
        Self::Rotate(value)
    }
}

impl From<Create> for Animation {
    fn from(value: Create) -> Self {
        Self::Create(value)
    }
}

impl From<FadeOut> for Animation {
    fn from(value: FadeOut) -> Self {
        Self::FadeOut(value)
    }
}

impl From<FadeIn> for Animation {
    fn from(value: FadeIn) -> Self {
        Self::FadeIn(value)
    }
}

pub trait IntoAnimations {
    fn into_animations(self) -> Vec<Animation>;
}

impl<T> IntoAnimations for T
where
    T: Into<Animation>,
{
    fn into_animations(self) -> Vec<Animation> {
        vec![self.into()]
    }
}

macro_rules! tuple_animations {
    ($($name:ident),+ $(,)?) => {
        impl<$($name),+> IntoAnimations for ($($name,)+)
        where
            $($name: Into<Animation>,)+
        {
            #[allow(non_snake_case)]
            fn into_animations(self) -> Vec<Animation> {
                let ($($name,)+) = self;
                vec![$($name.into(),)+]
            }
        }
    };
}

tuple_animations!(A, B);
tuple_animations!(A, B, C);
tuple_animations!(A, B, C, D);

#[derive(Clone, Debug, PartialEq)]
pub enum AuthoringError {
    UnknownObject(ObjectId),
    InvalidDuration(f64),
    InvalidRotationAngle(f32),
    RotateRequiresCenteredGeometry(ObjectId),
    StaticMutationAfterAnimation(ObjectId),
    CreateRequiresAbsent(ObjectId),
    CreateUnsupportedGeometry(ObjectId),
    FadeInRequiresAbsent(ObjectId),
    FadeOutRequiresPresent(ObjectId),
    Composition(CompositionError),
    Timeline(TimelineError),
}

impl std::fmt::Display for AuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownObject(id) => write!(formatter, "unknown object id {}", id.get()),
            Self::InvalidDuration(value) => write!(formatter, "invalid animation duration {value}"),
            Self::InvalidRotationAngle(value) => {
                write!(formatter, "invalid rotation angle {value}")
            }
            Self::RotateRequiresCenteredGeometry(id) => write!(
                formatter,
                "Rotate currently requires object {} geometry centered on its transform origin",
                id.get()
            ),
            Self::StaticMutationAfterAnimation(id) => write!(
                formatter,
                "object {} already has authored animation; use .animate for later changes",
                id.get()
            ),
            Self::CreateRequiresAbsent(id) => {
                write!(formatter, "Create requires absent object {}", id.get())
            }
            Self::CreateUnsupportedGeometry(id) => write!(
                formatter,
                "Create does not support geometry for object {}",
                id.get()
            ),
            Self::FadeInRequiresAbsent(id) => {
                write!(formatter, "FadeIn requires absent object {}", id.get())
            }
            Self::FadeOutRequiresPresent(id) => {
                write!(formatter, "FadeOut requires present object {}", id.get())
            }
            Self::Composition(error) => error.fmt(formatter),
            Self::Timeline(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for AuthoringError {}

impl From<CompositionError> for AuthoringError {
    fn from(value: CompositionError) -> Self {
        Self::Composition(value)
    }
}

impl From<TimelineError> for AuthoringError {
    fn from(value: TimelineError) -> Self {
        Self::Timeline(value)
    }
}

/// High-level authoring facade over one canonical [`SceneDefinition`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    definition: SceneDefinition,
    cursor: f64,
    authored: BTreeMap<ObjectId, ObjectSnapshot>,
    presence: BTreeMap<ObjectId, bool>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn time(&self) -> f64 {
        self.cursor
    }

    pub fn definition(&self) -> &SceneDefinition {
        &self.definition
    }

    pub fn into_definition(self) -> SceneDefinition {
        self.definition
    }

    pub fn add<T: IntoSnapshot>(&mut self, object: T) -> Mobject {
        let snapshot = object.into_snapshot();
        let id = self.definition.add_snapshot(snapshot.clone());
        self.authored.insert(id, snapshot);
        self.presence.insert(id, true);
        Mobject { id }
    }

    pub fn snapshot(&self, object: Mobject) -> Result<&ObjectSnapshot, AuthoringError> {
        self.authored
            .get(&object.id)
            .ok_or(AuthoringError::UnknownObject(object.id))
    }

    /// Edit a static object's semantic snapshot before it is animated.
    pub fn edit(&mut self, object: Mobject) -> Result<MobjectEditor<'_>, AuthoringError> {
        if self
            .definition
            .tracks()
            .iter()
            .any(|track| track.object == object.id)
        {
            return Err(AuthoringError::StaticMutationAfterAnimation(object.id));
        }
        if !self.authored.contains_key(&object.id) {
            return Err(AuthoringError::UnknownObject(object.id));
        }
        Ok(MobjectEditor {
            scene: self,
            object,
        })
    }

    pub fn play<A: IntoAnimations>(&mut self, animations: A) -> Play<'_> {
        Play {
            scene: self,
            animations: animations.into_animations(),
            rate_func: None,
        }
    }

    pub fn wait(&mut self, duration: f64) -> Result<&mut Self, AuthoringError> {
        if !duration.is_finite() || duration < 0.0 {
            return Err(AuthoringError::InvalidDuration(duration));
        }
        self.cursor += duration;
        Ok(self)
    }

    fn apply_static(
        &mut self,
        object: Mobject,
        snapshot: ObjectSnapshot,
    ) -> Result<(), AuthoringError> {
        if !self.definition.set_snapshot(object.id, snapshot.clone()) {
            return Err(AuthoringError::UnknownObject(object.id));
        }
        self.authored.insert(object.id, snapshot);
        Ok(())
    }

    fn schedule(
        &mut self,
        animations: Vec<Animation>,
        duration: f64,
        rate_func: Option<RateFunction>,
    ) -> Result<(), AuthoringError> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(AuthoringError::InvalidDuration(duration));
        }
        let start = self.cursor;
        let end = start + duration;
        let leaf_rate_func = rate_func.unwrap_or(RateFunction::Smooth);
        let timing = TrackTiming::new(start, duration, leaf_rate_func);

        for animation in animations {
            match animation {
                Animation::Animate(animation) => {
                    let from = self.snapshot(animation.object)?.clone();
                    let mut to = from.clone();
                    for operation in animation.operations {
                        to = apply_operation(to, operation);
                    }
                    self.definition.animate_transform(
                        animation.object.id,
                        from,
                        to.clone(),
                        timing,
                    )?;
                    self.authored.insert(animation.object.id, to);
                }
                Animation::Transform(animation) => {
                    let from = self.snapshot(animation.source)?.clone();
                    self.definition.animate_transform(
                        animation.source.id,
                        from,
                        animation.target.clone(),
                        timing,
                    )?;
                    self.authored.insert(animation.source.id, animation.target);
                }
                Animation::Rotate(animation) => {
                    if !animation.angle.is_finite() {
                        return Err(AuthoringError::InvalidRotationAngle(animation.angle));
                    }
                    let from = self.snapshot(animation.object)?.clone();
                    let center = from.center();
                    let origin = from.transform.translation;
                    if (center - origin).length() > 1e-5 {
                        return Err(AuthoringError::RotateRequiresCenteredGeometry(
                            animation.object.id,
                        ));
                    }
                    let from_rotation = from.transform.rotation;
                    let to_rotation = from_rotation + animation.angle;
                    self.definition.animate_scalar(
                        animation.object.id,
                        Property::Rotation,
                        from_rotation,
                        to_rotation,
                        timing,
                    )?;
                    let mut to = from;
                    to.transform.rotation = to_rotation;
                    self.authored.insert(animation.object.id, to);
                }
                Animation::Create(Create(object)) => {
                    let snapshot = self.snapshot(object)?;
                    if !matches!(
                        &snapshot.geometry,
                        GeometryRef::Circle { .. }
                            | GeometryRef::Rectangle { .. }
                            | GeometryRef::Line { .. }
                            | GeometryRef::VectorPath(_)
                    ) {
                        return Err(AuthoringError::CreateUnsupportedGeometry(object.id));
                    }
                    let has_presence_track = self.definition.tracks().iter().any(|track| {
                        track.object == object.id && track.property == Property::Presence
                    });
                    let is_present = self
                        .presence
                        .get(&object.id)
                        .copied()
                        .ok_or(AuthoringError::UnknownObject(object.id))?;
                    // A newly added object has no lifecycle tracks yet, so Create
                    // may establish its initial absent -> present lifecycle.
                    if has_presence_track && is_present {
                        return Err(AuthoringError::CreateRequiresAbsent(object.id));
                    }
                    let appearance = self
                        .definition
                        .tracks()
                        .iter()
                        .rev()
                        .find_map(|track| {
                            if track.object != object.id || track.property != Property::Appearance {
                                return None;
                            }
                            match &track.values {
                                TrackValues::Scalar { to, .. } => Some(*to),
                                _ => None,
                            }
                        })
                        .unwrap_or(1.0);
                    self.definition
                        .set_presence_at(object.id, false, true, start)?;
                    self.definition
                        .animate_reveal(object.id, 0.0, 1.0, timing)?;
                    if appearance != 1.0 {
                        self.definition
                            .animate_appearance(object.id, 1.0, 1.0, timing)?;
                    }
                    self.presence.insert(object.id, true);
                }
                Animation::FadeOut(FadeOut(object)) => {
                    let is_present = self
                        .presence
                        .get(&object.id)
                        .copied()
                        .ok_or(AuthoringError::UnknownObject(object.id))?;
                    if !is_present {
                        return Err(AuthoringError::FadeOutRequiresPresent(object.id));
                    }
                    self.definition
                        .animate_appearance(object.id, 1.0, 0.0, timing)?;
                    self.definition
                        .set_presence_at(object.id, true, false, end)?;
                    self.presence.insert(object.id, false);
                }
                Animation::FadeIn(FadeIn(object)) => {
                    let is_present = self
                        .presence
                        .get(&object.id)
                        .copied()
                        .ok_or(AuthoringError::UnknownObject(object.id))?;
                    if is_present {
                        return Err(AuthoringError::FadeInRequiresAbsent(object.id));
                    }
                    self.definition
                        .set_presence_at(object.id, false, true, start)?;
                    self.definition
                        .animate_appearance(object.id, 0.0, 1.0, timing)?;
                    self.presence.insert(object.id, true);
                }
                Animation::Group(group) => {
                    composition_authoring::schedule_group(self, group, start, duration, rate_func)?;
                }
            }
        }

        self.cursor = end;
        Ok(())
    }
}

fn apply_operation(snapshot: ObjectSnapshot, operation: Operation) -> ObjectSnapshot {
    match operation {
        Operation::Shift(value) => snapshot.shift(value),
        Operation::MoveTo(value) => snapshot.move_to(value),
        Operation::Scale(value) => snapshot.scale_by(value),
        Operation::ScaleXY(value) => snapshot.scale_xy(value),
        Operation::Rotate(value) => snapshot.rotate_by(value),
        Operation::SetColor(value) => snapshot.set_color(value),
        Operation::SetFill(color, opacity) => snapshot.set_fill(color, opacity),
        Operation::SetStroke(color, width) => snapshot.set_stroke(color, width),
        Operation::SetOpacity(value) => snapshot.set_opacity(value),
    }
}

/// Mutable semantic accessor for pre-animation object layout/style.
pub struct MobjectEditor<'a> {
    scene: &'a mut Scene,
    object: Mobject,
}

impl MobjectEditor<'_> {
    fn map(
        &mut self,
        operation: impl FnOnce(ObjectSnapshot) -> ObjectSnapshot,
    ) -> Result<&mut Self, AuthoringError> {
        let current = self.scene.snapshot(self.object)?.clone();
        self.scene.apply_static(self.object, operation(current))?;
        Ok(self)
    }

    pub fn shift(&mut self, offset: Vec2) -> Result<&mut Self, AuthoringError> {
        self.map(|snapshot| snapshot.shift(offset))
    }

    pub fn move_to(&mut self, point: Vec2) -> Result<&mut Self, AuthoringError> {
        self.map(|snapshot| snapshot.move_to(point))
    }

    pub fn scale(&mut self, factor: f32) -> Result<&mut Self, AuthoringError> {
        self.map(|snapshot| snapshot.scale_by(factor))
    }

    pub fn rotate(&mut self, angle: f32) -> Result<&mut Self, AuthoringError> {
        self.map(|snapshot| snapshot.rotate_by(angle))
    }

    pub fn set_color(&mut self, color: Color) -> Result<&mut Self, AuthoringError> {
        self.map(|snapshot| snapshot.set_color(color))
    }

    pub fn set_opacity(&mut self, opacity: f32) -> Result<&mut Self, AuthoringError> {
        self.map(|snapshot| snapshot.set_opacity(opacity))
    }

    pub fn next_to(
        &mut self,
        target: Mobject,
        direction: Vec2,
        buff: f32,
    ) -> Result<&mut Self, AuthoringError> {
        let target_snapshot = self.scene.snapshot(target)?.clone();
        self.map(|snapshot| snapshot.next_to(&target_snapshot, direction, buff))
    }

    pub fn align_to(
        &mut self,
        target: Mobject,
        direction: Vec2,
    ) -> Result<&mut Self, AuthoringError> {
        let target_snapshot = self.scene.snapshot(target)?.clone();
        self.map(|snapshot| snapshot.align_to(&target_snapshot, direction))
    }

    pub fn to_edge(&mut self, direction: Vec2, buff: f32) -> Result<&mut Self, AuthoringError> {
        self.map(|snapshot| snapshot.to_edge(direction, buff))
    }

    pub fn to_corner(&mut self, direction: Vec2, buff: f32) -> Result<&mut Self, AuthoringError> {
        self.map(|snapshot| snapshot.to_corner(direction, buff))
    }
}

/// Pending parallel `Scene::play` call. Timing is transient; applying it writes
/// ordinary explicit tracks to the canonical [`SceneDefinition`].
pub struct Play<'a> {
    scene: &'a mut Scene,
    animations: Vec<Animation>,
    rate_func: Option<RateFunction>,
}

impl Play<'_> {
    /// Set the Manim-compatible rate function for this play call.
    pub fn rate_func(mut self, rate_func: RateFunction) -> Self {
        self.rate_func = Some(rate_func);
        self
    }

    /// Backwards-compatible spelling for older Noon Rust authoring code.
    pub fn with_easing(self, easing: Easing) -> Self {
        self.rate_func(easing)
    }

    pub fn run_time(self, duration: f64) -> Result<(), AuthoringError> {
        let checkpoint = self.scene.clone();
        match self
            .scene
            .schedule(self.animations, duration, self.rate_func)
        {
            Ok(()) => Ok(()),
            Err(error) => {
                *self.scene = checkpoint;
                Err(error)
            }
        }
    }
}

pub use crate::analytic_geometry_authoring::*;
pub use crate::arc_authoring::*;
pub use crate::camera_authoring::*;
pub use crate::dashed_line_authoring::*;
pub use crate::elbow_authoring::*;
pub use crate::geometry_authoring::*;
pub use crate::line_matcher_authoring::*;
pub use crate::polygram_authoring::*;
pub use crate::rounded_rectangle_authoring::*;
pub use crate::sector_authoring::*;
pub use crate::shape_matcher_authoring::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facade_uses_core_named_vocabulary_without_new_scene_model() {
        let mut scene = Scene::new();
        let circle = scene.add(Circle::new(0.5).color(BLUE).shift(LEFT));
        let square = scene.add(Square::new(1.0).color(PINK));
        scene
            .edit(square)
            .unwrap()
            .next_to(circle, RIGHT, DEFAULT_MOBJECT_TO_MOBJECT_BUFFER)
            .unwrap();

        assert_eq!(scene.definition().objects().len(), 2);
        let circle_snapshot = scene.snapshot(circle).unwrap();
        assert_eq!(circle_snapshot.style.fill, Some(BLUE));
        let square_snapshot = scene.snapshot(square).unwrap();
        let gap = square_snapshot.world_bounds().unwrap().min.x
            - circle_snapshot.world_bounds().unwrap().max.x;
        assert!((gap - DEFAULT_MOBJECT_TO_MOBJECT_BUFFER).abs() < 1e-6);
    }

    #[test]
    fn animate_builder_lowers_directly_to_transform_track_and_chains_targets() {
        let mut scene = Scene::new();
        let circle = scene.add(Circle::new(0.5).color(BLUE));

        scene
            .play(circle.animate().shift(RIGHT).set_color(PURPLE))
            .rate_func(RateFunction::EaseInOutCubic)
            .run_time(1.5)
            .unwrap();
        scene
            .play(circle.animate().shift(UP).rotate(90.0 * DEGREES))
            .run_time(0.5)
            .unwrap();

        assert_eq!(scene.time(), 2.0);
        assert_eq!(scene.definition().tracks().len(), 2);
        assert_eq!(scene.definition().tracks()[0].property, Property::Transform);
        assert_eq!(
            scene.definition().tracks()[0].timing.easing,
            RateFunction::EaseInOutCubic
        );
        assert_eq!(scene.definition().tracks()[1].property, Property::Transform);
        assert_eq!(
            scene.definition().tracks()[1].timing.easing,
            RateFunction::Smooth
        );
        let target = scene.snapshot(circle).unwrap();
        assert_eq!(target.transform.translation, RIGHT + UP);
        assert_eq!(target.style.fill, Some(PURPLE));
        assert!((target.transform.rotation - PI / 2.0).abs() < 1e-6);
    }

    #[test]
    fn explicit_rotate_uses_scalar_rotation_while_animate_rotate_uses_transform() {
        let mut scene = Scene::new();
        let left = scene.add(Square::default().shift(LEFT * 2.0));
        let right = scene.add(Square::default().shift(RIGHT * 2.0));

        scene
            .play((left.animate().rotate(PI), Rotate::new(right, PI)))
            .run_time(2.0)
            .unwrap();

        let tracks = scene.definition().tracks();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].property, Property::Transform);
        assert_eq!(tracks[1].property, Property::Rotation);
        assert_eq!(tracks[0].timing.start_time, 0.0);
        assert_eq!(tracks[1].timing.start_time, 0.0);
        assert_eq!(tracks[0].timing.duration, 2.0);
        assert_eq!(tracks[1].timing.duration, 2.0);
        assert_eq!(tracks[0].timing.easing, RateFunction::Smooth);
        assert_eq!(tracks[1].timing.easing, RateFunction::Smooth);
        assert_eq!(tracks[1].values, TrackValues::Scalar { from: 0.0, to: PI });
        assert!((scene.snapshot(left).unwrap().transform.rotation - PI).abs() < 1e-6);
        assert!((scene.snapshot(right).unwrap().transform.rotation - PI).abs() < 1e-6);
        assert_eq!(scene.time(), 2.0);
    }

    #[test]
    fn legacy_with_easing_spelling_still_selects_the_same_rate_function() {
        let mut scene = Scene::new();
        let circle = scene.add(Circle::new(0.5));

        scene
            .play(circle.animate().shift(RIGHT))
            .with_easing(Easing::EaseInOutCubic)
            .run_time(1.0)
            .unwrap();

        assert_eq!(
            scene.definition().tracks()[0].timing.easing,
            RateFunction::EaseInOutCubic
        );
    }

    #[test]
    fn parallel_play_shares_start_and_advances_cursor_once() {
        let mut scene = Scene::new();
        let left = scene.add(Circle::new(0.4).shift(LEFT));
        let right = scene.add(Square::new(0.8).shift(RIGHT));

        scene
            .play((left.animate().shift(UP), right.animate().shift(DOWN)))
            .run_time(2.0)
            .unwrap();

        assert_eq!(scene.definition().tracks().len(), 2);
        assert_eq!(scene.definition().tracks()[0].timing.start_time, 0.0);
        assert_eq!(scene.definition().tracks()[1].timing.start_time, 0.0);
        assert_eq!(scene.time(), 2.0);
    }

    #[test]
    fn create_lowers_to_presence_and_reveal_without_rewriting_geometry() {
        let mut scene = Scene::new();
        let circle = scene.add(
            Circle::new(0.75)
                .set_fill(None, None)
                .set_stroke(Some(BLUE), Some(0.08)),
        );

        scene
            .play(Create::new(circle))
            .rate_func(RateFunction::EaseInOutCubic)
            .run_time(2.0)
            .unwrap();

        assert!(matches!(
            &scene.snapshot(circle).unwrap().geometry,
            GeometryRef::Circle { radius } if *radius == 0.75
        ));
        let tracks = scene.definition().tracks();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].property, Property::Presence);
        assert_eq!(tracks[0].timing.start_time, 0.0);
        assert_eq!(tracks[1].property, Property::Reveal);
        assert_eq!(tracks[1].timing.start_time, 0.0);
        assert_eq!(tracks[1].timing.duration, 2.0);
        assert_eq!(tracks[1].timing.easing, RateFunction::EaseInOutCubic);
        assert_eq!(tracks[1].values, TrackValues::Scalar { from: 0.0, to: 1.0 });
        assert_eq!(scene.time(), 2.0);
    }

    #[test]
    fn create_can_reintroduce_an_absent_object_but_not_redraw_a_present_one() {
        let mut scene = Scene::new();
        let line = scene.add(Line::default());
        scene.play(Create::new(line)).run_time(0.5).unwrap();
        assert!(matches!(
            scene.play(Create::new(line)).run_time(0.5),
            Err(AuthoringError::CreateRequiresAbsent(_))
        ));

        scene.play(FadeOut::new(line)).run_time(0.5).unwrap();
        scene.play(Create::new(line)).run_time(0.5).unwrap();
        assert_eq!(
            scene
                .definition()
                .tracks()
                .iter()
                .filter(|track| track.property == Property::Reveal)
                .count(),
            2
        );
        assert_eq!(
            scene
                .definition()
                .tracks()
                .iter()
                .filter(|track| track.property == Property::Appearance)
                .count(),
            2
        );
    }

    #[test]
    fn fade_uses_appearance_without_rewriting_semantic_opacity() {
        let mut scene = Scene::new();
        let circle = scene.add(Circle::new(0.5).color(BLUE).set_opacity(0.42));

        scene.play(FadeOut::new(circle)).run_time(0.5).unwrap();
        scene.wait(0.25).unwrap();
        scene.play(FadeIn::new(circle)).run_time(0.5).unwrap();

        assert_eq!(scene.snapshot(circle).unwrap().style.opacity, 0.42);
        let properties: Vec<_> = scene
            .definition()
            .tracks()
            .iter()
            .map(|track| track.property)
            .collect();
        assert_eq!(
            properties
                .iter()
                .filter(|&&p| p == Property::Appearance)
                .count(),
            2
        );
        assert_eq!(
            properties
                .iter()
                .filter(|&&p| p == Property::Presence)
                .count(),
            2
        );
    }

    #[test]
    fn direct_mutation_after_animation_is_rejected() {
        let mut scene = Scene::new();
        let circle = scene.add(Circle::new(0.5));
        scene
            .play(circle.animate().shift(RIGHT))
            .run_time(1.0)
            .unwrap();
        assert!(matches!(
            scene.edit(circle),
            Err(AuthoringError::StaticMutationAfterAnimation(_))
        ));
    }
}
