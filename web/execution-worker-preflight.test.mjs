import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

test("semantic readiness follows initial renderer publication", async () => {
  const source = await readFile(new URL("./semantic-engine-endpoint.js", import.meta.url), "utf8");
  const snapshot = source.indexOf("await publishCallbackPhase(player.initialCallbackPhaseJson(), { initial: true })");
  const ready = source.indexOf('post({ type: "ready", transportMode })');
  assert.ok(snapshot >= 0 && ready > snapshot);
  for (const retired of ["execution-engine-worker.js", "legacy-reactive-projection.js"]) {
    await assert.rejects(access(new URL(retired, import.meta.url)));
  }
});
