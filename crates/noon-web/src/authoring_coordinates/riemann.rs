//! Typed Riemann admission inputs shared by cold and live frontends.
use super::*;

#[wasm_bindgen]
pub struct WasmRiemannRectangleOptions {
    options: RiemannRectangleOptions,
}

impl WasmRiemannRectangleOptions {
    fn into_options(
        mut self,
        bounded_graph: Option<noon_core::SemanticNodeId>,
    ) -> RiemannRectangleOptions {
        self.options.bounded_graph = bounded_graph;
        self.options
    }
}

#[wasm_bindgen]
impl WasmCoordinateOptions {
    pub fn riemann(
        range: &[f64],
        dx: f64,
        sample: u32,
        width_scale_factor: f64,
    ) -> Result<WasmRiemannRectangleOptions, JsValue> {
        let x_range = match range {
            [] => None,
            [start, end] if start.is_finite() && end.is_finite() => Some([*start, *end]),
            _ => {
                return Err(js_error(AuthoringFailure::new(
                    "invalid_input",
                    "riemann.range",
                    "Riemann range requires two finite values",
                )))
            }
        };
        let sample = match sample {
            0 => RiemannSample::Left,
            1 => RiemannSample::Right,
            2 => RiemannSample::Center,
            _ => {
                return Err(js_error(AuthoringFailure::new(
                    "invalid_input",
                    "riemann.sample",
                    "sample must be left, right, or center",
                )))
            }
        };
        Ok(WasmRiemannRectangleOptions {
            options: RiemannRectangleOptions {
                x_range,
                dx,
                sample,
                width_scale_factor,
                ..Default::default()
            },
        })
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
        let colors = crate::authoring_mobject::gradient_colors(colors).map_err(js_error)?;
        let stroke_colors =
            crate::authoring_mobject::gradient_colors(stroke_color).map_err(js_error)?;
        let [stroke_color] = stroke_colors.as_slice() else {
            return Err(js_error(AuthoringFailure::new(
                "invalid_input",
                "riemann.stroke",
                "one stroke color is required",
            )));
        };
        self.options.colors = colors;
        self.options.stroke_color = *stroke_color;
        self.options.stroke_width = stroke_width;
        self.options.fill_opacity = fill_opacity;
        self.options.show_signed_area = show_signed_area;
        self.options.blend = blend;
        Ok(())
    }
}

/// Disposable shared-Rust preparation, retained only across host evaluation.
#[wasm_bindgen]
pub struct WasmRiemannSamplePlan {
    value: noon::RiemannRectanglePlan,
    source: WasmAuthoringMobjectHandle,
    live: bool,
}

fn provided(values: &[f64]) -> Option<&[f64]> {
    (!values.is_empty()).then_some(values)
}

#[wasm_bindgen]
impl WasmRiemannSamplePlan {
    pub fn starts(&self) -> Vec<f64> {
        self.value.starts().to_vec()
    }
    pub fn samples(&self) -> Vec<f64> {
        self.value.samples().collect()
    }

    /// Empty vectors select the retained fallback independently for each graph.
    /// A valid clipped plan always has at least one rectangle.
    pub fn publish(
        &self,
        top_values: &[f64],
        baseline_values: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        if self.live {
            return Err(js_error(
                "live Riemann plans require their execution context",
            ));
        }
        self.value
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
        if let Some(bound) = bounded {
            graph
                .semantic_mobject()
                .require_same_store(bound)
                .map_err(|error| js_error(AuthoringFailure::from(error)))?;
        }
        let value = axes
            .riemann_plan(
                graph.semantic_mobject(),
                options.into_options(bounded.map(|object| object.node_id())),
            )
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        Ok(WasmRiemannSamplePlan {
            value,
            source: WasmAuthoringMobjectHandle::from_semantic_mobject(
                graph.semantic_mobject().clone(),
            ),
            live: false,
        })
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
    #[wasm_bindgen(js_name = liveEffectiveRiemannSamplePlan)]
    pub fn live_effective_riemann_sample_plan(
        &mut self,
        axes: &WasmAuthoringFamilyHandle,
        graph: &WasmAuthoringMobjectHandle,
        options: WasmRiemannRectangleOptions,
        bounded_graph: &WasmAuthoringMobjectHandle,
        has_bounded_graph: bool,
    ) -> Result<WasmRiemannSamplePlan, JsValue> {
        let axes_object = ManimAxes::from_family(axes.semantic_family()?)
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        // All effective reads are within this one Rust call, before host code.
        let frame = AxesFrame::new(
            self.coordinate_line_frame(
                &axes_object
                    .x_axis()
                    .map_err(coordinate_failure)
                    .map_err(js_error)?,
            )?,
            self.coordinate_line_frame(
                &axes_object
                    .y_axis()
                    .map_err(coordinate_failure)
                    .map_err(js_error)?,
            )?,
        );
        let graph_path = self.query_mobject_path(graph)?;
        let bounded = has_bounded_graph
            .then(|| {
                self.query_mobject_path(bounded_graph)
                    .map(|path| (bounded_graph.semantic_mobject(), path))
            })
            .transpose()?;
        let value = noon::RiemannRectanglePlan::from_snapshot(
            frame,
            graph.semantic_mobject(),
            &graph_path.value,
            bounded
                .as_ref()
                .map(|(object, path)| (*object, &path.value)),
            options.into_options(bounded.as_ref().map(|(object, _)| object.node_id())),
        )
        .map_err(coordinate_failure)
        .map_err(js_error)?;
        Ok(WasmRiemannSamplePlan {
            value,
            source: WasmAuthoringMobjectHandle::from_semantic_mobject(
                graph.semantic_mobject().clone(),
            ),
            live: true,
        })
    }

    #[wasm_bindgen(js_name = livePublishRiemannPlan)]
    pub fn live_publish_riemann_plan(
        &mut self,
        plan: &WasmRiemannSamplePlan,
        top_values: &[f64],
        baseline_values: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        if !plan.live {
            return Err(js_error(
                "cold Riemann plan is not an effective execution plan",
            ));
        }
        // Enforce originating-store membership through the existing query
        // boundary. This observation is NOT used to recompute plan geometry.
        self.query_mobject_path(&plan.source)?;
        let paths = plan
            .value
            .paths(provided(top_values), provided(baseline_values))
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        self.publish_live_path_family(paths)
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveEffectiveRiemannRectangles)]
    pub fn live_effective_riemann_rectangles(
        &mut self,
        axes: &WasmAuthoringFamilyHandle,
        graph: &WasmAuthoringMobjectHandle,
        options: WasmRiemannRectangleOptions,
        bounded_graph: &WasmAuthoringMobjectHandle,
        has_bounded_graph: bool,
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let plan = self.live_effective_riemann_sample_plan(
            axes,
            graph,
            options,
            bounded_graph,
            has_bounded_graph,
        )?;
        self.live_publish_riemann_plan(&plan, &[], &[])
    }
}
