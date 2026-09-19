from pathlib import Path
import subprocess
BASE = 'f0c120e916ddde2714ed785b6c4114e6fb04ce83'
def original(path):
    return subprocess.check_output(['git', 'show', f'{BASE}:{path}'], text=True)
def replace(source, old, new):
    assert source.count(old) == 1, old[:100]
    return source.replace(old, new, 1)

path = 'crates/noon-web/src/authoring_coordinates/riemann.rs'
s = original(path)
start = s.index('#[wasm_bindgen]\nimpl WasmAuthoringFamilyHandle')
s = s[:start] + r'''/// Disposable shared-Rust preparation, retained only across host evaluation.
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
    pub fn starts(&self) -> Vec<f64> { self.value.starts().to_vec() }
    pub fn samples(&self) -> Vec<f64> { self.value.samples().collect() }

    /// Empty vectors select the retained fallback independently for each graph.
    /// A valid clipped plan always has at least one rectangle.
    pub fn publish(
        &self,
        top_values: &[f64],
        baseline_values: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        if self.live {
            return Err(js_error("live Riemann plans require their execution context"));
        }
        self.value.publish(provided(top_values), provided(baseline_values))
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
            .map_err(coordinate_failure).map_err(js_error)?;
        let bounded = has_bounded_graph.then(|| bounded_graph.semantic_mobject());
        if let Some(bound) = bounded {
            graph.semantic_mobject().require_same_store(bound)
                .map_err(|error| js_error(AuthoringFailure::from(error)))?;
        }
        let value = axes.riemann_plan(
            graph.semantic_mobject(),
            options.into_options(bounded.map(|object| object.node_id())),
        ).map_err(coordinate_failure).map_err(js_error)?;
        Ok(WasmRiemannSamplePlan {
            value,
            source: WasmAuthoringMobjectHandle::from_semantic_mobject(graph.semantic_mobject().clone()),
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
        self.riemann_sample_plan(graph, options, bounded_graph, has_bounded_graph)?.publish(&[], &[])
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
            .map_err(coordinate_failure).map_err(js_error)?;
        // All effective reads are within this one Rust call, before host code.
        let frame = AxesFrame::new(
            self.coordinate_line_frame(&axes_object.x_axis().map_err(coordinate_failure).map_err(js_error)?)?,
            self.coordinate_line_frame(&axes_object.y_axis().map_err(coordinate_failure).map_err(js_error)?)?,
        );
        let graph_path = self.query_mobject_path(graph)?;
        let bounded = has_bounded_graph.then(|| {
            self.query_mobject_path(bounded_graph).map(|path| (bounded_graph.semantic_mobject(), path))
        }).transpose()?;
        let value = noon::RiemannRectanglePlan::from_snapshot(
            frame,
            graph.semantic_mobject(),
            &graph_path.value,
            bounded.as_ref().map(|(object, path)| (*object, &path.value)),
            options.into_options(bounded.as_ref().map(|(object, _)| object.node_id())),
        ).map_err(coordinate_failure).map_err(js_error)?;
        Ok(WasmRiemannSamplePlan {
            value,
            source: WasmAuthoringMobjectHandle::from_semantic_mobject(graph.semantic_mobject().clone()),
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
        if !plan.live { return Err(js_error("cold Riemann plan is not an effective execution plan")); }
        // Enforce originating-store membership through the existing query
        // boundary. This observation is NOT used to recompute plan geometry.
        self.query_mobject_path(&plan.source)?;
        let paths = plan.value.paths(provided(top_values), provided(baseline_values))
            .map_err(coordinate_failure).map_err(js_error)?;
        self.publish_live_path_family(paths)
            .map(WasmAuthoringFamilyHandle::from_semantic_family).map_err(js_error)
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
        let plan = self.live_effective_riemann_sample_plan(axes, graph, options, bounded_graph, has_bounded_graph)?;
        self.live_publish_riemann_plan(&plan, &[], &[])
    }
}
'''
Path(path).write_text(s)

path = 'web/python/_manim_plotting.py'
s = Path(path).read_text()
start = s.index('        bounded = graph if bounded_graph is None else bounded_graph\n')
end = s.index('        members = [_leaf(member, _compat.Rectangle)', start)
s = s[:start] + r'''        plan = engine_call(prepare, *prepare_args, options, bounded_handle,
                           bounded_graph is not None)
        with _owned(plan):
            function = getattr(graph, "underlying_function", None)
            bounded_function = (None if bounded_graph is None else
                                getattr(bounded_graph, "underlying_function", None))
            top_values, baseline_values = [], []
            # Preserve per-rectangle top-then-lower invocation order. Each graph
            # independently uses its callable or the captured Rust path fallback.
            for x, sample_x in zip(engine_call(plan.starts), engine_call(plan.samples), strict=True):
                if function is not None:
                    top_values.append(float(function(float(sample_x))))
                if bounded_function is not None:
                    baseline_values.append(float(bounded_function(float(x))))
            values = (_array(top_values), _array(baseline_values))
            handle = (engine_call(plan.publish, *values) if context is None else
                      engine_call(context.livePublishRiemannPlan, plan, *values))
''' + s[end:]
# Resolve Python references/methods before allocating consuming options, so an
# argument lookup error cannot strand an unowned WASM allocation.
anchor = '        options = engine_call(_coordinate_options.riemann, _array(() if x_range is None else x_range),\n'
prefix = '''        bounded = graph if bounded_graph is None else bounded_graph
        graph_handle, bounded_handle = graph._semantic_handle, bounded._semantic_handle
        if context is None:
            prepare = self._semantic_family_handle.riemannSamplePlan
            prepare_args = (graph_handle,)
        else:
            prepare = context.liveEffectiveRiemannSamplePlan
            prepare_args = (self._semantic_family_handle, graph_handle)
'''
s = replace(s, anchor, prefix + anchor)
s = replace(s, '        # This is the only host function evaluation. No callback is retained in\n        # the returned curve or invoked during deterministic playback.\n', '        # Construction evaluates once. Scalar callable identity may be retained\n        # for explicit Riemann queries, never for deterministic frame playback.\n')
s = replace(s, '        """Publish one live Rust-owned rectangle family from effective snapshots."""', '        """Sample original scalar callables against one Rust-captured snapshot."""')
Path(path).write_text(s)
compile(s, path, 'exec')
