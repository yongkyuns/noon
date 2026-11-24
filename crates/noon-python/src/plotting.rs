//! Native immutable plot input over the same Rust sampling plan as WASM.
use crate::{engine_error, geometry::GeometryOptions};
use pyo3::prelude::*;
#[pyclass(unsendable, module = "_noon_native")]
pub struct PlotSamplingPlan {
    input: Option<noon::integration::PlotSamplingInput>,
}
impl PlotSamplingPlan {
    pub(crate) fn prepare(
        options: noon::PlotSamplingOptions,
        discontinuities: Vec<f64>,
        dt: Option<f64>,
        max_samples: Option<u32>,
        frame: Option<noon::AxesFrame>,
    ) -> PyResult<Self> {
        noon::integration::PlotSamplingInput::prepare(
            options,
            discontinuities,
            dt,
            max_samples,
            frame,
        )
        .map(|input| Self { input: Some(input) })
        .map_err(engine_error)
    }
    fn value(&self) -> PyResult<&noon::integration::PlotSamplingInput> {
        self.input
            .as_ref()
            .ok_or_else(|| engine_error("plot sampling plan has been freed"))
    }
}
#[pymethods]
impl PlotSamplingPlan {
    #[staticmethod]
    #[pyo3(signature=(range,discontinuities,dt,max_samples))]
    fn parametric(
        range: Vec<f64>,
        discontinuities: Vec<f64>,
        dt: Option<f64>,
        max_samples: Option<u32>,
    ) -> PyResult<Self> {
        noon::integration::PlotSamplingInput::parametric(&range, discontinuities, dt, max_samples)
            .map(|input| Self { input: Some(input) })
            .map_err(engine_error)
    }
    fn parameters(&self) -> PyResult<Vec<f64>> {
        Ok(self.value()?.parameters())
    }
    #[pyo3(name = "functionSamples")]
    fn function_samples(&self, values: Vec<f64>, smooth: bool) -> PyResult<GeometryOptions> {
        self.value()?
            .function_samples(&values, smooth)
            .map(GeometryOptions::from_options)
            .map_err(engine_error)
    }
    #[pyo3(name = "parametricSamples")]
    fn parametric_samples(&self, values: Vec<f64>, smooth: bool) -> PyResult<GeometryOptions> {
        self.value()?
            .parametric_samples(&values, smooth)
            .map(GeometryOptions::from_options)
            .map_err(engine_error)
    }
    fn free(&mut self) {
        self.input.take();
    }
}
