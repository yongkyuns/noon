import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";
import { sampleRendererFps } from "./playground-product-fps.mjs";

const { chromium } = playwright;
const { PNG } = pngjs;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const siteRoot = path.resolve(process.env.NOON_PRODUCT_SITE_ROOT ?? repoRoot);
const port = Number(process.env.NOON_PRODUCT_PORT ?? "4205");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  process.env.NOON_PRODUCT_ARTIFACT_DIR ?? path.join(repoRoot, "browser-smoke-artifacts/product-e2e"),
);
const label = process.env.NOON_PRODUCT_LABEL ?? "candidate";
const exampleId = process.env.NOON_PRODUCT_EXAMPLE ?? "parity-square-and-circle";
const PRODUCT_FIRST_PASS_SECONDS = 4;
const PRODUCT_ENDPOINT_HOLD_SECONDS = 0.5;
const PRODUCT_SOURCE_END_SECONDS = PRODUCT_FIRST_PASS_SECONDS + PRODUCT_ENDPOINT_HOLD_SECONDS;
const MIN_PRODUCT_MEASUREMENT_MS = 1_000;
const PRODUCT_WARMUP_SECONDS = 1;
const startedAtMs = Date.now();
const pair = process.env.NOON_PRODUCT_PAIR_INDEX === undefined ? null : {
  index: Number(process.env.NOON_PRODUCT_PAIR_INDEX),
  position: Number(process.env.NOON_PRODUCT_PAIR_POSITION),
};
if (pair !== null) {
  assert.ok(Number.isSafeInteger(pair.index) && pair.index >= 1 && pair.index <= 3,
    "product pair index must be between one and three");
  assert.ok(pair.position === 1 || pair.position === 2, "product pair position must be one or two");
}

await mkdir(artifactDir, { recursive: true });

let serverOutput = "";
const server = spawn(
  "python3",
  ["-u", "-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", siteRoot],
  { cwd: siteRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));
let serverStartError = null;
const serverClosed = new Promise((resolve) => {
  server.once("error", (error) => { serverStartError = error; resolve(); });
  server.once("close", resolve);
});

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (serverStartError !== null || server.exitCode !== null) {
      throw new Error(`product E2E server failed: ${serverStartError ?? server.exitCode}\n${serverOutput}`);
    }
    try {
      // A pre-existing server on this port must not masquerade as our checkout.
      if (serverOutput.includes("Serving HTTP on")) {
        const response = await fetch(`${baseUrl}/web/index.html`);
        if (response.ok) return;
        lastError = new Error(`HTTP ${response.status}`);
      }
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`product E2E server did not start: ${lastError}\n${serverOutput}`);
}

async function waitForApplied(page) {
  await page.waitForFunction(
    () => {
      const patch = document.querySelector("#patch-status");
      return patch?.dataset.state === "applied" || patch?.dataset.state === "error";
    },
    null,
    { timeout: 30_000 },
  );
  const state = await page.evaluate(() => ({
    patchState: document.querySelector("#patch-status")?.dataset.state ?? null,
    patchText:
      document.querySelector("#patch-status")?.value ??
      document.querySelector("#patch-status")?.textContent ?? "",
    statusState: document.querySelector("#status")?.dataset.state ?? null,
    statusText: document.querySelector("#status-text")?.textContent ?? "",
    backend: document.querySelector("#status")?.dataset.rendererBackend ?? "",
    executionMode: document.querySelector("#status")?.dataset.executionMode ?? "",
  }));
  assert.equal(
    state.patchState,
    "applied",
    `playground run failed: ${state.patchText} / ${state.statusText}`,
  );
  return state;
}

const runEvidence = [];

