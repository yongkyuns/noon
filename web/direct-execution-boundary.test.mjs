import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const rustHarness = await readFile(
  new URL("../crates/noon-web/src/direct_execution_smoke.rs", import.meta.url),
  "utf8",
);
const browserProbe = await readFile(
  new URL("./direct-execution-smoke-probe.js", import.meta.url),
  "utf8",
);
const browserSmokeHtml = await readFile(
  new URL("./browser-smoke.html", import.meta.url),
  "utf8",
);
const legacySceneDocumentType = ["Scene", "Spec"].join("");

for (const required of [
  "SemanticStore::new()",
  "ExecutionSession::from_semantic_store",
  "create_from_execution_session",
]) {
  assert.ok(rustHarness.includes(required), `direct Rust/WASM proof must contain ${required}`);
}
for (const forbidden of [
  "serde_json",
  legacySceneDocumentType,
  "ExecutionFrameMirror",
  "initial_delta_json",
  "apply_json",
]) {
  assert.equal(
    rustHarness.includes(forbidden),
    false,
    `direct Rust/WASM proof must not depend on ${forbidden}`,
  );
}

for (const required of ["createDirectExecutionSmokeRenderer", "new OffscreenCanvas"]) {
  assert.ok(browserProbe.includes(required), `browser direct-execution proof must contain ${required}`);
}
for (const forbidden of [
  "AuthoringSceneCore",
  "EngineScenePlayer",
  "ExecutionCanvasRenderer.create",
  "sceneJson",
  "initialDeltaJson",
  "applyDeltaJson",
  "JSON.stringify",
]) {
  assert.equal(
    browserProbe.includes(forbidden),
    false,
    `browser direct-execution proof must not route through ${forbidden}`,
  );
}

assert.ok(
  browserSmokeHtml.includes('src="./direct-execution-smoke-probe.js"'),
  "primary browser rendering smoke must execute the direct Rust/WASM proof",
);
