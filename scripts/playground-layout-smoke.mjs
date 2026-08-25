import assert from "node:assert/strict";
import { spawn } from "node:child_process";

import playwright from "playwright";

const { chromium } = playwright;
const port = Number(process.env.NOON_PLAYGROUND_LAYOUT_PORT ?? "4174");
const baseUrl = `http://127.0.0.1:${port}`;
const desktopMaxCanvasWidth = 44 * 16;
const deviceScaleFactor = 1.25;
const expectedRendererBackend = "WebGL2";

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

async function settleLayout(page) {
  await page.evaluate(
    () =>
      new Promise((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(resolve));
      }),
  );
}

async function layout(page) {
  const canvas = await page.locator("#scene").boundingBox();
  const wrap = await page.locator(".canvas-wrap").boundingBox();
  assert.ok(canvas, "playground canvas must be laid out");
  assert.ok(wrap, "canvas wrapper must be laid out");
  const browserState = await page.evaluate(() => {
    const scene = document.querySelector("#scene");
    const status = document.querySelector("#status");
    return {
      documentWidth: document.documentElement.scrollWidth,
      viewportWidth: window.innerWidth,
      clientWidth: scene.clientWidth,
      clientHeight: scene.clientHeight,
      backingWidth: scene.width,
      backingHeight: scene.height,
      devicePixelRatio: window.devicePixelRatio,
      rendererBackend: status.dataset.rendererBackend ?? null,
    };
  });
  return { canvas, wrap, ...browserState };
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

function assertInitialBackingStore(result, label) {
  assert.equal(
    result.rendererBackend,
    expectedRendererBackend,
    `${label}: live playground must actually initialize the WebGL2 renderer`,
  );
  assert.equal(
    result.devicePixelRatio,
    deviceScaleFactor,
    `${label}: Chromium must run at the requested fractional DPR`,
  );
  const expectedWidth = Math.max(1, Math.round(result.clientWidth * result.devicePixelRatio));
  const expectedHeight = Math.max(1, Math.round(result.clientHeight * result.devicePixelRatio));
  assert.equal(
    result.backingWidth,
    expectedWidth,
    `${label}: backing width must match CSS content width × DPR before WebGL surface creation`,
  );
  assert.equal(
    result.backingHeight,
    expectedHeight,
    `${label}: backing height must match CSS content height × DPR before WebGL surface creation`,
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
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor,
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
    (backend) => document.querySelector("#status")?.dataset.rendererBackend === backend,
    expectedRendererBackend,
    { timeout: 30_000 },
  );
  await settleLayout(page);

  const desktop = await layout(page);
  assertInitialBackingStore(desktop, "desktop WebGL");
  assert.ok(
    desktop.canvas.width <= desktopMaxCanvasWidth + 1,
    `desktop: canvas width ${desktop.canvas.width}px exceeds ${desktopMaxCanvasWidth}px cap`,
  );
  assertAspect(desktop.canvas, 16 / 9, "desktop");
  assertCentered(desktop.canvas, desktop.wrap, "desktop");
  assertNoOverflow(desktop, "desktop");

  await page.setViewportSize({ width: 900, height: 800 });
  await settleLayout(page);
  const stacked = await layout(page);
  assert.equal(stacked.rendererBackend, expectedRendererBackend, "stacked: renderer backend drifted");
  assert.ok(
    stacked.canvas.width <= desktopMaxCanvasWidth + 1,
    `stacked: canvas width ${stacked.canvas.width}px exceeds ${desktopMaxCanvasWidth}px cap`,
  );
  assertAspect(stacked.canvas, 16 / 9, "stacked");
  assertCentered(stacked.canvas, stacked.wrap, "stacked");
  assertNoOverflow(stacked, "stacked");

  await page.setViewportSize({ width: 390, height: 844 });
  await settleLayout(page);
  const mobile = await layout(page);
  assert.equal(mobile.rendererBackend, expectedRendererBackend, "mobile: renderer backend drifted");
  assertAspect(mobile.canvas, 4 / 3, "mobile");
  assertCentered(mobile.canvas, mobile.wrap, "mobile");
  assertNoOverflow(mobile, "mobile");

  assert.deepEqual(browserErrors, [], `live playground emitted browser errors:\n${browserErrors.join("\n")}`);

  console.log(
    `✓ live WebGL playground viewport @ ${deviceScaleFactor}x DPR: ` +
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
