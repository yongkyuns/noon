import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_TABLE_ARTIFACTS ?? "table-artifacts",
  cases: [{
    id: "table", file: "table.py", scene: "RetainedTable",
    factory: "createTableRenderer", objectCount: 28,
    preparation: { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" },
  }],
});
