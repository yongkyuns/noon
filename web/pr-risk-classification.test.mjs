import assert from "node:assert/strict";
import { classifyChangedPaths } from "../scripts/classify-pr-risk.mjs";

{
  const classification = classifyChangedPaths([
    "README.md",
    "crates/noon-core/src/lib.rs",
    "web/example-gallery.js",
  ]);
  assert.equal(classification.rendererCritical, false);
  assert.deepEqual(classification.rendererCriticalPaths, []);
}

{
  const classification = classifyChangedPaths([
    "crates/noon-render-wgpu/src/lib.rs",
    "crates/noon-text-render-wgpu/src/glyph.rs",
    "web/execution-render-worker.js",
    "web/retained-execution-render-worker.js",
  ]);
  assert.equal(classification.rendererCritical, true);
  assert.deepEqual(classification.rendererCriticalPaths, [
    "crates/noon-render-wgpu/src/lib.rs",
    "crates/noon-text-render-wgpu/src/glyph.rs",
    "web/execution-render-worker.js",
    "web/retained-execution-render-worker.js",
  ]);
}

{
  const classification = classifyChangedPaths([
    ".github/workflows/pr-fast.yml",
    "crates/noon-web/src/lib.rs",
    "scripts/browser-smoke.mjs",
    "web/browser-smoke.js",
    "web/authoring-render-worker.js",
    "web/render-gpu-diagnostics.js",
  ]);
  assert.equal(classification.rendererCritical, true);
  assert.deepEqual(classification.rendererCriticalPaths, [
    ".github/workflows/pr-fast.yml",
    "crates/noon-web/src/lib.rs",
    "scripts/browser-smoke.mjs",
    "web/browser-smoke.js",
    "web/authoring-render-worker.js",
    "web/render-gpu-diagnostics.js",
  ]);
}

{
  const classification = classifyChangedPaths(["", "  ", "README.md"]);
  assert.equal(classification.rendererCritical, false);
}

console.log("PR risk classification tests passed");
