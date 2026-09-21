from pathlib import Path
import subprocess

MASTER = 'bf776186325640496e58eac91962573811740356'
conflicts = subprocess.check_output(['git', 'diff', '--name-only', '--diff-filter=U'], text=True).splitlines()
assert sorted(conflicts) == ['crates/noon/src/lib.rs', 'web/python/noon.py'], conflicts

def replace_once(source, old, new):
    assert source.count(old) == 1, (old[:100], source.count(old))
    return source.replace(old, new, 1)

p = Path('crates/noon/src/lib.rs')
s = p.read_text()
s = replace_once(s, '''<<<<<<< HEAD
mod implicit_plotting;
pub use implicit_plotting::ImplicitPlotOptions;
=======
mod graph_topology;
pub use graph_topology::*;
>>>>>>> ''' + MASTER, '''mod graph_topology;
pub use graph_topology::*;
mod implicit_plotting;
pub use implicit_plotting::ImplicitPlotOptions;''')
assert '<<<<<<<' not in s
p.write_text(s)
p = Path('web/python/noon.py')
s = p.read_text()
s = replace_once(s, '''<<<<<<< HEAD
    "NumberPlane": "_manim_number_plane",
=======
    "UnitInterval": "_manim_plotting",
>>>>>>> ''' + MASTER, '''    "UnitInterval": "_manim_plotting",
    "NumberPlane": "_manim_number_plane",''')
assert '<<<<<<<' not in s
p.write_text(s)
subprocess.run(['git', 'add', 'crates/noon/src/lib.rs', 'web/python/noon.py'], check=True)

p = Path('web/plotting-smoke.js')
s = p.read_text()
js = '''function implicitPrecisionPreparation() {
  for (const range of [[1e9, 1e9 + 10, 1], [-1e9 - 10, -1e9, 1],
      [-1e-60, 1e-60, 1e-60], [-1e45, 1e45, 1e45]]) {
    const store = new WasmAuthoringStore();
    let axes, frame;
    try {
      const options = WasmCoordinateOptions.axes(range, [-1, 1, 1], 10, 4);
      options.setTicks(false, 0.1, true);
      axes = store.createCoordinates(options);
      frame = axes.axesFrame();
      const span = range[1] - range[0];
      const root = range[0] + span * 0.375;
      for (const smooth of [false, true]) {
        const curve = store.createManimGeometry(
          frame.implicitPlot((x, y) => (x - root) / span, 3, 256, smooth));
        let path;
        try {
          path = curve.pathQuery();
          const start = Array.from(path.start());
          const end = Array.from(path.end());
          if (path.curveCount < 1 || !start.concat(end).every(Number.isFinite) ||
              Math.abs(start[0] + 1.25) > 0.025 || Math.abs(end[0] + 1.25) > 0.025 ||
              Math.abs(end[1] - start[1]) < 3.5) {
            throw new Error(`implicit precision lost: ${range} ${smooth} ${start} ${end}`);
          }
        } finally { path?.free(); curve.free(); }
      }
    } finally { frame?.free(); axes?.free(); store.free(); }
  }
}

'''
s = replace_once(s, 'function directWasmPreparation() {', js + 'function directWasmPreparation() {\n  implicitPrecisionPreparation();')
py = '''        # Actual Python callbacks and the WASM adapter must preserve scalar
        # coordinate precision, not merely pass a Rust-only unit test.
        implicit_counts = []
        for limits in ((1e9, 1e9 + 10, 1), (-1e9 - 10, -1e9, 1),
                       (-1e-60, 1e-60, 1e-60), (-1e45, 1e45, 1e45)):
            precision_axes = Axes(limits, [-1, 1, 1], x_length=10, y_length=4, include_ticks=False)
            span = limits[1] - limits[0]
            root = limits[0] + span * 0.375
            for smooth_contour in (False, True):
                visits = [0]
                def implicit_field(x, y):
                    visits[0] += 1
                    return (x - root) / span
                implicit_curve = precision_axes.plot_implicit_curve(
                    implicit_field, min_depth=3, max_quads=256, use_smoothing=smooth_contour)
                first, last = implicit_curve.get_start(), implicit_curve.get_end()
                assert implicit_curve.get_num_curves() > 0
                assert abs(first[0] + 1.25) < 0.025, (limits, first)
                assert abs(last[0] + 1.25) < 0.025, (limits, last)
                assert abs(last[1] - first[1]) > 3.5, (first, last)
                assert visits[0] > 0
                implicit_counts.append((visits, visits[0]))

        # The exact same source contour remains a translated curve, including
        # interior spline points, when positive scene coordinates change the
        # signed closure test that would be incorrect after mapping.
        spline_axes = Axes([1, 3, 1], [1, 3, 1], x_length=4, y_length=4, include_ticks=False)
        def round_contour(x, y):
            return (x - 2)**2 + (y - 2)**2 - 0.36
        before = spline_axes.plot_implicit_curve(round_contour, min_depth=2, max_quads=64)
        spline_axes.shift((10, 10))
        after = spline_axes.plot_implicit_curve(round_contour, min_depth=2, max_quads=64)
        assert before.get_num_curves() == after.get_num_curves()
        for proportion in (0, 0.003, 0.019, 0.071, 0.333, 0.877, 0.995, 1):
            left = before.point_from_proportion(proportion)
            right = after.point_from_proportion(proportion)
            near(right, (left[0] + 10, left[1] + 10))

'''
s = replace_once(s, '        line = NumberLine([2, 6, 1], length=8, rotation=pi / 2)', py + '        line = NumberLine([2, 6, 1], length=8, rotation=pi / 2)')
s = replace_once(s, '        assert all(len(visited) == 2 for visited in endpoint_visits)', '        assert all(len(visited) == 2 for visited in endpoint_visits)\n        assert all(visits[0] == count for visits, count in implicit_counts), "implicit callback escaped preparation"')
p.write_text(s)
python_source = s.split('const source = `', 1)[1].split('\n`;', 1)[0]
compile(python_source, 'plotting-smoke.py', 'exec')
print('Resolved only the two inspected export conflicts; added direct WASM and real Python precision regressions')
