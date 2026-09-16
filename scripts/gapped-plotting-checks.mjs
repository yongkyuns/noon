// Independent external-test expectations, not an engine or example dependency.
import assert from "node:assert/strict";

// Sample the exact authored key produced by the explicit data-window mapping,
// not the adjacent representable literal 1.2 (which is just before that key).
const gapStart = (2 / 10) * 6;
const gapEnd = (5 / 10) * 6;
export const gapCheckpoints = [0.6, 1.19, gapStart, 1.21, 1.8, 2.99, gapEnd, 3.01, 4.5, 6];
export const gapArgumentSource = `from noon import *
from _manim_plotting import _owned, _array
from _noon_errors import engine_call
from pyodide.ffi import jsnull
class GapValidation(Scene):
    def construct(self):
        axes = Axes((0, 10, 2), (0, 3, 1), x_length=10, y_length=4)
        rows = (((0, 0), (2, 1), (5, 2), (10, 1)), ((0, 2), (3, 2.4), (7, 2), (10, 2)))
        plan = axes.gapped_series_plan(rows, break_after=((1,), ()), time_range=(0, 10), run_time=6)
        assert plan.data_times == (0, 2, 3, 5, 7, 10)
        assert plan.series_points[0][2] is None
        assert plan.series_points[0][1] == axes.c2p(2, 1)
        assert plan.series_points[0][3] == axes.c2p(5, 2)
        assert plan.series_segments[0][1:3] == (None, None)
        assert all(pair is not None for pair in plan.series_segments[1])
        empty = axes.gapped_series_plan(rows, break_after=((1,), ()), time_range=(3, 4), run_time=1)
        assert empty.series_points[0] == (None, None)
        assert empty.series_segments[0] == (None,)
        connected = axes.gapped_series_plan(rows, break_after=((), ()), time_range=(0, 10), run_time=6)
        dense = axes.synchronized_series_plan(rows, time_range=(0, 10), run_time=6)
        assert connected.series_points == dense.series_points
        for bad in (((-1,), ()), ((4,), ()), ((1, 1), ()), ((2, 1), ()), ((2**32,), ()), ()):
            try:
                axes.gapped_series_plan(rows, break_after=bad, time_range=(0, 10), run_time=6)
            except ValueError:
                pass
            else:
                raise AssertionError("invalid break list accepted")
        for index in (True, 0.5, "1"):
            try:
                axes.gapped_series_plan(rows, break_after=((index,), ()), time_range=(0, 10), run_time=6)
            except TypeError:
                pass
            else:
                raise AssertionError("noninteger Python break index accepted")
        with _owned(axes._coordinate_frame()) as frame:
            values, counts, window = _array((0, 1, 10, 2)), _array((2,)), _array((0, 10))
            for index in (-1, 0.5, 1, 2**32, float("nan"), float("inf")):
                try:
                    engine_call(frame.gappedSeriesPlan, values, counts, _array((index,)), _array((1,)), window, 6)
                except ValueError:
                    pass
                else:
                    raise AssertionError("invalid WASM break index accepted")
            for count in (-1, 0.5, 2**32, 2, float("nan")):
                try:
                    engine_call(frame.gappedSeriesPlan, values, counts, _array((0,)), _array((count,)), window, 6)
                except ValueError:
                    pass
                else:
                    raise AssertionError("invalid WASM break count accepted")
            with _owned(engine_call(frame.gappedSeriesPlan, values, counts, _array((0,)), _array((1,)), window, 6)) as raw:
                assert list(engine_call(raw.seriesSegments, 0)) == [jsnull]
                for index in (-1, 0.5, 1, 2**32, float("nan")):
                    for query in (raw.seriesPoints, raw.seriesSegments):
                        try:
                            engine_call(query, index)
                        except ValueError:
                            pass
                        else:
                            raise AssertionError("invalid gapped series index accepted")
        axes.shift((1, 0))
        assert plan.series_points[0][1] != axes.c2p(2, 1)
        self.add(axes)
`;

export function assertGapPixels(png, time, recordings, unionTimes, regionCount) {
  const t = time / 6 * 10;
  // Markers represent the outgoing interval; at recovery they appear at its
  // first measured endpoint. The completed blue history keeps the pre-gap end.
  const outage = time >= gapStart && time < gapEnd;
  const colors = [
    (r, g, b) => b > r + 35 && g > r + 20,
    (r, g, b) => r > g + 20 && g > b + 20,
  ];
  const point = (row, stamp) => {
    const source = recordings[row];
    const next = Math.max(1, source.findIndex(([x]) => x >= stamp));
    const [a, b] = [source[next - 1], source[next]];
    const value = a[1] + (b[1] - a[1]) * (stamp - a[0]) / (b[0] - a[0]);
    return [stamp - 5, value * 4 / 3 - 2];
  };
  for (let row = 0; row < 2; row++) {
    if (row !== 0 || !outage) {
      assert.ok(regionCount(png, ...point(row, t), colors[row]) > 35,
        `recording ${row} marker is not at data time ${t}`);
    }
    for (let i = 0; i + 1 < unionTimes.length; i++) {
      const a = unionTimes[i], b = unionTimes[i + 1];
      if (row === 0 && a >= 2 && b <= 5) continue;
      const stamp = (a + b) / 2;
      // Near-boundary samples intentionally overlap the actual marker disk.
      // Check history/future outside that disk instead of treating it as a line.
      if (Math.abs(stamp - t) < 0.15) continue;
      const count = regionCount(png, ...point(row, stamp), colors[row]);
      if (stamp < t) assert.ok(count > 2, `recording ${row} omitted measured history at ${stamp}`);
      else assert.equal(count, 0, `recording ${row} prematurely revealed ${stamp}`);
    }
  }
  if (outage) {
    assert.ok(regionCount(png, ...point(0, 2), colors[0]) < 35,
      "a stale blue marker was held at the last measurement");
  }
  for (const stamp of [2.25, 3, 3.75, 4.75]) {
    if (Math.abs(stamp - t) < 0.15) continue; // shared cursor, not a bridge
    assert.equal(regionCount(png, ...point(0, stamp), colors[0]), 0,
      "blue curve bridges the outage");
    assert.equal(regionCount(png, ...point(0, stamp), (r, g, b) =>
      r > 15 && Math.abs(r - g) < 4 && Math.abs(r - b) < 4), 0,
    "dim reference bridges the outage");
  }
  assert.ok(regionCount(png, t - 5, -1.8,
    (r, g, b) => g > r + 25 && g > b + 25) > 2, "shared cursor stopped or shifted during gap");
}
