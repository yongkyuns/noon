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
      if (response.ok) return;
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
  assert.ok(result.canvas.width <= result.wrap.width + 1, `${label}: canvas overflowed its wrapper`);
}

async function waitForAuthoredScene(page, expectedId, browserErrors) {
  await page.waitForFunction(
    (id) => {
      const patch = document.querySelector("#patch-status");
      const selected = document.querySelector(".example-card[aria-selected='true']")?.dataset.exampleId;
      return selected === id && (patch?.dataset.state === "applied" || patch?.dataset.state === "error");
    },
    expectedId,
    { timeout: 60_000 },
  );
  const result = await page.evaluate(() => ({
    state: document.querySelector("#patch-status")?.dataset.state,
    text:
      document.querySelector("#patch-status")?.value ??
      document.querySelector("#patch-status")?.textContent ??
      "",
  }));
  assert.equal(
    result.state,
    "applied",
    `${expectedId}: initial authoring failed: ${result.text}\n${browserErrors.join("\n")}`,
  );
}

async function sceneSource(page) {
  return page.evaluate(() => document.querySelector("#python-scene-source")?.value ?? "");
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
    if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/index.html?example=parity-square-and-circle`, {
    waitUntil: "load",
  });
  await page.waitForFunction(
    () =>
      document.querySelector("#status")?.dataset.rendererBackend === "WebGL2" &&
      document.querySelector(".example-card[aria-selected='true']")?.dataset.exampleId ===
        "parity-square-and-circle",
    null,
    { timeout: 30_000 },
  );
  await waitForAuthoredScene(page, "parity-square-and-circle", browserErrors);
  await page.waitForFunction(
    () =>
      ["metric-objects", "metric-draws", "metric-upload", "metric-time"].every((id) => {
        const metric = document.querySelector(`#${id}`);
        const value = metric?.value ?? metric?.textContent ?? "";
        return value !== "" && value !== "—";
      }),
    null,
    { timeout: 10_000 },
  );

  const galleryContract = await page.evaluate(() => ({
    cards: document.querySelectorAll(".example-card").length,
    exampleIds: [...document.querySelectorAll(".example-card")].map((card) => card.dataset.exampleId),
    canvases: document.querySelectorAll("canvas").length,
    selected: document.querySelector(".example-card[aria-selected='true']")?.dataset.exampleId,
    patchHidden: document.querySelector("#patch-tab")?.hidden,
    thumbnails: [...document.querySelectorAll(".example-thumb")].map((image) => image.getAttribute("src")),
    href: location.href,
  }));
  assert.ok(galleryContract.cards > 0, "gallery must render ready Manim parity examples");
  assert.equal(
    galleryContract.exampleIds.includes("parity-focus-on-point"),
    false,
    "Noon-specific FocusOn probe must stay out of the public exact-source gallery",
  );
  assert.ok(
    galleryContract.exampleIds.includes("parity-draw-border-then-fill-styled-square"),
    "gallery must retain literal upstream parity-qualified examples",
  );
  assert.equal(galleryContract.canvases, 1, "thumbnail gallery must keep exactly one live canvas");
  assert.equal(galleryContract.selected, "parity-square-and-circle");
  assert.equal(galleryContract.patchHidden, true, "Noon-native patch examples must not be public examples");
  assert.ok(galleryContract.thumbnails.every((src) => src?.includes("thumbnails/manim/")));
  assert.match(galleryContract.href, /example=parity-square-and-circle/);

  const presentationContract = await page.evaluate(() => {
    const workspace = document.querySelector(".workspace");
    const editor = document.querySelector(".editor-pane");
    const preview = document.querySelector(".preview-pane");
    const selected = document.querySelector(".selected-example");
    const workspaceRect = workspace.getBoundingClientRect();
    const editorRect = editor.getBoundingClientRect();
    const previewRect = preview.getBoundingClientRect();
    const metricLabels = [...document.querySelectorAll(".metric-label")].map(
      (label) => label.textContent ?? "",
    );
    const metricValues = [...document.querySelectorAll(".metric-value")].map(
      (metric) => metric.value ?? metric.textContent ?? "",
    );
    const realMetricIds = ["metric-objects", "metric-draws", "metric-upload", "metric-time"];
    return {
      selectedOutsideWorkspace: selected !== null && !workspace.contains(selected),
      selectedImmediatelyBeforeWorkspace: selected?.nextElementSibling === workspace,
      obsoletePanels: document.querySelectorAll(".below, .info-panel, .pipeline, .api-list").length,
      placeholderPerformancePanels: document.querySelectorAll(".perf-metrics").length,
      percentileLabels: metricLabels.filter((label) => /\bp(?:50|95)\b/i.test(label)).length,
      fakeTimingValues: metricValues.filter((value) => /^(?:engine|render) worker$/i.test(value)).length,
      realMetricsPresent: realMetricIds.every((id) => document.querySelector(`#${id}`) !== null),
      realMetricsPopulated: realMetricIds.every((id) => {
        const metric = document.querySelector(`#${id}`);
        const value = metric?.value ?? metric?.textContent ?? "";
        return value !== "" && value !== "—";
      }),
      editorTop: editorRect.top,
      previewTop: previewRect.top,
      editorShare: editorRect.width / workspaceRect.width,
    };
  });
  assert.equal(
    presentationContract.selectedOutsideWorkspace,
    true,
    "selected-example context must not offset only the editor pane",
  );
  assert.equal(
    presentationContract.selectedImmediatelyBeforeWorkspace,
    true,
    "selected-example context must sit directly above the shared source/preview workspace",
  );
  assert.equal(
    presentationContract.obsoletePanels,
    0,
    "obsolete architecture/API presentation panes must not be rendered",
  );
  assert.equal(
    presentationContract.placeholderPerformancePanels,
    0,
    "playground must not expose a placeholder frame-performance panel",
  );
  assert.equal(
    presentationContract.percentileLabels,
    0,
    "playground must not claim p50/p95 telemetry without measured percentiles",
  );
  assert.equal(
    presentationContract.fakeTimingValues,
    0,
    "worker ownership strings must not be presented as timing values",
  );
  assert.equal(presentationContract.realMetricsPresent, true, "real scene metrics must remain visible");
  assert.equal(presentationContract.realMetricsPopulated, true, "real scene metrics must keep updating");
  assert.ok(
    Math.abs(presentationContract.editorTop - presentationContract.previewTop) <= 1,
    `desktop source/preview panes must align at the top (${presentationContract.editorTop} vs ${presentationContract.previewTop})`,
  );
  assert.ok(
    presentationContract.editorShare >= 0.42 && presentationContract.editorShare <= 0.5,
    `desktop editor should receive a balanced workspace share, got ${presentationContract.editorShare}`,
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

  await page.locator(".gallery-controls input[type='search']").fill("DifferentRotations");
  await page.waitForFunction(() => document.querySelectorAll(".example-card").length === 1);
  assert.equal(
    await page.locator(".example-card").getAttribute("data-example-id"),
    "parity-different-rotations",
  );
  await page.locator(".example-card").click();
  await waitForAuthoredScene(page, "parity-different-rotations", browserErrors);
  assert.match(page.url(), /example=parity-different-rotations/);
  assert.match(await sceneSource(page), /Rotate\(right_square/);

  await page.waitForSelector("#scene-editor-panel .python-code-editor[data-editor-ready='true']");
  await page.locator("#scene-editor-panel .cm-content").fill("# local draft\n");
  assert.equal(await page.locator(".reset-example").isDisabled(), false);
  await page.locator(".reset-example").click();
  assert.match(await sceneSource(page), /from noon import \*/);

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
    `✓ Manim gallery + aligned WebGL2 viewport @ DPR ${deviceScaleFactor}: ` +
      `${galleryContract.cards} cards, desktop ${desktop.canvas.width.toFixed(0)}×${desktop.canvas.height.toFixed(0)}, ` +
      `mobile ${mobile.canvas.width.toFixed(0)}×${mobile.canvas.height.toFixed(0)}`,
  );
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
