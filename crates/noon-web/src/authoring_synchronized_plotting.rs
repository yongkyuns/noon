//! Optional language-boundary projection of shared multi-series preparation.

use noon::plot_presentation::{TimedPlotSample, MAX_TIMED_PLOT_SAMPLES};
use noon::synchronized_plot_presentation::{SynchronizedTimeSeriesPlan, MAX_SYNCHRONIZED_SERIES};
use wasm_bindgen::prelude::*;

use crate::authoring_error::{js_error, AuthoringFailure};
use crate::WasmAxesFrame;

mod gaps;
pub use gaps::*;

fn failure(error: impl std::fmt::Display) -> JsValue {
    js_error(AuthoringFailure::new(
        "invalid_input",
        "plot.synchronized",
        error.to_string(),
    ))
}

#[wasm_bindgen]
pub struct WasmSynchronizedTimeSeriesPlan {
    plan: SynchronizedTimeSeriesPlan,
}

#[wasm_bindgen]
impl WasmSynchronizedTimeSeriesPlan {
    #[wasm_bindgen(js_name = seriesCount)]
    pub fn series_count(&self) -> u32 {
        self.plan.series().len() as u32
    }
    #[wasm_bindgen(js_name = seriesPoints)]
    pub fn series_points(&self, index: f64) -> Result<Vec<f64>, JsValue> {
        let index = checked_index(index, self.plan.series().len())?;
        Ok(self.plan.series()[index]
            .points()
            .iter()
            .flatten()
            .copied()
            .collect())
    }
    #[wasm_bindgen(js_name = dataTimes)]
    pub fn data_times(&self) -> Vec<f64> {
        self.plan.series()[0]
            .samples()
            .iter()
            .map(|s| s.time)
            .collect()
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

fn checked_index(index: f64, count: usize) -> Result<usize, JsValue> {
    if !index.is_finite() || index.fract() != 0.0 || index < 0.0 || index >= count as f64 {
        return Err(failure("invalid synchronized series index"));
    }
    Ok(index as usize)
}

/// Decode one bounded host payload, then borrow its rows for shared preparation.
/// Both connected and gapped APIs use the same language-boundary admission.
fn with_series<T>(
    values: &[f64],
    counts: &[f64],
    time_range: &[f64],
    prepare: impl FnOnce(&[&[TimedPlotSample]], [f64; 2]) -> Result<T, JsValue>,
) -> Result<T, JsValue> {
    let [start, end] = time_range else {
        return Err(failure("synchronized time_range requires two values"));
    };
    let (pairs, remainder) = values.as_chunks::<2>();
    if !remainder.is_empty()
        || pairs.len() > MAX_TIMED_PLOT_SAMPLES
        || counts.is_empty()
        || counts.len() > MAX_SYNCHRONIZED_SERIES
    {
        return Err(failure("invalid synchronized sample payload"));
    }
    let mut total = 0usize;
    for &count in counts {
        if !count.is_finite()
            || count.fract() != 0.0
            || !(2.0..=MAX_TIMED_PLOT_SAMPLES as f64).contains(&count)
        {
            return Err(failure("invalid synchronized series sample count"));
        }
        total += count as usize;
    }
    if total != pairs.len() {
        return Err(failure(
            "synchronized sample counts do not match the payload",
        ));
    }
    let mut samples = Vec::new();
    samples.try_reserve_exact(total).map_err(failure)?;
    samples.extend(
        pairs
            .iter()
            .map(|&[time, value]| TimedPlotSample { time, value }),
    );
    let mut series = Vec::new();
    series.try_reserve_exact(counts.len()).map_err(failure)?;
    let mut offset = 0;
    for &count in counts {
        let end = offset + count as usize;
        series.push(&samples[offset..end]);
        offset = end;
    }
    prepare(&series, [*start, *end])
}

#[wasm_bindgen]
impl WasmAxesFrame {
    /// One validated cross-language payload, not an in-process scene format.
    #[wasm_bindgen(js_name = synchronizedSeriesPlan)]
    pub fn synchronized_series_plan(
        &self,
        values: &[f64],
        counts: &[f64],
        time_range: &[f64],
        run_time: f64,
    ) -> Result<WasmSynchronizedTimeSeriesPlan, JsValue> {
        with_series(values, counts, time_range, |series, range| {
            SynchronizedTimeSeriesPlan::new(self.frame, series, range, run_time)
                .map(|plan| WasmSynchronizedTimeSeriesPlan { plan })
                .map_err(failure)
        })
    }
}

#[cfg(all(
    feature = "renderer",
    any(debug_assertions, feature = "renderer-smoke")
))]
#[wasm_bindgen(js_name = createSynchronizedPlottingRenderer)]
pub async fn create_synchronized_plotting_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let program = noon::synchronized_plotting_example::program().map_err(failure)?;
    crate::WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}
