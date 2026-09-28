import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { PNG } from "pngjs";
import playwright from "playwright";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { layoutReplayViewport, replayViewport } from "./showcase-viewport.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.env.NOON_GALLERY_SELECTION_PORT ?? 4217);
const base = process.env.NOON_GALLERY_SELECTION_BASE ?? `http://127.0.0.1:${port}/web/`;
const artifacts = path.resolve(
  root,
  process.env.NOON_GALLERY_SELECTION_ARTIFACTS ??
    "browser-smoke-artifacts/gallery-pointer-selection",
);
await mkdir(artifacts, { recursive: true });

let server;
let browser;
let runtimeCache;
const captures = {};
const report = { legacyInteraction: "pointer-fill-selection", authoredInteraction: "click-indicate" };
let failure;

function changedPixels(leftBytes, rightBytes) {
  const left = PNG.sync.read(leftBytes);
  const right = PNG.sync.read(rightBytes);
  assert.equal(left.width, right.width);
  assert.equal(left.height, right.height);
  let changed = 0;
  for (let i = 0; i < left.data.length; i += 4) {
    if (
      left.data[i] !== right.data[i] ||
      left.data[i + 1] !== right.data[i + 1] ||
      left.data[i + 2] !== right.data[i + 2] ||
      left.data[i + 3] !== right.data[i + 3]
    ) {
      changed += 1;
    }
  }
  return changed;
}

async function waitForPresentation(page, previous) {
  await page.waitForFunction(
    () =>
      window.__noonExampleGallery !== undefined &&
      document.querySelector("#patch-status")?.dataset.state !== "error",
  );
  await page.waitForFunction(
    async (prior) => {
      const metrics = await window.__noonExampleGallery.executionMetrics();
      return Number(metrics?.metrics?.presentedFrames ?? 0) > prior;
    },
    previous,
    { timeout: 15000 },
  );
}

async function waitForExactPixels(canvas, baseline, label) {
  let last;
  for (let attempt = 0; attempt < 60; attempt += 1) {
    last = await canvas.screenshot();
    if (changedPixels(baseline, last) === 0) return last;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`${label} did not restore the exact baseline pixels`);
}

async function presentedFrames(page) {
  return page.evaluate(async () =>
    Number((await window.__noonExampleGallery.executionMetrics()).metrics.presentedFrames),
  );
}

async function assertSettled(page, label) {
  const before = await presentedFrames(page);
  await page.waitForTimeout(250);
  assert.equal(await presentedFrames(page), before, `${label} left a frame wake active`);
}

