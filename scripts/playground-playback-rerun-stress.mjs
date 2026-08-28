import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.env.NOON_PLAYGROUND_RERUN_STRESS_PORT ?? "4188");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_PLAYGROUND_RERUN_STRESS_ARTIFACTS ??
    "browser-smoke-artifacts/playground-playback-rerun-stress",
);

await mkdir(artifactDir, { recursive: true });

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
  throw new Error(`Playground rerun stress server did not start: ${lastError}\n${serverOutput}`);
}

async function waitForApplied(page, exampleId, timeout = 60_000) {
  await page.waitForFunction(
    (id) => {
      const patch = document.querySelector("#patch-status");
      return (
        patch?.dataset.exampleId === id &&
        (patch.dataset.state === "applied" || patch.dataset.state === "error")
      );
    },
    exampleId,
    { timeout },
  );
  const patch = await page.evaluate(() => ({
    state: document.querySelector("#patch-status")?.dataset.state ?? "",
    text:
      document.querySelector("#patch-status")?.value ??
      document.querySelector("#patch-status")?.textContent ??
      "",
  }));
  assert.equal(patch.state, "applied", `${exampleId} failed: ${patch.text}`);
}

async function snapshot(page) {
  return page.evaluate(() => {
    const controls = document.querySelector(".playback-controls");
    const scrubber = document.querySelector(".playback-scrubber");
    const patch = document.querySelector("#patch-status");
    const status = document.querySelector("#status");
    const canvas = document.querySelector("#scene");
    const metricTime =
      document.querySelector("#metric-time")?.value ??
      document.querySelector("#metric-time")?.textContent ??
      "";
    return {
      playing: controls?.dataset.playing ?? null,
      busy: controls?.dataset.busy ?? null,
      scrubberValue: Number(scrubber?.value ?? NaN),
      scrubberMax: Number(scrubber?.max ?? NaN),
      patchState: patch?.dataset.state ?? "",
      patchExample: patch?.dataset.exampleId ?? "",
      metricTime,
      canvasIdentity: canvas?.dataset.rerunStressIdentity ?? null,
      canvasCount: document.querySelectorAll("canvas").length,
      rendererBackend: status?.dataset.rendererBackend ?? null,
      runtimeState: status?.dataset.state ?? null,
      generationDiagnostics: window.__noonExampleGallery?.generationDiagnostics ?? null,
      authoringCount: window.__noonRerunStress?.authoringCount ?? 0,
    };
  });
}

async function holdNextRun(page, exampleId) {
  await page.evaluate((id) => {
    const stress = window.__noonRerunStress;
    stress.holdExample = id;
    stress.release = null;
    stress.holdReached = false;
  }, exampleId);
}

async function waitForHeldRun(page) {
  await page.waitForFunction(() => window.__noonRerunStress?.holdReached === true, null, {
    timeout: 30_000,
  });
}

async function releaseHeldRun(page) {
  await page.evaluate(() => {
    const release = window.__noonRerunStress?.release;
    window.__noonRerunStress.release = null;
    release?.();
  });
}

