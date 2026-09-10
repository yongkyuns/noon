import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { loadPreviewRuntimeConfig } from "../src/preview-isolation.mjs";
import { DockerPreviewSession } from "../src/preview-runner.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../../..");
const config = await loadPreviewRuntimeConfig({
  repoRoot,
  configPath: process.env.NOON_PREVIEW_RUNTIME_CONFIG,
});
const source = await readFile(path.join(repoRoot, "web/python/examples/manim_parity_square_to_circle.py"), "utf8");
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");

async function deterministicRun(label) {
  const session = new DockerPreviewSession(config);
  try {
    const initial = await session.open(source, { loopDurationSeconds: 4 });
    assert.equal(initial.state, "ready");
    assert.equal(initial.frame.requestedTime, 0);
    assert.equal(initial.frame.rendererBackend, "WebGL2");
    assert.equal(initial.image.mimeType, "image/png");
    assert.ok(initial.imageData.length > 1000);

    const one = await session.sample(1);
    const middle = await session.sample(1.5);
    const final = await session.sample(3);
    for (const [time, sample] of [[1, one], [1.5, middle], [3, final]]) {
      assert.equal(sample.state, "ready");
      assert.equal(sample.frame.requestedTime, time);
      assert.equal(sample.frame.publishedTime, time);
      assert.equal(sample.frame.rendererBackend, "WebGL2");
      assert.ok(sample.imageData.length > 1000);
    }
    const hashes = [initial, one, middle, final].map((sample) => hash(sample.imageData));
    assert.notEqual(hashes[0], hashes[1], "Create must change the preview image");
    assert.notEqual(hashes[1], hashes[2], "Transform must change the preview image");
    assert.notEqual(hashes[2], hashes[3], "FadeOut must change the preview image");

    const inspected = await session.inspect();
    assert.equal(inspected.frame.requestedTime, 3);
    assert.equal(inspected.image.sha256, hashes[3]);
    return { label, hashes, last: inspected };
  } finally {
    const closed = await session.close(`${label} complete`);
    assert.equal(closed.state, "closed");
    assert.equal(closed.cleanup.closed, true);
    assert.equal(closed.cleanup.cleanup.removed, true);
  }
}

const first = await deterministicRun("first");
const fresh = await deterministicRun("fresh");
assert.deepEqual(fresh.hashes, first.hashes, "fresh isolated session must reproduce deterministic frames");

const stuck = new DockerPreviewSession(config);
try {
  const stuckSource = "from noon import *\nclass Stuck(Scene):\n    def construct(self):\n        self.add(Circle())\n        self.wait(0.1)\n        while True:\n            pass\n";
  await stuck.open(stuckSource, { loopDurationSeconds: 4 });
  const pending = stuck.sample(0.2);
  pending.catch(() => {});
  await new Promise((resolve) => setTimeout(resolve, 250));
  const started = Date.now();
  const closed = await stuck.close("cancel stuck source");
  const elapsedMs = Date.now() - started;
  assert.equal(closed.state, "closed");
  assert.ok(elapsedMs < 10_000, `container cancellation exceeded bound: ${elapsedMs}ms`);
  await assert.rejects(pending);
  console.log(JSON.stringify({
    ok: true,
    firstHashes: first.hashes,
    freshHashes: fresh.hashes,
    cancelElapsedMs: elapsedMs,
    cleanup: closed.cleanup,
  }));
} finally {
  await stuck.close("stuck cleanup").catch(() => {});
}
