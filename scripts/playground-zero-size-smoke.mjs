import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

import playwright from "playwright";

const { chromium } = playwright;
const port = Number(process.env.NOON_PLAYGROUND_ZERO_SIZE_PORT ?? "4183");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  process.env.NOON_PLAYGROUND_ZERO_SIZE_ARTIFACTS ??
    "browser-smoke-artifacts/playground-zero-size",
);

await mkdir(artifactDir, { recursive: true });

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
  throw new Error(`Playground server did not start: ${lastError}\n${serverOutput}`);
}

async function snapshot(page) {
  return page.evaluate(() => {
    const canvas = document.querySelector("#scene");
    const status = document.querySelector("#status");
    const patchStatus = document.querySelector("#patch-status");
    const rect = canvas.getBoundingClientRect();
    return {
      cssWidth: canvas.clientWidth,
      cssHeight: canvas.clientHeight,
      rectWidth: rect.width,
      rectHeight: rect.height,
      rendererBackend: status?.dataset.rendererBackend ?? "",
      runtimeState: status?.dataset.state ?? "",
      presentedFrames: Number(status?.dataset.presentedFrames ?? "0"),
      executionMode: status?.dataset.executionMode ?? "",
      metricTime: document.querySelector("#metric-time")?.value ?? "",
      patchState: patchStatus?.dataset.state ?? "",
      patchText: patchStatus?.value ?? patchStatus?.textContent ?? "",
    };
  });
}

async function waitForFrameAfter(page, previousFrames, label) {
  await page.waitForFunction(
    (previous) => {
      const status = document.querySelector("#status");
      return (
        status?.dataset.state !== "error" &&
        Number(status?.dataset.presentedFrames ?? "0") > previous
      );
    },
    previousFrames,
    { timeout: 10_000 },
  );
  const current = await snapshot(page);
  assert.ok(current.presentedFrames > previousFrames, `${label}: no new frame was presented`);
  assert.equal(current.rendererBackend, "WebGL2", `${label}: renderer backend changed`);
  assert.notEqual(current.runtimeState, "error", `${label}: runtime entered an error state`);
  return current;
}

async function setCanvasContentSize(page, width, height) {
  await page.evaluate(
    ({ nextWidth, nextHeight }) => {
      const canvas = document.querySelector("#scene");
      if (!Object.prototype.hasOwnProperty.call(window, "__noonOriginalCanvasStyle")) {
        window.__noonOriginalCanvasStyle = canvas.getAttribute("style");
      }
      const widthValue = `${nextWidth}px`;
      const heightValue = `${nextHeight}px`;
      Object.assign(canvas.style, {
        boxSizing: "content-box",
        width: widthValue,
        height: heightValue,
        minWidth: widthValue,
        maxWidth: widthValue,
        minHeight: heightValue,
        maxHeight: heightValue,
        flex: "0 0 auto",
      });
    },
    { nextWidth: width, nextHeight: height },
  );
  await page.waitForFunction(
    ({ expectedWidth, expectedHeight }) => {
      const canvas = document.querySelector("#scene");
      return canvas.clientWidth === expectedWidth && canvas.clientHeight === expectedHeight;
    },
    { expectedWidth: width, expectedHeight: height },
    { timeout: 5_000 },
  );
}

async function restoreCanvasSize(page) {
  await page.evaluate(() => {
    const canvas = document.querySelector("#scene");
    const originalStyle = window.__noonOriginalCanvasStyle;
    if (originalStyle === null || originalStyle === undefined) {
      canvas.removeAttribute("style");
    } else {
      canvas.setAttribute("style", originalStyle);
    }
  });
  await page.waitForFunction(() => {
    const canvas = document.querySelector("#scene");
    return canvas.clientWidth >= 320 && canvas.clientHeight >= 180;
  }, null, { timeout: 5_000 });
}

const diagnostics = {
  browser: null,
  viewport: { width: 1000, height: 700 },
  devicePixelRatio: 1,
  snapshots: {},
  pageErrors: [],
  consoleErrors: [],
  serverOutput: "",
};

