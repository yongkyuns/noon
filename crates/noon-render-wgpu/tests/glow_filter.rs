//! Temporary M1 operator qualification entrypoint, owned by #1897.
//!
//! Compile the candidate renderer module before enabling its Scene execution
//! path. This is one implementation, not a test copy or another runtime. Remove
//! this staging entrypoint when `gpu::glow_filter` is wired into retained scene
//! preparation; keep its tests with the renderer module. Do not merge/qualify M1
//! from this operator-only entrypoint.

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
use noon_render_wgpu::{gpu, FramePreparer};

#[path = "../src/gpu/glow_filter.rs"]
pub mod glow_filter;
