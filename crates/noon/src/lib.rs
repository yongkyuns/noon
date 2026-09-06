//! Ergonomic Rust authoring facade for Noon.
//!
//! `Scene` and `Mobject` author directly into the shared semantic store and
//! lower through `ExecutionSession`. Snapshot-era APIs are explicit in `legacy`.

#![forbid(unsafe_code)]

mod analytic_geometry_authoring;
mod animation_authoring;
mod arc_authoring;
mod camera_authoring;
mod dashed_line_authoring;
mod elbow_authoring;
pub mod example_scenes;
mod execution_segment;
mod execution_session;
mod family_authoring;
mod geometry_authoring;
mod host_callbacks;
pub mod legacy;
mod line_matcher_authoring;
mod live_program;
mod live_session;
mod native_signal_authoring;
mod polygram_authoring;
mod retained_family_authoring_lowering;
mod rounded_rectangle_authoring;
mod scalar_authoring;
mod sector_authoring;
pub mod semantic_mobject;
mod shape_matcher_authoring;
mod text_authoring;

pub use animation_authoring::DeclaredAnimation;
pub use execution_segment::*;
pub use execution_session::*;
pub use family_authoring::MobjectFamily;
pub use host_callbacks::*;
pub use live_program::*;
pub use live_session::{
    AffineLifecycleDirection, AffineLifecycleEndpoint, AnimationCompositionRequest,
    EffectiveMobjectLayout, EffectiveMobjectState, IndicateOptions, LiveSession, LiveSessionError,
    TransformToRequest,
};
pub use native_signal_authoring::{NativeBoolSignal, NativeVectorSignal};
pub use noon_core::*;
pub use noon_runtime::{
    EffectiveObjectProperties, EvaluationError, FrameChanges, FrameObjectState, FrameState,
    RendererPublication, RuntimeIdentity, RuntimeWakeState, TimelineWakeState,
};
pub use retained_family_authoring_lowering::*;
pub use scalar_authoring::{TrackerPosition, ValueTracker, ValueTrackerPlay};
pub use semantic_mobject::{ManimPrimitiveOptions, Mobject};
mod scene;
pub use scene::Scene;
pub use text_authoring::*;

/// Common imports for direct typed semantic authoring.
pub mod prelude {
    pub use crate::{
        ContinuationStep, DeclaredAnimation, EffectiveMobjectState, ExecutionSession,
        LiveContinuation, LiveProgram, LiveSession, Mobject, MobjectFamily, NativeBoolSignal,
        NativeVectorSignal, Scene, TrackerPosition, ValueTracker,
    };
    pub use noon_core::{
        Color, SemanticObjectState, SemanticStyle, StoredGeometry, Vec2, VectorPath,
    };
}
