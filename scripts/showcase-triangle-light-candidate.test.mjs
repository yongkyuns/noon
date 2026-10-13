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
        if (fail) throw new Error("deliberate capture failure");
      } });
      if (fail) await assert.rejects(execution, /deliberate capture failure/);
      else await execution;
      assert.deepEqual(calls, fail ? ["showcase-capture.mjs"] : ["showcase-capture.mjs", "showcase-live-review.mjs"]);
      assert.equal(await readFile(path.join(entries, "noon_showcase_manifest.json"), "utf8"), original);
      assert.equal(await readFile(path.join(posters, "capture-evidence.json"), "utf8"), "existing-public-evidence");
      assert.equal(await readFile(path.join(output, "report.json"), "utf8"), "existing-public-report");
      assert.equal(await readFile(path.join(root, "browser-smoke-artifacts/triangle-light/webgl/report.json"), "utf8"), "candidate-report");
      await assert.rejects(readFile(path.join(posters, "showcase-triangle-light.png")), { code: "ENOENT" });
    } finally { await rm(root, { recursive: true, force: true }); }
  });
}