async function runAndMeasure(page, { captureFrames = false } = {}) {
  const started = performance.now();
  let sampling = captureFrames;
  const frameSamples = [];
  const evidence = { startedAtMs: Date.now(), captureFrames, frameSamples };
  runEvidence.push(evidence);
  const sampleFrames = (async () => {
    while (sampling) {
      const sample = await page.evaluate(async () => {
        const gallery = window.__noonExampleGallery;
        const probe = window.__noonProductRenderProbe;
        // Issue the existing aggregate metrics request without awaiting its
        // source-continuation half. The passive Worker observer records the
        // matching render reply, which is the real presentation counter.
        if (
          !probe.pending &&
          gallery?.executionMode !== null &&
          typeof gallery?.executionMetrics === "function"
        ) {
          probe.pending = true;
          void gallery.executionMetrics().catch(() => {
            probe.pending = false;
          });
        }
        return {
          phase: "source",
          ...probe.latest,
          now: performance.now(),
          runInFlight: gallery?.runInFlight ?? false,
          playbackControls: document.querySelector("#status")?.dataset.playbackControls ?? "",
        };
      });
      frameSamples.push(sample);
      await page.waitForTimeout(50);
    }
  })();
  try {
    await page.locator("#replace-scene").click();
    const state = await waitForApplied(page);
    const milliseconds = performance.now() - started;
    if (captureFrames) {
      // This is the final observation for this exact run, after its renderer
      // has presented the authored endpoint and before any warm/edit rerun.
      const endpoint = await page.evaluate(async () => {
        const report = await window.__noonExampleGallery?.executionMetrics();
        const metrics = report?.metrics;
        return { phase: "endpoint", ...window.__noonProductRenderProbe.observation(metrics),
          now: performance.now() };
      });
      frameSamples.push(endpoint);
    }
    return { milliseconds, state, frameSamples };
  } finally {
    sampling = false;
    try {
      await sampleFrames;
    } finally {
      evidence.finishedAtMs = Date.now();
    }
  }
}

function changedPixelStats(buffer) {
  const png = PNG.sync.read(buffer);
  const background = [png.data[0], png.data[1], png.data[2], png.data[3]];
  let changed = 0;
  for (let offset = 0; offset < png.data.length; offset += 4) {
    const distance =
      Math.abs(png.data[offset] - background[0]) +
      Math.abs(png.data[offset + 1] - background[1]) +
      Math.abs(png.data[offset + 2] - background[2]) +
      Math.abs(png.data[offset + 3] - background[3]);
    if (distance >= 32) changed += 1;
  }
  return { width: png.width, height: png.height, changedPixels: changed };
}

async function waitForRenderedEndpoint(page, seconds) {
  const deadline = performance.now() + 10_000;
  let observedReplies = await page.evaluate(
    () => window.__noonProductRenderProbe?.metricsReplies ?? 0,
  );
  while (performance.now() < deadline) {
    await page.evaluate(() => {
      const gallery = window.__noonExampleGallery;
      if (typeof gallery?.executionMetrics !== "function") {
        throw new Error("product E2E could not request renderer metrics");
      }
      const probe = window.__noonProductRenderProbe;
      if (probe?.pending) return;
      probe.pending = true;
      // The renderer reply is observed passively below. Do not await the
      // aggregate request: its source side may remain owned by an active
      // authoring continuation.
      void gallery.executionMetrics().catch(() => {
        probe.pending = false;
      });
    });
    await page.waitForTimeout(50);
    const sample = await page.evaluate(() => {
      const probe = window.__noonProductRenderProbe;
      return {
        metricsReplies: Number(probe?.metricsReplies ?? 0),
        latest: probe?.latest ?? null,
      };
    });
    const rendered = sample.latest;
    if (
      sample.metricsReplies > observedReplies &&
      rendered?.ready === true &&
      Number.isFinite(rendered?.frames) &&
      rendered.frames > 0 &&
      Number.isFinite(rendered?.time) &&
      rendered.needsPresent === false &&
      rendered.bufferedDeltas === 0 &&
      Math.abs(rendered.time - seconds) <= 0.001
    ) {
      return rendered;
    }
    observedReplies = sample.metricsReplies;
    await page.waitForTimeout(50);
  }
  throw new Error(`render worker did not present authored endpoint ${seconds.toFixed(3)} s`);
}

async function synchronizeFinalFrame(page, seconds) {
  const capability = await page.locator("#status").getAttribute("data-playback-controls");
  assert.ok(
    capability === "available" || capability === "unavailable",
    `product E2E did not publish playback capability (${capability})`,
  );
  const toggle = page.locator(".playback-controls .playback-toggle");
  const hasControls = (await toggle.count()) > 0;
  assert.equal(
    hasControls,
    capability === "available",
    "playback control DOM must match the execution ownership capability",
  );
  if (hasControls) {
    await toggle.waitFor({ state: "visible", timeout: 10_000 });
    if ((await toggle.getAttribute("aria-label")) === "Pause animation") {
      await toggle.click();
      await page.waitForFunction(
        () => document.querySelector(".playback-toggle")?.getAttribute("aria-label") === "Play animation",
      );
    }
    await page.locator(".playback-scrubber").evaluate((input, target) => {
      input.value = String(target);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }, seconds);
  }
  const rendered = await waitForRenderedEndpoint(page, seconds);
  assert.ok(rendered.frames > 0, "authored endpoint was not presented by the render worker");
  return { capability, rendered };
}

