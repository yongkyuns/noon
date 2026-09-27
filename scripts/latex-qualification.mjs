import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_LATEX_ARTIFACTS ?? "latex-artifacts",
  cases: [{
    id: "latex-text", file: "latex_text.py", factory: "createLatexTextRenderer", objectCount: 3,
    preparation: { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" },
  }],
});
