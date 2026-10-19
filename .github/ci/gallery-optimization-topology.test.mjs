import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const workflowDir = new URL("../workflows/", import.meta.url);

test("showcase backends share one authenticated producer artifact", async () => {
  const yaml = await readFile(new URL("playground-showcase-qualification.yml", workflowDir), "utf8");
  const job = name => yaml.split(`\n  ${name}:\n`)[1]?.split(/^  [a-z][a-z-]*:/m)[0];
  const build = job("build"), capture = job("capture"), native = job("callback-lifecycle");
  assert.ok(build && capture && native, "build, native and backend jobs must remain separate");
  assert.equal(yaml.split("uses: ./.github/actions/dev-wasm-build").length - 1, 1,
    "both backends must share a single current-source build");
  assert.match(build, /preflight: 'true'/);
  assert.match(build, /Source and editorial contracts/);
  assert.match(build, /uses: actions\/upload-artifact@v7/);
  for (const file of ["web/pkg/", "web/python-worker.js", "web/python/compat-bundle.*.json",
    "web/runtime-build-identity.json", "web/ci-artifact.json", "web/ci-Cargo.lock"]) {
    assert.ok(build.includes(file), `shared runtime is missing ${file}`);
  }
  assert.ok(build.includes("name: showcase-runtime-${{ github.run_id }}-${{ github.run_attempt }}"));
  assert.ok(build.includes("artifact-id: ${{ steps.runtime.outputs.artifact-id }}"));
  assert.match(build, /id: runtime/);
  assert.ok(capture.includes("artifact-ids: ${{ needs.build.outputs.artifact-id }}"),
    "consumers must reuse the producer's immutable artifact, including failed-job retries");
  assert.match(build, /if-no-files-found: error/);
  assert.match(build, /retention-days: 1/);
  assert.match(capture, /needs: build/);
  assert.doesNotMatch(capture.split("    steps:\n")[0], /if:/,
    "failed producers must block consumers through the ordinary needs barrier");
  assert.doesNotMatch(capture, /continue-on-error|dev-wasm-build|cargo test/);
  assert.match(capture, /backend: \[webgpu, webgl\]/);
  assert.match(capture, /fail-fast: false/);
  assert.match(capture, /uses: actions\/download-artifact@v8/);
  assert.match(capture, /path: web/);
  const verify = capture.indexOf("node .github/ci/wasm-build.mjs verify");
  assert.ok(verify > capture.indexOf("uses: actions/download-artifact@v8"));
  assert.ok(verify < capture.indexOf("npm install"), "reject wrong packages before browser setup");
  for (const command of ["node scripts/showcase-capture.mjs", "node scripts/showcase-live-review.mjs",
    "node scripts/showcase-triangle-light-motion.mjs"]) {
    assert.ok(capture.includes(command), `retain backend qualification: ${command}`);
  }
  assert.equal(yaml.split("cargo test -p noon-runtime replay::family_tests").length - 1, 1);
  assert.match(native, /cargo test -p noon-runtime replay::family_tests/);
});

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
  const selection = yaml.split("  pull_request:\n")[1]?.split("  workflow_dispatch:")[0];
  assert.ok(selection?.includes('      - "scripts/painter-order-browser-smoke.mjs"'),
    "painter-order harness edits must select their real browser qualification");
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
  const job = yaml.split("\n  browser-rendering-parity:\n")[1]?.split(/^  [a-z][a-z-]*:/m)[0];
  assert.ok(job, "missing consolidated rendering job");
  assert.doesNotMatch(job, /matrix\.backend/, "new backend checks must not retain a removed matrix guard");
});
