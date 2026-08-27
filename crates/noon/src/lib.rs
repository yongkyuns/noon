//! Ergonomic Rust authoring facade for Noon.
//!
//! The established deterministic authoring surface lives in `legacy`; native
//! reactive authoring is layered beside it without introducing another persisted
//! scene model. Both lower into `noon_core` semantics.

#![forbid(unsafe_code)]

mod camera_authoring;
mod geometry_authoring;
mod legacy;
mod reactive_authoring;
mod text_authoring;

pub use camera_authoring::*;
pub use geometry_authoring::*;
pub use legacy::*;
pub use reactive_authoring::*;
pub use text_authoring::*;

/// Common imports for deterministic and native-reactive Noon authoring.
pub mod prelude {
    pub use crate::legacy::prelude::*;
    pub use crate::{
        GeometryAuthoringError, MathTypst, MovingCameraScene, Polygon, ReactiveScene,
        ReactiveTimelineScene, RegularPolygon, RetainedMobject, RetainedScene, TextAuthoringError,
        Triangle, Typst, ValueTracker, VectorSignal,
    };
    pub use noon_core::{
        resolve_animation_options, resolve_composition_schedule, resolve_lifecycle_plan,
        resolve_uniform_composition_schedule, validate_presence_transition, AnimationDefaults,
        AnimationOptions, AnimationOptionsError, CompositionError, CompositionInterval,
        CompositionSchedule, LifecycleBinding, LifecycleError, LifecycleIntent, LifecyclePlan,
        LifecycleState, PresenceTransitionError, ResolvedAnimationOptions,
    };
}
