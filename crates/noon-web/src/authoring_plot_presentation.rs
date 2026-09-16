//! Thin preparation-only projections; no semantic identity or playback state.

use noon::plot_presentation::{
    number_labels, NumberLabel, PlotPresentationError, TimeSeriesPlan, TimedPlotSample,
};
use wasm_bindgen::prelude::*;

use crate::authoring_error::{js_error, AuthoringFailure};
use crate::{WasmAxesFrame, WasmNumberLineFrame};

fn failure(error: impl std::fmt::Display) -> JsValue {
    js_error(AuthoringFailure::new(
        "invalid_input",
        "plot.presentation",
        error.to_string(),
    ))
}

#[wasm_bindgen]
pub struct WasmNumberLabelPlan {
    labels: Vec<NumberLabel>,
}

#[wasm_bindgen]
impl WasmNumberLabelPlan {
    pub fn numbers(&self) -> Vec<f64> {
        self.labels.iter().map(|l| l.number).collect()
    }
    pub fn texts(&self) -> Vec<String> {
        self.labels.iter().map(|l| l.text.clone()).collect()
    }
    pub fn points(&self) -> Vec<f64> {
        self.labels.iter().flat_map(|l| l.point).collect()
    }
}

#[wasm_bindgen]
impl WasmNumberLineFrame {
    #[wasm_bindgen(js_name = numberLabelPlan)]
    pub fn number_label_plan(
        &self,
        values: &[f64],
        automatic: bool,
        decimal_places: f64,
        exclude_zero: bool,
    ) -> Result<WasmNumberLabelPlan, JsValue> {
        if automatic && !values.is_empty() {
            return Err(failure(
                "automatic labels cannot also supply explicit values",
            ));
        }
        if !decimal_places.is_finite()
            || decimal_places.fract() != 0.0
            || !(0.0..=f64::from(u32::MAX)).contains(&decimal_places)
        {
            return Err(failure(
                "number-label precision must fit an unsigned integer",
            ));
        }
        let labels = number_labels(
            self.frame,
            if automatic { None } else { Some(values) },
            decimal_places as u32,
            exclude_zero,
        )
        .map_err(failure)?;
        Ok(WasmNumberLabelPlan { labels })
    }
}

#[wasm_bindgen]
pub struct WasmTimeSeriesPlan {
    plan: TimeSeriesPlan,
}

#[wasm_bindgen]
impl WasmTimeSeriesPlan {
    pub fn points(&self) -> Vec<f64> {
        self.plan.points().iter().flatten().copied().collect()
    }
    #[wasm_bindgen(js_name = cursorPoints)]
    pub fn cursor_points(&self) -> Vec<f64> {
        self.plan
            .cursor_points()
            .iter()
            .flatten()
            .copied()
            .collect()
    }
    #[wasm_bindgen(js_name = keyTimes)]
    pub fn key_times(&self) -> Vec<f64> {
        self.plan.key_times().to_vec()
    }
    pub fn durations(&self) -> Vec<f64> {
        self.plan.durations().to_vec()
    }
    #[wasm_bindgen(js_name = runTime)]
    pub fn run_time(&self) -> f64 {
        self.plan.run_time()
    }
}

#[wasm_bindgen]
impl WasmAxesFrame {
    #[wasm_bindgen(js_name = timeSeriesPlan)]
    pub fn time_series_plan(
        &self,
        values: &[f64],
        run_time: f64,
    ) -> Result<WasmTimeSeriesPlan, JsValue> {
        let (pairs, remainder) = values.as_chunks::<2>();
        if !remainder.is_empty() || pairs.len() > noon::plot_presentation::MAX_TIMED_PLOT_SAMPLES {
            return Err(failure(
                "time-series samples require bounded timestamp/value pairs",
            ));
        }
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(pairs.len())
            .map_err(|_| failure(PlotPresentationError::AllocationFailed))?;
        for &[time, value] in pairs {
            samples.push(TimedPlotSample { time, value });
        }
        TimeSeriesPlan::new(self.frame, &samples, run_time)
            .map(|plan| WasmTimeSeriesPlan { plan })
            .map_err(failure)
    }
}

#[cfg(all(
    feature = "renderer",
    any(debug_assertions, feature = "renderer-smoke")
))]
#[wasm_bindgen(js_name = createTimeSeriesPlottingRenderer)]
pub async fn create_time_series_plotting_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let program = noon::time_series_plotting_example::program().map_err(failure)?;
    crate::WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}