let browser = null;
let context = null;
let page = null;
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
  context = await browser.newContext({
    viewport: diagnostics.viewport,
    deviceScaleFactor: diagnostics.devicePixelRatio,
  });
  page = await context.newPage();
  page.on("pageerror", (error) => diagnostics.pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") diagnostics.consoleErrors.push(message.text());
  });

  diagnostics.browser = await browser.version();
  await page.goto(`${baseUrl}/web/index.html?example=parity-square-and-circle`, {
    waitUntil: "load",
  });
  await page.waitForFunction(
    () => {
      const status = document.querySelector("#status");
      const patch = document.querySelector("#patch-status");
      return (
        status?.dataset.rendererBackend === "WebGL2" &&
        patch?.dataset.state === "applied" &&
        Number(status?.dataset.presentedFrames ?? "0") > 0
      );
    },
    null,
    { timeout: 60_000 },
  );

  diagnostics.snapshots.baseline = await snapshot(page);
  assert.ok(diagnostics.snapshots.baseline.cssWidth >= 320, "baseline canvas must be visible");
  assert.ok(diagnostics.snapshots.baseline.cssHeight >= 180, "baseline canvas must be visible");
  await page.locator("#scene").screenshot({ path: path.join(artifactDir, "baseline.png") });

  await setCanvasContentSize(page, 0, 0);
  diagnostics.snapshots.zero = await waitForFrameAfter(
    page,
    diagnostics.snapshots.baseline.presentedFrames,
    "zero-size transition",
  );
  assert.equal(diagnostics.snapshots.zero.cssWidth, 0, "canvas content width must reach zero");
  assert.equal(diagnostics.snapshots.zero.cssHeight, 0, "canvas content height must reach zero");
  await page.screenshot({ path: path.join(artifactDir, "zero-size-page.png"), fullPage: true });

  await setCanvasContentSize(page, 1, 1);
  diagnostics.snapshots.nearZero = await waitForFrameAfter(
    page,
    diagnostics.snapshots.zero.presentedFrames,
    "near-zero transition",
  );
  assert.equal(diagnostics.snapshots.nearZero.cssWidth, 1, "canvas content width must reach 1px");
  assert.equal(diagnostics.snapshots.nearZero.cssHeight, 1, "canvas content height must reach 1px");

  await page.evaluate(async () => {
    const canvas = document.querySelector("#scene");
    for (const size of [8, 2, 0, 4, 0]) {
      const value = `${size}px`;
      canvas.style.width = value;
      canvas.style.height = value;
      canvas.style.minWidth = value;
      canvas.style.maxWidth = value;
      canvas.style.minHeight = value;
      canvas.style.maxHeight = value;
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
  });
  await page.waitForFunction(() => {
    const canvas = document.querySelector("#scene");
    return canvas.clientWidth === 0 && canvas.clientHeight === 0;
  });
  diagnostics.snapshots.rapidZero = await waitForFrameAfter(
    page,
    diagnostics.snapshots.nearZero.presentedFrames,
    "rapid resize burst",
  );

  await restoreCanvasSize(page);
  diagnostics.snapshots.restored = await waitForFrameAfter(
    page,
    diagnostics.snapshots.rapidZero.presentedFrames,
    "restored canvas",
  );
  assert.ok(diagnostics.snapshots.restored.cssWidth >= 320, "restored canvas must be visible");
  assert.ok(diagnostics.snapshots.restored.cssHeight >= 180, "restored canvas must be visible");
  assert.equal(
    diagnostics.pageErrors.length,
    0,
    `zero-size transition produced unhandled page errors:\n${diagnostics.pageErrors.join("\n")}`,
  );

  await page.locator("#scene").screenshot({ path: path.join(artifactDir, "restored.png") });
  diagnostics.serverOutput = serverOutput;
  await writeFile(
    path.join(artifactDir, "diagnostics.json"),
    `${JSON.stringify(diagnostics, null, 2)}\n`,
  );
  console.log(
    `playground zero-size recovery ok: ${diagnostics.snapshots.baseline.presentedFrames} -> ` +
      `${diagnostics.snapshots.restored.presentedFrames} frames`,
  );
} catch (error) {
  diagnostics.failure = error instanceof Error ? error.stack ?? error.message : String(error);
  diagnostics.serverOutput = serverOutput;
  if (page !== null) {
    try {
      await page.screenshot({ path: path.join(artifactDir, "failure.png"), fullPage: true });
    } catch (screenshotError) {
      diagnostics.screenshotFailure = String(screenshotError);
    }
  }
  await writeFile(
    path.join(artifactDir, "diagnostics.json"),
    `${JSON.stringify(diagnostics, null, 2)}\n`,
  );
  throw error;
} finally {
  await context?.close().catch(() => {});
  await browser?.close().catch(() => {});
  server.kill("SIGTERM");
}
