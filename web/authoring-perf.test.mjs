import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

test("authoring tools cannot restore the deleted frontend identity authority", async () => {
  for (const file of ["authoring-perf.js", "main.js", "scene-perf.js"]) {
    const source = await readFile(new URL(file, import.meta.url), "utf8");
    assert.doesNotMatch(source, /SceneIdentityMap|NoonCanvasPlayer|\.reconcileScene\(/, file);
  }
  for (const file of ["scene-identity.js", "scene-pipeline-perf.mjs"]) {
    await assert.rejects(access(new URL(file, import.meta.url)), { code: "ENOENT" });
  }
});
