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
const sampleMs = Number(process.env.NOON_PRODUCT_FPS_SAMPLE_MS ?? "2000");

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

async function runAndMeasure(page) {
  const started = performance.now();
  await page.locator("#replace-scene").click();
  const state = await waitForApplied(page);
  return { milliseconds: performance.now() - started, state };
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

async function sampleRendererFps(page) {
  await page.waitForFunction(
    () => Number(document.querySelector("#status")?.dataset.presentedFrames ?? 0) > 0,
    null,
    { timeout: 10_000 },
  );
  const start = await page.evaluate(() => ({
    frames: Number(document.querySelector("#status")?.dataset.presentedFrames ?? 0),
    now: performance.now(),
  }));
  await page.waitForTimeout(sampleMs);
  const end = await page.evaluate(() => ({
    frames: Number(document.querySelector("#status")?.dataset.presentedFrames ?? 0),
    now: performance.now(),
  }));
  const elapsedSeconds = Math.max((end.now - start.now) / 1000, 0.001);
  return {
    startFrames: start.frames,
    endFrames: end.frames,
    elapsedMs: end.now - start.now,
    effectiveFps: Math.max(0, end.frames - start.frames) / elapsedSeconds,
  };
}

async function pauseAndSeek(page, seconds) {
  const capability = await page.locator("#status").getAttribute("data-playback-controls");
  assert.ok(
    capability === "available" || capability === "unavailable",
    `product E2E did not publish playback capability (${capability})`,
  );
  const toggle = page.locator(".playback-toggle");
  const hasControls = (await toggle.count()) > 0;
  assert.equal(
    hasControls,
    capability === "available",
    "playback control DOM must match the execution ownership capability",
  );
  if (!hasControls) return false;
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
  await page.waitForTimeout(150);
  return true;
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

  const cold = await runAndMeasure(page);
  assert.equal(cold.state.backend, "WebGL2", `expected WebGL2 product path, got ${cold.state.backend}`);
  const fps = await sampleRendererFps(page);

  const warm = await runAndMeasure(page);

  const marker = `# product gate ${label}`;
  await page.evaluate((text) => {
    const editor = document.querySelector("#python-scene-source");
    if (!(editor instanceof HTMLTextAreaElement)) throw new Error("scene editor is unavailable");
    editor.value = `${editor.value.trimEnd()}\n\n${text}\n`;
    editor.dispatchEvent(new Event("input", { bubbles: true }));
  }, marker);
  const edited = await runAndMeasure(page);

  const sought = await pauseAndSeek(page, 0.5);
  const screenshotName = sought ? "frame-0.5.png" : "frame-final.png";
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
    sought,
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
