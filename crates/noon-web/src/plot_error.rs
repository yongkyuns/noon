//! Shared plotting diagnostics for optional language projections.
pub(crate) use noon::integration::coordinate_failure;
#[cfg(target_arch = "wasm32")]
pub(crate) use noon::integration::plot_failure;
