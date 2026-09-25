import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_NUMERIC_ARTIFACTS ?? "numeric-artifacts",
  cases: [{
    id: "numeric-decimal",
    file: "numeric_decimal_number.py",
    factory: "createNumericDecimalRenderer",
    objectCount: 1,
    preparation: { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" },
  }],
});
