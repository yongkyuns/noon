import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.env.NOON_EXECUTION_HOST_PORT ?? "4185");
const baseUrl = `http://127.0.0.1:${port}`;
const source = await readFile(
  path.join(repoRoot, "web/python/examples/slow_host_updater.py"),
  "utf8",
);
const teardownSource = source.replace("0.080", "1.000");
assert.notEqual(teardownSource, source, "teardown smoke must extend the callback delay");

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/execution-worker-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Execution host smoke server did not start: ${lastError}\n${serverOutput}`);
}

const browserArgs = [
  "--enable-unsafe-webgpu",
  "--enable-unsafe-swiftshader",
  "--use-webgpu-adapter=swiftshader",
  "--use-gpu-in-tests",
  "--ignore-gpu-blocklist",
  "--enable-features=Vulkan",
  "--use-gl=angle",
  "--use-angle=swiftshader",
  "--use-vulkan=swiftshader",
  "--disable-gpu-sandbox",
  "--disable-dev-shm-usage",
];

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({ channel: "chromium", headless: true, args: browserArgs });
  const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
  });
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });

  const started = await page.evaluate(async (pythonSource) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { ExecutionWorkerClient } = await import("./execution-worker-client.js");
    const authoring = new PythonAuthoringClient();
    const authored = await authoring.run(pythonSource);
    if (authored.kind !== "scene_document" || authored.callbacks === null) {
      throw new Error("slow host smoke must author a callback scene");
    }
    const errors = [];
    const client = new ExecutionWorkerClient(document.querySelector("#scene"), {
      onError(error, owner) {
        errors.push(`${owner}: ${error}`);
      },
    });
    const ready = await client.start(JSON.stringify(authored.document), {
      loopDurationSeconds: 2,
      transportMode: "transferable",
    });
    await client.configureHostCallbacks(authored.callbacks, authoring);
    window.executionHostSmoke = { client, authoring, errors };
    return { ready, callbackCount: authored.callbacks.slots.length };
  }, source);

  assert.equal(started.ready.transportMode, "transferable");
  assert.equal(started.callbackCount, 1);

  await page.waitForFunction(async () => {
    const report = await window.executionHostSmoke.client.metrics();
    return report.engineMetrics.host.requests >= 1;
  }, null, { timeout: 30_000 });

  const before = await page.evaluate(() => window.executionHostSmoke.client.metrics());
  await page.waitForTimeout(420);
  const after = await page.evaluate(() => window.executionHostSmoke.client.metrics());

  assert.equal(after.metrics.ready, true);
  assert.ok(
    after.metrics.presentedFrames > before.metrics.presentedFrames,
    "native animation stopped presenting while Python callback was slow",
  );
  assert.ok(
    after.metrics.lastFrameTimestamp > before.metrics.lastFrameTimestamp,
    "render worker frame clock stopped while Python callback was slow",
  );
  assert.ok(after.engineMetrics.host.requests >= 1, "host callback was never requested");
  assert.ok(after.engineMetrics.host.completed >= 1, "slow Python callback never completed");
  assert.ok(
    after.engineMetrics.host.missedDeadlines >= 1,
    "slow Python callback did not register a missed realtime deadline",
  );
  assert.ok(
    after.engineMetrics.host.droppedLateResults >= 1,
    "late Python callback result was not dropped",
  );
  assert.equal(after.engineMetrics.host.errors, 0);

  const reconnected = await page.evaluate(async () => {
    const client = window.executionHostSmoke.client;
    const canvas = client.canvas;
    const beforeReconnect = await client.metrics();
    const ready = await client.restart({ failedOwner: "engine" });
    const immediate = await client.metrics();
    return {
      ready,
      sameCanvas: client.canvas === canvas,
      presentedBefore: beforeReconnect.metrics.presentedFrames,
      immediate,
    };
  });
  assert.equal(reconnected.ready.session, 2);
  assert.equal(reconnected.sameCanvas, true, "host-callback engine reconnect replaced canvas");
  assert.equal(reconnected.immediate.engineMetrics.host.enabled, true);
  assert.ok(
    reconnected.immediate.engineMetrics.host.requests >= 1,
    "engine reconnect did not synchronously request the restored callback phase",
  );
  assert.ok(
    reconnected.immediate.metrics.presentedFrames >= reconnected.presentedBefore,
    "host-callback engine reconnect reset renderer presentation history",
  );

  const afterReconnect = await page.evaluate(async () => {
    const client = window.executionHostSmoke.client;
    const deadline = performance.now() + 30_000;
    while (performance.now() < deadline) {
      const report = await client.metrics();
      if (report.engineMetrics.host.completed >= 1) {
        return report;
      }
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    throw new Error("reconnected host callback never completed");
  });
  assert.ok(afterReconnect.engineMetrics.host.requests >= 1);
  assert.ok(afterReconnect.engineMetrics.host.completed >= 1);
  assert.equal(afterReconnect.engineMetrics.host.errors, 0);

  const barrierAuthored = await page.evaluate(async (pythonSource) => {
    const { authoring, client } = window.executionHostSmoke;
    const authored = await authoring.run(pythonSource);
    if (authored.kind !== "scene_document" || authored.callbacks === null) {
      throw new Error("seek barrier smoke must author a callback scene");
    }
    await client.configureHostCallbacks(authored.callbacks, authoring);
    return { callbackCount: authored.callbacks.slots.length };
  }, teardownSource);
  assert.equal(barrierAuthored.callbackCount, 1);

  const seekBarrier = await page.evaluate(async () => {
    const { client } = window.executionHostSmoke;
    const deadline = performance.now() + 30_000;
    while (performance.now() < deadline) {
      const observedMetrics = await client.metrics();
      if (observedMetrics.engineMetrics.host.enabled && observedMetrics.engineMetrics.host.inFlight) {
        const beforeState = await client.state();
        const pausePromise = client.pause();
        const seekPromise = client.seek(0.5);
        const [paused, sought] = await Promise.all([pausePromise, seekPromise]);
        return {
          observedMetrics,
          beforeState,
          paused,
          sought,
          state: await client.state(),
          report: await client.metrics(),
        };
      }
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    throw new Error("seek barrier smoke never observed an in-flight host callback");
  });

  assert.equal(seekBarrier.observedMetrics.engineMetrics.host.inFlight, true);
  assert.equal(seekBarrier.paused.operation, "pause");
  assert.equal(seekBarrier.sought.operation, "seek");
  assert.equal(seekBarrier.sought.time, 0.5);
  assert.equal(seekBarrier.sought.playing, false);
  assert.equal(seekBarrier.state.time, 0.5);
  assert.equal(seekBarrier.state.playing, false);
  assert.equal(seekBarrier.report.engineMetrics.host.enabled, true);
  assert.equal(
    seekBarrier.report.engineMetrics.host.generation,
    seekBarrier.observedMetrics.engineMetrics.host.generation + 1,
    "seek did not advance the host callback generation",
  );
  assert.equal(
    seekBarrier.report.engineMetrics.host.nextSequence,
    seekBarrier.observedMetrics.engineMetrics.host.nextSequence,
    "seek reset the host callback patch sequence",
  );
  assert.ok(
    seekBarrier.report.engineMetrics.host.droppedLateResults >=
      seekBarrier.observedMetrics.engineMetrics.host.droppedLateResults + 1,
    "seek did not account for invalidated in-flight callback work",
  );
  assert.ok(
    seekBarrier.report.engineMetrics.host.requests >=
      seekBarrier.observedMetrics.engineMetrics.host.requests + 1,
    "seek did not request a replacement callback phase",
  );
  assert.equal(seekBarrier.report.engineMetrics.host.inFlight, true);
  assert.equal(
    seekBarrier.report.engineMetrics.host.lastFrameTime,
    0.5,
    "seek did not request the callback phase at the exact sought time",
  );
  assert.equal(
    seekBarrier.state.nextPatchSequence,
    seekBarrier.beforeState.nextPatchSequence,
    "seek consumed the interactive patch sequence",
  );

  await page.waitForTimeout(250);
  const afterSeekBarrier = await page.evaluate(async () => ({
    state: await window.executionHostSmoke.client.state(),
    report: await window.executionHostSmoke.client.metrics(),
  }));
  assert.equal(afterSeekBarrier.state.time, 0.5);
  assert.equal(afterSeekBarrier.state.playing, false);
  assert.equal(afterSeekBarrier.report.engineMetrics.host.enabled, true);
  assert.equal(afterSeekBarrier.report.engineMetrics.host.errors, 0);
  await page.evaluate(() => window.executionHostSmoke.client.resume());

  const teardownAuthored = await page.evaluate(async (pythonSource) => {
    const { authoring, client } = window.executionHostSmoke;
    const authored = await authoring.run(pythonSource);
    if (authored.kind !== "scene_document" || authored.callbacks === null) {
      throw new Error("teardown smoke must author a callback scene");
    }
    await client.configureHostCallbacks(authored.callbacks, authoring);
    return { callbackCount: authored.callbacks.slots.length };
  }, teardownSource);
  assert.equal(teardownAuthored.callbackCount, 1);

  const teardown = await page.evaluate(async () => {
    const { client } = window.executionHostSmoke;
    const deadline = performance.now() + 30_000;
    while (performance.now() < deadline) {
      const observedMetrics = await client.metrics();
      if (observedMetrics.engineMetrics.host.enabled && observedMetrics.engineMetrics.host.inFlight) {
        // Invalidate the callback generation in the same task that first observes
        // it in flight. A second host round trip here makes this oracle race the
        // deliberately slow Python callback instead of testing generation invalidation.
        await client.configureHostCallbacks(null);
        return {
          observedMetrics,
          disabledState: await client.state(),
          disabledMetrics: await client.metrics(),
        };
      }
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    throw new Error("teardown smoke never observed an in-flight host callback");
  });
  assert.equal(teardown.observedMetrics.engineMetrics.host.inFlight, true);
  assert.equal(teardown.disabledMetrics.engineMetrics.host.enabled, false);

  await page.waitForTimeout(1_100);
  const afterTeardown = await page.evaluate(async () => ({
    state: await window.executionHostSmoke.client.state(),
    report: await window.executionHostSmoke.client.metrics(),
  }));

  assert.equal(
    afterTeardown.state.nextPatchSequence,
    teardown.disabledState.nextPatchSequence,
    "callback result from an invalidated generation advanced the engine patch sequence",
  );
  assert.equal(afterTeardown.report.engineMetrics.host.enabled, false);
  assert.equal(
    afterTeardown.report.engineMetrics.host.committed,
    teardown.disabledMetrics.engineMetrics.host.committed,
    "callback result from the invalidated generation was committed",
  );
  assert.equal(
    afterTeardown.report.engineMetrics.host.errors,
    teardown.disabledMetrics.engineMetrics.host.errors,
    "invalidated callback generation introduced a host error",
  );
  assert.ok(
    afterTeardown.report.metrics.presentedFrames >
      teardown.disabledMetrics.metrics.presentedFrames,
    "native rendering stopped after callback teardown",
  );

  const clientErrors = await page.evaluate(() => window.executionHostSmoke.errors.slice());
  assert.deepEqual(clientErrors, []);
  assert.deepEqual(browserErrors, []);

  await page.evaluate(() => {
    window.executionHostSmoke.client.terminate();
    window.executionHostSmoke.authoring.terminate();
  });
  await page.close();
  console.log(
    `✓ slow Python host callback: ${after.engineMetrics.host.missedDeadlines} missed deadlines, ` +
      `${afterReconnect.metrics.presentedFrames} frames across engine reconnect; ` +
      "seek barrier and teardown both dropped stale callback work",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
