//! Ergonomic Rust authoring facade for Noon.
//!
//! The established deterministic authoring surface lives in `legacy`; native
//! reactive authoring is layered beside it without introducing another persisted
//! scene model. Both lower into `noon_core` semantics.

#![forbid(unsafe_code)]

mod analytic_geometry_authoring;
mod arc_authoring;
mod camera_authoring;
mod coordinate_system_authoring;
mod coordinate_transform_authoring;
mod elbow_authoring;
mod geometry_authoring;
mod legacy;
mod line_matcher_authoring;
mod polygram_authoring;
mod reactive_authoring;
mod rounded_rectangle_authoring;
mod sector_authoring;
mod shape_matcher_authoring;
mod text_authoring;

pub use analytic_geometry_authoring::*;
pub use arc_authoring::*;
pub use camera_authoring::*;
pub use coordinate_system_authoring::*;
pub use coordinate_transform_authoring::*;
pub use elbow_authoring::*;
pub use geometry_authoring::*;
pub use legacy::*;
pub use line_matcher_authoring::*;
pub use polygram_authoring::*;
pub use reactive_authoring::*;
pub use rounded_rectangle_authoring::*;
pub use sector_authoring::*;
pub use shape_matcher_authoring::*;
pub use text_authoring::*;

/// Common imports for deterministic and native-reactive Noon authoring.
pub mod prelude {
    pub use crate::legacy::prelude::*;
    pub use crate::{
        AnnularSector, Annulus, Arc, ArcAuthoringError, ArcBetweenPoints, Axes2DState,
        BackgroundRectangle, CoordinateSystemError, Cross, Dot, Elbow, ElbowAuthoringError,
        Ellipse, GeometryAuthoringError, LineMatcherAuthoringError, MathTypst, MovingCameraScene,
        NumberLineState, NumberRange, Polygon, Polygram, PolygramAuthoringError, ReactiveScene,
        ReactiveTimelineScene, RegularPolygon, RegularPolygram, RetainedMobject, RetainedScene,
        RoundedRectangle, RoundedRectangleAuthoringError, Sector, ShapeMatcherAuthoringError, Star,
        SurroundingRectangle, Text, TextAuthoringError, TransformedAxes2DState,
        TransformedNumberLineState, Triangle, Typst, Underline, ValueTracker, VectorSignal,
        BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY, DEFAULT_CROSS_SCALE_FACTOR,
        DEFAULT_CROSS_STROKE_WIDTH, DEFAULT_DOT_RADIUS, DEFAULT_ELBOW_ANGLE, DEFAULT_ELBOW_WIDTH,
        DEFAULT_NATIVE_TEXT_FONT_FAMILY, DEFAULT_NATIVE_TEXT_FONT_SIZE,
        DEFAULT_ROUNDED_RECTANGLE_CORNER_RADIUS, DEFAULT_UNDERLINE_BUFF,
        SURROUNDING_RECTANGLE_DEFAULT_COLOR,
    };
    pub use noon_core::{
        resolve_animation_options, resolve_composition_schedule, resolve_lifecycle_plan,
        resolve_uniform_composition_schedule, validate_presence_transition, AnimationDefaults,
        AnimationOptions, AnimationOptionsError, CompositionError, CompositionInterval,
        CompositionSchedule, LifecycleBinding, LifecycleError, LifecycleIntent, LifecyclePlan,
        LifecycleState, PresenceTransitionError, ResolvedAnimationOptions,
    };
}
