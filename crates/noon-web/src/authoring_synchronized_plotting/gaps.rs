//! Optional points/segments across the actual Python/WASM boundary.

use super::{checked_index, failure, with_series};
use crate::WasmAxesFrame;
use noon::synchronized_plot_presentation::GappedTimeSeriesPlan;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmGappedTimeSeriesPlan { plan: GappedTimeSeriesPlan }

#[wasm_bindgen]
impl WasmGappedTimeSeriesPlan {
    #[wasm_bindgen(js_name = seriesCount)]
    pub fn series_count(&self) -> u32 { self.plan.series().len() as u32 }

    #[wasm_bindgen(js_name = seriesPoints)]
    pub fn series_points(&self, index: f64) -> Result<js_sys::Array, JsValue> {
        let row = &self.plan.series()[checked_index(index, self.plan.series().len())?];
        let result = js_sys::Array::new();
        for point in row.points() {
            result.push(&match point {
                Some(point) => js_sys::Float64Array::from(point.as_slice()).into(),
                None => JsValue::NULL,
            });
        }
        Ok(result)
    }

    #[wasm_bindgen(js_name = seriesSegments)]
    pub fn series_segments(&self, index: f64) -> Result<js_sys::Array, JsValue> {
        let row = &self.plan.series()[checked_index(index, self.plan.series().len())?];
        let result = js_sys::Array::new();
        for segment in row.segments() {
            result.push(&match segment {
                Some([[ax, ay], [bx, by]]) => js_sys::Float64Array::from(&[*ax, *ay, *bx, *by][..]).into(),
                None => JsValue::NULL,
            });
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = dataTimes)]
    pub fn data_times(&self) -> Vec<f64> { self.plan.data_times().to_vec() }
    #[wasm_bindgen(js_name = cursorPoints)]
    pub fn cursor_points(&self) -> Vec<f64> {
        self.plan.cursor_points().iter().flatten().copied().collect()
    }
    #[wasm_bindgen(js_name = keyTimes)]
    pub fn key_times(&self) -> Vec<f64> { self.plan.key_times().to_vec() }
    pub fn durations(&self) -> Vec<f64> { self.plan.durations().to_vec() }
    #[wasm_bindgen(js_name = runTime)]
    pub fn run_time(&self) -> f64 { self.plan.run_time() }
}

#[wasm_bindgen]
impl WasmAxesFrame {
    #[wasm_bindgen(js_name = gappedSeriesPlan)]
    pub fn gapped_series_plan(
        &self, values: &[f64], counts: &[f64], breaks: &[f64], break_counts: &[f64],
        time_range: &[f64], run_time: f64,
    ) -> Result<WasmGappedTimeSeriesPlan, JsValue> {
        with_series(values, counts, time_range, |series, range| {
            if break_counts.len() != series.len() || breaks.len() > values.len() / 2 {
                return Err(failure("one bounded break list is required per series"));
            }
            let mut total = 0usize;
            for (&count, source) in break_counts.iter().zip(series) {
                if !count.is_finite() || count.fract() != 0.0
                    || count < 0.0 || count >= source.len() as f64 {
                    return Err(failure("invalid series break count"));
                }
                total += count as usize;
            }
            if total != breaks.len() { return Err(failure("break counts do not match payload")); }
            let mut indices = Vec::new();
            indices.try_reserve_exact(total).map_err(failure)?;
            let mut offset = 0;
            for (&count, source) in break_counts.iter().zip(series) {
                let end = offset + count as usize;
                for &index in &breaks[offset..end] {
                    indices.push(checked_index(index, source.len() - 1)?);
                }
                offset = end;
            }
            let mut rows = Vec::new();
            rows.try_reserve_exact(series.len()).map_err(failure)?;
            offset = 0;
            for &count in break_counts {
                let end = offset + count as usize;
                rows.push(&indices[offset..end]);
                offset = end;
            }
            GappedTimeSeriesPlan::new(self.frame, series, &rows, range, run_time)
                .map(|plan| WasmGappedTimeSeriesPlan { plan }).map_err(failure)
        })
    }
}

#[cfg(all(feature = "renderer", any(debug_assertions, feature = "renderer-smoke")))]
#[wasm_bindgen(js_name = createGappedPlottingRenderer)]
pub async fn create_gapped_plotting_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let program = noon::synchronized_plotting_example::gaps::program().map_err(failure)?;
    crate::WasmExecutionCanvasRenderer::create_from_live_program(canvas, program).await
}
