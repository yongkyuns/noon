import assert from "node:assert/strict";
import { qualifyPairedAuthoring, qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_BAR_CHART_ARTIFACTS ?? "bar-chart-artifacts",
  cases: [{
    id: "bar-chart", file: "bar_chart.py", scene: "BarChartExample",
    factory: "createBarChartRenderer",
    preparation: { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" },
  }],
  async qualifyLifecycle(context, baseUrl, expectedBackend) {
    const source = `from noon import *

class LiveBarChart(Scene):
    async def construct(self):
        await prepare_latex()
        marker = Dot(4 * LEFT)
        self.add(marker)
        await self.wait(0.25)
        chart = BarChart([-2, 0, 3], bar_names=["A", "B", "C"],
                         y_range=[-4, 4, 1], x_length=6, y_length=4)
        assert self.mobjects == [marker]
        self.add(chart)
        chart.shift(UP)
        third = chart.bars[2]
        chart.change_bar_values([2, -1], update_colors=False)
        assert chart.bars[2] is third
        labels = chart.get_bar_labels()
        assert [label.tex_string for label in labels] == ["2", "-1", "3"]
        assert labels[0].get_bottom()[1] > chart.bars[0].get_top()[1]
        assert labels[1].get_top()[1] < chart.bars[1].get_bottom()[1]
        assert self.mobjects == [marker, chart]
        self.add(labels)
        await self.wait(0.25)
`;
    const result = await qualifyPythonPlayback(context, baseUrl, source, [0, 0.25, 0.5]);
    assert.equal(result.rendererBackend, expectedBackend);
    assert.equal(result.presented, true);
    assert.equal(result.authoredDuration, 0.5);
    return result;
  },
});
