import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const rustHarness = await readFile(
  new URL("../crates/noon-web/src/direct_execution_smoke.rs", import.meta.url),
  "utf8",
);
const directCanvasHost = await readFile(
  new URL("../crates/noon-web/src/execution_canvas.rs", import.meta.url),
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

for (const required of [
  "SemanticStore::new()",
  "SemanticObjectRole::Camera2D",
  "session.camera()",
  "ExecutionSession::from_semantic_store",
  "create_from_execution_session",
]) {
  assert.ok(rustHarness.includes(required), `direct Rust/WASM proof must contain ${required}`);
}
for (const forbidden of [
  "serde_json",
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

for (const required of [
  "let camera = session.camera().map_err(js_error)?;",
  "self.sync_camera(camera)?;",
]) {
  assert.ok(
    directCanvasHost.includes(required),
    `direct canvas host must consume canonical session camera through ${required}`,
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
  "setCamera",
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
