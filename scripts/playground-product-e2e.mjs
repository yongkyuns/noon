import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";
import { productMeasurement, sampleRendererFps, samplePresentationGaps, sampleRendererCosts } from "./playground-product-fps.mjs";
import { packageSizes } from "../.github/ci/wasm-build.mjs";
import { readProductPair } from "./paired-product-metrics.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

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
const measurement = productMeasurement(exampleId);
const MIN_PRODUCT_MEASUREMENT_MS = 1_000;
const startedAtMs = Date.now();
const pair = readProductPair(process.env);
const browserWsEndpoint = process.env.NOON_PRODUCT_BROWSER_WS_ENDPOINT?.trim() || null;

await mkdir(artifactDir, { recursive: true });

// #1933 Apple A/A diagnostic-only server: the Python 3.14 CLI -m http.server
// did not bind on macos-15. ThreadingTCPServer binds directly and preserves
// SimpleHTTPRequestHandler's normal static-file semantics/concurrency.
// Every child gets a new random identity so stale localhost servers fail closed.
const serverReadyToken = randomBytes(16).toString("hex");
let serverOutput = "";
const server = spawn(
  "python3",
  ["-u", path.join(scriptDir, "product-aa-apple-http-server.py"),
    "--port", String(port), "--directory", siteRoot,
    "--ready-token", serverReadyToken],
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
  // Probe the *real* listening service instead of trusting a stdout banner.
  // All probes happen BEFORE shell/runtime/scored timing starts.
  const expectedIdentity = JSON.parse(await readFile(
    path.join(siteRoot, "web/runtime-build-identity.json"), "utf8"));
  let lastError = null;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (serverStartError !== null || server.exitCode !== null || server.signalCode !== null) {
      throw new Error(`product E2E server failed: ${serverStartError ?? server.exitCode ?? server.signalCode}\n${serverOutput}`);
    }
    try {
      // No other server on this port can know this new process's random token.
      const proof = await fetch(`${baseUrl}/__noon_aa_ready__/${serverReadyToken}`, {
        cache: "no-store", signal: AbortSignal.timeout(900),
      });
      if (!proof.ok || await proof.text() !== serverReadyToken + "\n") {
        throw new Error(`incorrect local server identity: HTTP ${proof.status}`);
      }
      // Also confirm that static source files come from this authenticated
      // producer checkout; a stale/wrong original build must never be scored.
      const identityResponse = await fetch(`${baseUrl}/web/runtime-build-identity.json`, {
        cache: "no-store", signal: AbortSignal.timeout(900),
      });
      if (!identityResponse.ok) throw new Error(`static source HTTP ${identityResponse.status}`);
      const identity = await identityResponse.json();
      assert.deepEqual(identity, expectedIdentity, "HTTP server returned wrong production build");
      return;
    } catch (error) {
      lastError = error;
    }
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error(`product E2E server not operational or identity mismatch: ${lastError}\n${serverOutput}`);
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
  await page.evaluate(capture => {
    const probe = window.__noonProductRenderProbe;
    probe.captureStages = capture && probe.profileStages;
    probe.presentationSamples = [];
    probe.stageKeys.clear();
  }, captureFrames);
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
        // Sample only the renderer. Aggregate metrics also query the source
        // owner and would add work to the callback lane being measured.
        probe.requestMetrics();
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
    // Run may replace its canvas during startup; the preview frame is stable.
    await page.locator(".canvas-frame").scrollIntoViewIfNeeded();
    const state = await waitForApplied(page);
    const milliseconds = performance.now() - started;
    if (captureFrames) {
      // This is the final observation for this exact run, after its renderer
      // has presented the authored endpoint and before any warm/edit rerun.
      const endpoint = { phase: "endpoint", ...await sampleRendererObservation(page) };
      frameSamples.push(endpoint);
    }
    const presentationSamples = await page.evaluate(() => window.__noonProductRenderProbe.presentationSamples);
    evidence.presentationSamples = presentationSamples;
    return { milliseconds, state, frameSamples, presentationSamples };
  } finally {
    sampling = false;
    try {
      await sampleFrames;
    } finally {
      evidence.finishedAtMs = Date.now();
    }
  }
}

