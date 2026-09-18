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
      controllable: controls?.dataset.controllable ?? null,
      scrubberDisabled: scrubber?.disabled ?? true,
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
      runtimeStartup: status?.dataset.runtimeStartup ?? "",
      playbackAvailability: status?.dataset.playbackControls ?? null,
      generationDiagnostics: window.__noonExampleGallery?.generationDiagnostics ?? null,
      runInFlight: window.__noonExampleGallery?.runInFlight ?? false,
      authoringCount: window.__noonRerunStress?.authoringCount ?? 0,
      sourceOwnedTransitions: window.__noonRerunStress?.sourceOwnedTransitions?.length ?? 0,
    };
  });
}

async function waitForInitialReplay(page, exampleId) {
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  // Preload starts after the first paint, so a later driver snapshot is not a
  // cold-start observation. Verify the recorded deferred state after preload
  // completes, then stress the same completed replay without a duplicate Run.
  await waitForApplied(page, exampleId);
  await page.waitForFunction(() =>
    document.querySelector("#status")?.dataset.liveAuthoring === "ready" &&
    window.__noonExampleGallery?.runInFlight === false, null, { timeout: 60_000 });
  const deferred = await page.evaluate(() => window.__noonRerunStress.initialDeferred);
  assert.ok(deferred, "rerun stress never observed deferred startup");
  assert.equal(deferred.runtimeStartup, "deferred", "rerun stress must begin without a runtime");
  assert.equal(deferred.rendererBackend, null, "deferred rerun stress must not initialize a renderer");
  assert.equal(deferred.playing, null, "deferred rerun stress must not allocate playback controls");
  return deferred;
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

async function waitForCompletedReplay(page, exampleId) {
  await waitForApplied(page, exampleId);
  await page.waitForFunction(
    () =>
      document.querySelector("#status")?.dataset.playbackControls === "available" &&
      document.querySelector(".playback-controls")?.dataset.busy === "false",
  );
}

async function waitForSourceOwnedPass(page, previousTransitions) {
  await page.waitForFunction(
    (previous) =>
      window.__noonRerunStress?.sourceOwnedTransitions?.length > previous &&
      window.__noonExampleGallery?.runInFlight === true &&
      document.querySelector("#status")?.dataset.playbackControls === "unavailable",
    previousTransitions,
    { timeout: 30_000 },
  );
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
      initialDeferred: null,
      holdExample: null,
      holdReached: false,
      release: null,
      sourceOwnedTransitions: [],
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
    const observeSourceOwnership = () => {
      const status = document.querySelector("#status");
      const stress = window.__noonRerunStress;
      if (status?.dataset.runtimeStartup === "deferred" && stress.initialDeferred === null) {
        stress.initialDeferred = {
          runtimeStartup: status.dataset.runtimeStartup,
          rendererBackend: status.dataset.rendererBackend ?? null,
          playing: document.querySelector(".playback-controls")?.dataset.playing ?? null,
        };
      }
      if (
        status?.dataset.playbackControls === "unavailable" &&
        window.__noonExampleGallery?.runInFlight === true
      ) {
        const latest = stress.sourceOwnedTransitions.at(-1);
        const runGeneration = window.__noonExampleGallery.generationDiagnostics?.runGeneration ?? null;
        if (latest?.runGeneration !== runGeneration) {
          stress.sourceOwnedTransitions.push({ runGeneration, at: performance.now() });
        }
      }
    };
    const sourceOwnershipObserver = new MutationObserver(observeSourceOwnership);
    sourceOwnershipObserver.observe(document, {
      attributes: true,
      subtree: true,
      attributeFilter: ["data-playback-controls", "data-runtime-startup"],
    });
    observeSourceOwnership();
  });

  const exampleId = "parity-create-circle";
  await page.goto(`${baseUrl}/web/index.html?example=${exampleId}`, { waitUntil: "load" });
  diagnostics.deferred = await waitForInitialReplay(page, exampleId);
  await page.waitForSelector(".playback-controls");
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "false");
  await page.evaluate(() => {
    document.querySelector("#scene").dataset.rerunStressIdentity = "original";
  });

  const initial = await snapshot(page);
  diagnostics.phases.push({ phase: "initial", ...initial });
  assert.equal(initial.authoringCount, 1, "automatic preload must author the initial scene exactly once");
  assert.equal(initial.playing, "false", "completed replay must begin paused");
  assert.equal(initial.playbackAvailability, "available");
  assert.ok(initial.sourceOwnedTransitions >= 1, "initial source pass never established exclusive playback ownership");
  assert.equal(initial.canvasIdentity, "original");
  assert.equal(initial.canvasCount, 1);
  assert.ok(initial.rendererBackend === "WebGL2" || initial.rendererBackend === "WebGPU");
  assert.ok(Number.isFinite(initial.scrubberMax) && initial.scrubberMax > 0);

  // A completed replay is seekable while paused. The next explicit Run must
  // temporarily return control to source ownership, then expose a new paused
  // replay lease without replacing the canvas or wedging controls.
  const pausedBaselineCount = (await snapshot(page)).authoringCount;
  const pausedBaselineTransitions = (await snapshot(page)).sourceOwnedTransitions;
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
  await waitForSourceOwnedPass(page, pausedBaselineTransitions);
  await waitForHeldRun(page);
  const pausedHeld = await snapshot(page);
  diagnostics.phases.push({ phase: "paused-held", target: finalScrubTarget, ...pausedHeld });
  assert.equal(pausedHeld.authoringCount, pausedBaselineCount + 1, "duplicate paused Run was not coalesced");
  assert.equal(pausedHeld.playbackAvailability, "unavailable", "source/replay transition must hide host controls");
  assert.equal(pausedHeld.controllable, "false", "source-owned pass must disable replay commands");
  assert.equal(pausedHeld.scrubberDisabled, true);
  assert.equal(pausedHeld.runInFlight, true, "held source completion must retain its active Run");
  assert.equal(pausedHeld.canvasIdentity, "original");
  assert.equal(pausedHeld.canvasCount, 1);

  await releaseHeldRun(page);
  await page.evaluate(async () => Promise.all([window.__pausedRunA, window.__pausedRunB]));
  await waitForCompletedReplay(page, exampleId);
  const pausedSettled = await snapshot(page);
  diagnostics.phases.push({ phase: "paused-settled", ...pausedSettled });
  assert.equal(pausedSettled.playing, "false", "completed replay must return paused after the source pass");
  assert.equal(pausedSettled.playbackAvailability, "available");
  assert.equal(pausedSettled.canvasIdentity, "original");
  assert.equal(pausedSettled.canvasCount, 1);
  assert.equal(pausedSettled.runtimeState, "ready");
  assert.equal(pausedSettled.patchExample, exampleId);
  assert.ok(
    pausedSettled.scrubberValue >= 0 && pausedSettled.scrubberValue <= pausedSettled.scrubberMax,
    "settled scrubber escaped the authored duration",
  );

  // Resume the completed replay, then verify an explicit Run enters exclusive
  // source ownership again and returns another completed replay lease.
  await page.locator(".playback-toggle").click();
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.playing === "true");
  const runningBaselineCount = (await snapshot(page)).authoringCount;
  const runningBaselineTransitions = (await snapshot(page)).sourceOwnedTransitions;
  await holdNextRun(page, exampleId);
  await page.evaluate(() => {
    window.__runningRunA = window.__noonExampleGallery.run();
    window.__runningRunB = window.__noonExampleGallery.run();
  });
  await waitForSourceOwnedPass(page, runningBaselineTransitions);
  await waitForHeldRun(page);
  const runningHeld = await snapshot(page);
  diagnostics.phases.push({ phase: "running-held", ...runningHeld });
  assert.equal(runningHeld.authoringCount, runningBaselineCount + 1, "duplicate running Run was not coalesced");
  assert.equal(runningHeld.playbackAvailability, "unavailable", "source ownership must hide replay controls");
  assert.equal(runningHeld.controllable, "false", "source-owned pass must disable replay commands");
  assert.equal(runningHeld.scrubberDisabled, true);
  assert.equal(runningHeld.runInFlight, true);
  assert.equal(runningHeld.canvasIdentity, "original");

  await releaseHeldRun(page);
  await page.evaluate(async () => Promise.all([window.__runningRunA, window.__runningRunB]));
  await waitForCompletedReplay(page, exampleId);
  const runningSettled = await snapshot(page);
  diagnostics.phases.push({ phase: "running-settled", ...runningSettled });
  assert.equal(runningSettled.playing, "false", "completed replay must not inherit source-pass wall-clock playback");
  assert.equal(runningSettled.playbackAvailability, "available");
  assert.equal(runningSettled.canvasIdentity, "original");
  assert.equal(runningSettled.canvasCount, 1);
  assert.equal(runningSettled.runtimeState, "ready");

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
    const before = await snapshot(page);
    const beforeCount = before.authoringCount;
    const beforeTransitions = before.sourceOwnedTransitions;
    await page.evaluate(async () => {
      await Promise.all([window.__noonExampleGallery.run(), window.__noonExampleGallery.run()]);
    });
    await page.waitForFunction(
      (previous) => window.__noonRerunStress?.sourceOwnedTransitions?.length > previous,
      beforeTransitions,
      { timeout: 30_000 },
    );
    await waitForCompletedReplay(page, exampleId);
    const settled = await snapshot(page);
    diagnostics.phases.push({ phase: `iteration-${iteration}`, ...settled });
    assert.equal(settled.authoringCount, beforeCount + 1, `iteration ${iteration}: duplicate Run was not coalesced`);
    assert.equal(settled.playing, "false", "each completed source replay must settle paused");
    assert.equal(settled.playbackAvailability, "available");
    assert.equal(settled.canvasIdentity, "original");
    assert.equal(settled.canvasCount, 1);
    assert.equal(settled.patchState, "applied");
    assert.equal(settled.patchExample, exampleId);
    assert.equal(settled.runtimeState, "ready");
  }

  assert.deepEqual(pageErrors, [], `page errors: ${pageErrors.join("\n")}`);
  assert.deepEqual(consoleErrors, [], `console errors: ${consoleErrors.join("\n")}`);
  diagnostics.pageErrors = pageErrors;
  diagnostics.consoleErrors = consoleErrors;
  diagnostics.serverOutput = serverOutput;
  await page.screenshot({ path: path.join(artifactDir, "playground.png"), fullPage: true });
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  console.log("✓ replay rerun stress: source ownership, seek queues, duplicate Runs and canvas reuse remain coherent");
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
