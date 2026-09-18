#![forbid(unsafe_code)]

#[cfg(target_arch = "wasm32")]
mod authoring_arc;
#[cfg(target_arch = "wasm32")]
mod authoring_arrow;
#[cfg(target_arch = "wasm32")]
mod authoring_arrow_endpoints;
#[cfg(target_arch = "wasm32")]
mod authoring_brace;
#[cfg(target_arch = "wasm32")]
mod authoring_coordinates;
mod authoring_error;
#[cfg(target_arch = "wasm32")]
mod authoring_geometry;
#[cfg(target_arch = "wasm32")]
mod authoring_image;
#[cfg(target_arch = "wasm32")]
mod authoring_implicit_plotting;
mod authoring_mobject;
#[cfg(target_arch = "wasm32")]
mod authoring_number_labels;
#[cfg(target_arch = "wasm32")]
mod authoring_number_plane;
mod authoring_options;
#[cfg(target_arch = "wasm32")]
mod authoring_plot_presentation;
#[cfg(target_arch = "wasm32")]
mod authoring_plotting;
#[cfg(target_arch = "wasm32")]
mod authoring_svg;
#[cfg(target_arch = "wasm32")]
mod authoring_synchronized_plotting;
#[cfg(target_arch = "wasm32")]
mod authoring_tangent_line;
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
#[cfg(all(
    feature = "renderer",
    target_arch = "wasm32",
    any(debug_assertions, feature = "renderer-smoke")
))]
mod matching_shapes_smoke;
#[cfg(test)]
mod morph_preload_cases;
#[cfg(any(target_arch = "wasm32", test))]
mod plot_error;
#[cfg(all(
    feature = "renderer",
    target_arch = "wasm32",
    any(debug_assertions, feature = "renderer-smoke")
))]
mod raster_image_smoke;
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
mod retained_image_transport;
mod retained_resource_mutation_encoder;
mod retained_resource_mutation_transport;
mod retained_resource_transport;
#[cfg(feature = "renderer")]
mod retained_typst_canvas;
mod semantic_execution_player;
#[cfg(target_arch = "wasm32")]
mod text_colors;
#[cfg(target_arch = "wasm32")]
mod text_parts;

#[cfg(target_arch = "wasm32")]
pub use authoring_arrow::*;
#[cfg(target_arch = "wasm32")]
pub use authoring_coordinates::*;
pub use authoring_error::AuthoringFailure;
#[cfg(target_arch = "wasm32")]
pub use authoring_geometry::*;
#[cfg(target_arch = "wasm32")]
pub use authoring_image::*;
pub use authoring_mobject::*;
#[cfg(target_arch = "wasm32")]
pub use authoring_number_labels::*;
pub use authoring_options::*;
#[cfg(target_arch = "wasm32")]
pub use authoring_plot_presentation::*;
#[cfg(target_arch = "wasm32")]
pub use authoring_plotting::*;
#[cfg(target_arch = "wasm32")]
pub use authoring_synchronized_plotting::*;
pub use canonical_authoring_scene::*;
pub use clock::{ClockError, PlaybackClock};
pub use determinism::*;
#[cfg(all(feature = "renderer", target_arch = "wasm32", debug_assertions))]
pub use direct_execution_smoke::*;
#[cfg(all(feature = "renderer", target_arch = "wasm32"))]
pub use execution_canvas::*;
pub use execution_transport::*;
pub use execution_wake::*;
#[cfg(all(
    feature = "renderer",
    target_arch = "wasm32",
    any(debug_assertions, feature = "renderer-smoke")
))]
pub use matching_shapes_smoke::*;
#[cfg(all(
    feature = "renderer",
    target_arch = "wasm32",
    any(debug_assertions, feature = "renderer-smoke")
))]
pub use raster_image_smoke::*;
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
pub use retained_image_transport::{TransportImageResourceHandle, TransportImageSampling};
pub use retained_resource_mutation_encoder::*;
pub use retained_resource_mutation_transport::*;
pub use retained_resource_transport::*;
#[cfg(all(feature = "renderer", target_arch = "wasm32"))]
pub use retained_typst_canvas::*;
pub use semantic_execution_player::*;
#[cfg(target_arch = "wasm32")]
pub use text_colors::*;
#[cfg(target_arch = "wasm32")]
pub use text_parts::*;
