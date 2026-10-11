import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const workflowDir = new URL("../workflows/", import.meta.url);

test("cross-browser matrix covers all browsers using exactly three gallery shards", async () => {
  const yaml = await readFile(new URL("playground-cross-browser.yml", workflowDir), "utf8");
  assert.match(yaml, /^  gallery:$/m);
  assert.ok(yaml.includes("browser: [chromium, firefox, webkit]"));
  assert.ok(yaml.includes("shard: [0, 1, 2]"));
  assert.ok(yaml.includes('NOON_GALLERY_SHARD_INDEX: "${{ matrix.shard }}"'));
  assert.ok(yaml.includes('NOON_GALLERY_SHARD_COUNT: "3"'));
  assert.ok(yaml.includes("node --test scripts/gallery-shards.test.mjs"));
  assert.ok(yaml.includes("node scripts/playground-gallery-runtime-smoke.mjs"));
  assert.ok(yaml.includes("name: playground-gallery-${{ matrix.browser }}-shard-${{ matrix.shard }}"));
  assert.doesNotMatch(yaml, /^      - name: Test every selectable gallery example$/m,
    "unsharded gallery runner should not duplicate the full inventory");
});

test("renderer parity runs both real backends and comparison on one runner", async () => {
  const yaml = await readFile(new URL("ci.yml", workflowDir), "utf8");
  assert.match(yaml, /^  browser-rendering-parity:$/m);
  assert.doesNotMatch(yaml, /^  browser-rendering:$/m);
  assert.ok(yaml.includes("NOON_BROWSER_SMOKE_BACKEND: webgpu"));
  assert.ok(yaml.includes("NOON_BROWSER_SMOKE_BACKEND: webgl"));
  assert.equal(yaml.split("node scripts/browser-smoke.mjs").length - 1, 2);
  assert.equal(yaml.split("node scripts/spatial-surface-light-qualification.mjs").length - 1, 2);
  assert.equal(yaml.split("node scripts/camera-density-smoke.mjs").length - 1, 2);
  assert.ok(yaml.includes("node scripts/special-camera-qualification.mjs"));
  assert.ok(yaml.includes("NOON_CAMERA_NO_JSPI=1"));
  assert.ok(yaml.includes("node scripts/painter-order-browser-smoke.mjs"));
  assert.ok(yaml.includes("node scripts/browser-backend-visual-parity.mjs"));
  assert.ok(yaml.includes("name: browser-backend-visual-parity"));
  assert.doesNotMatch(yaml, /needs: browser-rendering/);
});
