//! Ergonomic Rust authoring facade for Noon.
//!
//! The established deterministic authoring surface lives in `legacy`; native
//! reactive authoring is layered beside it without introducing another persisted
//! scene model. Both lower into `noon_core` semantics.

#![forbid(unsafe_code)]

mod analytic_geometry_authoring;
mod arc_authoring;
mod area_authoring;
mod axis_tick_authoring;
mod camera_authoring;
mod coordinate_system_authoring;
mod coordinate_transform_authoring;
mod dashed_line_authoring;
mod elbow_authoring;
mod geometry_authoring;
mod graph_query_authoring;
mod legacy;
mod line_graph_authoring;
mod line_matcher_authoring;
mod number_plane_authoring;
mod plot_geometry_authoring;
mod plot_sampling_authoring;
mod polar_plane_authoring;
mod polygram_authoring;
mod reactive_authoring;
mod riemann_authoring;
mod rounded_rectangle_authoring;
mod sector_authoring;
mod shape_matcher_authoring;
mod text_authoring;

pub use analytic_geometry_authoring::*;
pub use arc_authoring::*;
pub use area_authoring::*;
pub use axis_tick_authoring::*;
pub use camera_authoring::*;
pub use coordinate_system_authoring::*;
pub use coordinate_transform_authoring::*;
pub use dashed_line_authoring::*;
pub use elbow_authoring::*;
pub use geometry_authoring::*;
pub use graph_query_authoring::*;
pub use legacy::*;
pub use line_graph_authoring::*;
pub use line_matcher_authoring::*;
pub use number_plane_authoring::*;
pub use plot_geometry_authoring::*;
pub use plot_sampling_authoring::*;
pub use polar_plane_authoring::*;
pub use polygram_authoring::*;
pub use reactive_authoring::*;
pub use riemann_authoring::*;
pub use rounded_rectangle_authoring::*;
pub use sector_authoring::*;
pub use shape_matcher_authoring::*;
pub use text_authoring::*;

/// Common imports for deterministic and native-reactive Noon authoring.
pub mod prelude {
    pub use crate::legacy::prelude::*;
    pub use crate::{
        axes_function_vector_path, axes_line_graph_vector_path, parametric_vector_path,
        transformed_axes_function_vector_path, transformed_axes_line_graph_vector_path,
        transformed_axes_sampled_values_vector_path, transformed_graph_point_for_x,
        transformed_graph_point_from_proportion, AnnularSector, Annulus, Arc, ArcAuthoringError,
        ArcBetweenPoints, AreaAuthoringError, AreaSamplePlan, Axes2DState, AxisTickError,
        BackgroundRectangle, CoordinateSystemError, Cross, DashedLine, DashedLineAuthoringError,
        Dot, Elbow, ElbowAuthoringError, Ellipse, GeometryAuthoringError, GraphParameterRange,
        GraphQueryError, LineGraphAuthoringError, LineMatcherAuthoringError, MathTypst,
        MovingCameraScene, NumberLineGeometryPlan, NumberLineState, NumberLineTick,
        NumberLineTickOptions, NumberPlaneAuthoringError, NumberPlaneGridLine, NumberPlaneGridPlan,
        NumberPlaneLineStyle, NumberRange, ParametricSamplePlan, PlotGeometryError,
        PlotRangeRequest, PlotSamplingError, PolarPlaneAuthoringError, PolarPlaneAzimuthDirection,
        PolarPlaneCircle, PolarPlaneGridPlan, PolarPlaneRadialLine, Polygon, Polygram,
        PolygramAuthoringError, ReactiveScene, ReactiveTimelineScene, RegularPolygon,
        RegularPolygram, RetainedMobject, RetainedScene, RiemannAuthoringError,
        RiemannRectangleGeometry, RiemannSample, RiemannSamplePlan, RiemannSampleType,
        RoundedRectangle, RoundedRectangleAuthoringError, SampleRange, SampleSpan, Sector,
        ShapeMatcherAuthoringError, Star, SurroundingRectangle, Text, TextAuthoringError,
        TransformedAxes2DState, TransformedNumberLineState, Triangle, Typst, Underline,
        ValueTracker, VectorSignal, BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY,
        DEFAULT_CROSS_SCALE_FACTOR, DEFAULT_CROSS_STROKE_WIDTH, DEFAULT_DASHED_RATIO,
        DEFAULT_DASH_LENGTH, DEFAULT_DOT_RADIUS, DEFAULT_ELBOW_ANGLE, DEFAULT_ELBOW_WIDTH,
        DEFAULT_NATIVE_TEXT_FONT_FAMILY, DEFAULT_NATIVE_TEXT_FONT_SIZE,
        DEFAULT_ROUNDED_RECTANGLE_CORNER_RADIUS, DEFAULT_UNDERLINE_BUFF,
        MANIM_DEFAULT_DISCONTINUITY_DT, MANIM_DEFAULT_PARAMETRIC_STEP, MANIM_DEFAULT_RIEMANN_DX,
        MANIM_DEFAULT_RIEMANN_WIDTH_SCALE_FACTOR, MANIM_GRAPH_X_SEARCH_TOLERANCE,
        MANIM_SAMPLED_GRAPH_POINTS_PER_TICK, SURROUNDING_RECTANGLE_DEFAULT_COLOR,
    };
    pub use noon_core::{
        resolve_animation_options, resolve_composition_schedule, resolve_lifecycle_plan,
        resolve_uniform_composition_schedule, validate_presence_transition, AnimationDefaults,
        AnimationOptions, AnimationOptionsError, CompositionError, CompositionInterval,
        CompositionSchedule, LifecycleBinding, LifecycleError, LifecycleIntent, LifecyclePlan,
        LifecycleState, PresenceTransitionError, ResolvedAnimationOptions,
    };
}
