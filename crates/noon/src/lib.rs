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
mod execution_segment;
mod execution_session;
mod geometry_authoring;
pub mod legacy;
mod line_matcher_authoring;
mod live_session;
mod polygram_authoring;
mod reactive_authoring;
mod retained_family_authoring_lowering;
mod rounded_rectangle_authoring;
mod sector_authoring;
pub mod semantic_mobject;
mod shape_matcher_authoring;
mod text_authoring;

pub use animation_authoring::DeclaredAnimation;
pub use execution_segment::*;
pub use execution_session::*;
pub use live_session::{EffectiveMobjectState, LiveSession, LiveSessionError};
pub use noon_core::*;
pub use noon_runtime::{
    EvaluationError, FrameChanges, FrameObjectState, FrameState, RendererPublication,
    RuntimeWakeState, TimelineWakeState,
};
pub use reactive_authoring::*;
pub use retained_family_authoring_lowering::*;
pub use semantic_mobject::Mobject;
mod scene;
pub use scene::Scene;
pub use text_authoring::*;

/// Common imports for direct typed semantic authoring.
pub mod prelude {
    pub use crate::{
        DeclaredAnimation, EffectiveMobjectState, ExecutionSession, LiveSession, Mobject, Scene,
    };
    pub use noon_core::{
        Color, SemanticObjectState, SemanticStyle, StoredGeometry, Vec2, VectorPath,
    };
}