try {
  if (!process.env.NOON_GALLERY_SELECTION_BASE) {
    server = spawn(
      "python3",
      ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", root],
      { stdio: "ignore" },
    );
    let ready = false;
    for (let i = 0; i < 100; i += 1) {
      ready = await fetch(base).then((response) => response.ok).catch(() => false);
      if (ready) break;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    assert.ok(ready, "gallery selection HTTP server did not start");
  }

  const worker = await fetch(new URL("python-worker.js", base));
  assert.ok(worker.ok, "python worker source is unavailable");
  runtimeCache = createPyodideResourceCache(await worker.text());

  browser = await playwright.chromium.launch({
    headless: true,
    args: ["--disable-gpu-sandbox", "--disable-dev-shm-usage"],
  });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  await runtimeCache.install(context);
  const page = await context.newPage();
  page.setDefaultTimeout(30000);
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });

  await page.goto(`${base}?example=noon-pointer-selection`, { waitUntil: "domcontentloaded" });
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  assert.equal(
    await page.evaluate(() => window.__noonExampleGallery.selectedExampleId),
    "noon-pointer-selection",
  );
  const canvas = page.locator("#scene");
  const captureSize = { width: 960, height: 540 };
  // Compare the renderer's pixels, not rounded-corner browser antialiasing.
  // Keep real host pointer events enabled on the existing canvas.
  await layoutReplayViewport(canvas, captureSize);
  await canvas.evaluate(element => element.style.setProperty("pointer-events", "auto", "important"));
  await page.evaluate(() => window.__noonExampleGallery.run());
  await page.waitForFunction(
    () =>
      document.querySelector("#patch-status")?.dataset.state === "applied" &&
      window.__noonExampleGallery.runInFlight === false,
    null,
    { timeout: 60000 },
  );
  assert.equal(
    await page.evaluate(() => document.querySelector("#status")?.dataset.interaction),
    "pointer-fill-selection",
    "gallery manifest must enable selection on the public runtime",
  );

  report.captureViewport = await replayViewport(canvas, captureSize);
  const box = await canvas.boundingBox();
  assert.ok(box && box.width > 0 && box.height > 0, "gallery canvas is not drawable");
  const baseline = await canvas.screenshot();
  captures.baseline = baseline;

  const beforeSelect = await page.evaluate(async () =>
    Number((await window.__noonExampleGallery.executionMetrics()).metrics.presentedFrames),
  );
  await page.mouse.click(
    box.x + box.width / 2 - box.height / 4,
    box.y + box.height / 2,
  );
  await waitForPresentation(page, beforeSelect);
  const selected = await canvas.screenshot();
  captures.selected = selected;
  const selectedChanged = changedPixels(baseline, selected);
  report.selectedChanged = selectedChanged;
  assert.ok(selectedChanged > 500, `selection changed only ${selectedChanged} pixels`);

  const beforeClear = await page.evaluate(async () =>
    Number((await window.__noonExampleGallery.executionMetrics()).metrics.presentedFrames),
  );
  await page.mouse.click(box.x + 18, box.y + 18);
  await waitForPresentation(page, beforeClear);
  const cleared = await canvas.screenshot();
  captures.cleared = cleared;
  const clearDifference = changedPixels(baseline, cleared);
  report.clearDifference = clearDifference;
  assert.equal(clearDifference, 0, "background clear must restore the authored image exactly");
  assert.deepEqual(errors, []);

  report.legacyRenderer = await page.evaluate(() => document.querySelector("#status")?.dataset.rendererBackend);

  // The curated lesson carries no manifest interaction policy. Its Rust-owned
  // click declaration must still work after the authored introduction has
  // finished and replay has been paused at that endpoint.
  await page.goto(`${base}?catalog=showcase&example=showcase-pointer-selection`, { waitUntil: "domcontentloaded" });
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  assert.equal(
    await page.evaluate(() => window.__noonExampleGallery.selectedExampleId),
    "showcase-pointer-selection",
  );
  assert.equal(
    await page.evaluate(() => document.querySelector("#status")?.dataset.interaction),
    "none",
    "source-declared click actions must not require manifest interaction policy",
  );
  const source = await fetch(new URL("python/examples/showcase_pointer_selection.py", base)).then((response) => response.text());
  assert.match(source, /\.on_click\s*\(/, "showcase source must declare its click action");
  const authoredCanvas = page.locator("#scene");
  await layoutReplayViewport(authoredCanvas, captureSize);
  await authoredCanvas.evaluate(element => element.style.setProperty("pointer-events", "auto", "important"));
  await page.evaluate(() => window.__noonExampleGallery.run());
  await page.waitForFunction(
    () => document.querySelector("#patch-status")?.dataset.state === "applied" && !window.__noonExampleGallery.runInFlight,
    null,
    { timeout: 60000 },
  );
  const pause = page.getByRole("button", { name: "Pause animation", exact: true });
  if (await pause.count()) await pause.click();
  const scrubber = page.locator(".playback-scrubber");
  const endpoint = Number(await scrubber.getAttribute("max"));
  await scrubber.evaluate((input, time) => {
    input.value = String(time);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }, endpoint);
  await page.waitForFunction((time) => {
    const controls = document.querySelector(".playback-controls");
    return controls?.dataset.busy === "false" &&
      controls.querySelector(".playback-toggle")?.getAttribute("aria-label") === "Play animation" &&
      Math.abs(Number(controls.dataset.elapsedSeconds) - time) < 1e-7;
  }, endpoint);
  const authoredBox = await authoredCanvas.boundingBox();
  assert.ok(authoredBox && authoredBox.width > 0 && authoredBox.height > 0, "authored canvas is not drawable");
  const authoredBaseline = await authoredCanvas.screenshot();
  captures.authoredBaseline = authoredBaseline;
  const clickCircle = () => page.mouse.click(
    authoredBox.x + authoredBox.width * 0.36,
    authoredBox.y + authoredBox.height * 0.5,
  );
  const waitChanged = async (baseline, label) => {
    for (let attempt = 0; attempt < 40; attempt += 1) {
      const image = await authoredCanvas.screenshot();
      if (changedPixels(baseline, image) > 500) return image;
      await page.waitForTimeout(25);
    }
    throw new Error(`${label} did not change trusted canvas pixels`);
  };
  await clickCircle();
  const indicated = await waitChanged(authoredBaseline, "first authored click");
  captures.indicated = indicated;
  report.indicatedChanged = changedPixels(authoredBaseline, indicated);
  const restored = await waitForExactPixels(authoredCanvas, authoredBaseline, "first authored click");
  captures.restored = restored;
  await assertSettled(page, "first authored click");
  await clickCircle();
  const repeated = await waitChanged(authoredBaseline, "repeated authored click");
  captures.repeated = repeated;
  await waitForExactPixels(authoredCanvas, authoredBaseline, "repeated authored click");
  await assertSettled(page, "repeated authored click");
  const beforeBackground = await presentedFrames(page);
  await page.mouse.click(authoredBox.x + 12, authoredBox.y + 12);
  await page.waitForTimeout(150);
  assert.equal(changedPixels(authoredBaseline, await authoredCanvas.screenshot()), 0, "background click must not change the source-declared scene");
  assert.equal(await presentedFrames(page), beforeBackground, "background click must not create interaction work");
  report.authoredRenderer = await page.evaluate(() => document.querySelector("#status")?.dataset.rendererBackend);
} catch (error) {
  failure = error;
  throw error;
} finally {
  // Persist the original captures after the verdict, including failed/partial
  // runs. Do not insert artifact I/O, extra captures or waits into the sample path.
  try {
    const writes = await Promise.allSettled([
      ...Object.entries(captures).map(([name, bytes]) =>
        writeFile(path.join(artifacts, `${name}.png`), bytes),
      ),
      writeFile(
        path.join(artifacts, "result.json"),
        `${JSON.stringify({ ...report, error: failure ? String(failure) : null }, null, 2)}\n`,
      ),
    ]);
    const rejected = writes.find((write) => write.status === "rejected");
    if (rejected) {
      if (!failure) throw rejected.reason;
      // Failure to retain diagnostics must not hide the original assertion.
      console.error("Could not retain gallery selection diagnostics:", rejected.reason);
    }
  } finally {
    try {
      await browser?.close();
    } finally {
      server?.kill("SIGTERM");
    }
  }
}
console.log(
  `Gallery pointer interactions passed: legacy=${report.selectedChanged}, authored=${report.indicatedChanged}`,
);
