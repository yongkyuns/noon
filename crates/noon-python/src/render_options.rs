//! Value projection only: all output option meanings live in shared Rust.
use noon::integration::{RenderOptionInputs, ResolvedRenderOptions};
use pyo3::prelude::*;

use crate::engine_error;

#[pyclass(module = "_noon_native")]
pub struct RenderOptionsResolution(ResolvedRenderOptions);

#[pymethods]
impl RenderOptionsResolution {
    #[getter(pixelWidth)]
    fn width(&self) -> u32 {
        self.0.pixel_width
    }
    #[getter(pixelHeight)]
    fn height(&self) -> u32 {
        self.0.pixel_height
    }
    #[getter(frameRateNumerator)]
    fn numerator(&self) -> u32 {
        self.0.frame_rate.numerator()
    }
    #[getter(frameRateDenominator)]
    fn denominator(&self) -> u32 {
        self.0.frame_rate.denominator()
    }
    #[getter(format)]
    fn format(&self) -> &'static str {
        self.0.format.name()
    }
}

#[pyfunction]
#[pyo3(signature = (quality=None, resolution=None, fps=None, format=None, width=None, height=None))]
pub fn resolve_render_options(
    quality: Option<String>,
    resolution: Option<String>,
    fps: Option<String>,
    format: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
) -> PyResult<RenderOptionsResolution> {
    RenderOptionInputs {
        quality: quality.as_deref(),
        resolution: resolution.as_deref(),
        frame_rate: fps.as_deref(),
        format: format.as_deref(),
        pixel_width: width,
        pixel_height: height,
    }
    .resolve()
    .map(RenderOptionsResolution)
    .map_err(engine_error)
}
