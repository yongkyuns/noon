import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const artifact = path.resolve(root, process.env.NOON_SCENE_PERF_REPORT ?? "browser-smoke-artifacts/scene-perf-report.json");
const server = await serveRepository(root, Number(process.env.NOON_SCENE_PERF_PORT ?? 4193));
const browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs("webgpu") });
const reports = [];
try {
  const cases = [
    { name: "real-demo", frames: 4, hz: 1, objects: 3, continuation: true },
    { name: "static", source: "from noon import Scene, Circle\nresult = Scene()\nresult.add(Circle(0.5))", objects: 1, continuation: false },
    { name: "empty", source: "from noon import Scene\nresult = Scene()", objects: 0, continuation: false },
    { name: "source-error", source: 'raise ValueError("profile source failure")', error: "profile source failure" },
    { name: "continuation-error", source: 'from noon import Scene\nclass Failure(Scene):\n    def construct(self):\n        self.wait(0.1)\n        raise ValueError("profile continuation failure")', hz: 1, error: "profile continuation failure" },
  ];
  for (const spec of cases) {
    const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
    try {
      if (spec.source) await page.route("**/python/demo_scene.py", route => route.fulfill({ contentType: "text/plain", body: spec.source }));
      await page.goto(`${server.baseUrl}/web/scene-perf.html?warmup=1&frames=${spec.frames ?? 2}&targetHz=${spec.hz ?? 60}`);
      await page.waitForFunction(() => ["complete", "error"].includes(document.querySelector("#status")?.dataset.state), null, { timeout: 60000 });
      const result = await page.evaluate(() => ({ state: document.querySelector("#status").dataset.state, status: document.querySelector("#status").value, report: window.__NOON_SCENE_PERF__ }));
      if (spec.error) {
        assert.equal(result.state, "error");
        assert.ok(result.status.includes(spec.error), result.status);
      } else {
        assert.equal(result.state, "complete", result.status);
        assert.equal(result.report.schemaVersion, 2);
        assert.equal(result.report.execution.mode, "semantic");
        assert.equal(result.report.execution.sourceContinuation, spec.continuation);
        assert.equal(result.report.scene.objects, spec.objects);
        assert.ok(result.report.cadence.frames > 0);
        assert.ok(result.report.pipeline.advanceRoundTripMs.p95 >= 0);
        assert.equal(result.report.cpu, undefined);
        assert.equal(result.report.setup.serializationMs, undefined);
      }
      reports.push({ name: spec.name, ...result });
      console.log(`PASS ${spec.name}`);
    } finally { await page.close(); }
  }
  await mkdir(path.dirname(artifact), { recursive: true });
  await writeFile(artifact, JSON.stringify(reports, null, 2));
} finally { await browser.close(); await server.close(); }
