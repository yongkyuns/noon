// Paired, equal-work host-boundary measurements on the exact production packages.
// No wall-clock animation sleeps, dropped-case retries, threshold adaptation,
// per-frame Python instrumentation, or mutation of the runtime under test.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { stringifyEvidence } from "./python-host-report.mjs";
import { PERF_PROTOCOL as protocol, pairedCost, performanceSource } from "./python-host-perf-protocol.mjs";

const roots = ["BASELINE", "CANDIDATE"].map(side => {
  assert.ok(process.env[`NOON_PERF_${side}_ROOT`], `missing ${side} source`);
  return path.resolve(process.env[`NOON_PERF_${side}_ROOT`]);
});
const output = path.join(roots[1], "browser-smoke-artifacts/product-gate/host-cost");
await mkdir(output, { recursive: true });
const identities = await Promise.all(roots.map(async root => JSON.parse(await readFile(path.join(root, "web/ci-artifact.json"), "utf8"))));
assert.equal(identities[0].source, process.env.NOON_PRODUCT_BASE_SHA, "unmatched baseline package");
assert.equal(identities[1].source, process.env.GITHUB_SHA, "unmatched candidate package");
assert.deepEqual(identities[0].build, identities[1].build, "different build configuration");
const cache = createPyodideResourceCache(await readFile(path.join(roots[1], "web/python-worker.source.js"), "utf8"));
const servers = [], contexts = [], pages = [], reports = [[], []], warmups = [], rows = [], failures = [];
let browser;
try {
  browser = await playwright.chromium.launch({ headless: true, args: browserArgs("webgl") });
  for (const root of roots) {
    const server = await serveRepository(root, 0, { crossOriginIsolated: true }); servers.push(server);
    const context = await browser.newContext({ viewport: { width: 320, height: 180 } }); contexts.push(context);
    await cache.install(context);
    const page = await context.newPage(); pages.push(page);
    page.on("console", message => {
      if (message.text().startsWith("NOON_PERF_REPORT ")) reports[pages.indexOf(page)].push(JSON.parse(message.text().slice(17)));
    });
    page.on("pageerror", error => failures.push({ kind: "pageerror", message: String(error) }));
    await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
    await page.evaluate(async () => {
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      window.perfAuthoring = new PythonAuthoringClient();
      await window.perfAuthoring.ready();
    });
  }
  const measure = async (side, source, mode, workload) => {
    const page = pages[side], count = reports[side].length;
    let timer;
    const operation = page.evaluate(async ({ source, samples, sampleHz }) => {
      const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
      const authoring = window.perfAuthoring;
      const canvas = document.createElement("canvas"); canvas.width = 320; canvas.height = 180;
      document.body.replaceChildren(canvas);
      const errors = [];
      let resolveAttachment, rejectAttachment, attachment;
      const attached = new Promise((resolve, reject) => { resolveAttachment = resolve; rejectAttachment = reject; });
      attached.catch(() => {});
      const execution = new AuthoringExecutionClient(canvas, { onError(error) { errors.push(String(error)); rejectAttachment(error); } });
      const started = performance.now();
      const terminal = authoring.run(source, {}, { onSemanticContinuation(registration) {
        attachment = execution.startSemanticExecution(registration.semanticExecution, {
          authoringClient: authoring, transportMode: "transferable", pacing: "external_samples",
        });
        attachment.then(resolveAttachment, rejectAttachment);
      } }).then(value => ({ ok: true, value }), error => ({ ok: false, error: String(error) }));
      const failed = terminal.then(result => result.ok ? new Promise(() => {}) : Promise.reject(new Error(result.error)));
      failed.catch(() => {});
      const advanceMs = [];
      try {
        await Promise.race([attached, failed]);
        for (let i = 0; i <= samples; ++i) {
          const start = performance.now();
          const receipt = await Promise.race([execution.sampleToAuthoredTime(i / sampleHz, { stopAtSourceCompletion: true }), failed]);
          advanceMs.push(performance.now() - start);
          if (receipt.sourceCompleted) break;
        }
        const completed = await terminal;
        if (!completed.ok) throw new Error(completed.error);
        if (errors.length) throw new Error(errors.join("\n"));
        return { elapsedMs: performance.now() - started, advanceMs,
          duration: completed.value.duration, metrics: (await execution.metrics()).metrics };
      } finally {
        await attachment?.catch(() => {});
        execution.terminate(); canvas.remove();
      }
    }, { source, samples: protocol.samples, sampleHz: protocol.sampleHz });
    let result;
    try {
      result = await Promise.race([operation, new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`timed out: ${side}/${workload}/${mode}`)), 120000);
      })]);
    } finally { clearTimeout(timer); }
    assert.equal(reports[side].length, count + 1, "missing Python timing observation");
    const report = reports[side].at(-1);
    assert.equal(report.mode, mode); assert.equal(report.workload, workload);
    assert.equal(report.coroutine, mode !== "jspi", "wrong source execution path");
    assert.equal(result.duration, 1.25, "changed authored extent");
    assert.equal(result.metrics.backend, "WebGL2", "changed renderer");
    assert.equal(result.metrics.objectCount, protocol.objects, "changed object work");
    assert.equal(result.metrics.geometryCacheMisses, 0, "steady-state geometry rebuilt");
    assert.ok(result.metrics.presentedFrames > 1 && result.metrics.instancesDrawn > 0, "no rendering");
    return { ...report, ...result };
  };
  for (const workload of protocol.workloads) {
    for (const mode of protocol.modes) {
      const source = performanceSource(mode, workload);
      const sourceSha = createHash("sha256").update(source).digest("hex");
      await writeFile(path.join(output, `${workload}-${mode}.py`), source);
      for (let warm = 0; warm < protocol.warmups; ++warm) {
        for (const side of warm % 2 ? [1, 0] : [0, 1]) warmups.push({ side, workload, mode,
          result: await measure(side, source, mode, workload) });
      }
      const pairs = [];
      for (let pair = 0; pair < protocol.pairs; ++pair) {
        const values = [];
        for (const side of pair % 2 ? [1, 0] : [0, 1]) values[side] = await measure(side, source, mode, workload);
        assert.equal(values[1].callback_calls, values[0].callback_calls, "different Python callback work");
        assert.ok(values[0].center.every((v, i) => Math.abs(v - values[1].center[i]) < 2e-5), "different semantic result");
        assert.equal(values[0].metrics.presentedFrames, values[1].metrics.presentedFrames, "different frame work");
        pairs.push(values);
        // Persist immediately; retain every fixed pair and all failed evidence.
        await writeFile(path.join(output, `${workload}-${mode}.json`), stringifyEvidence({ sourceSha, pairs }) + "\n");
      }
      const costs = Object.fromEntries(["creation_ms", "local_ms", "execution_ms"].map(key => [key,
        pairedCost(pairs.map(pair => pair[0][key]), pairs.map(pair => pair[1][key]))]));
      rows.push({ workload, mode, sourceSha, pairs, costs });
      for (const [key, cost] of Object.entries(costs)) {
        console.log(`${workload}/${mode}/${key}: ${cost.ratio.toFixed(4)}x [${cost.lower.toFixed(4)}, ${cost.upper.toFixed(4)}] ${cost.status}`);
        if (cost.status !== "pass") failures.push({ workload, mode, key, ...cost });
      }
    }
  }
  // Do not turn the old, loose 20% product threshold into a no-regression claim.
  for (const cohort of ["", "camera/"]) {
    const comparison = JSON.parse(await readFile(path.join(output, "..", cohort, "candidate/comparison.json"), "utf8"));
    const ratio = comparison.fps.candidate / comparison.fps.baseline;
    if (ratio < 0.97) failures.push({ kind: "product_fps", cohort, ratio });
    for (const [name, value] of Object.entries(comparison.latency)) {
      if (value.candidateMs > value.baselineMs * 1.03 + 20) failures.push({ kind: "product_latency", cohort, name, ...value });
    }
  }
} catch (error) {
  failures.push({ kind: "execution", message: String(error), stack: error.stack });
} finally {
  await writeFile(path.join(output, "comparison.json"), stringifyEvidence({ schema: 1, protocol, identities,
    host: { cpu: os.cpus()[0]?.model, platform: os.platform(), arch: os.arch(), node: process.version },
    warmups, rows, failures, cache: cache.stats() }) + "\n");
  for (const context of contexts) await context.close();
  await browser?.close();
  for (const server of servers) await server.close();
}
assert.deepEqual(failures, [], "performance regression or inconclusive qualification; all samples retained");
