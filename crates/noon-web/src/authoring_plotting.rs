//! Typed WASM projection of the shared disposable plot preparation.
use crate::{authoring_error::js_error, WasmManimGeometryOptions};
pub(crate) use noon::integration::{coordinate_failure, coordinate_math_failure, plot_failure};
use wasm_bindgen::prelude::*;
pub(crate) fn data_points(values: &[f64]) -> Result<Vec<[f64; 2]>, JsValue> {
    noon::integration::plot_data_points(values).map_err(js_error)
}
#[wasm_bindgen]
pub struct WasmPlotSamplingPlan {
    input: noon::integration::PlotSamplingInput,
}
impl WasmPlotSamplingPlan {
    pub(crate) fn prepare(
        options: noon::PlotSamplingOptions,
        discontinuities: Vec<f64>,
        dt: Option<f64>,
        max_samples: Option<u32>,
        frame: Option<noon::AxesFrame>,
    ) -> Result<Self, JsValue> {
        noon::integration::PlotSamplingInput::prepare(
            options,
            discontinuities,
            dt,
            max_samples,
            frame,
        )
        .map(|input| Self { input })
        .map_err(js_error)
    }
}
#[wasm_bindgen]
impl WasmPlotSamplingPlan {
    pub fn parametric(
        range: &[f64],
        discontinuities: Vec<f64>,
        dt: Option<f64>,
        max_samples: Option<u32>,
    ) -> Result<Self, JsValue> {
        noon::integration::PlotSamplingInput::parametric(range, discontinuities, dt, max_samples)
            .map(|input| Self { input })
            .map_err(js_error)
    }
    pub fn parameters(&self) -> Vec<f64> {
        self.input.parameters()
    }
    #[wasm_bindgen(js_name=functionSamples)]
    pub fn function_samples(
        &self,
        values: &[f64],
        smooth: bool,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        self.input
            .function_samples(values, smooth)
            .map(WasmManimGeometryOptions::from_options)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name=parametricSamples)]
    pub fn parametric_samples(
        &self,
        values: &[f64],
        smooth: bool,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        self.input
            .parametric_samples(values, smooth)
            .map(WasmManimGeometryOptions::from_options)
            .map_err(js_error)
    }
}
