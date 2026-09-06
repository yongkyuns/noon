import assert from "node:assert/strict";
import test from "node:test";

import {
  classifyPrRisk,
  requiresHostUpdaterDiagnostic,
  requiresRendererCriticalWebglSmoke,
  requiresRenderModeSwitchSmoke,
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

const renderModeSwitchChanges = [
  "scripts/pr-risk-classifier.mjs",
  "web/authoring-execution-client.js",
  "web/authoring-render-worker.js",
  "web/execution-engine-worker.js",
  "web/execution-render-worker.js",
  "web/execution-transport.js",
  "web/execution-worker-client.js",
  "web/execution-worker-smoke.html",
  "web/retained-execution-engine-worker.js",
];

const rendererCriticalChanges = [
  ".github/workflows/pr-fast.yml",
  "scripts/browser-smoke.mjs",
  "scripts/pr-risk-classifier.mjs",
  "web/authoring-render-worker.js",
  "web/browser-smoke.html",
  "web/browser-smoke.js",
  "web/execution-render-worker.js",
  "web/render-gpu-diagnostics.js",
  "crates/noon-render-wgpu/src/lib.rs",
  "crates/noon-text-render-wgpu/src/glyph.rs",
  "crates/noon-web/src/lib.rs",
];

const ordinaryChanges = [
  "docs/ci.md",
  "web/main.js",
  "web/playground-gallery.js",
  "crates/noon-core/src/lib.rs",
  "crates/noon-geometry/src/lib.rs",
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

test("render-owner, transition, and risk-policy changes escalate the mode-switch smoke", () => {
  const risk = classifyPrRisk(renderModeSwitchChanges);
  assert.equal(risk.renderModeSwitch, true);
  assert.deepEqual(risk.renderModeSwitchPaths, renderModeSwitchChanges.slice().sort());
  for (const path of renderModeSwitchChanges) {
    assert.equal(requiresRenderModeSwitchSmoke(path), true, path);
  }
});

test("renderer crates, browser render owners, smoke plumbing, and policy escalate WebGL", () => {
  const risk = classifyPrRisk(rendererCriticalChanges);
  assert.equal(risk.rendererCritical, true);
  assert.deepEqual(risk.rendererCriticalPaths, rendererCriticalChanges.slice().sort());
  for (const path of rendererCriticalChanges) {
    assert.equal(requiresRendererCriticalWebglSmoke(path), true, path);
  }
  assert.equal(requiresRendererCriticalWebglSmoke("README.md"), false);
});

test("ordinary docs, demo, core, and geometry changes stay on the base fast path", () => {
  const risk = classifyPrRisk(ordinaryChanges);
  assert.equal(risk.retainedExecution, false);
  assert.deepEqual(risk.retainedExecutionPaths, []);
  assert.equal(risk.renderModeSwitch, false);
  assert.deepEqual(risk.renderModeSwitchPaths, []);
  assert.equal(risk.hostUpdaterDiagnostics, false);
  assert.deepEqual(risk.hostUpdaterDiagnosticPaths, []);
  assert.equal(risk.rendererCritical, false);
  assert.deepEqual(risk.rendererCriticalPaths, []);
});

test("classifier normalizes diff-style paths and de-duplicates triggers", () => {
  const risk = classifyPrRisk([
    "./web/authoring-render-worker.js",
    "web\\authoring-render-worker.js",
    "web/authoring-render-worker.js",
    "",
  ]);
  assert.deepEqual(risk.retainedExecutionPaths, ["web/authoring-render-worker.js"]);
  assert.deepEqual(risk.renderModeSwitchPaths, ["web/authoring-render-worker.js"]);
  assert.deepEqual(risk.rendererCriticalPaths, ["web/authoring-render-worker.js"]);
});

test("host-updater changes escalate the backend diagnostic harness", () => {
  assert.equal(requiresHostUpdaterDiagnostic("./scripts/manim-host-updater-diagnostics.mjs"), true);
  assert.equal(requiresHostUpdaterDiagnostic("crates/noon-render-wgpu/src/gpu/retained_text.rs"), true);
  assert.equal(requiresHostUpdaterDiagnostic("web/authoring-render-worker.js"), true);
  assert.equal(requiresHostUpdaterDiagnostic("web/main.js"), false);

  const risk = classifyPrRisk([
    "./scripts/manim-host-updater-diagnostics.mjs",
    "scripts/manim-host-updater-diagnostics.mjs",
    "web/main.js",
  ]);
  assert.equal(risk.hostUpdaterDiagnostics, true);
  assert.deepEqual(risk.hostUpdaterDiagnosticPaths, ["scripts/manim-host-updater-diagnostics.mjs"]);
});
