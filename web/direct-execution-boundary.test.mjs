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
const browserWakePlan = await readFile(
  new URL("../crates/noon-web/src/execution_wake.rs", import.meta.url),
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
  "session\n        .activate_animation(",
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
  "BrowserExecutionWakePlan::from_session(session)",
  "with_additional_presentation_pending(!self.pending_changes.is_empty())",
  "directWakeCadence",
  "advanceDirectToSceneTime",
]) {
  assert.ok(
    directCanvasHost.includes(required),
    `direct canvas host must consume canonical session state through ${required}`,
  );
}
assert.ok(
  browserWakePlan.includes("with_additional_presentation_pending"),
  "browser wake plan must preserve presentation work after the canvas host drains session changes",
);

for (const required of [
  "createDirectExecutionSmokeRenderer",
  "new OffscreenCanvas",
  "directPresentPending",
  "directWakeCadence",
  "directTimerDelaySeconds",
  "advanceDirectToSceneTime",
  "requestAnimationFrame",
  'cadence === "idle"',
]) {
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
