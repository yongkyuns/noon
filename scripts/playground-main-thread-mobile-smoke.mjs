import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { webkit } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_PLAYGROUND_MAIN_THREAD_PORT ?? "4186");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_PLAYGROUND_MATRIX_ARTIFACTS ??
    "browser-smoke-artifacts/playground-matrix/webkit-mobile-dpr2",
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
  throw new Error(`Mobile fallback server did not start: ${lastError}\n${serverOutput}`);
}

async function waitForIdle(page) {
  await page.waitForFunction(
    () => window.__noonExampleGallery && !window.__noonExampleGallery.runInFlight,
    null,
    { timeout: 60_000 },
  );
}

async function assertApplied(page, expectedId) {
  await page.waitForFunction(
    (id) => {
      const patch = document.querySelector("#patch-status");
      if (patch?.dataset.state === "error") return true;
      return patch?.dataset.state === "applied" && patch?.dataset.exampleId === id;
    },
    expectedId,
    { timeout: 60_000 },
  );
  const result = await page.evaluate(() => ({
    patchState: document.querySelector("#patch-status")?.dataset.state ?? null,
    patchText: document.querySelector("#patch-status")?.value ?? "",
    statusText: document.querySelector("#status-text")?.textContent ?? "",
    selectedExampleId: window.__noonExampleGallery?.selectedExampleId ?? null,
  }));
  assert.equal(
    result.patchState,
    "applied",
    `${expectedId} failed: ${result.patchText} ${result.statusText}`,
  );
  assert.equal(result.selectedExampleId, expectedId);
}

let browser = null;
try {
  await waitForServer();
  browser = await webkit.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    deviceScaleFactor: 2,
  });
  const page = await context.newPage();
  const pageErrors = [];
  const consoleErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });

  await page.goto(
    `${baseUrl}/web/index.html?example=parity-create-circle&renderHost=main-thread`,
    { waitUntil: "load" },
  );
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  await waitForIdle(page);

  await page.evaluate(() => window.__noonExampleGallery.run());
  await assertApplied(page, "parity-create-circle");

  let metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
  assert.equal(metrics?.renderHost, "main-thread", "mobile fallback did not select main-thread host");
  assert.ok(metrics?.metrics?.presentedFrames > 0, "mobile fallback did not present a frame");

  await page.evaluate(() => window.__noonExampleGallery.select("compatible-text-write"));
  await assertApplied(page, "compatible-text-write");

  const marker = "# mobile main-thread render fallback rerun";
  await page.evaluate((value) => {
    const editor = document.querySelector("#python-scene-source");
    if (!(editor instanceof HTMLTextAreaElement)) throw new Error("Python editor is unavailable");
    editor.value = `${editor.value.trimEnd()}\n\n${value}\n`;
    editor.dispatchEvent(new Event("input", { bubbles: true }));
  }, marker);
  await page.evaluate(() => window.__noonExampleGallery.run());
  await assertApplied(page, "compatible-text-write");

  for (const viewport of [
    { width: 430, height: 760 },
    { width: 360, height: 780 },
    { width: 390, height: 844 },
  ]) {
    await page.setViewportSize(viewport);
  }
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));

  metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
  assert.equal(metrics?.renderHost, "main-thread");
  assert.ok(metrics?.metrics?.presentedFrames > 0);
  assert.equal(pageErrors.length, 0, pageErrors.join("\n"));
  assert.equal(consoleErrors.length, 0, consoleErrors.join("\n"));

  const diagnostics = {
    renderHost: metrics.renderHost,
    rendererBackend: metrics.metrics?.backend ?? null,
    presentedFrames: metrics.metrics?.presentedFrames ?? null,
    selectedExampleId: await page.evaluate(() => window.__noonExampleGallery.selectedExampleId),
    pageErrors,
    consoleErrors,
  };
  await writeFile(
    path.join(artifactDir, "main-thread-mobile.json"),
    `${JSON.stringify(diagnostics, null, 2)}\n`,
    "utf8",
  );
  await page.screenshot({
    path: path.join(artifactDir, "main-thread-mobile.png"),
    fullPage: false,
  });
  console.log(
    `✓ mobile WebKit main-thread render host: ${diagnostics.rendererBackend}, ${diagnostics.presentedFrames} frames`,
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
