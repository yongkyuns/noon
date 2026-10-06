import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const script = path.join(root, "scripts/perf-corpus.mjs");

for (const [name, env, expected] of [
  ["diagnostic flags", { NOON_CORPUS_INCLUDE_SAMPLES: "true" }, "NOON_CORPUS_INCLUDE_SAMPLES must be 0 or 1"],
  ["browser mode", { NOON_CORPUS_BROWSER_MODE: "physical" }, "NOON_CORPUS_BROWSER_MODE must be headless or headful"],
]) {
  test(`rejects invalid ${name} before starting the browser`, () => {
    const result = spawnSync(process.execPath, [script], {
      cwd: root,
      encoding: "utf8",
      env: { ...process.env, ...env },
    });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, new RegExp(expected.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
    assert.doesNotMatch(result.stderr, /Corpus .*…/);
  });
}
