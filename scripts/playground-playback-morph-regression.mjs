import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.env.NOON_MORPH_REGRESSION_PORT ?? 4199);
const base = `http://127.0.0.1:${port}/web/`;
const artifacts = path.resolve(root, process.env.NOON_MORPH_REGRESSION_ARTIFACTS ??
  "browser-smoke-artifacts/playground-playback-morph");
const backend = process.env.NOON_MORPH_BACKEND ?? "webgl";
assert.ok(["webgl", "webgpu"].includes(backend));
// Original gallery timings: wait .5, morph 1.8, hold .75, return 1.8, hold .85.
// Samples bracket BOTH completion boundaries and include the formerly wrong gap.
const times = [0, 0.25, 0.499, 0.5, 1.4, 2.299, 2.3, 2.301, 2.7,
  3.049, 3.05, 3.051, 3.95, 4.849, 4.85, 5.3, 5.65];
const report = { backend, times, comparisons: [], errors: [] };
await mkdir(artifacts, { recursive: true });
const server = spawn("python3", ["-m", "http.server", String(port), "--bind", "127.0.0.1",
  "--directory", root], { stdio: "ignore" });
let browser;
function difference(left, right) {
  const a = PNG.sync.read(left), b = PNG.sync.read(right);
  assert.equal(a.width, b.width); assert.equal(a.height, b.height);
  let changed = 0;
  for (let i = 0; i < a.data.length; i += 4) {
    if ([0, 1, 2, 3].some(channel => Math.abs(a.data[i + channel] - b.data[i + channel]) > 2)) changed += 1;
  }
  return changed / (a.width * a.height);
}
try {
  let ready = false;
  for (let i = 0; i < 100; i += 1) {
    ready = await fetch(base).then(r => r.ok).catch(() => false);
    if (ready) break;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.ok(ready, "morph regression server did not start");
  const worker = await fetch(new URL("python-worker.js", base));
  assert.ok(worker.ok);
  const cache = createPyodideResourceCache(await worker.text());
  // Use the same Chromium compositor and software-adapter configuration as
  // the existing raster qualification gates, rather than the headless shell.
  browser = await playwright.chromium.launch({
    channel: "chromium", headless: true, args: browserArgs(backend),
  });
  const context = await browser.newContext({ viewport: { width: 900, height: 600 }, deviceScaleFactor: 1 });
  await cache.install(context);
  await context.route("**/morph-regression.html", route => route.fulfill({
    contentType: "text/html",
    body: '<!doctype html><canvas id="scene" width="704" height="396" style="width:704px;height:396px"></canvas>',
  }));
  const page = await context.newPage();
  page.setDefaultTimeout(60_000);
  page.on("pageerror", error => report.errors.push(error.stack ?? String(error)));
  page.on("console", message => { if (message.type() === "error") report.errors.push(message.text()); });
  await page.goto(`${base}morph-regression.html`);
  const source = await readFile(path.join(root, "web/python/examples/manim_compatible_svg_tiger_morph.py"), "utf8");
  await page.evaluate(async source => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const authoring = new PythonAuthoringClient();
    const execution = new AuthoringExecutionClient(document.querySelector("#scene"));
    let resolveAttached, rejectAttached;
    const attached = new Promise((resolve, reject) => { resolveAttached = resolve; rejectAttached = reject; });
    const run = authoring.run(source, {}, {
      async onSemanticContinuation(registration) {
        await execution.prepare({ transportMode: "transferable" });
        await execution.startSemanticExecution(registration.semanticExecution, {
          authoringClient: authoring, loopDurationSeconds: registration.duration,
          transportMode: "transferable", pacing: "external_samples",
        });
        resolveAttached();
      },
    });
    void run.catch(rejectAttached);
    window.morphRegression = { authoring, execution, run };
    await attached;
  }, source);
  report.initialRenderer = await page.evaluate(() => window.morphRegression.execution.metrics());
  assert.equal(report.initialRenderer.metrics.backend, backend === "webgl" ? "WebGL2" : "WebGPU");
  const originals = new Map();
  for (const time of times) {
    const state = await page.evaluate(t => window.morphRegression.execution.sampleToAuthoredTime(t), time);
    assert.ok(Math.abs(state.time - time) < 1e-9, `first-pass sample missed ${time}: ${state.time}`);
    originals.set(time, await page.locator("#scene").screenshot({ path: path.join(artifacts, `source-${time}.png`) }));
  }
  // The first pass itself must hold the rocket and must not be an empty or
  // unchanging scene. Parity alone would allow two identically broken paths.
  assert.ok(difference(originals.get(0.25), originals.get(2.7)) > 0.02, "tiger and rocket must be visibly different");
  assert.ok(difference(originals.get(2.301), originals.get(2.7)) < 0.002, "first-pass rocket must remain held");
  assert.ok(difference(originals.get(2.7), originals.get(3.049)) < 0.002, "first-pass rocket must remain held until return");
  report.completed = await page.evaluate(async () => {
    const h = window.morphRegression;
    // Cross the floating-point final endpoint with the explicit completion flag.
    const final = await h.execution.sampleToAuthoredTime(6, { stopAtSourceCompletion: true });
    const authored = await h.run;
    await h.execution.reconcileSemanticExecution({
      contextId: authored.semanticExecution.contextId,
      callbackSessionId: authored.semanticExecution.callbackSessionId ?? null,
      continuationGeneration: null,
    }, { authoringClient: h.authoring, loopDurationSeconds: authored.duration });
    await h.execution.pause();
    return { final, duration: authored.duration, metrics: await h.execution.metrics() };
  });
  assert.equal(report.completed.final.sourceCompleted, true);
  assert.ok(Math.abs(report.completed.duration - 5.7) < 1e-9);
  assert.equal(report.completed.metrics.metrics.backend, backend === "webgl" ? "WebGL2" : "WebGPU");
  for (const mode of ["seek", "forward"]) {
    await page.evaluate(() => window.morphRegression.execution.seek(0));
    const samples = mode === "seek" ? [...times].reverse() : times;
    for (const time of samples) {
      const state = await page.evaluate(async ({ time, mode }) => {
        const player = window.morphRegression.execution;
        if (mode === "seek") await player.seek(time);
        // Same-time forward acknowledgement waits for the exact seek publication
        // (or the last coherent static frame), not an arbitrary screenshot delay.
        return player.advanceTo(time);
      }, { time, mode });
      assert.ok(Math.abs(state.time - time) < 1e-9);
      const capture = await page.locator("#scene").screenshot({ path: path.join(artifacts, `${mode}-${time}.png`) });
      const mismatch = difference(originals.get(time), capture);
      report.comparisons.push({ mode, time, mismatch });
    }
  }
  assert.deepEqual(report.errors, []);
  const bad = report.comparisons.filter(sample => sample.mismatch >= 0.002);
  assert.deepEqual(bad, [], "completed replay must preserve the first-pass frame throughout both morphs and holds");
  report.outcome = "pass";
  console.log(`${backend}: ${report.comparisons.length} source/replay morph samples passed`);
} catch (error) {
  report.outcome = "fail";
  report.failure = error.stack ?? String(error);
  throw error;
} finally {
  try {
    await writeFile(path.join(artifacts, "morph-regression.json"),
      JSON.stringify(report, (_, value) => typeof value === "bigint" ? value.toString() : value, 2));
  } finally {
    await browser?.close();
    server.kill("SIGTERM");
  }
}
