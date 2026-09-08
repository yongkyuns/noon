import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const command = new URL("./perf-compare.mjs", import.meta.url);
test("shared authoring comparison measures reruns and rejects obsolete schemas", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "noon-perf-compare-"));
  try {
    const artifact = (schemaVersion, latency) => ({
      schemaVersion, benchmark: "Noon shared authoring latency matrix",
      cases: [{ workload: { objects: 10 }, warmUnchanged: { totalRoundTripMs: { p95: latency } } }],
    });
    const before = path.join(directory, "before.json");
    const after = path.join(directory, "after.json");
    await writeFile(before, JSON.stringify(artifact(2, 10)));
    await writeFile(after, JSON.stringify(artifact(2, 20)));
    const run = () => spawnSync(process.execPath, [command.pathname, before, after], {
      encoding: "utf8", env: { ...process.env, NOON_PERF_REGRESSION_PCT: "50" },
    });
    const regression = run();
    assert.equal(regression.status, 2, regression.stderr);
    assert.match(regression.stdout, /unchanged rerun p95 ms.*\+100\.0%/);
    await writeFile(before, JSON.stringify(artifact(1, 10)));
    const obsolete = run();
    assert.equal(obsolete.status, 1);
    assert.match(obsolete.stderr, /authoring baseline must use the shared profiler schema/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