let browser = null;
let page = null;
const pageErrors = [];
const consoleErrors = [];
const diagnostics = { phases: [] };

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
    window.__noonRerunStress = {
      authoringCount: 0,
      holdExample: null,
      holdReached: false,
      release: null,
    };
    window.__NOON_PLAYGROUND_TEST_HOOKS__ = {
      afterAuthoring(payload) {
        const stress = window.__noonRerunStress;
        stress.authoringCount += 1;
        if (stress.holdExample !== payload.exampleId) return undefined;
        stress.holdExample = null;
        stress.holdReached = true;
        return new Promise((resolve) => {
          stress.release = resolve;
        });
      },
    };
  });

  const exampleId = "parity-create-circle";
  await page.goto(`${baseUrl}/web/index.html?example=${exampleId}`, { waitUntil: "load" });
  await waitForApplied(page, exampleId);
  await page.waitForSelector(".playback-controls");
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "false");
  await page.evaluate(() => {
    document.querySelector("#scene").dataset.rerunStressIdentity = "original";
  });

  const initial = await snapshot(page);
  diagnostics.phases.push({ phase: "initial", ...initial });
  assert.equal(initial.playing, "true");
  assert.equal(initial.canvasIdentity, "original");
  assert.equal(initial.canvasCount, 1);
  assert.ok(initial.rendererBackend === "WebGL2" || initial.rendererBackend === "WebGPU");
  assert.ok(Number.isFinite(initial.scrubberMax) && initial.scrubberMax > 0);

  // Pause, enqueue a burst of scrub requests, then overlap a held rerun. The
  // playback controller may coalesce seeks, but the scene rerun must not unpause,
  // replace the canvas, or wedge the controls.
  await page.locator(".playback-toggle").click();
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.playing === "false");
  const pausedBaselineCount = (await snapshot(page)).authoringCount;
  await holdNextRun(page, exampleId);
  const finalScrubTarget = await page.locator(".playback-scrubber").evaluate((input) => {
    const max = Number(input.max);
    let last = 0;
    for (let index = 0; index < 40; index += 1) {
      last = max * (((index * 17) % 37) + 1) / 38;
      input.value = String(last);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }
    window.__pausedRunA = window.__noonExampleGallery.run();
    window.__pausedRunB = window.__noonExampleGallery.run();
    return last;
  });
  await waitForHeldRun(page);
  const pausedHeld = await snapshot(page);
  diagnostics.phases.push({ phase: "paused-held", target: finalScrubTarget, ...pausedHeld });
  assert.equal(pausedHeld.authoringCount, pausedBaselineCount + 1, "duplicate paused Run was not coalesced");
  assert.equal(pausedHeld.playing, "false");
  assert.equal(pausedHeld.canvasIdentity, "original");
  assert.equal(pausedHeld.canvasCount, 1);

  await releaseHeldRun(page);
  await page.evaluate(async () => Promise.all([window.__pausedRunA, window.__pausedRunB]));
  await waitForApplied(page, exampleId);
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "false");
  const pausedSettled = await snapshot(page);
  diagnostics.phases.push({ phase: "paused-settled", ...pausedSettled });
  assert.equal(pausedSettled.playing, "false", "rerun while paused resumed playback");
  assert.equal(pausedSettled.canvasIdentity, "original");
  assert.equal(pausedSettled.canvasCount, 1);
  assert.equal(pausedSettled.runtimeState, "running");
  assert.equal(pausedSettled.patchExample, exampleId);
  assert.ok(
    pausedSettled.scrubberValue >= 0 && pausedSettled.scrubberValue <= pausedSettled.scrubberMax,
    "settled scrubber escaped the authored duration",
  );

  // Resume and repeat the held duplicate rerun while logical time is advancing.
  await page.locator(".playback-toggle").click();
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.playing === "true");
  const runningBaselineCount = (await snapshot(page)).authoringCount;
  await holdNextRun(page, exampleId);
  await page.evaluate(() => {
    window.__runningRunA = window.__noonExampleGallery.run();
    window.__runningRunB = window.__noonExampleGallery.run();
  });
  await waitForHeldRun(page);
  const runningHeld = await snapshot(page);
  diagnostics.phases.push({ phase: "running-held", ...runningHeld });
  assert.equal(runningHeld.authoringCount, runningBaselineCount + 1, "duplicate running Run was not coalesced");
  assert.equal(runningHeld.playing, "true");
  assert.equal(runningHeld.canvasIdentity, "original");

  await releaseHeldRun(page);
  await page.evaluate(async () => Promise.all([window.__runningRunA, window.__runningRunB]));
  await waitForApplied(page, exampleId);
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "false");
  const runningSettled = await snapshot(page);
  diagnostics.phases.push({ phase: "running-settled", ...runningSettled });
  assert.equal(runningSettled.playing, "true", "rerun while playing paused playback");
  assert.equal(runningSettled.canvasIdentity, "original");
  assert.equal(runningSettled.canvasCount, 1);
  assert.equal(runningSettled.runtimeState, "running");

  // Repeat state transitions without hooks to catch command-queue ordering bugs.
  for (let iteration = 0; iteration < 8; iteration += 1) {
    const shouldPause = iteration % 2 === 0;
    const current = await snapshot(page);
    if ((current.playing === "true") === shouldPause) {
      await page.locator(".playback-toggle").click();
    }
    await page.waitForFunction(
      (paused) => document.querySelector(".playback-controls")?.dataset.playing === (paused ? "false" : "true"),
      shouldPause,
    );
    if (shouldPause) {
      await page.locator(".playback-scrubber").evaluate((input, seed) => {
        const max = Number(input.max);
        for (let index = 0; index < 12; index += 1) {
          input.value = String(max * (((seed + index * 7) % 23) + 1) / 24);
          input.dispatchEvent(new Event("input", { bubbles: true }));
        }
      }, iteration);
    }
    const beforeCount = (await snapshot(page)).authoringCount;
    await page.evaluate(async () => {
      await Promise.all([window.__noonExampleGallery.run(), window.__noonExampleGallery.run()]);
    });
    await waitForApplied(page, exampleId);
    await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "false");
    const settled = await snapshot(page);
    diagnostics.phases.push({ phase: `iteration-${iteration}`, ...settled });
    assert.equal(settled.authoringCount, beforeCount + 1, `iteration ${iteration}: duplicate Run was not coalesced`);
    assert.equal(settled.playing, shouldPause ? "false" : "true");
    assert.equal(settled.canvasIdentity, "original");
    assert.equal(settled.canvasCount, 1);
    assert.equal(settled.patchState, "applied");
    assert.equal(settled.patchExample, exampleId);
    assert.equal(settled.runtimeState, "running");
  }

  assert.deepEqual(pageErrors, [], `page errors: ${pageErrors.join("\n")}`);
  assert.deepEqual(consoleErrors, [], `console errors: ${consoleErrors.join("\n")}`);
  diagnostics.pageErrors = pageErrors;
  diagnostics.consoleErrors = consoleErrors;
  diagnostics.serverOutput = serverOutput;
  await page.screenshot({ path: path.join(artifactDir, "playground.png"), fullPage: true });
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  console.log("✓ playback-state rerun stress: paused/scrub/running commands remain coherent");
} catch (error) {
  diagnostics.pageErrors = pageErrors;
  diagnostics.consoleErrors = consoleErrors;
  diagnostics.serverOutput = serverOutput;
  diagnostics.error = error instanceof Error ? { name: error.name, message: error.message, stack: error.stack } : String(error);
  if (page !== null) {
    try {
      diagnostics.failure = await snapshot(page);
      await page.screenshot({ path: path.join(artifactDir, "failure.png"), fullPage: true });
    } catch {
      // Preserve the original failure.
    }
  }
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  throw error;
} finally {
  await browser?.close().catch(() => {});
  server.kill("SIGTERM");
}
