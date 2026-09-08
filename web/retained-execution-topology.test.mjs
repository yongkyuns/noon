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
const renderEntry = await readFile(new URL("./execution-render-worker.js", import.meta.url), "utf8");
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

for (const filename of [
  "canonical_retained_engine_player.rs",
  "retained_authoring.rs",
  "retained_authoring_scene.rs",
  "retained_authoring_scene_spec.rs",
  "retained_authoring_tracks.rs",
  "retained_authoring_wire_scene.rs",
  "retained_authoring_player.rs",
  "canonical_family_animation.rs",
  "retained_family_execution_player.rs",
  "retained_scene_spec_runtime.rs",
  "authoring_semantics.rs",
]) {
  await assert.rejects(
    access(new URL(`../crates/noon-web/src/${filename}`, import.meta.url)),
    (error) => error?.code === "ENOENT",
    `the split authoring schema module must stay deleted: ${filename}`,
  );
}

for (const path of [
  "noon/src/retained_family_authoring_lowering.rs",
  "noon-runtime/src/reactive/family_plan_set_runtime.rs",
]) {
  await assert.rejects(
    access(new URL(`../crates/${path}`, import.meta.url)),
    (error) => error?.code === "ENOENT",
    `the separate family execution owner must stay deleted: ${path}`,
  );
}

assert.match(smoke, /startSemanticExecution/);
assert.doesNotMatch(smoke, /sceneSpecJson\(|bindMobject|startRetainedCanonical/);
await assert.rejects(access(new URL("./retained-execution-engine-worker.js", import.meta.url)),
  error => error?.code === "ENOENT");
assert.doesNotMatch(generalClient, /startRetainedCanonical|sceneSpecJson|retained-execution-engine-worker/);
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
  renderEntry,
  /import "\.\/authoring-render-worker\.js";/,
  "legacy and retained execution must share the permanent authoring render owner",
);

console.log("✓ retained browser execution has one shared semantic client/engine/render topology");
