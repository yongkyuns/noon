//! Direct Rust authoring and coherent live execution for Noon.
//!
//! Start with [`Scene`] and [`Mobject`]. Before execution, their edits author the
//! scene. After lowering, use [`Scene::live`] for edits and effective observations:
//! it publishes through the existing [`ExecutionSession`], not a second scene.
//!
//! # Public surface
//!
//! - The crate root and [`prelude`] contain explicit authoring values, handles,
//!   live operations, logical completion, and their errors.
//! - [`integration`] contains raw arena access types, host/callback plumbing and
//!   renderer observations. These are advanced facilities, not ordinary authoring.
//! - `diagnostics` (feature gated) provides explicit debug/export observations.
//!
//! Use [`LiveSession::authored`] for base state and [`LiveSession::effective`] for
//! the current published value. Complete a segment with
//! [`LiveSession::complete_segment`] before resuming dependent authoring. Ordinary
//! completion is not a GPU-retirement fence. See the `shared_authoring` example.
//!
//! A glob import cannot accidentally expose raw semantic storage or frame plumbing:
//!
//! ```compile_fail,E0433
//! use noon::*;
//! let _ = SemanticStore::new();
//! ```
//!
//! ```compile_fail,E0432
//! use noon::FrameState;
//! ```
//!
//! Implementation modules are not an alternative public facade:
//!
//! ```compile_fail,E0603
//! use noon::semantic_mobject::Mobject;
//! ```
//!
//! Raw mutable storage requires the explicitly named integration accessors:
//!
//! ```compile_fail,E0599
//! let _ = noon::Scene::new().store();
//! ```
//!
//! ```compile_fail,E0599
//! fn raw(object: &noon::Mobject) { let _ = object.store(); }
//! ```
//!
//! ```compile_fail,E0599
//! fn raw(family: &noon::MobjectFamily) { let _ = family.store(); }
//! ```

#![forbid(unsafe_code)]

mod animation_authoring;
mod arc_authoring;
mod arrow_authoring;
mod authoring_error;
mod boolean_authoring;
mod camera_authoring;
mod compact_value_authoring;
mod dashed_line_authoring;
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
mod dimension_fit;
mod elbow_authoring;
pub mod example_scenes;
mod execution_segment;
mod execution_session;
mod family_affine;
mod family_arrangement;
mod family_authoring;
mod family_callback_paint;
mod family_callback_translation;
mod family_copy;
mod family_gradient;
mod family_grid;
mod family_layout;
mod family_style;
mod focus_on_authoring;
mod geometry_authoring;
mod host_callbacks;
pub mod integration;
mod live_program;
mod live_session;
mod native_signal_authoring;
mod path_alignment;
mod path_editing;
mod path_queries;
mod path_smoothing;
mod point_matching;
mod rotation_authoring;
mod rounded_rectangle_authoring;
mod scalar_authoring;
mod scene;
mod scene_membership;
mod sector_authoring;
mod semantic_mobject;
mod state_replacement;
#[cfg(any(feature = "native-text", feature = "typst"))]
mod text_authoring;
#[cfg(any(feature = "native-text", feature = "typst"))]
mod text_part_authoring;
mod vector_field_authoring;
mod z_index;

