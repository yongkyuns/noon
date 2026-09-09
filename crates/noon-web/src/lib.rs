#![forbid(unsafe_code)]

#[cfg(target_arch = "wasm32")]
mod authoring_geometry;
mod authoring_mobject;
mod authoring_options;
mod canonical_authoring_scene;
mod clock;
mod determinism;
#[cfg(all(feature = "renderer", target_arch = "wasm32", debug_assertions))]
mod direct_execution_smoke;
#[cfg(feature = "renderer")]
mod execution_canvas;
mod execution_transport;
mod execution_wake;
#[cfg(any(target_arch = "wasm32", test))]
mod geometry_export;
#[cfg(all(feature = "renderer", any(target_arch = "wasm32", test)))]
mod gpu_diagnostics;
#[cfg(all(feature = "renderer", target_arch = "wasm32"))]
mod gpu_timestamps;
mod manim_scale_bridge;
#[cfg(target_arch = "wasm32")]
mod manim_shape_matcher_handle_bridge;
mod renderer_observation;
#[cfg(all(
    feature = "renderer",
    target_arch = "wasm32",
    any(debug_assertions, feature = "renderer-smoke")
))]
mod renderer_recovery_smoke;
#[cfg(feature = "renderer")]
mod retained_execution_canvas;
mod retained_execution_resources;
mod retained_execution_transport;
mod retained_family_execution_encoder;
mod retained_family_execution_transport;
mod retained_family_transport;
mod retained_resource_mutation_encoder;
mod retained_resource_mutation_transport;
mod retained_resource_transport;
mod retained_text_family_transport;
#[cfg(feature = "renderer")]
mod retained_typst_canvas;
mod semantic_execution_player;

#[cfg(target_arch = "wasm32")]
pub use authoring_geometry::*;
pub use authoring_mobject::*;
pub use authoring_options::*;
pub use canonical_authoring_scene::*;
pub use clock::{ClockError, PlaybackClock};
pub use determinism::*;
#[cfg(all(feature = "renderer", target_arch = "wasm32", debug_assertions))]
pub use direct_execution_smoke::*;
#[cfg(all(feature = "renderer", target_arch = "wasm32"))]
pub use execution_canvas::*;
pub use execution_transport::*;
pub use execution_wake::*;
pub use renderer_observation::*;
#[cfg(all(
    feature = "renderer",
    target_arch = "wasm32",
    any(debug_assertions, feature = "renderer-smoke")
))]
pub use renderer_recovery_smoke::*;
#[cfg(all(feature = "renderer", target_arch = "wasm32"))]
pub use retained_execution_canvas::*;
pub use retained_execution_resources::*;
pub use retained_execution_transport::*;
pub use retained_family_execution_encoder::*;
pub use retained_family_execution_transport::*;
pub use retained_family_transport::*;
pub use retained_resource_mutation_encoder::*;
pub use retained_resource_mutation_transport::*;
pub use retained_resource_transport::*;
pub use retained_text_family_transport::*;
#[cfg(all(feature = "renderer", target_arch = "wasm32"))]
pub use retained_typst_canvas::*;
pub use semantic_execution_player::*;
