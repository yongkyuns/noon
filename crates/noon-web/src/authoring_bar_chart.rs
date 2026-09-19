//! Direct WASM qualification for the shared Rust BarChart scene builder.

use wasm_bindgen::prelude::*;

use crate::{CanonicalAuthoringSceneContext, WasmAuthoringFamilyHandle, WasmAuthoringStore};

/// Inert BarChart inputs. Coordinates and bar geometry are prepared in Rust.
#[wasm_bindgen]
pub struct WasmBarChartOptions {
    options: noon::ManimBarChartOptions,
}

#[wasm_bindgen]
impl WasmBarChartOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(
        values: &[f64],
        y_range: &[f64],
        x_length: f64,
        y_length: f64,
    ) -> Result<Self, JsValue> {
        let options = noon::ManimBarChartOptions::manim_defaults(
            values.to_vec(),
            (!y_range.is_empty()).then_some(y_range),
            (x_length != 0.0).then_some(x_length),
            (y_length != 0.0).then_some(y_length),
        )
        .map_err(crate::authoring_plotting::coordinate_failure)
        .map_err(crate::authoring_error::js_error)?;
        Ok(Self { options })
    }

    #[wasm_bindgen(js_name = setStyle)]
    pub fn set_style(&mut self, bar_width: f64, fill_opacity: f64, stroke_width: f64) {
        self.options.bar_width = bar_width;
        self.options.bar_fill_opacity = fill_opacity;
        self.options.bar_stroke_width = stroke_width;
    }

    #[wasm_bindgen(js_name = setColors)]
    pub fn set_colors(&mut self, rgba: &[f64]) -> Result<(), JsValue> {
        if rgba.is_empty() || rgba.len() % 4 != 0 {
            return Err(crate::authoring_error::js_error(
                "bar colors require one or more RGBA values",
            ));
        }
        let mut colors = Vec::with_capacity(rgba.len() / 4);
        for color in rgba.chunks_exact(4) {
            let Some(color) = crate::authoring_mobject::family_color(
                true, color[0], color[1], color[2], color[3],
            )
            .map_err(crate::authoring_error::js_error)?
            else {
                unreachable!()
            };
            colors.push(color);
        }
        self.options.bar_colors = colors;
        Ok(())
    }
}

/// Typed wrapper retaining the shared Rust chart handle, never a JS chart model.
#[wasm_bindgen]
pub struct WasmBarChartHandle {
    chart: noon::ManimBarChart,
}

#[wasm_bindgen]
impl WasmAuthoringStore {
    #[wasm_bindgen(js_name = createBarChart)]
    pub fn create_bar_chart(
        &self,
        options: WasmBarChartOptions,
    ) -> Result<WasmBarChartHandle, JsValue> {
        noon::ManimBarChart::create(std::rc::Rc::clone(&self.semantics), &options.options)
            .map(|chart| WasmBarChartHandle { chart })
            .map_err(crate::authoring_plotting::coordinate_failure)
            .map_err(crate::authoring_error::js_error)
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = liveCreateBarChart)]
    pub fn live_create_bar_chart(
        &mut self,
        options: WasmBarChartOptions,
    ) -> Result<WasmBarChartHandle, JsValue> {
        self.inner
            .live_create_bar_chart(&options.options)
            .map(|chart| WasmBarChartHandle { chart })
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = liveChangeBarValues)]
    pub fn live_change_bar_values(
        &mut self,
        chart: &mut WasmBarChartHandle,
        values: &[f64],
        update_colors: bool,
    ) -> Result<(), JsValue> {
        self.inner
            .live_change_bar_values(&mut chart.chart, values, update_colors)
            .map_err(crate::authoring_error::js_error)
    }
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    /// Reconstruct a typed chart handle from a copied authoritative family.
    /// This keeps Python wrapper copies as aliases of one Rust semantic copy,
    /// rather than deep-copying a host-side chart model.
    #[wasm_bindgen(js_name = barChart)]
    pub fn bar_chart(&self) -> Result<WasmBarChartHandle, JsValue> {
        noon::ManimBarChart::from_family(self.semantic_family())
            .map(|chart| WasmBarChartHandle { chart })
            .map_err(crate::authoring_plotting::coordinate_failure)
            .map_err(crate::authoring_error::js_error)
    }
}

#[wasm_bindgen]
impl WasmBarChartHandle {
    pub fn family(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.chart.family().clone())
    }

    pub fn axes(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.chart.axes().family().clone())
    }

    pub fn bars(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.chart.bars().clone())
    }

    #[wasm_bindgen(js_name = changeBarValues)]
    pub fn change_bar_values(
        &mut self,
        values: &[f64],
        update_colors: bool,
    ) -> Result<(), JsValue> {
        self.chart
            .change_bar_values_cold(values, update_colors)
            .map_err(crate::authoring_plotting::coordinate_failure)
            .map_err(crate::authoring_error::js_error)
    }
}

/// Native and single-context WASM execute the same typed BarChart scene.
#[cfg(all(
    feature = "renderer",
    any(debug_assertions, feature = "renderer-smoke")
))]
#[wasm_bindgen(js_name = createBarChartRenderer)]
pub async fn create_bar_chart_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session =
        noon::example_scenes::bar_chart::session().map_err(crate::authoring_error::js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}
