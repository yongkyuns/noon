import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { qualifyTriangleLight } from "./showcase-triangle-light-candidate.mjs";

for (const fail of [false, true]) {
  test(`candidate qualification restores public metadata and evidence on ${fail ? "failure" : "success"}`, async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "noon-triangle-test-"));
    try {
      const entries = path.join(root, "web/python/examples");
      const posters = path.join(root, "web/thumbnails/showcase");
      const output = path.join(root, "browser-smoke-artifacts/showcase/webgl");
      await Promise.all([entries, posters, output].map(dir => mkdir(dir, { recursive: true })));
      const candidateBytes = await readFile(new URL("../web/python/examples/showcase_triangle_light.candidate.json", import.meta.url));
      await writeFile(path.join(entries, "showcase_triangle_light.candidate.json"), candidateBytes);
      const original = '{"version":1,"publication":"curated","entries":[]}\n';
      await writeFile(path.join(entries, "noon_showcase_manifest.json"), original);
      await writeFile(path.join(posters, "capture-evidence.json"), "existing-public-evidence");
      await writeFile(path.join(output, "report.json"), "existing-public-report");
      const calls = [];
      const execution = qualifyTriangleLight(root, { backend: "webgl", run: async script => {
        calls.push(script);
        const active = JSON.parse(await readFile(path.join(entries, "noon_showcase_manifest.json")));
        assert.deepEqual(active.entries.map(entry => entry.id), ["showcase-triangle-light"]);
        await mkdir(output, { recursive: true });
        await writeFile(path.join(output, "report.json"), "candidate-report");
        await writeFile(path.join(posters, "capture-evidence.json"), "candidate-evidence");
        await writeFile(path.join(posters, "showcase-triangle-light.png"), "candidate-poster");
        if (fail) throw new Error(`deliberate capture failure: ${script}`);
      } });
      if (fail) await assert.rejects(execution, error => {
        assert.ok(error instanceof AggregateError);
        assert.equal(error.errors.length, 2, "neither failure is swallowed");
        assert.match(error.message, /showcase-capture.mjs/);
        assert.match(error.message, /showcase-live-review.mjs/);
        return true;
      });
      else await execution;
      assert.deepEqual(calls, ["showcase-capture.mjs", "showcase-live-review.mjs"],
        "ordinary playback and replay must still run after capture fails");
      assert.equal(await readFile(path.join(entries, "noon_showcase_manifest.json"), "utf8"), original);
      assert.equal(await readFile(path.join(posters, "capture-evidence.json"), "utf8"), "existing-public-evidence");
      assert.equal(await readFile(path.join(output, "report.json"), "utf8"), "existing-public-report");
      assert.equal(await readFile(path.join(root, "browser-smoke-artifacts/triangle-light/webgl/report.json"), "utf8"), "candidate-report");
      await assert.rejects(readFile(path.join(posters, "showcase-triangle-light.png")), { code: "ENOENT" });
    } finally { await rm(root, { recursive: true, force: true }); }
  });
}

test("pre-merge review runs capture and replay against an exact-source WASM build", async () => {
  const workflow = await readFile(new URL("../.github/workflows/playground-triangle-light-review.yml", import.meta.url), "utf8");
  assert.match(workflow, /Build this exact PR head for source-bound capture and replay/);
  assert.match(workflow, /uses: \.\/\.github\/actions\/dev-wasm-build/);
  assert.match(workflow, /built\.get\("sourceRevision"\) != expected/);
  assert.doesNotMatch(workflow, /BASE_SHA|gh run download|runtime\.outputs\.reused/);
  const verify = workflow.indexOf("Verify the compiled runtime and checkout share the exact PR revision");
  const capture = workflow.indexOf("Capture and record the actual candidate through the existing Noon hosts");
  const publish = workflow.indexOf("Package only after actual candidate playback and replay pass");
  assert.ok(verify >= 0 && capture > verify && publish > capture, "review must verify provenance before capture and preview delivery");
  const captureStep = workflow.slice(capture, workflow.indexOf("Retain actual frames", capture));
  assert.match(captureStep, /if: \$\{\{ success\(\) \}\}/, "failed build identity must not launch the capture");
});
