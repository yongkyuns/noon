import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_BRACE_ARTIFACTS ?? "brace-artifacts",
  cases: [{ id: "brace-text", file: "ordinary_brace_text.py", scene: "BraceTextExample",
    factory: "createBraceTextRenderer", objectCount: 6, duration: 0.2, sampleTime: 0.2 }],
});
