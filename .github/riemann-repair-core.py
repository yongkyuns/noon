from pathlib import Path
import subprocess

BASE = 'f0c120e916ddde2714ed785b6c4114e6fb04ce83'
def original(path):
    return subprocess.check_output(['git', 'show', f'{BASE}:{path}'], text=True)
def replace(source, old, new):
    assert source.count(old) == 1, old[:100]
    return source.replace(old, new, 1)

path = 'crates/noon/src/coordinate_authoring/area.rs'
s = original(path)
start = s.index('pub(crate) fn prepare_riemann_paths(')
end = s.index('/// Reuse the existing bounded NumPy-compatible plot sample planner.', start)
s = s[:start] + r'''/// Immutable preparation for one Riemann request. All coordinate/path reads and
/// interval/paint validation happen before host evaluation. Returned scalar
/// values cannot replace the Rust-owned partition or trigger a second snapshot.
/// This is disposable authoring data, not a scene cache or a runtime subscription.
pub struct RiemannRectanglePlan {
    store: Rc<RefCell<SemanticStore>>,
    frame: AxesFrame,
    partition: noon_geometry::PlotSamplingPlan,
    graph_path: PathQuery,
    bounded_path: Option<PathQuery>,
    options: RiemannRectangleOptions,
    colors: Vec<noon_core::Color>,
    style: SemanticStyle,
}

impl RiemannRectanglePlan {
    /// Integration entry point. The caller must capture all operands from the
    /// same authored state or effective publication, without interleaved callbacks.
    pub fn from_snapshot(
        frame: AxesFrame,
        graph: &Mobject,
        graph_path: &PathQuery,
        bounded: Option<(&Mobject, &PathQuery)>,
        options: RiemannRectangleOptions,
    ) -> Result<Self, CoordinateAuthoringError> {
        if !options.dx.is_finite() || options.dx <= 0.0 || !options.width_scale_factor.is_finite() {
            return Err(CoordinateAuthoringError::InvalidOptions(
                "invalid Riemann rectangle dimensions",
            ));
        }
        if let Some((bound, _)) = bounded {
            graph.require_same_store(bound)?;
        }
        let range = graph_range(graph)?;
        let mut interval = clip_interval(options.x_range.unwrap_or(range), range)?;
        if let Some((bound, _)) = bounded {
            interval = clip_interval(interval, graph_range(bound)?)?;
        }
        let partition = riemann_partition_plan(interval, options.dx)
            .map_err(PlotAuthoringError::from)?;
        let style = SemanticStyle {
            fill_opacity: options.fill_opacity,
            stroke: Some(SemanticPaint::Solid(options.stroke_color)),
            stroke_width: options.stroke_width * 0.01,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            stroke_cap: StrokeCap::Butt,
            stroke_join: StrokeJoin::Miter,
            ..Default::default()
        };
        if !style.is_finite() || !(0.0..=1.0).contains(&options.fill_opacity)
            || options.stroke_width < 0.0
        {
            return Err(CoordinateAuthoringError::InvalidOptions("invalid Riemann paint"));
        }
        let colors = crate::color_gradient(&options.colors, partition.parameters().len() - 1)?;
        let plan = Self {
            store: Rc::clone(graph.integration_store()),
            frame,
            partition,
            graph_path: graph_path.clone(),
            bounded_path: bounded.map(|(_, path)| path.clone()),
            options,
            colors,
            style,
        };
        // Reject unrepresentable requested inputs before invoking a host callback.
        for (&x, sample) in plan.starts().iter().zip(plan.samples()) {
            if !sample.is_finite() || !(x + plan.options.dx * plan.options.width_scale_factor).is_finite() {
                return Err(CoordinateAuthoringError::InvalidOptions("nonfinite Riemann sample"));
            }
        }
        Ok(plan)
    }

    /// Half-open arange samples. Only the separately appended terminal endpoint
    /// is excluded; rounded endpoints and repeated regular samples are preserved.
    pub fn starts(&self) -> &[f64] {
        &self.partition.parameters()[..self.partition.parameters().len() - 1]
    }

    /// Top-function inputs. A bounded function instead receives `starts()`.
    pub fn samples(&self) -> impl ExactSizeIterator<Item = f64> + '_ {
        self.starts().iter().map(|&x| match self.options.sample {
            RiemannSample::Left => x,
            RiemannSample::Right => x + self.options.dx,
            RiemannSample::Center => x + self.options.dx * 0.5,
        })
    }

    /// Each optional value set independently selects exact callback evaluation.
    /// None selects the captured retained-path fallback (or the unbounded axis).
    /// No semantic or runtime state is read again after this plan was captured.
    pub fn paths(
        &self,
        top_values: Option<&[f64]>,
        baseline_values: Option<&[f64]>,
    ) -> Result<Vec<(noon_core::VectorPath, SemanticStyle)>, CoordinateAuthoringError> {
        if baseline_values.is_some() && self.bounded_path.is_none() {
            return Err(CoordinateAuthoringError::InvalidOptions("unbounded Riemann plan has no lower graph"));
        }
        for values in [top_values, baseline_values].into_iter().flatten() {
            if values.len() != self.starts().len() || values.iter().any(|value| !value.is_finite()) {
                return Err(CoordinateAuthoringError::InvalidOptions(
                    "Riemann callback values must match the Rust plan and be finite",
                ));
            }
        }
        let mut paths = Vec::new();
        paths.try_reserve_exact(self.starts().len()).map_err(|_| CoordinateError::AllocationFailed)?;
        for (index, (&x, sample_x)) in self.starts().iter().zip(self.samples()).enumerate() {
            let top = match top_values {
                Some(values) => values[index],
                None => graph_y_at(self.frame, &self.graph_path, sample_x)?,
            };
            let baseline = match (baseline_values, self.bounded_path.as_ref()) {
                (Some(values), _) => values[index],
                (None, Some(path)) => graph_y_at(self.frame, path, x)?,
                (None, None) => noon_geometry::origin_shift(self.frame.y().range()),
            };
            let bottom_left = self.frame.coords_to_point(x, baseline)?;
            let bottom_right = self.frame.coords_to_point(x + self.options.dx * self.options.width_scale_factor, baseline)?;
            let graph_point = self.frame.coords_to_point(sample_x, top)?;
            let min_x = bottom_left[0].min(bottom_right[0]).min(graph_point[0]);
            let max_x = bottom_left[0].max(bottom_right[0]).max(graph_point[0]);
            let min_y = bottom_left[1].min(bottom_right[1]).min(graph_point[1]);
            let max_y = bottom_left[1].max(bottom_right[1]).max(graph_point[1]);
            let mut color = self.colors[index];
            // Only retained f32 path queries need a bisection-noise allowance.
            // Exact scalar results must preserve genuinely small negative areas.
            let retained_height = top_values.is_none()
                || (self.bounded_path.is_some() && baseline_values.is_none());
            let height_precision = if retained_height {
                4.0 * f64::from(f32::EPSILON) * graph_point[1].abs().max(bottom_left[1].abs()).max(1.0)
                    / self.frame.y().unit_size()
            } else { 0.0 };
            if top < baseline - height_precision && self.options.show_signed_area {
                color = noon_core::Color::rgba(1.0 - color.red, 1.0 - color.green, 1.0 - color.blue, color.alpha);
            }
            let mut style = self.style.clone();
            style.fill = Some(SemanticPaint::Solid(color));
            if self.options.blend { style.stroke = Some(SemanticPaint::Solid(color)); }
            paths.push((closed_path(&[[max_x, max_y], [min_x, max_y], [min_x, min_y], [max_x, min_y]])?, style));
        }
        Ok(paths)
    }

    /// Cold authoring publication into the originating store. Live integrations
    /// use `paths()` with their existing Scene/LiveSession family transaction.
    pub fn publish(
        &self,
        top_values: Option<&[f64]>,
        baseline_values: Option<&[f64]>,
    ) -> Result<MobjectFamily, CoordinateAuthoringError> {
        publish_path_family(Rc::clone(&self.store), self.paths(top_values, baseline_values)?)
    }
}

pub(crate) fn prepare_riemann_paths(
    frame: AxesFrame,
    graph: &Mobject,
    graph_path: &PathQuery,
    bounded: Option<(&Mobject, &PathQuery)>,
    options: RiemannRectangleOptions,
) -> Result<Vec<(noon_core::VectorPath, SemanticStyle)>, CoordinateAuthoringError> {
    RiemannRectanglePlan::from_snapshot(frame, graph, graph_path, bounded, options)?.paths(None, None)
}

''' + s[end:]
Path(path).write_text(s)

