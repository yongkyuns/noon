import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

const preparation = { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" };
await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_LATEX_ARTIFACTS ?? "latex-artifacts",
  cases: [{
    id: "latex-text", file: "latex_text.py", factory: "createLatexTextRenderer", objectCount: 3,
    preparation,
  }, ...[0, 0.875, 1.75, 2.75].map(sampleTime => ({
    id: `equation-card-${sampleTime}`, file: "showcase_equation_card.py", factory: "createLatexGroupRenderer",
    duration: 2.75, playback: "live", sampleTime, objectCount: sampleTime < 0.5 ? 1 : 7,
    boundaries: [0.5, 1.25, 1.5, 2], stopAtSourceCompletion: sampleTime === 2.75,
    preparation,
  }))],
});
