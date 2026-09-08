import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("engine ready is emitted only after the complete initial render bootstrap is queued", async () => {
  const legacy = await readFile(new URL("./execution-engine-worker.js", import.meta.url), "utf8");
  const retained = await readFile(
    new URL("./retained-execution-engine-worker.js", import.meta.url),
    "utf8",
  );

  const legacySnapshot = legacy.indexOf("sendDeltaOrThrow(initial)");
  const legacyReady = legacy.indexOf('postMain({ type: "ready", transportMode })');
  assert.ok(legacySnapshot >= 0 && legacyReady > legacySnapshot);

  const retainedResources = retained.indexOf('type: "retained_resources"');
  const retainedSnapshot = retained.indexOf("sendDeltaOrThrow(player.initialDeltaJson())");
  const retainedReady = retained.indexOf('type: "ready"', retainedSnapshot);
  assert.ok(retainedResources >= 0);
  assert.ok(retainedSnapshot > retainedResources);
  assert.ok(retainedReady > retainedSnapshot);
});
