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
  const match = String(text).match(
    /^\s*([0-9]+(?:\.[0-9]+)?)(?:\s*\/\s*[0-9]+(?:\.[0-9]+)?)?\s*s\s*$/,
  );
  assert.ok(match, `unable to parse seconds from ${JSON.stringify(text)}`);
  return Number(match[1]);
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
  const deferred = await page.evaluate(() => {
    const status = document.querySelector("#status");
    return {
      runtimeStartup: status?.dataset.runtimeStartup ?? "",
      executionMode: window.__noonExampleGallery?.executionMode ?? null,
      visibleExampleCount: window.__noonExampleGallery?.visibleExampleCount ?? Infinity,
      hasPlaybackControls: document.querySelector(".playback-controls") !== null,
    };
  });
  assert.equal(deferred.runtimeStartup, "deferred", "playground must not start runtime on page load");
  assert.equal(deferred.executionMode, null, "deferred playground must not own an execution runtime yet");
  assert.equal(deferred.hasPlaybackControls, false, "playback controls must not allocate before runtime start");
  assert.ok(deferred.visibleExampleCount <= 18, "gallery DOM residency must remain bounded on startup");

  const runButton = page.locator("#replace-scene");
  // Source loading may temporarily disable Run after the shell is attached.
  // Playwright's click waits for the current actionable state instead of
  // asserting against a stale readiness snapshot.
  await runButton.click();
  await waitForAppliedScene(page, expectedExampleId);
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
      canvasIdentity: canvas?.dataset.playbackSmokeIdentity ?? null,
      canvasCount: document.querySelectorAll("canvas").length,
      documentWidth: document.documentElement.scrollWidth,
      viewportWidth: window.innerWidth,
    };
  });
}

async function waitForMetricNear(page, target, tolerance = 0.08, timeout = 10_000) {
  await page.waitForFunction(
    ({ target, tolerance }) => {
      const raw =
        document.querySelector("#metric-time")?.value ??
        document.querySelector("#metric-time")?.textContent ??
        "";
      const match = String(raw).match(/([0-9]+(?:\.[0-9]+)?)\s*s/);
      return match !== null && Math.abs(Number(match[1]) - target) <= tolerance;
    },
    { target, tolerance },
    { timeout },
  );
}

