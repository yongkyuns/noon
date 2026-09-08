import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = await readFile(path.join(root, "web/python/examples/slow_host_updater.py"), "utf8");
const server = await serveRepository(root, Number(process.env.NOON_EXECUTION_HOST_PORT ?? 4185), { crossOriginIsolated: true });
const reports = [];
let browser;
try {
  browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs("webgpu") });
  for (const transportMode of ["transferable", "shared"]) {
    const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
    const errors = [];
    page.on("pageerror", error => errors.push(String(error)));
    page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
    let timer;
    try {
      await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
      const report = await Promise.race([
        page.evaluate(async ({ source, transportMode }) => {
          const { PythonAuthoringClient } = await import("./authoring-client.js");
          const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
          const authoring = new PythonAuthoringClient();
          let rejectFailure;
          const failure = new Promise((_, reject) => { rejectFailure = reject; });
          failure.catch(() => {});
          const execution = new AuthoringExecutionClient(document.querySelector("#scene"), { onError: rejectFailure });
          let resolveAttached;
          const attached = new Promise(resolve => { resolveAttached = resolve; });
          let registration;
          const authored = authoring.run(source, {}, {
            async onSemanticContinuation(next) {
              if (registration) throw new Error("required callback source created a second context");
              registration = next;
              resolveAttached(await execution.startSemanticExecution(next.semanticExecution, {
                authoringClient: authoring, transportMode, pacing: "external_samples",
              }));
            },
          });
          authored.catch(rejectFailure);
          try {
            const ready = await Promise.race([attached, failure]);
            let rafCount = 0;
            let raf;
            const frame = () => { rafCount += 1; raf = requestAnimationFrame(frame); };
            raf = requestAnimationFrame(frame);
            const samples = [];
            try {
              for (const time of [0, 0.1, 0.2, 0.3, 0.4]) {
                const started = performance.now();
                const state = await Promise.race([
                  execution.sampleToAuthoredTime(time, { stopAtSourceCompletion: true }), failure,
                ]);
                samples.push({ requested: time, elapsedMs: performance.now() - started, ...state });
              }
            } finally { cancelAnimationFrame(raf); }
            // Completion runs the Python assertions: each ordered observer sees
            // the preceding write, dt sums to .4, and both final positions agree.
            const completed = await Promise.race([authored, failure]);
            return { ready, samples, rafCount, duration: completed.duration,
              sameContext: completed.semanticExecution.contextId === registration.semanticExecution.contextId,
              mode: execution.mode, metrics: (await execution.metrics()).metrics };
          } finally { execution.terminate(); authoring.terminate(); }
        }, { source, transportMode }),
        new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${transportMode} callback qualification timed out`)), 90000); }),
      ]);
      assert.equal(report.mode, "semantic");
      assert.equal(report.ready.transportMode, transportMode);
      assert.equal(report.sameContext, true);
      assert.equal(report.duration, 0.4);
      for (const sample of report.samples) {
        assert.equal(sample.time, sample.requested, "required callback must finish at the requested authored time");
        if (sample.requested > 0) assert.ok(sample.elapsedMs >= 75, "required slow callback was skipped");
      }
      assert.ok(report.rafCount > 0, "Python callback blocked the browser main thread");
      assert.equal(report.metrics.objectCount, 2);
      assert.ok(report.metrics.presentedFrames > 0 && report.metrics.drawCalls > 0);
      assert.deepEqual(errors, []);
      reports.push({ transportMode, ...report });
      console.log(`PASS ${transportMode}: required slow callbacks, ordered observations, exact authored samples, coherent final rendering`);
    } finally { clearTimeout(timer); await page.close(); }
  }
  if (process.env.NOON_EXECUTION_HOST_REPORT) {
    await writeFile(process.env.NOON_EXECUTION_HOST_REPORT, JSON.stringify(reports, (_key, value) => typeof value === "bigint" ? value.toString() : value, 2));
  }
} finally { await browser?.close(); await server.close(); }