async function sampleRendererObservation(page) {
  const previousReplies = await page.evaluate(() => {
    const probe = window.__noonProductRenderProbe;
    const previous = probe.metricsReplies;
    probe.requestMetrics();
    return previous;
  });
  await page.waitForFunction(previous => window.__noonProductRenderProbe.metricsReplies > previous,
    previousReplies, { timeout: 10_000 });
  return page.evaluate(() => ({ ...window.__noonProductRenderProbe.latest, now: performance.now() }));
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
    await page.evaluate(() => window.__noonProductRenderProbe.requestMetrics());
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
  if (capability === "available") {
    assert.ok(hasControls, "replayable execution must expose playback controls");
    await toggle.waitFor({ state: "visible", timeout: 10_000 });
    assert.equal(await toggle.isEnabled(), true, "completed replay controls must be enabled");
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
  } else if (hasControls) {
    assert.equal(await toggle.isDisabled(), true, "nonreplayable execution must disable playback");
    assert.equal(await page.locator(".playback-scrubber").isDisabled(), true,
      "nonreplayable execution must disable seeking");
  }
  const rendered = await waitForRenderedEndpoint(page, seconds);
  assert.ok(rendered.frames > 0, "authored endpoint was not presented by the render worker");
  return { capability, rendered };
}

