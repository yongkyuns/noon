import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { PNG } from "pngjs";
import playwright from "playwright";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";

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

  const canvas = page.locator("#scene");
  const box = await canvas.boundingBox();
  assert.ok(box && box.width > 0 && box.height > 0, "gallery canvas is not drawable");
  const baseline = await canvas.screenshot();

  const beforeSelect = await page.evaluate(async () =>
    Number((await window.__noonExampleGallery.executionMetrics()).metrics.presentedFrames),
  );
  await page.mouse.click(
    box.x + box.width / 2 - box.height / 4,
    box.y + box.height / 2,
  );
  await waitForPresentation(page, beforeSelect);
  const selected = await canvas.screenshot();
  const selectedChanged = changedPixels(baseline, selected);
  assert.ok(selectedChanged > 500, `selection changed only ${selectedChanged} pixels`);

  const beforeClear = await page.evaluate(async () =>
    Number((await window.__noonExampleGallery.executionMetrics()).metrics.presentedFrames),
  );
  await page.mouse.click(box.x + 18, box.y + 18);
  await waitForPresentation(page, beforeClear);
  const cleared = await canvas.screenshot();
  const clearDifference = changedPixels(baseline, cleared);
  assert.equal(clearDifference, 0, "background clear must restore the authored image exactly");
  assert.deepEqual(errors, []);

  const report = {
    selectedChanged,
    clearDifference,
    renderer: await page.evaluate(() => document.querySelector("#status")?.dataset.rendererBackend),
    interaction: "pointer-fill-selection",
  };
  await writeFile(
    path.join(artifacts, "result.json"),
    `${JSON.stringify(report, null, 2)}\n`,
  );
  await writeFile(path.join(artifacts, "baseline.png"), baseline);
  await writeFile(path.join(artifacts, "selected.png"), selected);
  await writeFile(path.join(artifacts, "cleared.png"), cleared);
  console.log(
    `Gallery pointer selection passed: selectedChanged=${selectedChanged}, clearDifference=${clearDifference}`,
  );
} finally {
  await browser?.close();
  server?.kill("SIGTERM");
}
