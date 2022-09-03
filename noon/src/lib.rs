pub mod animation;
pub mod color;
pub mod component;
pub mod consts;
pub mod ease;
pub mod geom;
pub mod object;
pub mod path;
pub mod scene;
pub mod system;

pub use crate::animation::{
    Align, AnimBuilder, Animation, AnimationType, Animations, Arrange, Create, EntityAnimations,
    WithAngle, WithArrange, WithColor, WithFill, WithFontSize, WithId, WithPath, WithPosition,
    WithSize, WithStroke, WithStrokeWeight,
};

pub use crate::color::{Color, ColorExtension};
pub use crate::component::{
    Angle, Depth, FillColor, FontSize, HasFill, Interpolate, Name, Opacity, Origin, PathCompletion,
    Position, Previous, Scale, StrokeColor, StrokeWeight, Transform, Value,
};

pub use crate::geom::{point, BoundingSize, PixelFrame, Point, Size, Vector};
pub use crate::path::{GetPartial, Path, PathComponent, PixelPath};
pub use consts::*;
pub use ease::EaseType;
pub use object::*;
pub use scene::{Bounds, Scene};
pub use system::{animate, init_from_target, print, update_time, Time};

pub use nannou;
pub use nannou::{app, rand};

pub mod prelude {
    pub use crate::animation::{
        Align, AnimBuilder, Animation, AnimationType, Animations, Create, EntityAnimations,
        WithAngle, WithArrange, WithColor, WithFill, WithFontSize, WithId, WithPath, WithPosition,
        WithSize, WithStroke, WithStrokeWeight,
    };
    pub use crate::consts::*;
    pub use crate::{
        geom::Direction,
        object::{CircleId, TextId},
        CircleBuilder, Color, ColorExtension, EaseType, Scene, StrokeWeight, TextBuilder,
    };
    pub use nannou::app;
    pub use nannou::app::ModelFn;
    pub use nannou::geom::Rect;
    pub use nannou::prelude::*;
}
