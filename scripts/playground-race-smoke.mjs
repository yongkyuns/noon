import assert from "node:assert/strict";
import { spawn } from "node:child_process";

import playwright from "playwright";

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
    };
    window.__NOON_PLAYGROUND_TEST_HOOKS__ = {
      afterAuthoring(payload) {
        const race = window.__noonRace;
        race.authoringCounts[payload.exampleId] =
          (race.authoringCounts[payload.exampleId] ?? 0) + 1;
        if (race.holdNextExample !== payload.exampleId) return undefined;
        race.holdNextExample = null;
        race.holdReached += 1;
        return new Promise((resolve) => {
          race.release = resolve;
        });
      },
      beforeReconcile(payload) {
        window.__noonRace.reconciles.push(payload.exampleId);
      },
    };
  });

  await page.goto(`${baseUrl}/web/index.html?example=parity-square-and-circle`, {
    waitUntil: "load",
  });
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
  await page.waitForFunction(
    () => window.__noonExampleGallery.generationDiagnostics.selectionRequestGeneration >= 3,
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
    staleRace.reconciles.includes("parity-different-rotations"),
    false,
    "stale authored scene must never reach reconcileScene",
  );
  assert.equal(
    staleRace.reconciles.at(-1),
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

  assert.deepEqual(browserErrors, [], `playground emitted browser errors:\n${browserErrors.join("\n")}`);
  console.log(
    `✓ playground generations: ${staleRace.diagnostics.staleDrops} stale result(s) rejected; duplicate Run coalesced`,
  );
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
