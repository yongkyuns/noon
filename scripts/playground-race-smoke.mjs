import assert from "node:assert/strict";
import { spawn } from "node:child_process";

import playwright from "playwright";

import { PlaygroundGeneration } from "../web/playground-generation.js";
import { NEWEST_SOURCE, SUPERSEDED_SOURCE } from "./playground-source-edit-race-fixture.mjs";

function stressGenerationGate() {
  const generations = new PlaygroundGeneration();
  let seed = 0x6e6f6f6e;
  const random = () => {
    seed ^= seed << 13;
    seed ^= seed >>> 17;
    seed ^= seed << 5;
    return seed >>> 0;
  };

  let activeExample = "scene-0";
  generations.commitSelection(generations.beginSelectionRequest(activeExample));
  let latestRun = generations.beginRun(activeExample);
  const staleRuns = [];

  for (let index = 0; index < 500; index += 1) {
    if (random() % 3 === 0) {
      const request = generations.beginSelectionRequest(`scene-${index + 1}`);
      const committed = generations.commitSelection(request);
      assert.ok(committed);
      staleRuns.push(latestRun);
      activeExample = committed.exampleId;
      latestRun = generations.beginRun(activeExample);
    } else {
      staleRuns.push(latestRun);
      latestRun = generations.beginRun(activeExample);
    }
    assert.equal(generations.isRunCurrent(latestRun, activeExample), true);
    const sample = staleRuns[random() % staleRuns.length];
    if (sample) {
      assert.equal(generations.isRunCurrent(sample, activeExample), false);
    }
  }
}

stressGenerationGate();

const { chromium } = playwright;
const port = Number(process.env.NOON_PLAYGROUND_RACE_PORT ?? "4184");
const baseUrl = `http://127.0.0.1:${port}`;

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", "."],
  { stdio: ["ignore", "pipe", "pipe"] },
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
  throw new Error(`Playground race server did not start: ${lastError}\n${serverOutput}`);
}

async function waitForApplied(page, exampleId, browserErrors) {
  await page.waitForFunction(
    (id) => {
      const gallery = window.__noonExampleGallery;
      const patch = document.querySelector("#patch-status");
      return (
        gallery?.selectedExampleId === id &&
        patch?.dataset.exampleId === id &&
        (patch.dataset.state === "applied" || patch.dataset.state === "error")
      );
    },
    exampleId,
    { timeout: 60_000 },
  );
  const status = await page.evaluate(() => ({
    state: document.querySelector("#patch-status")?.dataset.state,
    text:
      document.querySelector("#patch-status")?.value ??
      document.querySelector("#patch-status")?.textContent ??
      "",
  }));
  assert.equal(
    status.state,
    "applied",
    `${exampleId}: authoring failed: ${status.text}\n${browserErrors.join("\n")}`,
  );
}

async function startDeferredRuntime(page) {
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  const deferred = await page.evaluate(() => {
    const status = document.querySelector("#status");
    return {
      runtimeStartup: status?.dataset.runtimeStartup ?? "",
      rendererBackend: status?.dataset.rendererBackend ?? "",
      presentedFrames: Number(status?.dataset.presentedFrames ?? "0"),
    };
  });
  assert.equal(deferred.runtimeStartup, "deferred", "race page load must leave runtime deferred");
  assert.equal(deferred.rendererBackend, "", "deferred race page must not initialize a renderer");
  assert.equal(deferred.presentedFrames, 0, "deferred race page must not present frames");
  await page.locator("#replace-scene").click();
}

