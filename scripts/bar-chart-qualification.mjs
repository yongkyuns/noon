import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_BAR_CHART_ARTIFACTS ?? "bar-chart-artifacts",
  cases: [{
    id: "bar-chart", file: "bar_chart.py", scene: "BarChartExample",
    factory: "createBarChartRenderer",
    preparation: { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" },
  }],
});
