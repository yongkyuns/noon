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

    #[wasm_bindgen(js_name = setNames)]
    pub fn set_names(&mut self, names: Vec<String>, font_size: f64) -> Result<(), JsValue> {
        self.options.name_font_size =
            crate::authoring_mobject::text_authoring_f32("font size", font_size)
                .map_err(crate::authoring_error::js_error)?;
        self.options.bar_names = Some(names);
        Ok(())
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
impl WasmAuthoringStore {
    #[wasm_bindgen(js_name = createLabeledBarChart)]
    pub fn create_labeled_bar_chart(
        &self,
        options: WasmBarChartOptions,
        labels: crate::WasmNumberLabelOptions,
        compiler: &mut crate::WasmLatexCompiler,
    ) -> Result<WasmBarChartHandle, JsValue> {
        noon::ManimBarChart::create_with_axis_labels(
            std::rc::Rc::clone(&self.semantics),
            &options.options,
            &labels.options,
            compiler,
        )
        .map(|chart| WasmBarChartHandle { chart })
        .map_err(crate::authoring_number_labels::failure)
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
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = liveCreateLabeledBarChart)]
    pub fn live_create_labeled_bar_chart(
        &mut self,
        options: WasmBarChartOptions,
        labels: crate::WasmNumberLabelOptions,
        compiler: &mut crate::WasmLatexCompiler,
    ) -> Result<WasmBarChartHandle, JsValue> {
        self.inner
            .live_create_labeled_bar_chart(&options.options, &labels.options, compiler)
            .map(|chart| WasmBarChartHandle { chart })
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

    #[wasm_bindgen(js_name = xLabels)]
    pub fn x_labels(&self) -> Result<Option<WasmAuthoringFamilyHandle>, JsValue> {
        self.chart
            .x_labels()
            .map(|family| family.map(WasmAuthoringFamilyHandle::from_semantic_family))
            .map_err(crate::authoring_plotting::coordinate_failure)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = yLabels)]
    pub fn y_labels(&self) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        self.chart
            .y_labels()
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_plotting::coordinate_failure)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = barPrefix)]
    pub fn bar_prefix(
        &self,
        count: usize,
    ) -> Result<Vec<crate::WasmAuthoringMobjectHandle>, JsValue> {
        self.chart
            .bar_prefix(count)
            .map(|bars| {
                bars.into_iter()
                    .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                    .collect()
            })
            .map_err(crate::authoring_plotting::coordinate_failure)
            .map_err(crate::authoring_error::js_error)
    }

    pub fn values(&self) -> Result<Vec<f64>, JsValue> {
        self.chart
            .values()
            .map_err(crate::authoring_plotting::coordinate_failure)
            .map_err(crate::authoring_error::js_error)
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

fn label_options(
    font_size: f64,
    buff: f64,
    math: bool,
    rgba: &[f64],
) -> Result<noon::BarLabelOptions, JsValue> {
    let color = match rgba {
        [] => None,
        [r, g, b, a] => crate::authoring_mobject::family_color(true, *r, *g, *b, *a)
            .map_err(crate::authoring_error::js_error)?,
        _ => {
            return Err(crate::authoring_error::js_error(
                "label color requires four RGBA values",
            ))
        }
    };
    Ok(noon::BarLabelOptions {
        font_size: crate::authoring_mobject::text_authoring_f32("font size", font_size)
            .map_err(crate::authoring_error::js_error)?,
        buff,
        color,
        math,
    })
}

#[wasm_bindgen]
impl WasmBarChartHandle {
    #[wasm_bindgen(js_name = labelFamily)]
    pub fn label_family(
        &self,
        font_size: f64,
        buff: f64,
        math: bool,
        rgba: &[f64],
        compiler: &mut crate::WasmLatexCompiler,
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        self.chart
            .get_bar_labels(compiler, &label_options(font_size, buff, math, rgba)?)
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_number_labels::failure)
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = liveBarLabelFamily)]
    pub fn live_bar_label_family(
        &mut self,
        chart: &WasmBarChartHandle,
        font_size: f64,
        buff: f64,
        math: bool,
        rgba: &[f64],
        compiler: &mut crate::WasmLatexCompiler,
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        self.inner
            .live_bar_labels(
                &chart.chart,
                compiler,
                &label_options(font_size, buff, math, rgba)?,
            )
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
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
    compiler: &mut crate::WasmLatexCompiler,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::bar_chart::session(compiler)
        .map_err(crate::authoring_error::js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}
