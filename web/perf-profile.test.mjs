import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

test("frame profiling stays on direct typed Rust execution", async () => {
  const source = await readFile(new URL("./perf-profile.js", import.meta.url), "utf8");
  assert.match(source, /createDirectAnalyticProfileRenderer/);
  assert.match(source, /advanceDirectRealtime/);
  assert.doesNotMatch(source, /EngineScenePlayer|initialDeltaJson|applyDeltaJson|seekDeltaJson|setCamera\(|buildAnalyticScene/);
  await assert.rejects(access(new URL("./perf-workloads.js", import.meta.url)), { code: "ENOENT" });
});
