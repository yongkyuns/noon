//! Python value conversion over the common Rust option resolver.
use crate::engine_error;
use pyo3::prelude::*;
#[pyclass(module = "_noon_native")]
pub struct OptionsResolution(noon_core::ResolvedAnimationOptions);
#[pymethods]
impl OptionsResolution {
    #[getter(runTime)]
    fn run_time(&self) -> f64 {
        self.0.run_time
    }
    #[getter(rateFunc)]
    fn rate_func(&self) -> &'static str {
        self.0.rate_func.semantic_id()
    }
    #[getter(lagRatio)]
    fn lag_ratio(&self) -> f64 {
        self.0.lag_ratio
    }
    #[getter(pathArc)]
    fn path_arc(&self) -> f64 {
        self.0.path_arc
    }
    #[getter(reverseRateFunction)]
    fn reverse(&self) -> bool {
        self.0.reverse_rate_function
    }
}
#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn resolve_animation_options(
    default_lag: f64,
    duration: f64,
    rate: &str,
    lag: f64,
    path: f64,
    reverse: i32,
    play_duration: f64,
    play_rate: &str,
    play_lag: f64,
) -> PyResult<OptionsResolution> {
    noon::integration::resolve_frontend_animation_options(
        default_lag,
        duration,
        rate,
        lag,
        path,
        reverse,
        play_duration,
        play_rate,
        play_lag,
    )
    .map(OptionsResolution)
    .map_err(engine_error)
}
#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn resolve_transform_options(
    default_lag: f64,
    duration: f64,
    rate: &str,
    lag: f64,
    path: f64,
    reverse: i32,
    play_duration: f64,
    play_rate: &str,
    play_lag: f64,
    play_path: f64,
) -> PyResult<OptionsResolution> {
    noon::integration::resolve_frontend_transform_animation_options(
        default_lag,
        duration,
        rate,
        lag,
        path,
        reverse,
        play_duration,
        play_rate,
        play_lag,
        play_path,
    )
    .map(OptionsResolution)
    .map_err(engine_error)
}
