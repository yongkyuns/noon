import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_PLAYGROUND_PLAYBACK_PORT ?? "4184");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_PLAYGROUND_PLAYBACK_ARTIFACTS ??
    "browser-smoke-artifacts/playground-playback-controls",
);

await mkdir(artifactDir, { recursive: true });

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => {
  serverOutput += chunk;
});
server.stderr.on("data", (chunk) => {
  serverOutput += chunk;
});

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/index.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Playground playback server did not start: ${lastError}\n${serverOutput}`);
}

function parseSeconds(text) {
  const match = String(text).match(/([0-9]+(?:\.[0-9]+)?)\s*s/);
  assert.ok(match, `unable to parse seconds from ${JSON.stringify(text)}`);
  return Number(match[1]);
}

async function waitForMetricNear(page, target, tolerance = 0.08, timeout = 10_000) {
  await page.waitForFunction(
    ({ target: expected, tolerance: allowed }) => {
      const raw =
        document.querySelector("#metric-time")?.value ??
        document.querySelector("#metric-time")?.textContent ??
        "";
      const observed = Number(String(raw).match(/([0-9]+(?:\.[0-9]+)?)\s*s/)?.[1]);
      return Number.isFinite(observed) && Math.abs(observed - expected) <= allowed;
    },
    { target, tolerance },
    { timeout },
  );
}

async function waitForAppliedScene(page, expectedExampleId, timeout = 60_000) {
  await page.waitForFunction(
    (id) => {
      const patch = document.querySelector("#patch-status");
      if (patch?.dataset.state === "error") return true;
      return patch?.dataset.state === "applied" && patch?.dataset.exampleId === id;
    },
    expectedExampleId,
    { timeout },
  );
  const snapshot = await page.evaluate(() => ({
    state: document.querySelector("#patch-status")?.dataset.state ?? "",
    text:
      document.querySelector("#patch-status")?.value ??
      document.querySelector("#patch-status")?.textContent ??
      "",
  }));
  assert.equal(snapshot.state, "applied", `${expectedExampleId} failed: ${snapshot.text}`);
}

async function startDeferredRuntime(page, expectedExampleId) {
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  const deferred = await page.evaluate(() => window.__noonPlaybackInitialDeferred ?? null);
  assert.ok(deferred, "playground never exposed its deferred startup contract");
  assert.equal(deferred.runtimeStartup, "deferred", "playground must not start runtime on page load");
  assert.equal(deferred.executionMode, null, "deferred playground must not own an execution runtime yet");
  assert.equal(deferred.hasPlaybackControls, false, "playback controls must not allocate before runtime start");
  assert.ok(deferred.visibleExampleCount <= 18, "gallery DOM residency must remain bounded on startup");
  // Live authoring deliberately issues the first source-owned Run after the
  // initial paint. Join that run instead of starting a duplicate execution.
  await waitForAppliedScene(page, expectedExampleId);
  await page.waitForFunction(() => window.__noonExampleGallery?.runInFlight === false);
}

async function playbackSnapshot(page) {
  return page.evaluate(() => {
    const controls = document.querySelector(".playback-controls");
    const play = document.querySelector(".playback-toggle");
    const restart = document.querySelector(".playback-restart");
    const scrubber = document.querySelector(".playback-scrubber");
    const time = document.querySelector(".playback-time");
    const canvas = document.querySelector("#scene");
    const status = document.querySelector("#status");
    return {
      hasControls: controls !== null,
      playing: controls?.dataset.playing ?? null,
      busy: controls?.dataset.busy ?? null,
      playText: play?.textContent ?? "",
      playDisabled: play?.disabled ?? true,
      restartDisabled: restart?.disabled ?? true,
      scrubberDisabled: scrubber?.disabled ?? true,
      scrubberValue: scrubber?.value ?? null,
      scrubberMax: scrubber?.max ?? null,
      timeText: time?.value ?? time?.textContent ?? "",
      metricTime:
        document.querySelector("#metric-time")?.value ??
        document.querySelector("#metric-time")?.textContent ??
        "",
      rendererBackend: status?.dataset.rendererBackend ?? null,
      executionMode: status?.dataset.executionMode ?? null,
      playbackAvailability: status?.dataset.playbackControls ?? null,
      runText: document.querySelector("#replace-scene")?.textContent?.trim() ?? "",
      runDisabled: document.querySelector("#replace-scene")?.disabled ?? true,
      patchState: document.querySelector("#patch-status")?.dataset.state ?? "",
      patchSequence: document.querySelector("#patch-status")?.dataset.sequence ?? "",
      patchRunGeneration: document.querySelector("#patch-status")?.dataset.runGeneration ?? "",
      runGeneration: window.__noonExampleGallery?.generationDiagnostics?.runGeneration ?? null,
      canvasIdentity: canvas?.dataset.playbackSmokeIdentity ?? null,
      canvasCount: document.querySelectorAll("canvas").length,
      documentWidth: document.documentElement.scrollWidth,
      viewportWidth: window.innerWidth,
    };
  });
}

let browser = null;
let page = null;
const pageErrors = [];
const consoleErrors = [];
const diagnostics = {};

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
  page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });

  await page.addInitScript(() => {
    window.addEventListener("DOMContentLoaded", () => {
      const status = document.querySelector("#status");
      if (!status) return;
      const capture = () => {
        if (status.dataset.runtimeStartup !== "deferred") return false;
        window.__noonPlaybackInitialDeferred = {
          runtimeStartup: "deferred",
          executionMode: window.__noonExampleGallery?.executionMode ?? null,
          visibleExampleCount: window.__noonExampleGallery?.visibleExampleCount ?? Infinity,
          hasPlaybackControls: document.querySelector(".playback-controls") !== null,
        };
        return true;
      };
      if (capture()) return;
      const observer = new MutationObserver(() => {
        if (capture()) observer.disconnect();
      });
      observer.observe(status, { attributes: true });
    }, { once: true });

    window.__noonPlaybackLifecycle = { sawSourceOwnedPass: false, samples: [] };
    const observePlaybackOwnership = () => {
      const currentStatus = document.querySelector("#status");
      if (!currentStatus) return;
      const sample = {
        controls: currentStatus.dataset.playbackControls ?? null,
        hasControls: document.querySelector(".playback-controls") !== null,
        runInFlight: window.__noonExampleGallery?.runInFlight ?? false,
      };
      window.__noonPlaybackLifecycle.samples.push(sample);
      if (sample.controls === "unavailable" && sample.runInFlight) {
        window.__noonPlaybackLifecycle.sawSourceOwnedPass = true;
      }
    };
    const playbackObserver = new MutationObserver(observePlaybackOwnership);
    playbackObserver.observe(document, {
      attributes: true,
      subtree: true,
      attributeFilter: ["data-playback-controls"],
    });
    observePlaybackOwnership();
  });

  await page.goto(`${baseUrl}/web/index.html?example=parity-square-and-circle`, {
    waitUntil: "load",
  });
  await startDeferredRuntime(page, "parity-square-and-circle");
  await page.evaluate(() => {
    document.querySelector("#scene").dataset.playbackSmokeIdentity = "original";
  });

  const initial = await playbackSnapshot(page);
  diagnostics.initial = initial;
  diagnostics.lifecycle = await page.evaluate(() => window.__noonPlaybackLifecycle);
  assert.equal(
    diagnostics.lifecycle.sawSourceOwnedPass,
    true,
    "the first source-owned pass must keep host playback controls unavailable",
  );
  assert.ok(
    diagnostics.lifecycle.samples
      .filter((sample) => sample.controls === "unavailable")
      .every((sample) => !sample.hasControls),
    "unavailable source-owned playback must not expose host controls",
  );
  assert.equal(initial.hasControls, true, "completed source playback must expose its replay lease");
  assert.equal(initial.playbackAvailability, "available");
  assert.equal(initial.runText, "Run");
  assert.equal(initial.runDisabled, false, "Run must remain available after source completion");
  assert.equal(initial.scrubberDisabled, false, "completed replay must remain seekable");
  assert.ok(Number(initial.scrubberMax) > 0, "completed replay must expose authored duration");
  assert.equal(initial.canvasCount, 1);
  assert.equal(initial.canvasIdentity, "original");
  assert.ok(initial.rendererBackend === "WebGL2" || initial.rendererBackend === "WebGPU");

  assert.ok(
    initial.playing === "true" || initial.playing === "false",
    "completed replay lease must publish a playback state",
  );
  if (initial.playing === "true") {
    await page.locator(".playback-toggle").click();
    await page.waitForFunction(
      () => document.querySelector(".playback-controls")?.dataset.playing === "false",
    );
  }
  const seekTarget = Number(initial.scrubberMax) * 0.6;
  await page.locator(".playback-scrubber").evaluate((input, target) => {
    input.value = String(target);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }, seekTarget);
  await waitForMetricNear(page, seekTarget);
  const sought = await playbackSnapshot(page);
  diagnostics.sought = sought;
  assert.equal(sought.playing, "false", "seeking a completed replay must preserve its paused state");
  assert.equal(sought.canvasIdentity, "original", "completed replay seek must preserve the canvas");
  assert.equal(sought.rendererBackend, initial.rendererBackend, "completed replay seek changed renderer backend");
  assert.ok(
    Math.abs(parseSeconds(sought.metricTime) - seekTarget) <= 0.08,
    `completed replay seek missed target: ${sought.metricTime} vs ${seekTarget}`,
  );

  const initialGeneration = initial.runGeneration;
  await page.evaluate(() => {
    const source = document.querySelector("#python-scene-source");
    source.value = `${source.value.trimEnd()}\n        self.wait(0.25)\n`;
    source.dispatchEvent(new Event("input", { bubbles: true }));
  });
  const edited = await playbackSnapshot(page);
  diagnostics.edited = edited;
  assert.equal(edited.patchState, "ready", "editing must leave the completed preview intact until Run");
  assert.equal(edited.runGeneration, initialGeneration, "editing must not start a replacement run");
  assert.equal(edited.hasControls, true, "editing must retain the completed replay lease");
  assert.equal(edited.canvasIdentity, "original", "editing must not replace the canvas");

  const runButton = page.locator("#replace-scene");
  await runButton.click();
  await page.waitForFunction(() => window.__noonExampleGallery?.runInFlight === true);
  await page.waitForFunction(
    () =>
      window.__noonExampleGallery?.runInFlight === false &&
      document.querySelector("#patch-status")?.dataset.state === "applied" &&
      !document.querySelector("#replace-scene")?.disabled,
    null,
    { timeout: 60_000 },
  );
  const rerun = await playbackSnapshot(page);
  diagnostics.rerun = rerun;
  assert.equal(rerun.hasControls, true, "explicit Run must restore a completed replay lease");
  assert.equal(rerun.playbackAvailability, "available");
  assert.equal(rerun.runDisabled, false);
  assert.equal(rerun.patchRunGeneration, String(rerun.runGeneration));
  assert.ok(rerun.runGeneration > initialGeneration, "explicit Run must apply the newest source generation");
  assert.equal(
    Number(rerun.scrubberMax),
    Number(initial.scrubberMax) + 0.25,
    "Run must execute the edited source's added wait",
  );
  assert.equal(rerun.canvasIdentity, "original", "source rerun must preserve the canvas");
  assert.equal(rerun.rendererBackend, initial.rendererBackend, "source rerun changed renderer backend");
  assert.equal(rerun.canvasCount, 1);

  await page.setViewportSize({ width: 390, height: 844 });
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const mobile = await playbackSnapshot(page);
  diagnostics.mobile = mobile;
  assert.ok(
    mobile.documentWidth <= mobile.viewportWidth + 1,
    `source controls overflow mobile viewport (${mobile.documentWidth}px > ${mobile.viewportWidth}px)`,
  );
  await runButton.screenshot({ path: path.join(artifactDir, "controls-mobile.png") });
  await page.screenshot({ path: path.join(artifactDir, "playground.png"), fullPage: true });

  assert.deepEqual(pageErrors, [], `page errors: ${pageErrors.join("\n")}`);
  assert.deepEqual(consoleErrors, [], `console errors: ${consoleErrors.join("\n")}`);
  diagnostics.pageErrors = pageErrors;
  diagnostics.consoleErrors = consoleErrors;
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  console.log("✓ first-pass source ownership transitions to a seekable completed replay without replacing the canvas");
} catch (error) {
  if (page !== null) {
    try {
      await page.screenshot({ path: path.join(artifactDir, "failure.png"), fullPage: true });
      diagnostics.failure = await playbackSnapshot(page);
    } catch {
      // Preserve the original failure.
    }
  }
  diagnostics.pageErrors = pageErrors;
  diagnostics.consoleErrors = consoleErrors;
  diagnostics.error =
    error instanceof Error ? { name: error.name, message: error.message, stack: error.stack } : String(error);
  diagnostics.serverOutput = serverOutput;
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  throw error;
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
