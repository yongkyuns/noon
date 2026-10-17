import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { qualifyPairedAuthoring, qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";

// These pixels qualify one shared scene across Rust/WASM and the production
// Python worker. They do not claim Pygments or Manim raster-oracle equivalence.
await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_CODE_ARTIFACTS ?? "code-artifacts",
  cases: [{
    id: "code-block", file: "code_block.py", scene: "CodeBlocks",
    factory: "createCodeRenderer", objectCount: 24,
  }],
  async qualifyLifecycle(context, baseUrl, expectedBackend) {
    const source = await readFile(new URL("./code-authoring-contract.py", import.meta.url), "utf8");
    const result = await qualifyPythonPlayback(context, baseUrl, source, [0, 0.05, 0.1]);
    assert.equal(result.rendererBackend, expectedBackend);
    assert.equal(result.presented, true);
    assert.ok(Math.abs(result.authoredDuration - 0.1) < 1e-6);
    return result;
  },
});
