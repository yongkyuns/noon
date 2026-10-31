// G4 authoring parity, using the existing real Rust/Pyodide pixel harness.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { createRuntimeBuildIdentity } from "./build-runtime-identity.mjs";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const identity = JSON.parse(await readFile(root + "/web/runtime-build-identity.json", "utf8"));
assert.deepEqual(await createRuntimeBuildIdentity(root), identity, "exact clean source/package required");
assert.ok(identity.sourceRevision, "committed source revision required");
const source = await readFile(root + "/web/python/examples/glow_profile.py", "utf8");
await qualifyPairedAuthoring({
  artifactDirectory: "browser-smoke-artifacts/glow-profile",
  prepareContext: disableAuthoringJspi,
  cases: [1, 100].flatMap(active => ["intensity", "radius"].flatMap(property =>
    [0.5, 5, 9.5].map(sampleTime => ({
      id: `g4-${active}-${property}-${sampleTime}`,
      source: `context = ${JSON.stringify({ active_glows: active, property, duration: 10 })}\n${source}`,
      factory: "createDirectGlowProfileRenderer",
      factoryArgs: [active, property, 10],
      canvasSize: [1152, 648],
      duration: 10,
      sampleTime,
      objectCount: 601,
    })))),
  async qualifyLifecycle(context, baseUrl, backend) {
    const page = await context.newPage();
    try {
      if (backend === "WebGL2") await page.addInitScript(() =>
        Object.defineProperty(navigator, "gpu", { value: undefined }));
      const glue = await readFile(root + "/web/pkg/noon_web.js", "utf8");
      const mutated = glue + `
const originalColdRender = ExecutionCanvasRenderer.prototype.render;
ExecutionCanvasRenderer.prototype.render = function() {
  if (this.time() >= 10) { globalThis.refusedColdEndpoint = this.time(); return false; }
  return originalColdRender.call(this);
};`;
      await page.route("**/web/pkg/noon_web.js", route =>
        route.fulfill({ contentType: "application/javascript", body: mutated }));
      await page.goto(baseUrl + "/web/perf-profile.html?glows=1&objects=600&layout=fixed&glowProperty=intensity&warmup=30&frames=540&targetHz=60&width=1152&height=648");
      await page.waitForFunction(() => document.querySelector("#status")?.dataset.state === "error"
        || window.__NOON_PERF_REPORT__, null, { timeout: 90000 });
      const result = await page.evaluate(() => ({
        status: document.querySelector("#status").dataset.state,
        failure: window.__NOON_PERF_FAILURE__,
        endpoint: globalThis.refusedColdEndpoint,
        accepted: !!window.__NOON_PERF_REPORT__,
      }));
      assert.equal(result.status, "error");
      assert.equal(result.accepted, false);
      assert.equal(result.endpoint, 10, "actual runtime reached the endpoint");
      assert.equal(result.failure.coldPass, null, "missing presentation cannot complete the cold pass");
      assert.equal(result.failure.rawSamples.coldFrames.length, 599);
      assert.equal(result.failure.rawSamples.frames.length, 0);
      assert.match(result.failure.error, /performance frame was not presented at t=10/);
      return { mutationSha256: createHash("sha256").update(mutated).digest("hex"), ...result };
    } finally { await page.close(); }
  },
});
assert.deepEqual(await createRuntimeBuildIdentity(root), identity, "on-disk source/package unchanged");
