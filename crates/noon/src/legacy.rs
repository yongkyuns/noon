//! Remaining geometry value adapters and explicit export codec, deleted by #959.
//! Normal authoring uses the shared `Scene` and `Mobject` at the crate root.

mod semantic_snapshot;
use noon_core::*;
pub use semantic_snapshot::export_mobject_snapshot;

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

pub use crate::analytic_geometry_authoring::*;
pub use crate::arc_authoring::*;
pub use crate::dashed_line_authoring::*;
pub use crate::elbow_authoring::*;
pub use crate::geometry_authoring::*;
pub use crate::line_matcher_authoring::*;
pub use crate::polygram_authoring::*;
pub use crate::rounded_rectangle_authoring::*;
pub use crate::sector_authoring::*;
pub use crate::shape_matcher_authoring::*;
