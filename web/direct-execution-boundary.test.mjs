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
const wakeProjection = await readFile(
  new URL("../crates/noon-web/src/execution_wake.rs", import.meta.url),
  "utf8",
);
const browserDriver = await readFile(
  new URL("./direct-execution-wake-driver.js", import.meta.url),
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
const browserSmokeRunner = await readFile(
  new URL("../scripts/browser-smoke.mjs", import.meta.url),
  "utf8",
);

for (const required of [
  "SemanticStore::new()",
  "SemanticObjectRole::Camera2D",
  "session.camera()",
  "ExecutionSession::from_semantic_store",
  "create_from_execution_session",
  "activate_animation",
  "Mobject::from_text",
  "Text::new(\"Noon\")",
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
  "session.wake_state().frame_pending()",
  "session.take_renderer_publication()",
  "directWakeDirectiveJson",
  "advanceDirectRealtime",
]) {
  assert.ok(
    directCanvasHost.includes(required),
    `direct canvas host must consume canonical session authority through ${required}`,
  );
}
assert.equal(
  directCanvasHost.includes("let publication = session.take_renderer_publication();"),
  true,
  "direct canvas host must consume one typed publication only after presentation work begins",
);

for (const required of [
  "BrowserExecutionWakeClock",
  "BrowserHostWake::AnimationFrame",
  "BrowserHostWake::TimerAfterMilliseconds",
  "self.anchor = None",
]) {
  assert.ok(
    wakeProjection.includes(required),
    `browser wake projection must retain ${required}`,
  );
}

for (const required of [
  "directWakeDirectiveJson",
  "advanceDirectRealtime",
  "requestAnimationFrame",
  "setTimeout",
]) {
  assert.ok(browserDriver.includes(required), `browser wake driver must contain ${required}`);
}
for (const forbidden of [
  ".evaluate(",
  ".seek(",
  "ExecutionFrameMirror",
  "sceneJson",
  "initialDeltaJson",
  "applyDeltaJson",
]) {
  assert.equal(
    browserDriver.includes(forbidden),
    false,
    `browser wake driver must not create timeline or transport authority through ${forbidden}`,
  );
}

for (const required of [
  "createDirectExecutionSmokeRenderer",
  "createDirectExecutionWakeDriver",
  "new OffscreenCanvas",
  "renderer.time() >= 0.1",
  "textDrawCalls",
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
  "throw error;",
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

for (const required of [
  "window.noonDirectExecutionSmoke?.ready === true",
  "direct Rust/WASM execution proof failed",
  "direct.metrics.backend",
  "direct Rust/WASM execution did not present",
]) {
  assert.ok(
    browserSmokeRunner.includes(required),
    `browser smoke must gate the direct Rust/WASM proof through ${required}`,
  );
}