pub use animation_authoring::DeclaredAnimation;
pub use arc_authoring::ArcAuthoringError;
pub use arrow_authoring::{
    ManimArrow, ManimArrowOptions, DEFAULT_ARROW_STROKE_WIDTH, DEFAULT_ARROW_STROKE_WIDTH_RATIO,
    DEFAULT_ARROW_TIP_LENGTH, DEFAULT_ARROW_TIP_LENGTH_RATIO,
};
pub use authoring_error::{AuthoringError, UnsupportedAuthoringOperation};
pub use boolean_authoring::{BooleanOperation, BooleanPathError};
pub use dashed_line_authoring::DashedLineAuthoringError;
pub use dimension_fit::LayoutDimension;
pub use elbow_authoring::ElbowAuthoringError;
pub use execution_segment::{
    ExecutionSegment, ExecutionSegmentAdvanceError, ExecutionSegmentError, ExecutionSegmentState,
};
pub use execution_session::{
    ExecutionSegmentCompletionError, ExecutionSession, ExecutionSessionAnimationError,
    ExecutionSessionCallbackError, ExecutionSessionCallbackReadError, ExecutionSessionCameraError,
    ExecutionSessionCreateError, ExecutionSessionFadeError, ExecutionSessionInputError,
    ExecutionSessionPublicationError, SignalTimelineAppendError,
};
pub use family_arrangement::FamilyArrangeOptions;
pub use family_authoring::{MobjectFamily, MobjectFamilyMember};
pub use family_callback_paint::{FamilyCallbackPaintError, FamilyPaint};
pub use family_callback_translation::{CallbackFamilyTranslation, FamilyCallbackTranslationError};
pub use family_copy::FamilyCopy;
pub use family_grid::{FamilyGridOptions, GridFlow};
pub use family_layout::{FamilyLayout, FamilyLayoutTarget, LayoutAnchor};
pub use family_style::StyleUpdate;
pub use focus_on_authoring::FocusOnOptions;
pub use host_callbacks::{RustHostCallbackContext, RustHostCallbackError, RustHostCallbackTable};
pub use live_program::{
    ContinuationStep, LiveContinuation, LiveProgram, LiveProgramError, LiveProgramStatus,
};
pub use live_session::{
    AffineLifecycleDirection, AffineLifecycleEndpoint, AnimationCompositionRequest,
    DrawBorderThenFillOptions, EffectiveMobjectLayout, EffectiveMobjectState, FadeEndpoint,
    FadeTranslation, IndicateOptions, LiveLayoutTarget, LiveSession, LiveSessionError,
    SubsetDisplayMode, TransformToRequest,
};
pub use native_signal_authoring::{NativeBoolSignal, NativeVectorSignal};
pub use noon_core::{
    AnimationOptions, Bounds2D64, Color, ExecutionRevision, FrameEpoch, GeometryRef, PathCommand,
    PublicationContext, RateFunction, Rect, SceneRevision, SemanticAnimationCompositionKind,
    SemanticFadeDirection, SemanticNodeId, SemanticObjectProperty, SemanticObjectState,
    SemanticPaint, SemanticSignalValue, SemanticStyle, SemanticTransform2_5D,
    SemanticTransformInterpolation, SemanticVec3, StoredGeometry, StrokeCap, StrokeJoin,
    StrokeWidthMode, Style, TextPart, TextPartQueryError, TextSourceSpan, Transform2D, Vec2,
    VectorPath, BLACK, BLUE, BLUE_A, BLUE_B, BLUE_C, BLUE_D, BLUE_E, DEFAULT_FRAME_HEIGHT,
    DEFAULT_FRAME_WIDTH, DEFAULT_MOBJECT_TO_EDGE_BUFFER, DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    DEGREES, DL, DOWN, DR, GOLD, GRAY, GREEN, GREEN_A, GREEN_B, GREEN_C, GREEN_D, GREEN_E, GREY,
    LARGE_BUFF, LEFT, LIGHT_PINK, MAROON, MED_LARGE_BUFF, MED_SMALL_BUFF, ORANGE, ORIGIN, PI, PINK,
    PURPLE, PURPLE_A, PURPLE_B, PURPLE_C, PURPLE_D, PURPLE_E, RED, RED_A, RED_B, RED_C, RED_D,
    RED_E, RIGHT, SMALL_BUFF, TAU, TEAL, TEAL_A, TEAL_B, TEAL_C, TEAL_D, TEAL_E, UL, UP, UR, WHITE,
    YELLOW, YELLOW_A, YELLOW_B, YELLOW_C, YELLOW_D, YELLOW_E,
};
pub use noon_geometry::{
    StaticVectorFieldError, VectorFieldAxis, VectorFieldAxisRange, VectorFieldPoint,
    VectorFieldRanges2D, DEFAULT_VECTOR_FIELD_STEP,
};
pub use noon_runtime::EvaluationError;
pub use path_queries::PathQuery;
pub use rotation_authoring::ManimRotationPivot;
pub use rounded_rectangle_authoring::RoundedRectangleAuthoringError;
pub use scalar_authoring::{TrackerPosition, ValueTracker, ValueTrackerPlay};
pub use scene::Scene;
pub use scene_membership::SceneMembershipRequest;
pub use semantic_mobject::{ManimGeometryOptions, ManimLineEndpoints, ManimNextToArgs, Mobject};
pub use state_replacement::ManimBecomeOptions;
#[cfg(any(feature = "native-text", feature = "typst"))]
pub use text_authoring::TextAuthoringError;
#[cfg(feature = "typst")]
pub use text_authoring::{MathTypst, Typst, TypstBackendError, DEFAULT_TYPST_FONT_SIZE};
#[cfg(feature = "native-text")]
pub use text_authoring::{
    NativeFontFace, Text, DEFAULT_NATIVE_TEXT_FONT_FAMILY, DEFAULT_NATIVE_TEXT_FONT_SIZE,
};
#[cfg(any(feature = "native-text", feature = "typst"))]
pub use text_part_authoring::TextPartAuthoringError;
pub use vector_field_authoring::{ArrowVectorFieldAuthoringError, ManimArrowVectorField};

/// Common imports for direct typed semantic authoring and live publication.
/// Host integration and mutable arena access must be imported explicitly.
pub mod prelude {
    pub use crate::{
        AnimationOptions, ArrowVectorFieldAuthoringError, AuthoringError, BooleanOperation, Color,
        ContinuationStep, DeclaredAnimation, DrawBorderThenFillOptions, EffectiveMobjectState,
        ExecutionSession, FadeEndpoint, FadeTranslation, LiveContinuation, LiveProgram,
        LiveSession, LiveSessionError, ManimArrow, ManimArrowOptions, ManimArrowVectorField, Mobject,
        MobjectFamily, MobjectFamilyMember, NativeBoolSignal, NativeVectorSignal, RateFunction,
        Scene, SemanticObjectState, SemanticStyle, StoredGeometry, TrackerPosition, ValueTracker,
        Vec2, VectorFieldAxisRange, VectorFieldPoint, VectorFieldRanges2D, VectorPath,
    };
}

pub use family_gradient::color_gradient;
