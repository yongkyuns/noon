import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";

const smoke = await readFile(
  new URL("../scripts/retained-execution-worker-smoke.mjs", import.meta.url),
  "utf8",
);
const generalClient = await readFile(new URL("./execution-worker-client.js", import.meta.url), "utf8");
const authoringClient = await readFile(
  new URL("./authoring-execution-client.js", import.meta.url),
  "utf8",
);
const retainedEngine = await readFile(
  new URL("./retained-execution-engine-worker.js", import.meta.url),
  "utf8",
);
const renderEntry = await readFile(new URL("./execution-render-worker.js", import.meta.url), "utf8");
const retainedAuthoringPlayer = await readFile(
  new URL("../crates/noon-web/src/retained_authoring_player.rs", import.meta.url),
  "utf8",
);
const canonicalRetainedEnginePlayer = await readFile(
  new URL("../crates/noon-web/src/canonical_retained_engine_player.rs", import.meta.url),
  "utf8",
);

await assert.rejects(
  access(new URL("./retained-execution-worker-client.js", import.meta.url)),
  (error) => error?.code === "ENOENT",
  "the standalone retained execution client must stay retired",
);
await assert.rejects(
  access(new URL("./retained-execution-render-worker.js", import.meta.url)),
  (error) => error?.code === "ENOENT",
  "retained execution must not regain a second render owner",
);

assert.match(
  smoke,
  /import\("\.\/execution-worker-client\.js"\)/,
  "retained browser qualification must exercise the shared execution client",
);
assert.match(
  smoke,
  /startRetainedCanonical\(sceneSpecJson,/,
  "retained browser qualification must start from canonical SceneSpec",
);
assert.doesNotMatch(
  smoke,
  /RetainedExecutionWorkerClient|runCompatibilityFallback|\.start\(sceneJson, retainedDocumentJson,/,
  "retained browser qualification must not preserve the retired split-player path",
);
assert.match(
  generalClient,
  /new URL\("\.\/retained-execution-engine-worker\.js", import\.meta\.url\)/,
  "the shared execution client remains the retained engine owner",
);
for (const [surface, source] of [
  ["execution client", generalClient],
  ["authoring client", authoringClient],
]) {
  assert.doesNotMatch(
    source,
    /async startRetained\(/,
    `${surface} must not expose split retained startup`,
  );
}
for (const method of ["switchToRetained", "rebuildRetained"]) {
  assert.doesNotMatch(
    generalClient,
    new RegExp(`async ${method}\\(`),
    `execution client must not expose split ${method}`,
  );
}
assert.match(
  retainedEngine,
  /CanonicalRetainedEngineScenePlayer/,
  "retained engine must lower canonical SceneSpec",
);
assert.doesNotMatch(
  retainedEngine,
  /MixedRetainedEngineScenePlayer|AUTHORING_COMPATIBILITY/,
  "retained engine must not retain split authoring execution",
);
assert.match(
  retainedEngine,
  /retained execution init accepts only canonical sceneSpecJson/,
  "retained engine must reject legacy split wire fields",
);
assert.doesNotMatch(
  retainedAuthoringPlayer,
  /RetainedAuthoringEnginePlayer|MixedRetainedEngineScenePlayer|WasmMixedRetainedEngineScenePlayer/,
  "noon-web must not re-export a split retained engine facade",
);
assert.match(
  canonicalRetainedEnginePlayer,
  /js_name = CanonicalRetainedEngineScenePlayer/,
  "canonical SceneSpec must remain the sole retained WASM engine constructor",
);
assert.match(
  renderEntry,
  /import "\.\/authoring-render-worker\.js";/,
  "legacy and retained execution must share the permanent authoring render owner",
);

console.log("✓ retained browser execution has one canonical client/engine/render topology");
