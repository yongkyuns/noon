import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_MATRIX_ARTIFACTS ?? "matrix-artifacts",
  cases: [{
    id: "matrix", file: "ordinary_matrix.py", scene: "OrdinaryMatrix",
    factory: "createMatrixRenderer", objectCount: 6,
    preparation: { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" },
  }],
});
