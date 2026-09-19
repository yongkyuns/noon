from pathlib import Path

# Unit tests mock the actual transport conversion as well as the plan handle.
p = Path('web/python/test_manim_riemann_plan.py')
s = p.read_text()
needle = "        patch.object(plotting, '_to_js', side_effect=lambda values: values).start()\n"
assert s.count(needle) == 1
s = s.replace(needle, needle + "        patch.object(plotting._shared, '_gradient_components', return_value=[]).start()\n", 1)
p.write_text(s)

p = Path('web/plotting-smoke.js')
s = p.read_text()
needle = 'function directWasmPreparation() {\n'
assert s.count(needle) == 1
helper = r'''function qualifyExactRiemannPlan() {
  const store = new WasmAuthoringStore();
  let axes, frame, plot, graph, plan;
  const check = (values, expected) => {
    const rectangles = plan.publish(values, []);
    let members = [];
    try {
      members = Array.from(rectangles.directMobjects());
      if (members.length !== 4) throw new Error("exact Riemann plan lost rectangles");
      for (const [index, member] of members.entries()) {
        const path = member.pathQuery();
        try {
          near(Array.from(path.start()), [-0.5 + index * 0.5, expected[index]],
            "exact scalar rectangle geometry");
        } finally { path.free(); }
      }
    } finally { members.forEach(member => member.free()); rectangles.free(); }
  };
  try {
    const options = WasmCoordinateOptions.axes([-1, 1, 1], [-1, 1, 1], 2, 2);
    options.setTicks(false, 0.1, true);
    axes = store.createCoordinates(options);
    frame = axes.axesFrame();
    plot = frame.plotPlan([-1, 1, 1], [], undefined, undefined);
    graph = store.createManimGeometry(plot.functionSamples([1, 0, 1], false));
    plan = axes.riemannSamplePlan(graph,
      WasmCoordinateOptions.riemann([-1, 1], 0.5, 2, 1), graph, false);
    near(Array.from(plan.starts()), [-1, -0.5, 0, 0.5], "Riemann starts");
    near(Array.from(plan.samples()), [-0.75, -0.25, 0.25, 0.75], "Riemann midpoint samples");
    const exact = Array.from(plan.samples(), x => x * x);
    check(exact, [0.5625, 0.0625, 0.0625, 0.5625]);
    // The fallback is observably different; invoking a callback without using
    // its returned values cannot satisfy this geometry oracle.
    check([], [0.75, 0.25, 0.25, 0.75]);
    for (const invalid of [[1], [1, 1, 1, 1, 1], [NaN, 0, 0, 0], [Infinity, 0, 0, 0]]) {
      let rejected = false;
      try { const unexpected = plan.publish(invalid, []); unexpected.free(); }
      catch (error) { rejected = error.category === "invalid_input"; }
      if (!rejected) throw new Error("invalid Riemann values escaped typed validation");
    }
    check(exact, [0.5625, 0.0625, 0.0625, 0.5625]);
  } finally {
    plan?.free(); graph?.free(); plot?.free(); frame?.free(); axes?.free(); store.free();
  }
}

'''
s = s.replace(needle, helper + needle + '  qualifyExactRiemannPlan();\n', 1)

needle = '        # Extreme parameter values may still map to ordinary visible geometry.\n'
assert s.count(needle) == 1
fixture = r'''        # Keep this oracle axis-aligned so physical rectangle corners, not
        # merely callback counts, distinguish exact values from interpolation.
        sample_axes = Axes([-1, 1, 1], [-1, 1, 1], x_length=2, y_length=2, include_ticks=False)
        sample_calls = []
        def quadratic(x):
            sample_calls.append(x)
            return x*x
        sample_graph = sample_axes.plot(quadratic, [-1, 1, 1], use_smoothing=False)
        midpoint_values = (0.5625, 0.0625, 0.0625, 0.5625)
        def rectangles_on_sample_axes(graph, bound=None):
            return sample_axes.get_riemann_rectangles(
                graph, [-1, 1], dx=0.5, input_sample_type="center",
                bounded_graph=bound, width_scale_factor=1)
        def check_sample_rectangles(rectangles, values, offset=(0, 0)):
            assert len(rectangles.submobjects) == 4
            for index, (rectangle, value) in enumerate(zip(rectangles.submobjects, values)):
                near(rectangle.get_start(), (-0.5 + index*0.5 + offset[0], value + offset[1]))
        check_sample_rectangles(rectangles_on_sample_axes(sample_graph), midpoint_values)
        assert sample_calls == [-1, 0, 1, -0.75, -0.25, 0.25, 0.75], sample_calls
        lower_graph = sample_axes.plot(lambda x: -0.25, [-1, 1, 1], use_smoothing=False)
        del lower_graph.underlying_function
        check_sample_rectangles(rectangles_on_sample_axes(sample_graph, lower_graph), midpoint_values)
        del sample_graph.underlying_function
        lower_graph.underlying_function = lambda x: -0.25
        check_sample_rectangles(rectangles_on_sample_axes(sample_graph, lower_graph), (0.75, 0.25, 0.25, 0.75))
        sample_graph.underlying_function = quadratic
        callback_failure = RuntimeError("Riemann scalar callback failed")
        def failing_sample(x):
            raise callback_failure
        sample_graph.underlying_function = failing_sample
        try:
            rectangles_on_sample_axes(sample_graph)
        except RuntimeError as caught:
            assert caught is callback_failure
        else:
            raise AssertionError("Riemann callback failure was swallowed")
        sample_graph.underlying_function = lambda x: float('nan')
        try:
            rectangles_on_sample_axes(sample_graph)
        except NoonValueError:
            pass
        else:
            raise AssertionError("nonfinite Riemann scalar was accepted")
        sample_graph.underlying_function = quadratic
        check_sample_rectangles(rectangles_on_sample_axes(sample_graph), midpoint_values)

        # A callable is arbitrary host code: moving an axis while evaluating it
        # must not change the immutable coordinate snapshot captured at entry.
        moved = []
        def moving_sample(x):
            if not moved:
                sample_axes.shift(UP)
                moved.append(True)
            return x*x
        sample_graph.underlying_function = moving_sample
        check_sample_rectangles(rectangles_on_sample_axes(sample_graph), midpoint_values)
        near(sample_axes.c2p(0, 0), (0, 1))
        sample_axes.shift(DOWN)
        sample_graph.underlying_function = quadratic

'''
s = s.replace(needle, fixture + needle, 1)
needle = '        self.add(axes, data, sentinel)\n'
assert s.count(needle) == 1
s = s.replace(needle, '        sample_calls_before_play = len(sample_calls)\n        self.add(sample_axes, sample_graph)\n' + needle, 1)
needle = '        self.play(Create(curve), run_time=0.2, rate_func=linear)\n'
assert s.count(needle) == 1
s = s.replace(needle, needle + r'''        assert len(sample_calls) == sample_calls_before_play, "static playback invoked a graph callable"
        live_rectangles = rectangles_on_sample_axes(sample_graph)
        self.add(live_rectangles)
        check_sample_rectangles(live_rectangles, midpoint_values)
        self.remove(live_rectangles, sample_axes, sample_graph)
''', 1)
p.write_text(s)
# Validate the embedded fixture before shipping it to the actual browser gate.
source = s.split('const source = `\n', 1)[1].split('\n`;', 1)[0]
compile(source, 'plotting-smoke-fixture.py', 'exec')