let browser = null;
const pageErrors = [];
const consoleErrors = [];
try {
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: [
      "--disable-features=WebGPU",
      "--enable-unsafe-swiftshader",
      "--ignore-gpu-blocklist",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ],
  });
  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
  });
  const page = await context.newPage();
  await page.addInitScript(() => {
    const NativeWorker = window.Worker;
    const probe = { pending: false, latest: null, metricsReplies: 0,
      observation: (metrics) => ({
        frames: metrics?.presentedFrames,
        time: metrics?.time,
        session: metrics?.presentedSession,
        clockOriginMs: metrics?.performanceTimeOriginMs,
        rendererAt: metrics?.sampledAtMs,
        ready: metrics?.ready === true,
        needsPresent: metrics?.needsPresent === true,
        bufferedDeltas: metrics?.bufferedDeltas,
        metricAt: performance.now(),
        metricReply: probe.metricsReplies,
      }),
    };
    window.__noonProductRenderProbe = probe;

    // This test-only observer sees actual render-worker metrics replies without
    // changing their request IDs, contents, scheduling, or ownership.
    class ObservedWorker extends NativeWorker {
      constructor(url, options) {
        super(url, options);
        if (!String(url).includes("execution-render-worker.js")) return;
        this.addEventListener("message", (event) => {
          const message = event.data;
          const presentedFrames = Number(message?.metrics?.presentedFrames);
          if (message?.channel !== "noon.render" || message?.type !== "metrics" ||
              !Number.isFinite(presentedFrames)) return;
          probe.metricsReplies += 1;
          probe.latest = probe.observation(message.metrics);
          probe.pending = false;
        });
      }
    }
    Object.defineProperty(window, "Worker", {
      configurable: true,
      writable: true,
      value: ObservedWorker,
    });
  });
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });

  const navigationStarted = performance.now();
  await page.goto(`${baseUrl}/web/index.html?example=${encodeURIComponent(exampleId)}`, {
    waitUntil: "load",
  });
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined, null, {
    timeout: 20_000,
  });
  const shellReadyMs = performance.now() - navigationStarted;
  const shell = await page.evaluate(() => ({
    selected: window.__noonExampleGallery?.selectedExampleId ?? null,
    runtimeStartup: document.querySelector("#status")?.dataset.runtimeStartup ?? null,
    executionMode: document.querySelector("#status")?.dataset.executionMode ?? null,
    controls: document.querySelector(".playback-controls") !== null,
    statusText: document.querySelector("#status-text")?.textContent ?? "",
  }));
  assert.equal(shell.selected, exampleId, "product E2E loaded the wrong example");
  assert.equal(shell.executionMode, null, "page shell entered an execution mode before Run");
  assert.equal(shell.controls, false, "page shell allocated playback controls before Run");

  // Baseline and candidate run this exact authored source. It extends only the
  // animation duration and adds a bounded static hold, exposing its endpoint
  // before source completion hands presentation to a different session.
  await page.evaluate(({ durationSeconds, holdSeconds }) => {
    const editor = document.querySelector("#python-scene-source");
    if (!(editor instanceof HTMLTextAreaElement)) throw new Error("scene editor is unavailable");
    const updated = editor.value.replace(
      "self.play(Create(circle), Create(square))",
      `self.play(Create(circle), Create(square), run_time=${durationSeconds})\n        self.wait(${holdSeconds})`,
    );
    if (updated === editor.value) throw new Error("product first-pass fixture was not found");
    editor.value = updated;
  }, { durationSeconds: PRODUCT_FIRST_PASS_SECONDS, holdSeconds: PRODUCT_ENDPOINT_HOLD_SECONDS });

  const cold = await runAndMeasure(page);
  assert.equal(cold.state.backend, "WebGL2", `expected WebGL2 product path, got ${cold.state.backend}`);
  // Complete a cold pass before scoring a warm run. Discard the declared first
  // authored second of that run; cold/warm authoring latency stays separate.
  const warm = await runAndMeasure(page, { captureFrames: true });
  const fps = sampleRendererFps(warm.frameSamples, PRODUCT_FIRST_PASS_SECONDS, {
    minMeasurementMs: MIN_PRODUCT_MEASUREMENT_MS,
    warmupSeconds: PRODUCT_WARMUP_SECONDS,
  });

  const marker = `# product gate ${label}`;
  await page.evaluate((text) => {
    const editor = document.querySelector("#python-scene-source");
    if (!(editor instanceof HTMLTextAreaElement)) throw new Error("scene editor is unavailable");
    editor.value = `${editor.value.trimEnd()}\n\n${text}\n`;
  }, marker);
  const edited = await runAndMeasure(page);

  const endpoint = await synchronizeFinalFrame(page, PRODUCT_SOURCE_END_SECONDS);
  const screenshotName = "frame-final.png";
  const screenshotPath = path.join(artifactDir, screenshotName);
  const canvas = page.locator("#scene");
  await canvas.scrollIntoViewIfNeeded();
  const bounds = await canvas.boundingBox();
  assert.ok(bounds?.width > 0 && bounds?.height > 0, "product canvas must have positive bounds");
  // Element screenshots round both edges outwards, so moving identical content
  // to a fractional page offset can add a row. Compare the same pixel-sized
  // region in both runs; genuine canvas size changes still fail comparison.
  const captureBounds = Object.fromEntries(Object.entries(bounds).map(([key, value]) => [key, Math.round(value)]));
  const screenshot = await page.screenshot({ path: screenshotPath, clip: captureBounds });
  const visual = changedPixelStats(screenshot);
  assert.ok(visual.changedPixels > 100, `product frame is effectively blank (${visual.changedPixels} changed pixels)`);

  assert.deepEqual(pageErrors, [], `product E2E page errors:\n${pageErrors.join("\n")}`);
  assert.deepEqual(consoleErrors, [], `product E2E console errors:\n${consoleErrors.join("\n")}`);

  const report = {
    schemaVersion: 2,
    label,
    exampleId,
    siteRoot,
    pair,
    startedAtMs,
    finishedAtMs: Date.now(),
    runtimeIdentity: JSON.parse(await readFile(path.join(siteRoot, "web/runtime-build-identity.json"), "utf8")),
    measurement: { version: 1, clock: "renderer-sampled", preparation: "completed-cold-pass",
      authoredSeconds: PRODUCT_FIRST_PASS_SECONDS, warmupSeconds: PRODUCT_WARMUP_SECONDS,
      endpointHoldSeconds: PRODUCT_ENDPOINT_HOLD_SECONDS },
    shellReadyMs,
    shell,
    coldRunMs: cold.milliseconds,
    warmRunMs: warm.milliseconds,
    editRunMs: edited.milliseconds,
    fps,
    // Preserve the actual observations so a short-window regression can be
    // diagnosed without reconstructing telemetry from an aggregate FPS value.
    fpsSamples: warm.frameSamples,
    visual,
    fixedFrame: {
      authoredEndpointSeconds: PRODUCT_SOURCE_END_SECONDS,
      presentation: endpoint,
    },
    screenshot: screenshotName,
    captureBounds,
    runtime: {
      backend: cold.state.backend,
      executionMode: cold.state.executionMode,
      browserVersion: browser.version(),
      viewport: { width: 1280, height: 800 },
      deviceScaleFactor: 1,
      gpuMode: "software-WebGL",
    },
    pageErrors,
    consoleErrors,
  };
  await writeFile(path.join(artifactDir, "report.json"), `${JSON.stringify(report, null, 2)}\n`, "utf8");
  console.log(
    `${label}: shell ${shellReadyMs.toFixed(0)} ms, cold ${cold.milliseconds.toFixed(0)} ms, ` +
      `warm ${warm.milliseconds.toFixed(0)} ms, edit ${edited.milliseconds.toFixed(0)} ms, ` +
      `${fps.effectiveFps.toFixed(1)} FPS, ${visual.changedPixels} visible pixels`,
  );
} catch (error) {
  await writeFile(path.join(artifactDir, "failure.json"), `${JSON.stringify({
    label, exampleId, siteRoot, pair, startedAtMs, finishedAtMs: Date.now(),
    error: error?.stack ?? String(error), runEvidence, pageErrors, consoleErrors, serverOutput,
  }, null, 2)}\n`, "utf8");
  throw error;
} finally {
  await browser?.close();
  server.kill("SIGTERM");
  await serverClosed;
}