path = 'crates/noon/src/coordinate_authoring.rs'
s = original(path)
s = replace(s, 'pub(crate) mod area;\n', 'pub(crate) mod area;\npub use area::RiemannRectanglePlan;\n')
start = s.index('    pub fn get_riemann_rectangles(')
end = s.index('    fn require_graph_store(', start)
s = s[:start] + r'''    pub fn get_riemann_rectangles(
        &self,
        graph: &Mobject,
        options: RiemannRectangleOptions,
    ) -> Result<MobjectFamily, CoordinateAuthoringError> {
        self.riemann_plan(graph, options)?.publish(None, None)
    }

    /// Capture one cold authored snapshot before evaluating arbitrary functions.
    /// Rust owns the partition; evaluate `samples()` and optionally `starts()`,
    /// then publish the returned values through this plan.
    pub fn riemann_plan(
        &self,
        graph: &Mobject,
        options: RiemannRectangleOptions,
    ) -> Result<RiemannRectanglePlan, CoordinateAuthoringError> {
        self.require_graph_store(graph)?;
        let bounded = options.bounded_graph
            .map(|id| Mobject::from_node(Rc::clone(self.family.integration_store()), id))
            .transpose()?;
        if let Some(ref bound) = bounded { self.require_graph_store(bound)?; }
        let frame = self.authored_frame()?;
        let graph_path = graph.path_query()?;
        let bounded_path = bounded.as_ref().map(Mobject::path_query).transpose()?;
        RiemannRectanglePlan::from_snapshot(
            frame, graph, &graph_path, bounded.as_ref().zip(bounded_path.as_ref()), options,
        )
    }

''' + s[end:]
# Live planning reuses the existing coherent source capture and publication owner.
start = s.index('    pub fn effective_riemann_rectangles(')
end = s.index('\n}\n\nimpl ManimGeometryOptions', start)
section = s[start:end]
section = replace(section, '    pub fn effective_riemann_rectangles(\n        &mut self,', '    pub fn effective_riemann_plan(\n        &self,')
section = replace(section, ') -> Result<MobjectFamily, CoordinateAuthoringError> {', ') -> Result<RiemannRectanglePlan, CoordinateAuthoringError> {')
section = replace(section, '        let paths = area::prepare_riemann_paths(', '        RiemannRectanglePlan::from_snapshot(')
section = replace(section, '        )?;\n        crate::scene::publish_path_family(self, paths).map_err(Into::into)', '        )')
wrapper = r'''    pub fn effective_riemann_rectangles(
        &mut self,
        axes: &ManimAxes,
        graph: &Mobject,
        options: RiemannRectangleOptions,
    ) -> Result<MobjectFamily, CoordinateAuthoringError> {
        let paths = self.effective_riemann_plan(axes, graph, options)?.paths(None, None)?;
        crate::scene::publish_path_family(self, paths).map_err(Into::into)
    }

    /// Immutable effective source snapshot for exact scalar callback evaluation.
'''
s = s[:start] + wrapper + section + s[end:]
Path(path).write_text(s)

path = 'crates/noon/src/lib.rs'
s = original(path)
s = replace(s, 'ManimNumberPlaneOptions, RiemannRectangleOptions,', 'ManimNumberPlaneOptions, RiemannRectangleOptions, RiemannRectanglePlan,')
Path(path).write_text(s)

path = 'crates/noon/src/example_scenes/area_helpers.rs'
s = original(path)
s = replace(s, '    let mut graph = axes.plot(|x| 0.25 * x * x - 0.5,', '    let function = |x: f64| 0.25 * x * x - 0.5;\n    let mut graph = axes.plot(function,')
s = replace(s, '    let rectangles = axes.get_riemann_rectangles(', '    let plan = axes.riemann_plan(')
s = replace(s, '    let mut title = scene.text(', '    let values = plan.samples().map(function).collect::<Vec<_>>();\n    let rectangles = plan.publish(Some(&values), None)?;\n    let mut title = scene.text(')
Path(path).write_text(s)
