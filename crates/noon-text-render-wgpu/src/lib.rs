#![forbid(unsafe_code)]

mod preparation;
pub use preparation::*;

#[cfg_attr(test, allow(clippy::single_range_in_vec_init))]
mod gpu;
pub use gpu::*;
