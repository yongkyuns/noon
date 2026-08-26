import assert from "node:assert/strict";
import { spawn } from "node:child_process";

import playwright from "playwright";

const { chromium } = playwright;
const port = Number(process.env.NOON_PLAYGROUND_LAYOUT_PORT ?? "4174");
const baseUrl = `http://127.0.0.1:${port}`;
const desktopMaxCanvasWidth = 44 * 16;
const deviceScaleFactor = 1.25;

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
      if (response.ok) {
        return;
      }
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Playground layout server did not start: ${lastError}\n${serverOutput}`);
}

async function layout(page) {
  const canvas = await page.locator("#scene").boundingBox();
  const wrap = await page.locator(".canvas-wrap").boundingBox();
  assert.ok(canvas, "playground canvas must be laid out");
  assert.ok(wrap, "canvas wrapper must be laid out");
  const documentWidth = await page.evaluate(() => document.documentElement.scrollWidth);
  const viewportWidth = await page.evaluate(() => window.innerWidth);
  return { canvas, wrap, documentWidth, viewportWidth };
}

function assertCentered(canvas, wrap, label) {
  const canvasCenter = canvas.x + canvas.width / 2;
  const wrapCenter = wrap.x + wrap.width / 2;
  assert.ok(
    Math.abs(canvasCenter - wrapCenter) <= 2,
    `${label}: canvas must stay centered in preview (${canvasCenter} vs ${wrapCenter})`,
  );
}

function assertAspect(canvas, expected, label) {
  const actual = canvas.width / canvas.height;
  assert.ok(
    Math.abs(actual - expected) <= 0.02,
    `${label}: expected aspect ${expected.toFixed(3)}, got ${actual.toFixed(3)}`,
  );
}

function assertNoOverflow(result, label) {
  assert.ok(
    result.documentWidth <= result.viewportWidth + 1,
    `${label}: page overflowed horizontally (${result.documentWidth}px > ${result.viewportWidth}px)`,
  );
  assert.ok(
    result.canvas.width <= result.wrap.width + 1,
    `${label}: canvas overflowed its wrapper`,
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
  const context = await browser.newContext({
    deviceScaleFactor,
    viewport: { width: 1440, height: 900 },
  });
  const page = await context.newPage();
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") {
      browserErrors.push(`console: ${message.text()}`);
    }
  });
  await page.goto(`${baseUrl}/web/index.html`, { waitUntil: "load" });
  await page.waitForFunction(
    () => document.querySelector("#status")?.dataset.rendererBackend === "WebGL2",
    null,
    { timeout: 30_000 },
  );

  const initialBacking = await page.evaluate(() => {
    const canvas = document.querySelector("#scene");
    return {
      backend: document.querySelector("#status")?.dataset.rendererBackend,
      backingWidth: canvas.width,
      backingHeight: canvas.height,
      cssWidth: canvas.clientWidth,
      cssHeight: canvas.clientHeight,
      devicePixelRatio: window.devicePixelRatio,
    };
  });
  assert.equal(initialBacking.backend, "WebGL2", "playground must exercise the WebGL2 fallback");
  assert.equal(initialBacking.devicePixelRatio, deviceScaleFactor);
  assert.equal(
    initialBacking.backingWidth,
    Math.max(1, Math.round(initialBacking.cssWidth * deviceScaleFactor)),
    "initial offscreen backing width must match laid-out CSS width × DPR before transfer",
  );
  assert.equal(
    initialBacking.backingHeight,
    Math.max(1, Math.round(initialBacking.cssHeight * deviceScaleFactor)),
    "initial offscreen backing height must match laid-out CSS height × DPR before transfer",
  );

  const desktop = await layout(page);
  assert.ok(
    desktop.canvas.width <= desktopMaxCanvasWidth + 1,
    `desktop: canvas width ${desktop.canvas.width}px exceeds ${desktopMaxCanvasWidth}px cap`,
  );
  assertAspect(desktop.canvas, 16 / 9, "desktop");
  assertCentered(desktop.canvas, desktop.wrap, "desktop");
  assertNoOverflow(desktop, "desktop");

  await page.setViewportSize({ width: 900, height: 800 });
  const stacked = await layout(page);
  assert.ok(
    stacked.canvas.width <= desktopMaxCanvasWidth + 1,
    `stacked: canvas width ${stacked.canvas.width}px exceeds ${desktopMaxCanvasWidth}px cap`,
  );
  assertAspect(stacked.canvas, 16 / 9, "stacked");
  assertCentered(stacked.canvas, stacked.wrap, "stacked");
  assertNoOverflow(stacked, "stacked");

  await page.setViewportSize({ width: 390, height: 844 });
  const mobile = await layout(page);
  assertAspect(mobile.canvas, 4 / 3, "mobile");
  assertCentered(mobile.canvas, mobile.wrap, "mobile");
  assertNoOverflow(mobile, "mobile");

  assert.deepEqual(browserErrors, [], `playground emitted browser errors:\n${browserErrors.join("\n")}`);
  console.log(
    `✓ playground WebGL2 viewport @ DPR ${deviceScaleFactor}: initial backing ` +
      `${initialBacking.backingWidth}×${initialBacking.backingHeight}, ` +
      `desktop ${desktop.canvas.width.toFixed(0)}×${desktop.canvas.height.toFixed(0)}, ` +
      `stacked ${stacked.canvas.width.toFixed(0)}×${stacked.canvas.height.toFixed(0)}, ` +
      `mobile ${mobile.canvas.width.toFixed(0)}×${mobile.canvas.height.toFixed(0)}`,
  );
} finally {
  if (browser !== null) {
    await browser.close();
  }
  server.kill("SIGTERM");
}
