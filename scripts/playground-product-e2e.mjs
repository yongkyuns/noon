import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

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
const PRODUCT_FIRST_PASS_SECONDS = 3;
const MIN_PRODUCT_MEASUREMENT_MS = 1_000;

await mkdir(artifactDir, { recursive: true });

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", siteRoot],
  { cwd: siteRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/index.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
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

async function runAndMeasure(page, { captureFrames = false } = {}) {
  const started = performance.now();
  let sampling = captureFrames;
  const frameSamples = [];
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
          frames: Number(probe.latest?.frames ?? 0),
          metricAt: Number(probe.latest?.at ?? Number.NaN),
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
    return { milliseconds: performance.now() - started, state, frameSamples };
  } finally {
    sampling = false;
    await sampleFrames;
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

function sampleRendererFps(frameSamples) {
  // Measure render-worker presentation only while the Python continuation owns
  // the deliberately long, identical first pass used by both baseline and candidate.
  // The renderer metric is derived telemetry and does not wait on the suspended
  // authoring continuation or model scene/runtime state in the frontend.
  const sourceOwned = frameSamples.filter(
    (sample) =>
      sample.runInFlight &&
      sample.playbackControls === "unavailable" &&
      Number.isFinite(sample.metricAt),
  );
  assert.ok(sourceOwned.length >= 10, "source-owned product run did not expose enough presentation samples");
  const measurementMs = sourceOwned.at(-1).now - sourceOwned[0].now;
  assert.ok(
    measurementMs >= MIN_PRODUCT_MEASUREMENT_MS,
    `source-owned presentation window was too short (${measurementMs.toFixed(0)} ms)`,
  );
  const changes = sourceOwned.filter((sample, index) => index > 0 &&
    sample.frames > sourceOwned[index - 1].frames);
  assert.ok(changes.length >= 2, "first authored pass did not expose enough presentation samples");
  const start = changes[0];
  const end = changes.at(-1);
  const elapsedSeconds = Math.max((end.now - start.now) / 1000, 0.001);
  return {
    startFrames: start.frames,
    endFrames: end.frames,
    sampleCount: sourceOwned.length,
    measurementMs,
    elapsedMs: end.now - start.now,
    effectiveFps: Math.max(0, end.frames - start.frames) / elapsedSeconds,
  };
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
    const probe = { pending: false, latest: null, metricsReplies: 0 };
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
          probe.latest = {
            frames: presentedFrames,
            time: Number(message.metrics.time),
            ready: message.metrics.ready === true,
            needsPresent: message.metrics.needsPresent === true,
            bufferedDeltas: Number(message.metrics.bufferedDeltas),
            at: performance.now(),
          };
          probe.metricsReplies += 1;
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
  // visible first-pass duration, giving the real renderer enough time to report
  // multiple presented frames without changing final scene semantics.
  await page.evaluate((durationSeconds) => {
    const editor = document.querySelector("#python-scene-source");
    if (!(editor instanceof HTMLTextAreaElement)) throw new Error("scene editor is unavailable");
    const updated = editor.value.replace(
      "self.play(Create(circle), Create(square))",
      `self.play(Create(circle), Create(square), run_time=${durationSeconds})`,
    );
    if (updated === editor.value) throw new Error("product first-pass fixture was not found");
    editor.value = updated;
  }, PRODUCT_FIRST_PASS_SECONDS);

  const cold = await runAndMeasure(page, { captureFrames: true });
  assert.equal(cold.state.backend, "WebGL2", `expected WebGL2 product path, got ${cold.state.backend}`);
  const fps = sampleRendererFps(cold.frameSamples);

  const warm = await runAndMeasure(page);

  const marker = `# product gate ${label}`;
  await page.evaluate((text) => {
    const editor = document.querySelector("#python-scene-source");
    if (!(editor instanceof HTMLTextAreaElement)) throw new Error("scene editor is unavailable");
    editor.value = `${editor.value.trimEnd()}\n\n${text}\n`;
  }, marker);
  const edited = await runAndMeasure(page);

  const endpoint = await synchronizeFinalFrame(page, PRODUCT_FIRST_PASS_SECONDS);
  const screenshotName = "frame-final.png";
  const screenshotPath = path.join(artifactDir, screenshotName);
  const screenshot = await page.locator("#scene").screenshot({ path: screenshotPath });
  const visual = changedPixelStats(screenshot);
  assert.ok(visual.changedPixels > 100, `product frame is effectively blank (${visual.changedPixels} changed pixels)`);

  assert.deepEqual(pageErrors, [], `product E2E page errors:\n${pageErrors.join("\n")}`);
  assert.deepEqual(consoleErrors, [], `product E2E console errors:\n${consoleErrors.join("\n")}`);

  const report = {
    schemaVersion: 1,
    label,
    exampleId,
    siteRoot,
    shellReadyMs,
    shell,
    coldRunMs: cold.milliseconds,
    warmRunMs: warm.milliseconds,
    editRunMs: edited.milliseconds,
    fps,
    visual,
    fixedFrame: {
      authoredEndpointSeconds: PRODUCT_FIRST_PASS_SECONDS,
      presentation: endpoint,
    },
    screenshot: screenshotName,
    runtime: {
      backend: cold.state.backend,
      executionMode: cold.state.executionMode,
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
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
