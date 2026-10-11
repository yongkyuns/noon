import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const scripts = [
  "../scripts/retained-text-animation-smoke.mjs",
  "../scripts/retained-text-family-fade-smoke.mjs",
  "../scripts/retained-text-scene-lifecycle-smoke.mjs",
];

for (const relative of scripts) {
  test(`${relative} uses the bounded shared browser harness without broad ready probes`, async () => {
    const source = await readFile(new URL(relative, import.meta.url), "utf8");
    assert.match(source, /serveRepository\(repoRoot, port, \{ crossOriginIsolated: true \}\)/);
    assert.match(source, /browserArgs\("webgpu"\)/);
    assert.doesNotMatch(source, /python3.*http\.server/);
    assert.doesNotMatch(source, /noonManimCompat\.ready\(\)/);
  });
}