async function waitForMetricAdvance(page, lowerBound, timeout = 10_000) {
  await page.waitForFunction(
    (minimum) => {
      const raw =
        document.querySelector("#metric-time")?.value ??
        document.querySelector("#metric-time")?.textContent ??
        "";
      const match = String(raw).match(/([0-9]+(?:\.[0-9]+)?)\s*s/);
      return match !== null && Number(match[1]) >= minimum;
    },
    lowerBound,
    { timeout },
  );
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

  await page.goto(`${baseUrl}/web/index.html?example=parity-square-and-circle`, {
    waitUntil: "load",
  });
  await startDeferredRuntime(page, "parity-square-and-circle");
  await page.waitForSelector(".playback-controls");
  await page.evaluate(() => {
    document.querySelector("#scene").dataset.playbackSmokeIdentity = "original";
  });
  await waitForMetricAdvance(page, 0.1);

  const initial = await playbackSnapshot(page);
  diagnostics.initial = initial;
  assert.equal(initial.hasControls, true);
  assert.equal(initial.playText, "Pause");
  assert.equal(initial.playing, "true");
  assert.equal(initial.playDisabled, false);
  assert.equal(initial.restartDisabled, false);
  assert.equal(initial.scrubberDisabled, false);
  assert.equal(initial.canvasCount, 1);
  assert.equal(initial.canvasIdentity, "original");
  assert.ok(Number(initial.scrubberMax) > 0, "authored duration must be exposed to the scrubber");
  assert.ok(initial.rendererBackend === "WebGL2" || initial.rendererBackend === "WebGPU");

  await page.locator(".playback-toggle").click();
  await page.waitForFunction(() => document.querySelector(".playback-toggle")?.textContent === "Play");
  await new Promise((resolve) => setTimeout(resolve, 350));
  const pausedA = await playbackSnapshot(page);
  await new Promise((resolve) => setTimeout(resolve, 450));
  const pausedB = await playbackSnapshot(page);
  diagnostics.paused = { first: pausedA, second: pausedB };
  assert.equal(pausedB.playing, "false");
  assert.equal(pausedB.canvasIdentity, "original");
  assert.equal(pausedB.rendererBackend, initial.rendererBackend);
  assert.ok(
    Math.abs(parseSeconds(pausedB.timeText) - parseSeconds(pausedA.timeText)) <= 0.02,
    `pause did not freeze logical time: ${pausedA.timeText} -> ${pausedB.timeText}`,
  );
  const pausedTime = parseSeconds(pausedB.timeText);
  await waitForMetricNear(page, pausedTime, 0.03);
  const pausedMetrics = await playbackSnapshot(page);
  diagnostics.paused.metricsConverged = pausedMetrics;
  assert.ok(
    Math.abs(parseSeconds(pausedMetrics.metricTime) - pausedTime) <= 0.03,
    `polled metrics did not converge to paused logical time: ${pausedMetrics.metricTime} vs ${pausedB.timeText}`,
  );

  const duration = Number(pausedB.scrubberMax);
  const seekTarget = Math.min(duration * 0.6, Math.max(0.05, duration - 0.05));
  await page.locator(".playback-scrubber").evaluate((input, target) => {
    input.value = String(target);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }, seekTarget);
  await waitForMetricNear(page, seekTarget, 0.08);
  const sought = await playbackSnapshot(page);
  diagnostics.sought = sought;
  assert.equal(sought.playing, "false", "seek must preserve paused state");
  assert.equal(sought.canvasIdentity, "original", "seek must preserve the canvas");
  assert.equal(sought.rendererBackend, initial.rendererBackend, "seek must preserve renderer backend");
  assert.ok(Math.abs(parseSeconds(sought.metricTime) - seekTarget) <= 0.08);

  const restartButton = page.getByRole("button", { name: "Restart animation from the beginning" });
  await restartButton.click();
  await waitForMetricNear(page, 0, 0.03);
  const restarted = await playbackSnapshot(page);
  diagnostics.restarted = restarted;
  assert.equal(restarted.playing, "false", "playback restart must preserve paused state");
  assert.equal(restarted.canvasIdentity, "original", "playback restart must not replace the canvas");
  assert.equal(restarted.rendererBackend, initial.rendererBackend);
  assert.ok(parseSeconds(restarted.metricTime) <= 0.03);

  await page.locator(".playback-toggle").click();
  await page.waitForFunction(() => document.querySelector(".playback-toggle")?.textContent === "Pause");
  await waitForMetricAdvance(page, 0.12);
  const resumed = await playbackSnapshot(page);
  diagnostics.resumed = resumed;
  assert.equal(resumed.playing, "true");
  assert.equal(resumed.canvasIdentity, "original");
  assert.equal(resumed.rendererBackend, initial.rendererBackend);
  assert.ok(
    parseSeconds(resumed.metricTime) < 0.8,
    `resume caught up wall-clock time instead of re-anchoring: ${resumed.metricTime}`,
  );

  const targetExampleId = await page.evaluate(() => {
    const selected = document.querySelector(".example-card[aria-selected='true']")?.dataset.exampleId;
    return [...document.querySelectorAll(".example-card")]
      .map((card) => card.dataset.exampleId)
      .find((id) => id && id !== selected);
  });
  assert.ok(targetExampleId, "gallery must expose another example");
  await page.locator("#example-browser-trigger").click();
  await page.locator(`.example-card[data-example-id="${targetExampleId}"]`).click();
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "true");
  const busy = await playbackSnapshot(page);
  diagnostics.busy = busy;
  assert.equal(busy.playDisabled, true);
  assert.equal(busy.restartDisabled, true);
  assert.equal(busy.scrubberDisabled, true);
  await waitForAppliedScene(page, targetExampleId);
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "false");
  const switched = await playbackSnapshot(page);
  diagnostics.switched = switched;
  assert.equal(switched.canvasCount, 1);
  assert.ok(Number(switched.scrubberMax) > 0);
  assert.equal(switched.playDisabled, false);

  await page.setViewportSize({ width: 390, height: 844 });
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const mobile = await playbackSnapshot(page);
  diagnostics.mobile = mobile;
  assert.ok(
    mobile.documentWidth <= mobile.viewportWidth + 1,
    `playback controls overflow mobile viewport (${mobile.documentWidth}px > ${mobile.viewportWidth}px)`,
  );
  await page.locator(".playback-controls").screenshot({ path: path.join(artifactDir, "controls-mobile.png") });
  await page.screenshot({ path: path.join(artifactDir, "playground.png"), fullPage: true });

  assert.deepEqual(pageErrors, [], `page errors: ${pageErrors.join("\n")}`);
  assert.deepEqual(consoleErrors, [], `console errors: ${consoleErrors.join("\n")}`);
  diagnostics.pageErrors = pageErrors;
  diagnostics.consoleErrors = consoleErrors;
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  console.log("✓ public Playground play/pause/seek/restart controls are deterministic");
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
