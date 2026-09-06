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
const port = Number(process.env.NOON_RENDERER_CONTEXT_LOSS_PORT ?? "4192");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_RENDERER_CONTEXT_LOSS_ARTIFACTS ??
    "browser-smoke-artifacts/renderer-context-loss",
);
const sampleTime = 0.75;

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
      const response = await fetch(`${baseUrl}/web/browser-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Renderer context-loss server did not start: ${lastError}\n${serverOutput}`);
}

function collectBrowserErrors(page) {
  const pageErrors = [];
  const consoleErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  return { pageErrors, consoleErrors };
}

async function waitForHarness(page) {
  await page.goto(`${baseUrl}/web/browser-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonSmoke?.state.ready === true, null, {
    timeout: 30_000,
  });
  const metrics = await page.evaluate(() => window.noonSmoke.metrics());
  assert.equal(metrics.error, null, `renderer failed to initialize: ${metrics.error}`);
  assert.equal(metrics.rendererBackend, "WebGL2", `expected WebGL2, got ${metrics.rendererBackend}`);
  const loaded = await page.evaluate(async () => {
    const wasm = await import("./pkg/noon_web.js");
    const { createExplicitTransportSceneJson } = await import(
      "../scripts/explicit-transport-scene-fixture.js"
    );
    return window.noonSmoke.loadScene(createExplicitTransportSceneJson(wasm));
  });
  assert.equal(loaded.objectCount, 4, "context-loss fixture must contain visible geometry");
  return page.evaluate(() => window.noonSmoke.metrics());
}

async function renderAndCapture(page, name) {
  const metrics = await page.evaluate((time) => window.noonSmoke.renderAt(time), sampleTime);
  assert.equal(metrics.error, null, `${name}: renderer reported an error`);
  assert.equal(metrics.rendererBackend, "WebGL2", `${name}: backend changed unexpectedly`);
  assert.equal(metrics.presented, true, `${name}: frame was not presented`);
  assert.ok(metrics.drawCalls > 0, `${name}: frame emitted no draw calls`);
  const screenshot = await page.locator("#scene").screenshot({
    path: path.join(artifactDir, `${name}.png`),
  });
  return { metrics, screenshot };
}

function changedPixelCount(leftBuffer, rightBuffer) {
  const left = PNG.sync.read(leftBuffer);
  const right = PNG.sync.read(rightBuffer);
  assert.equal(left.width, right.width, "recovery comparison width mismatch");
  assert.equal(left.height, right.height, "recovery comparison height mismatch");
  let changed = 0;
  for (let offset = 0; offset < left.data.length; offset += 4) {
    if (
      left.data[offset] !== right.data[offset] ||
      left.data[offset + 1] !== right.data[offset + 1] ||
      left.data[offset + 2] !== right.data[offset + 2] ||
      left.data[offset + 3] !== right.data[offset + 3]
    ) {
      changed += 1;
    }
  }
  return changed;
}

async function writeDiagnostics(name, value) {
  await writeFile(
    path.join(artifactDir, `${name}.json`),
    `${JSON.stringify(value, null, 2)}\n`,
    "utf8",
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

  const page = await browser.newPage({ viewport: { width: 1000, height: 600 } });
  const browserErrors = collectBrowserErrors(page);
  const initial = await waitForHarness(page);
  const baseline = await renderAndCapture(page, "baseline");

  const injection = await page.evaluate(() => {
    window.__noonContextRecovery = window.noonSmoke.webglContextControl();
    return { available: window.__noonContextRecovery !== null };
  });
  assert.equal(
    injection.available,
    true,
    `WEBGL_lose_context is unavailable: ${JSON.stringify(injection)}`,
  );

  await page.evaluate(() => window.__noonContextRecovery.lose());
  await page.waitForFunction(() => window.__noonContextRecovery?.state.lost === 1, null, {
    timeout: 10_000,
  });

  const duringLoss = await page.evaluate(() => ({
    metrics: window.noonSmoke.metrics(),
    context: {
      lost: window.__noonContextRecovery.state.lost,
      restored: window.__noonContextRecovery.state.restored,
    },
  }));
  assert.equal(duringLoss.metrics.rendererBackend, "WebGL2", "context loss changed semantic backend identity");
  assert.equal(duringLoss.metrics.revision, baseline.metrics.revision, "context loss reset scene revision");
  assert.equal(duringLoss.metrics.objectCount, baseline.metrics.objectCount, "context loss reset scene objects");

  await page.evaluate(() => window.__noonContextRecovery.restore());
  await page.waitForFunction(() => window.__noonContextRecovery?.state.restored === 1, null, {
    timeout: 10_000,
  });

  let recovered = null;
  let lastRecoveryError = null;
  for (let attempt = 0; attempt < 8 && recovered === null; attempt += 1) {
    try {
      const candidate = await page.evaluate((time) => window.noonSmoke.renderAt(time), sampleTime);
      if (candidate.presented) recovered = candidate;
    } catch (error) {
      lastRecoveryError = String(error);
    }
    if (recovered === null) {
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
  }
  assert.ok(recovered, `WebGL context did not recover: ${lastRecoveryError ?? "no frame presented"}`);
  assert.equal(recovered.error, null, "recovered renderer reported an error");
  assert.equal(recovered.rendererBackend, "WebGL2", "recovered renderer changed backend");
  assert.equal(recovered.revision, baseline.metrics.revision, "recovery reset scene revision");
  assert.equal(recovered.objectCount, baseline.metrics.objectCount, "recovery reset object count");
  assert.ok(Math.abs(recovered.time - sampleTime) < 1e-6, "recovery changed semantic playhead time");

  const recoveredScreenshot = await page.locator("#scene").screenshot({
    path: path.join(artifactDir, "recovered.png"),
  });

  const freshPage = await browser.newPage({ viewport: { width: 1000, height: 600 } });
  const freshErrors = collectBrowserErrors(freshPage);
  await waitForHarness(freshPage);
  const fresh = await renderAndCapture(freshPage, "fresh");

  const changedPixels = changedPixelCount(recoveredScreenshot, fresh.screenshot);
  assert.equal(
    changedPixels,
    0,
    `recovered WebGL frame differs from fresh renderer at ${changedPixels} pixels`,
  );
  assert.equal(fresh.metrics.objectCount, recovered.objectCount, "fresh/recovered object count mismatch");
  assert.ok(Math.abs(fresh.metrics.time - recovered.time) < 1e-6, "fresh/recovered time mismatch");

  assert.deepEqual(browserErrors.pageErrors, [], "context-loss recovery emitted page errors");
  assert.deepEqual(freshErrors.pageErrors, [], "fresh comparison renderer emitted page errors");
  const unexpectedConsoleErrors = browserErrors.consoleErrors.filter(
    (message) => !/(context.*lost|context_lost_webgl|losecontext)/i.test(message),
  );
  assert.deepEqual(
    unexpectedConsoleErrors,
    [],
    `context-loss recovery emitted unexpected console errors:\n${unexpectedConsoleErrors.join("\n")}`,
  );
  assert.deepEqual(freshErrors.consoleErrors, [], "fresh comparison renderer emitted console errors");

  const contextState = await page.evaluate(() => ({
    lost: window.__noonContextRecovery.state.lost,
    restored: window.__noonContextRecovery.state.restored,
  }));
  await writeDiagnostics("context-loss-recovery", {
    browserVersion: browser.version(),
    initial,
    baseline: baseline.metrics,
    duringLoss,
    recovered,
    fresh: fresh.metrics,
    contextState,
    changedPixels,
    browserErrors,
    freshErrors,
  });
  await freshPage.close();
  await page.close();
  console.log("✓ WebGL context loss/restoration preserves scene, time, backend, and exact rendered frame");
} catch (error) {
  await writeDiagnostics("failure", {
    browserVersion: browser?.version() ?? null,
    error:
      error instanceof Error
        ? { name: error.name, message: error.message, stack: error.stack }
        : String(error),
    serverOutput,
  });
  throw error;
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