let browser = null;
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
  const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
  });

  await page.addInitScript(() => {
    window.__noonRace = {
      authoringCounts: {},
      reconciles: [],
      holdNextExample: null,
      holdReached: 0,
      release: null,
      holdNextSourceRun: false,
      sourceHoldReached: 0,
      heldSourceRun: null,
    };
    window.__NOON_PLAYGROUND_TEST_HOOKS__ = {
      afterAuthoring(payload) {
        const race = window.__noonRace;
        race.authoringCounts[payload.exampleId] =
          (race.authoringCounts[payload.exampleId] ?? 0) + 1;
        if (race.holdNextSourceRun) {
          race.holdNextSourceRun = false;
          race.sourceHoldReached += 1;
          race.heldSourceRun = { ...payload };
          return new Promise((resolve) => {
            race.release = resolve;
          });
        }
        if (race.holdNextExample !== payload.exampleId) return undefined;
        race.holdNextExample = null;
        race.holdReached += 1;
        return new Promise((resolve) => {
          race.release = resolve;
        });
      },
      beforeReconcile(payload) {
        window.__noonRace.reconciles.push({
          exampleId: payload.exampleId,
          runGeneration: payload.runGeneration,
        });
      },
    };
  });

  await page.goto(`${baseUrl}/web/index.html?example=parity-square-and-circle`, {
    waitUntil: "load",
  });
  await startDeferredRuntime(page);
  await waitForApplied(page, "parity-square-and-circle", browserErrors);

  const raceBaseline = await page.evaluate(() => window.__noonRace.reconciles.length);
  await page.evaluate(() => {
    window.__noonRace.holdNextExample = "parity-different-rotations";
    window.__staleSelection = window.__noonExampleGallery.select("parity-different-rotations");
  });
  await page.waitForFunction(() => window.__noonRace.holdReached === 1, null, {
    timeout: 30_000,
  });

  await page.evaluate(() => {
    window.__newestSelection = window.__noonExampleGallery.select("parity-create-circle");
  });
  // A selection request intentionally does not invalidate the current scene while
  // its source is still loading. Wait for the newer selection to commit: that is
  // the contract boundary that advances selection/run generations and makes the
  // held older authoring result stale.
  await page.waitForFunction(
    () => window.__noonExampleGallery.generationDiagnostics.selectionGeneration >= 3,
    null,
    { timeout: 30_000 },
  );
  await page.evaluate(() => {
    const release = window.__noonRace.release;
    window.__noonRace.release = null;
    release?.();
  });
  await page.evaluate(async () => {
    await Promise.all([window.__staleSelection, window.__newestSelection]);
  });
  await waitForApplied(page, "parity-create-circle", browserErrors);

  const staleRace = await page.evaluate((baseline) => ({
    selected: window.__noonExampleGallery.selectedExampleId,
    diagnostics: window.__noonExampleGallery.generationDiagnostics,
    reconciles: window.__noonRace.reconciles.slice(baseline),
    patchExample: document.querySelector("#patch-status")?.dataset.exampleId,
  }), raceBaseline);
  assert.equal(staleRace.selected, "parity-create-circle");
  assert.equal(staleRace.patchExample, "parity-create-circle");
  assert.ok(staleRace.diagnostics.staleDrops >= 1, "stale result must be counted");
  assert.equal(
    staleRace.reconciles.some(({ exampleId }) => exampleId === "parity-different-rotations"),
    false,
    "stale authored scene must never reach reconcileScene",
  );
  assert.equal(
    staleRace.reconciles.at(-1)?.exampleId,
    "parity-create-circle",
    "newest selection must own the final reconciliation",
  );

  const duplicateBaseline = await page.evaluate(
    () => window.__noonRace.authoringCounts["parity-create-circle"] ?? 0,
  );
  await page.evaluate(() => {
    window.__noonRace.holdNextExample = "parity-create-circle";
    window.__runA = window.__noonExampleGallery.run();
    window.__runB = window.__noonExampleGallery.run();
  });
  await page.waitForFunction(() => window.__noonRace.holdReached === 2, null, {
    timeout: 30_000,
  });
  const whileHeld = await page.evaluate(
    () => window.__noonRace.authoringCounts["parity-create-circle"] ?? 0,
  );
  assert.equal(
    whileHeld,
    duplicateBaseline + 1,
    "two simultaneous Run requests must start only one Python authoring request",
  );
  await page.evaluate(() => {
    const release = window.__noonRace.release;
    window.__noonRace.release = null;
    release?.();
  });
  await page.evaluate(async () => {
    await Promise.all([window.__runA, window.__runB]);
  });
  await waitForApplied(page, "parity-create-circle", browserErrors);
  const duplicateFinal = await page.evaluate(
    () => window.__noonRace.authoringCounts["parity-create-circle"] ?? 0,
  );
  assert.equal(duplicateFinal, duplicateBaseline + 1);

  const sourceRaceBaseline = await page.evaluate(async () => ({
    staleDrops: window.__noonExampleGallery.generationDiagnostics.staleDrops,
    metrics: await window.__noonExampleGallery.executionMetrics(),
    reconciles: window.__noonRace.reconciles.length,
  }));
  await page.evaluate(() => { window.__noonRace.holdNextSourceRun = true; });
  await submitSourceText(page, SUPERSEDED_SOURCE);
  await page.waitForFunction(
    () => window.__noonRace.sourceHoldReached === 1 && window.__noonRace.heldSourceRun !== null,
    null,
    { timeout: 60_000 },
  );
  const heldSourceRun = await page.evaluate(() => window.__noonRace.heldSourceRun);
  assert.ok(Number.isSafeInteger(heldSourceRun.runGeneration));
  await submitSourceText(page, NEWEST_SOURCE);
  const newerSourceGeneration = await page.evaluate(
    () => window.__noonExampleGallery.generationDiagnostics.runGeneration,
  );
  assert.ok(newerSourceGeneration > heldSourceRun.runGeneration,
    "new source input must invalidate the authored older run before it is released");
  await page.evaluate(() => {
    const release = window.__noonRace.release;
    window.__noonRace.release = null;
    release?.();
  });
  await waitForApplied(page, "parity-create-circle", browserErrors);
  const sourceRace = await page.evaluate(async (baseline) => ({
    source: document.querySelector("#python-scene-source")?.value,
    patch: {
      state: document.querySelector("#patch-status")?.dataset.state,
      runGeneration: Number(document.querySelector("#patch-status")?.dataset.runGeneration),
    },
    diagnostics: window.__noonExampleGallery.generationDiagnostics,
    reconciles: window.__noonRace.reconciles.slice(baseline.reconciles),
    metrics: await window.__noonExampleGallery.executionMetrics(),
  }), sourceRaceBaseline);
  assert.equal(sourceRace.source, NEWEST_SOURCE);
  assert.equal(sourceRace.patch.state, "applied");
  assert.equal(sourceRace.patch.runGeneration, sourceRace.diagnostics.runGeneration);
  assert.ok(sourceRace.diagnostics.staleDrops > sourceRaceBaseline.staleDrops,
    "superseded source run must be rejected by the controller generation gate");
  assert.equal(sourceRace.reconciles.some(({ runGeneration }) =>
    runGeneration === heldSourceRun.runGeneration), false,
  "the obsolete source must never reach semantic reconciliation");
  assert.equal(sourceRace.reconciles.at(-1)?.runGeneration, sourceRace.patch.runGeneration,
    "newest accepted source must own the final semantic reconciliation");
  assert.equal(sourceRace.metrics?.metrics?.objectCount, 2,
    "the newest source's distinct two-object scene must own the runtime");
  assert.notEqual(sourceRace.metrics?.metrics?.presentedSession,
    sourceRaceBaseline.metrics?.metrics?.presentedSession,
  "newest source must reconcile into a distinct presented session");

  assert.deepEqual(browserErrors, [], `playground emitted browser errors:\n${browserErrors.join("\n")}`);
  console.log(
    `✓ playground generations: 500 seeded operations + ${sourceRace.diagnostics.staleDrops} stale result(s) rejected; duplicate Run coalesced; older edited source never reconciled`,
  );
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}

async function submitSourceText(page, source) {
  await page.evaluate((value) => {
    const editor = document.querySelector("#python-scene-source");
    editor.value = value;
    editor.dispatchEvent(new Event("input", { bubbles: true }));
  }, source);
}
