import assert from "node:assert/strict";
import test from "node:test";

import {
  classifyPrRisk,
  requiresHostUpdaterDiagnostic,
  requiresRetainedExecutionSmoke,
} from "./pr-risk-classifier.mjs";

const canonicalRetainedRoutingChange = [
  "web/authoring-execution-client.js",
  "web/execution-worker-client.js",
  "web/scene-identity.js",
];

const retainedRuntimeChanges = [
  "web/retained-execution-engine-worker.js",
  "web/execution-render-worker.js",
  "web/authoring-render-worker.js",
  "web/execution-transport.js",
  "crates/noon-web/src/canonical_retained_engine_player.rs",
  "crates/noon-web/src/retained_execution_canvas.rs",
  "crates/noon-render-wgpu/src/retained_scene.rs",
  "crates/noon-web/src/lib.rs",
];

const ordinaryChanges = [
  ".github/workflows/pr-fast.yml",
  "docs/ci.md",
  "scripts/pr-risk-classifier.mjs",
  "web/main.js",
  "web/playground-gallery.js",
  "web/execution-engine-worker.js",
  "crates/noon-geometry/src/lib.rs",
  "crates/noon-web/src/manim_geometry_bridge.rs",
];

test("canonical retained routing changes escalate the retained worker smoke", () => {
  const risk = classifyPrRisk(canonicalRetainedRoutingChange);
  assert.equal(risk.retainedExecution, true);
  assert.deepEqual(risk.retainedExecutionPaths, canonicalRetainedRoutingChange.slice().sort());
});

test("retained engine, shared render, and Rust ownership boundaries escalate", () => {
  for (const path of retainedRuntimeChanges) {
    assert.equal(requiresRetainedExecutionSmoke(path), true, path);
  }
});

test("ordinary CI, docs, demo, legacy engine, geometry, and Manim bridge changes stay fast", () => {
  const risk = classifyPrRisk(ordinaryChanges);
  assert.equal(risk.retainedExecution, false);
  assert.deepEqual(risk.retainedExecutionPaths, []);
});

test("classifier normalizes diff-style paths and de-duplicates triggers", () => {
  const risk = classifyPrRisk([
    "./web/authoring-render-worker.js",
    "web\\authoring-render-worker.js",
    "web/authoring-render-worker.js",
    "",
  ]);
  assert.deepEqual(risk.retainedExecutionPaths, ["web/authoring-render-worker.js"]);
});

test("host-updater changes escalate the backend diagnostic harness", () => {
  assert.equal(requiresHostUpdaterDiagnostic("./scripts/manim-host-updater-diagnostics.mjs"), true);
  assert.equal(requiresHostUpdaterDiagnostic("crates/noon-render-wgpu/src/gpu_geometry.rs"), true);
  assert.equal(requiresHostUpdaterDiagnostic("web/main.js"), false);

  const risk = classifyPrRisk([
    "./scripts/manim-host-updater-diagnostics.mjs",
    "scripts/manim-host-updater-diagnostics.mjs",
    "web/main.js",
  ]);
  assert.equal(risk.hostUpdaterDiagnostics, true);
  assert.deepEqual(risk.hostUpdaterDiagnosticPaths, ["scripts/manim-host-updater-diagnostics.mjs"]);
});