let browser = null;
let context = null;
const pageErrors = [];
const consoleErrors = [];
try {
  await waitForServer();
  browser = browserWsEndpoint === null
    ? await chromium.launch({ channel: "chromium", headless: true, args: browserArgs("webgl") })
    : await chromium.connect(browserWsEndpoint);
  context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
  });
  const page = await context.newPage();
  await page.addInitScript(profileStages => {
    const NativeWorker = window.Worker;
    // The product client rejects this ID before issuing it (counter exhausted).
    // Consume only this test-owned diagnostic reply before its normal listener.
    const diagnosticRequestId = Number.MAX_SAFE_INTEGER;
    const probe = { worker: null, pending: false, latest: null, metricsReplies: 0,
      profileStages, captureStages: false, presentationSamples: [], stageKeys: new Set(),
      requestMetrics() {
        if (probe.worker === null || probe.pending) return;
        probe.pending = true;
        probe.worker.postMessage({ channel: "noon.render", protocolVersion: 1,
          type: "metrics", requestId: diagnosticRequestId,
          profilePublicationStages: probe.captureStages });
      },
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
        counters: Object.fromEntries(["drawCalls", "instancesDrawn", "bytesUploaded", "geometryCacheMisses",
          "objectCount", "rendererRebuilds", "modeSwitches"].map(key => [key, metrics?.[key]])),
      }),
    };
    window.__noonProductRenderProbe = probe;

    // Test-only read-only metrics requests use the existing render control lane.
    // Ordinary client replies and all semantic messages pass through untouched.
    class ObservedWorker extends NativeWorker {
      constructor(url, options) {
        super(url, options);
        if (!String(url).includes("execution-render-worker.js")) return;
        probe.worker = this;
        probe.pending = false;
        this.addEventListener("message", (event) => {
          const message = event.data;
          if (message?.channel !== "noon.render" || message.requestId !== diagnosticRequestId) return;
          event.stopImmediatePropagation();
          if (probe.worker !== this) return;
          probe.pending = false;
          const presentedFrames = Number(message?.metrics?.presentedFrames);
          if (message?.type !== "metrics" || !Number.isFinite(presentedFrames)) {
            throw new Error(`product renderer metrics failed: ${message.message ?? message.type}`);
          }
          probe.metricsReplies += 1;
          probe.latest = probe.observation(message.metrics);
          if (probe.captureStages) {
            for (const sample of message.metrics.publicationStageSamples ?? []) {
              const clockOriginMs = message.metrics.performanceTimeOriginMs;
              const key = `${clockOriginMs}:${sample.session}:${sample.sequence}`;
              if (probe.stageKeys.has(key)) continue;
              if (probe.stageKeys.size >= 2_000) throw new Error("product publication capture exceeded its bound");
              probe.stageKeys.add(key);
              probe.presentationSamples.push({ ...sample, clockOriginMs });
            }
          }
        });
      }
    }
    Object.defineProperty(window, "Worker", {
      configurable: true,
      writable: true,
      value: ObservedWorker,
    });
  }, measurement.gapClock !== null);
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

  // All source arms execute the same candidate-owned fixture bytes. In
  // particular, an older anchor's gallery cannot silently substitute its source.
  let authoredSource = await readFile(path.join(repoRoot, "web", measurement.sourcePath), "utf8");
  if (exampleId === "parity-square-and-circle") {
    // Preserve the established square/circle duration and endpoint hold.
    const target = "self.play(Create(circle), Create(square))";
    assert.equal(authoredSource.split(target).length, 2, "product first-pass fixture was not found exactly once");
    authoredSource = authoredSource.replace(target,
      `self.play(Create(circle), Create(square), run_time=${measurement.windowEndSeconds})\n        self.wait(${measurement.endpointHoldSeconds})`);
  }
  await page.locator("#python-scene-source").evaluate((editor, source) => { editor.value = source; }, authoredSource);
  const source = { path: measurement.sourcePath,
    sha256: createHash("sha256").update(authoredSource).digest("hex") };

  const cold = await runAndMeasure(page);
  assert.equal(cold.state.backend, "WebGL2", `expected WebGL2 product path, got ${cold.state.backend}`);
  // Complete a cold pass before scoring the declared warm authored window.
  const warm = await runAndMeasure(page, { captureFrames: true });
  const fps = sampleRendererFps(warm.frameSamples, measurement.windowEndSeconds, {
    minMeasurementMs: MIN_PRODUCT_MEASUREMENT_MS,
    warmupSeconds: measurement.windowStartSeconds,
  });
  const presentationGaps = measurement.gapClock === null ? null
    : samplePresentationGaps(warm.presentationSamples, fps);

  const marker = `# product gate ${label}`;
  await page.evaluate((text) => {
    const editor = document.querySelector("#python-scene-source");
    if (!(editor instanceof HTMLTextAreaElement)) throw new Error("scene editor is unavailable");
    editor.value = `${editor.value.trimEnd()}\n\n${text}\n`;
  }, marker);
  const edited = await runAndMeasure(page);

  const endpoint = await synchronizeFinalFrame(page, measurement.sourceEndSeconds);
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
    measurement,
    source,
    shellReadyMs,
    shell,
    coldRunMs: cold.milliseconds,
    warmRunMs: warm.milliseconds,
    editRunMs: edited.milliseconds,
    fps,
    // Preserve the actual observations so a short-window regression can be
    // diagnosed without reconstructing telemetry from an aggregate FPS value.
    fpsSamples: warm.frameSamples,
    presentationSamples: warm.presentationSamples,
    presentationGaps,
    rendererCosts: measurement.gapClock === null ? null
      : sampleRendererCosts(warm.presentationSamples, warm.frameSamples, fps),
    packageSizes: await packageSizes(siteRoot),
    visual,
    fixedFrame: {
      authoredEndpointSeconds: measurement.sourceEndSeconds,
      presentation: endpoint,
    },
    screenshot: screenshotName,
    captureBounds,
    runtime: {
      runnerPlatform: process.platform,
      backend: cold.state.backend,
      executionMode: cold.state.executionMode,
      browserVersion: browser.version(),
      viewport: { width: 1280, height: 800 },
      deviceScaleFactor: 1,
      gpuMode: "hardware-WebGL", // #1933 exact diagnostic overlay; no timing logic changed.
      sharedBrowserProcess: browserWsEndpoint !== null,
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
  if (presentationGaps !== null) {
    console.log(`${exampleId} ${measurement.windowStartSeconds}–${measurement.windowEndSeconds} s: ` +
      `submission gaps p95 ${presentationGaps.intervalMs.p95.toFixed(1)} ms, ` +
      `max ${presentationGaps.intervalMs.max.toFixed(1)} ms, ` +
      `${presentationGaps.cadence.longFrames}/${presentationGaps.intervalCount} intervals ≥25 ms`);
  }
} catch (error) {
  await writeFile(path.join(artifactDir, "failure.json"), `${JSON.stringify({
    label, exampleId, siteRoot, pair, startedAtMs, finishedAtMs: Date.now(),
    error: error?.stack ?? String(error), runEvidence, pageErrors, consoleErrors, serverOutput,
  }, null, 2)}\n`, "utf8");
  throw error;
} finally {
  await context?.close();
  // Browser.close() disconnects this client when connected via launchServer;
  // without it the Node child never exits even after writing its report.
  await browser?.close();
  server.kill("SIGTERM");
  await serverClosed;
}
