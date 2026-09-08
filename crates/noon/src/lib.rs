//! Ergonomic Rust authoring facade for Noon.
//!
//! `Scene` and `Mobject` author directly into the shared semantic store and
//! lower through `ExecutionSession` using typed in-process Rust boundaries.

#![forbid(unsafe_code)]

mod animation_authoring;
mod arc_authoring;
mod camera_authoring;
mod compact_value_authoring;
mod dashed_line_authoring;
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
mod elbow_authoring;
pub mod example_scenes;
mod execution_segment;
mod execution_session;
mod family_authoring;
mod focus_on_authoring;
mod rotation_authoring;
pub use rotation_authoring::ManimRotationPivot;
mod geometry_authoring;
mod host_callbacks;
mod live_program;
mod live_session;
mod native_signal_authoring;
mod rounded_rectangle_authoring;
mod scalar_authoring;
mod sector_authoring;
pub mod semantic_mobject;
mod text_authoring;

pub use animation_authoring::DeclaredAnimation;
pub use compact_value_authoring::semantic_object_state_from_compact;
pub use execution_segment::*;
pub use execution_session::*;
pub use family_authoring::{
    semantic_family_leaf_ids, FamilyTranslation, MobjectFamily, MobjectFamilyMember,
};
pub use focus_on_authoring::FocusOnOptions;
pub use host_callbacks::*;
pub use live_program::*;
pub use live_session::{
    AffineLifecycleDirection, AffineLifecycleEndpoint, AnimationCompositionRequest,
    DrawBorderThenFillOptions, EffectiveMobjectLayout, EffectiveMobjectState, FadeEndpoint,
    FadeTranslation, IndicateOptions, LiveSession, LiveSessionError, SubsetDisplayMode,
    TransformToRequest,
};
pub use native_signal_authoring::{NativeBoolSignal, NativeVectorSignal};
pub use noon_core::*;
pub use noon_runtime::{
    EffectiveObjectProperties, EvaluationError, FrameChanges, FrameObjectState, FrameState,
    RendererPublication, RuntimeIdentity, RuntimeWakeState, TimelineWakeState,
};
pub use scalar_authoring::{TrackerPosition, ValueTracker, ValueTrackerPlay};
pub use semantic_mobject::{ManimBecomeOptions, ManimGeometryOptions, ManimLineEndpoints, Mobject};
mod scene;
mod scene_membership;
pub use scene::Scene;
pub use scene_membership::SceneMembershipRequest;
pub use text_authoring::*;

/// Common imports for direct typed semantic authoring.
pub mod prelude {
    pub use crate::{
        ContinuationStep, DeclaredAnimation, DrawBorderThenFillOptions, EffectiveMobjectState,
        ExecutionSession, FadeEndpoint, FadeTranslation, LiveContinuation, LiveProgram,
        LiveSession, Mobject, MobjectFamily, MobjectFamilyMember, NativeBoolSignal,
        NativeVectorSignal, Scene, TrackerPosition, ValueTracker,
    };
    pub use noon_core::{
        Color, SemanticObjectState, SemanticStyle, StoredGeometry, Vec2, VectorPath,
    };
}
