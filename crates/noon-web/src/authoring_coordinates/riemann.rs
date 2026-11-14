//! Disposable Riemann preparation and Scene publication for browser authoring.
use super::*;
use noon::integration::RiemannInput;

#[wasm_bindgen]
pub struct WasmRiemannRectangleOptions(RiemannInput);

#[wasm_bindgen]
impl WasmCoordinateOptions {
    pub fn riemann(
        range: &[f64],
        dx: f64,
        sample: u32,
        width_scale_factor: f64,
    ) -> Result<WasmRiemannRectangleOptions, JsValue> {
        RiemannInput::new(range, dx, sample, width_scale_factor)
            .map(WasmRiemannRectangleOptions)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmRiemannRectangleOptions {
    #[wasm_bindgen(js_name = setPaint)]
    pub fn set_paint(
        &mut self,
        colors: &[f64],
        stroke_color: &[f64],
        stroke_width: f64,
        fill_opacity: f64,
        show_signed_area: bool,
        blend: bool,
    ) -> Result<(), JsValue> {
        self.0
            .set_paint(
                colors,
                stroke_color,
                stroke_width,
                fill_opacity,
                show_signed_area,
                blend,
            )
            .map_err(js_error)
    }
}

/// Disposable shared-Rust preparation, retained only across host evaluation.
#[wasm_bindgen]
pub struct WasmRiemannSamplePlan(noon::RiemannRectanglePlan);

fn provided(values: &[f64]) -> Option<&[f64]> {
    (!values.is_empty()).then_some(values)
}

#[wasm_bindgen]
impl WasmRiemannSamplePlan {
    pub fn starts(&self) -> Vec<f64> {
        self.0.starts().to_vec()
    }
    pub fn samples(&self) -> Vec<f64> {
        self.0.samples().collect()
    }

    /// Empty vectors select the retained fallback independently for each graph.
    /// A valid clipped plan always has at least one rectangle.
    pub fn publish(
        &self,
        top_values: &[f64],
        baseline_values: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        self.0
            .publish(provided(top_values), provided(baseline_values))
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(coordinate_failure)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    #[wasm_bindgen(js_name = riemannSamplePlan)]
    pub fn riemann_sample_plan(
        &self,
        graph: &WasmAuthoringMobjectHandle,
        options: WasmRiemannRectangleOptions,
        bounded_graph: &WasmAuthoringMobjectHandle,
        has_bounded_graph: bool,
    ) -> Result<WasmRiemannSamplePlan, JsValue> {
        let axes = ManimAxes::from_family(self.semantic_family()?)
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        let bounded = has_bounded_graph.then(|| bounded_graph.semantic_mobject());
        let graph = graph.semantic_mobject();
        let options = options.0.into_options(graph, bounded).map_err(js_error)?;
        axes.riemann_plan(graph, options)
            .map(WasmRiemannSamplePlan)
            .map_err(coordinate_failure)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = riemannRectangles)]
    pub fn riemann_rectangles(
        &self,
        graph: &WasmAuthoringMobjectHandle,
        options: WasmRiemannRectangleOptions,
        bounded_graph: &WasmAuthoringMobjectHandle,
        has_bounded_graph: bool,
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        self.riemann_sample_plan(graph, options, bounded_graph, has_bounded_graph)?
            .publish(&[], &[])
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = prepareRiemannPlan)]
    pub fn prepare_riemann_plan(
        &mut self,
        axes: &WasmAuthoringFamilyHandle,
        graph: &WasmAuthoringMobjectHandle,
        options: WasmRiemannRectangleOptions,
        bounded_graph: &WasmAuthoringMobjectHandle,
        has_bounded_graph: bool,
    ) -> Result<WasmRiemannSamplePlan, JsValue> {
        let axes = ManimAxes::from_family(axes.semantic_family()?)
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        let graph = graph.semantic_mobject();
        let bounded = has_bounded_graph.then(|| bounded_graph.semantic_mobject());
        let options = options.0.into_options(graph, bounded).map_err(js_error)?;
        self.inner
            .prepare_riemann_plan(&axes, graph, options)
            .map(WasmRiemannSamplePlan)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = publishRiemannPlan)]
    pub fn publish_riemann_plan(
        &mut self,
        plan: &WasmRiemannSamplePlan,
        top_values: &[f64],
        baseline_values: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        self.inner
            .publish_riemann_plan(&plan.0, provided(top_values), provided(baseline_values))
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(js_error)
    }
}
