import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { promisify } from "node:util";
import { chromium } from "playwright";
import pngjs from "pngjs";

import { FrameArtifactStore } from "../../../scripts/agent-preview-artifacts.mjs";
import { runPreviewCli } from "../src/preview-cli.mjs";

const { PNG } = pngjs;
const execFileAsync = promisify(execFile);
const repoRoot = fileURLToPath(new URL("../../../", import.meta.url));
const cli = fileURLToPath(new URL("../bin/noon-feedback.mjs", import.meta.url));
const artifacts = path.resolve(process.env.NOON_FEEDBACK_REVIEW_ARTIFACTS ??
  path.join(repoRoot, "browser-smoke-artifacts/feedback-review"));
await mkdir(artifacts, { recursive: true });
const root = await mkdtemp(path.join(artifacts, "run-"));
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const SOURCE = "# Synthetic UI fixture; this source was not rendered.\nfrom noon import *\n";
const BUILD = {
  engineRevision: "a".repeat(40), wasmSha256: "b".repeat(64), workerSha256: "c".repeat(64),
  buildId: "fixture-only </script><script>globalThis.__feedbackInjected=true</script>",
};
const NOTE = {
  observation: "The blue contribution is difficult to distinguish from the displayed sum.",
  desiredChange: "Keep the contribution visible while changing only the sum.",
  preserve: "Preserve input magnitudes, labels, scale, and accepted narration.",
};

// Raster fixtures exercise image fitting and review interactions only. They are
// not Noon frames, a rendered educational example, or playback/audio evidence.
function fixturePng(width, height, ordinal) {
  const image = new PNG({ width, height });
  const blueColor = [42, 107 + ordinal * 8, 218], amberColor = [228, 162, 51], background = [235, 241, 248];
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const index = (y * width + x) * 4;
      const blue = x >= width * 0.18 && x < width * 0.46 && y >= height * 0.2 && y < height * 0.8;
      const amber = x >= width * 0.58 && x < width * 0.84 && y >= height * 0.35 && y < height * 0.65;
      const color = blue ? blueColor : amber ? amberColor : background;
      image.data[index] = color[0]; image.data[index + 1] = color[1];
      image.data[index + 2] = color[2]; image.data[index + 3] = 255;
    }
  }
  return PNG.sync.write(image);
}

function fixtureService() {
  const images = [fixturePng(800, 600, 0), fixturePng(800, 450, 1), fixturePng(600, 800, 2)];
  const store = new FrameArtifactStore();
  const owner = Object.freeze({});
  const sessionId = "feedback-browser-fixture-run";
  let scope;
  let ordinal = 0;
  const put = (time) => {
    const png = images[ordinal++];
    const artifact = scope.putFrame({
      png, requestedTime: time, publishedTime: time,
      backend: "synthetic-ui-fixture", sceneRevision: null, frameRevision: null,
    });
    return { artifact, snapshot: {
      state: "ready", frame: { requestedTime: time, publishedTime: time, rendererBackend: "synthetic-ui-fixture" },
      buildIdentity: {
        sourceRevision: BUILD.engineRevision, buildId: BUILD.buildId,
        files: { wasm: { sha256: BUILD.wasmSha256 }, worker: { sha256: BUILD.workerSha256 } },
      },
      image: {
        mimeType: artifact.mimeType, width: artifact.width, height: artifact.height,
        encodedBytes: artifact.byteLength, sha256: artifact.sha256,
      },
    } };
  };
  return {
    openScope: () => owner,
    async open(actualOwner, source) {
      assert.equal(actualOwner, owner);
      scope = store.openScope({ sessionId, sourceSha256: hash(source), build: BUILD });
      return { sessionId, ...put(0) };
    },
    async sampleFrames(actualOwner, actualSession, times) {
      assert.equal(actualOwner, owner); assert.equal(actualSession, sessionId);
      return times.map(put);
    },
    getArtifact(actualOwner, actualSession, id) {
      assert.equal(actualOwner, owner); assert.equal(actualSession, sessionId);
      return scope.getFrame(id);
    },
    async close() { scope.close(); return { state: "closed" }; },
    async closeScope() { return []; },
    async dispose() { store.dispose(); return []; },
  };
}

async function feedbackCommand(args) {
  const { stdout } = await execFileAsync(process.execPath, [cli, ...args], {
    cwd: root, env: {}, timeout: 15_000, maxBuffer: 1024 * 1024,
  });
  return JSON.parse(stdout.trim());
}

async function selectFrame(page, index) {
  await page.locator(`#frame-list [data-sample-index="${index}"]`).click();
  await page.waitForFunction((sampleIndex) => {
    const image = document.querySelector("#frame-image");
    const filename = `frame-${String(sampleIndex).padStart(3, "0")}.png`;
    return image.complete && image.naturalWidth > 0 && new URL(image.currentSrc).pathname.endsWith(filename) &&
      document.querySelector("#image-status").dataset.state === "ready";
  }, index);
}

async function paintedBounds(page) {
  return page.locator("#frame-image").evaluate((image) => {
    const box = image.getBoundingClientRect();
    const scale = Math.min(box.width / image.naturalWidth, box.height / image.naturalHeight);
    const width = image.naturalWidth * scale, height = image.naturalHeight * scale;
    return {
      x: box.x + (box.width - width) / 2, y: box.y + (box.height - height) / 2,
      width, height, box: { x: box.x, y: box.y, width: box.width, height: box.height },
    };
  });
}

async function enableDrawing(page) {
  if (await page.locator("#mark-region").getAttribute("aria-pressed") !== "true") {
    await page.locator("#mark-region").click();
  }
  await page.locator("#frame-stage").scrollIntoViewIfNeeded();
  return paintedBounds(page);
}

async function numericRegion(page) {
  const result = {};
  for (const key of ["x", "y", "width", "height"]) result[key] = Number(await page.locator(`#region-${key}`).inputValue()) / 100;
  return result;
}

function assertRegion(actual, expected, label) {
  for (const key of ["x", "y", "width", "height"]) {
    assert.ok(Math.abs(actual[key] - expected[key]) < 0.006, `${label}: ${key} ${actual[key]} != ${expected[key]}`);
  }
}

async function assertOverlay(page, region, label) {
  // Allow the normal resize/ResizeObserver presentation work to settle before
  // comparing actual CSS geometry with the exported normalized rectangle.
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const image = await paintedBounds(page);
  const overlay = await page.locator("#region-overlay").boundingBox();
  assert.ok(overlay, `${label}: marked region must be visible`);
  const expected = {
    x: image.x + region.x * image.width, y: image.y + region.y * image.height,
    width: region.width * image.width, height: region.height * image.height,
  };
  for (const key of ["x", "y", "width", "height"]) {
    assert.ok(Math.abs(overlay[key] - expected[key]) <= 1.5,
      `${label}: visible ${key} ${overlay[key]} does not match captured-image region ${expected[key]}`);
  }
}

async function downloadNote(page, filename) {
  const downloaded = page.waitForEvent("download");
  await page.locator("#download-note").click();
  const download = await downloaded;
  assert.equal(download.suggestedFilename(), "noon-feedback-note.json");
  const notePath = path.join(root, filename);
  await download.saveAs(notePath);
  return { notePath, note: JSON.parse(await readFile(notePath, "utf8")) };
}

async function enterNote(page) {
  await page.locator("#observation").fill(NOTE.observation);
  await page.locator("#desired-change").fill(NOTE.desiredChange);
  await page.locator("#preserve").fill(NOTE.preserve);
}

let browser, page;
let stage = "prepare fixture";
const screenshots = {};
const report = {
  fixtureOnly: true, sceneExecuted: false, playbackQualified: false, audioQualified: false,
  browser: "chromium", inputDirectory: root,
};
try {
  await writeFile(path.join(root, "fixture_scene.py"), SOURCE);
  const preview = await runPreviewCli({
    cwd: root,
    argv: ["--source", "fixture_scene.py", "--output", "preview", "--time", "1.5", "--time", "3"],
    serviceFactory: fixtureService,
  });
  const review = await feedbackCommand([
    "review", "--preview", preview.manifestPath, "--source", "fixture_scene.py", "--output", "review",
  ]);
  stage = "launch browser";
  browser = await chromium.launch({
    headless: true,
    ...(process.env.NOON_BROWSER_EXECUTABLE ? { executablePath: process.env.NOON_BROWSER_EXECUTABLE } : {}),
  });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, acceptDownloads: true });
  await context.setOffline(true);
  const networkRequests = [], errors = [];
  context.on("request", (request) => { if (/^https?:/.test(request.url())) networkRequests.push(request.url()); });
  page = await context.newPage();
  page.on("pageerror", (error) => errors.push(String(error)));
  stage = "open offline review";
  await page.goto(pathToFileURL(review.indexPath).href);
  await selectFrame(page, 1);
  assert.equal(await page.evaluate(() => globalThis.__feedbackInjected), undefined, "embedded evidence must remain data");
  assert.equal(await page.locator("#interval-start").inputValue(), "1.5");
  assert.equal(await page.locator("#interval-end").inputValue(), "1.5");
  await enterNote(page);
  await page.locator("#interval-start").fill("1");
  await page.locator("#interval-end").fill("2");

  stage = "reverse drag and fitted image";
  let bounds = await enableDrawing(page);
  await page.mouse.move(bounds.x + bounds.width * 0.8, bounds.y + bounds.height * 0.8);
  await page.mouse.down();
  await page.mouse.move(bounds.x + bounds.width * 0.2, bounds.y + bounds.height * 0.2, { steps: 5 });
  await page.mouse.up();
  const committed = { x: 0.2, y: 0.2, width: 0.6, height: 0.6 };
  assertRegion(await numericRegion(page), committed, "reverse drag");
  assert.equal(await page.locator("#region-enabled").isChecked(), true);
  await assertOverlay(page, committed, "desktop reverse drag");

  bounds = await enableDrawing(page);
  const horizontalGap = bounds.x - bounds.box.x;
  const verticalGap = bounds.y - bounds.box.y;
  assert.ok(horizontalGap > 4 || verticalGap > 4, "fixture must actually exercise letterboxing");
  const letterbox = horizontalGap > 4
    ? { x: bounds.box.x + horizontalGap / 2, y: bounds.y + bounds.height / 2 }
    : { x: bounds.x + bounds.width / 2, y: bounds.box.y + verticalGap / 2 };
  await page.mouse.move(letterbox.x, letterbox.y);
  await page.mouse.down();
  await page.mouse.move(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2, { steps: 4 });
  await page.mouse.up();
  assertRegion(await numericRegion(page), committed, "letterbox drag must not replace selection");
  await page.keyboard.press("Escape");

  stage = "cancel unfinished selection";
  bounds = await enableDrawing(page);
  await page.mouse.move(bounds.x + bounds.width * 0.05, bounds.y + bounds.height * 0.05);
  await page.mouse.down();
  await page.mouse.move(bounds.x + bounds.width * 0.5, bounds.y + bounds.height * 0.5, { steps: 3 });
  await page.keyboard.press("Escape");
  await page.mouse.up();
  assertRegion(await numericRegion(page), committed, "Escape restores committed selection");

  stage = "clamped boundary selection";
  bounds = await enableDrawing(page);
  await page.mouse.move(bounds.x + bounds.width / 24, bounds.y + bounds.height * 0.25);
  await page.mouse.down();
  await page.mouse.move(bounds.x + bounds.width + 20, bounds.y + bounds.height * 0.75, { steps: 5 });
  await page.mouse.up();
  const boundaryRegion = { x: 1 / 24, y: 0.25, width: 23 / 24, height: 0.5 };
  assertRegion(await numericRegion(page), boundaryRegion, "drag beyond image edge clamps to boundary");
  assert.equal(await page.locator("#download-note").isDisabled(), false,
    "a region that reaches the right boundary must remain a valid note");
  const boundaryDownload = await downloadNote(page, "boundary-note.json");
  const exportedBoundary = boundaryDownload.note.feedback.region;
  assert.ok(exportedBoundary.x + exportedBoundary.width <= 1,
    "percentage conversion must not export a rectangle beyond the right boundary");
  assertRegion(exportedBoundary, boundaryRegion, "exported boundary rectangle");
  await assertOverlay(page, exportedBoundary, "visible boundary rectangle matches exported note");
  const boundaryImported = await feedbackCommand([
    "import", "--review", review.outputDir, "--note", boundaryDownload.notePath, "--output", "boundary-feedback",
  ]);
  assert.equal(boundaryImported.feedback.status, "pending_review");
  assert.deepEqual(boundaryImported.feedback.feedback.region, exportedBoundary);

  stage = "keyboard region and per-frame drafts";
  await page.locator("#clear-region").click();
  assert.equal(await page.locator("#region-enabled").isChecked(), false);
  await page.locator("#region-enabled").focus();
  await page.keyboard.press("Space");
  const keyboardRegion = { x: 0.1, y: 0.2, width: 0.4, height: 0.5 };
  for (const [key, value] of Object.entries(keyboardRegion)) {
    await page.locator(`#region-${key}`).focus();
    await page.keyboard.press("Control+a");
    await page.keyboard.type(String(value * 100));
    await page.keyboard.press("Tab");
  }
  assertRegion(await numericRegion(page), keyboardRegion, "keyboard input");
  await selectFrame(page, 2);
  assert.equal(await page.locator("#region-enabled").isChecked(), false, "new capture must not inherit another image's region");
  assert.equal(await page.locator("#observation").inputValue(), "");
  assert.equal(await page.locator("#interval-start").inputValue(), "3");
  await selectFrame(page, 1);
  assert.equal(await page.locator("#observation").inputValue(), NOTE.observation);
  assertRegion(await numericRegion(page), keyboardRegion, "returning to original capture restores its draft");
  await assertOverlay(page, keyboardRegion, "desktop keyboard rectangle");
  screenshots.desktop = path.join(root, "review-desktop.png");
  await page.screenshot({ path: screenshots.desktop, fullPage: true });

  stage = "download and canonical import";
  assert.equal(await page.locator("#download-note").isDisabled(), false);
  const { notePath, note } = await downloadNote(page, "downloaded-note.json");
  assert.equal(note.kind, "noon-frame-feedback-note");
  assert.equal(note.sourceSha256, hash(SOURCE));
  assert.equal(note.sampleIndex, 1);
  assert.match(note.frameIdentity, /^[a-f0-9]{64}$/);
  assert.deepEqual(note.feedback, { interval: { start: 1, end: 2 }, region: keyboardRegion, ...NOTE });
  const imported = await feedbackCommand([
    "import", "--review", review.outputDir, "--note", notePath, "--output", "imported-feedback",
  ]);
  assert.equal(imported.feedback.status, "pending_review");
  const { retentionMs: _retentionMs, ...expectedFrame } = preview.manifest.samples[1].artifact;
  assert.deepEqual(imported.feedback.frame, expectedFrame);
  assert.deepEqual(imported.feedback.feedback, note.feedback);
  assert.deepEqual(await readFile(path.join(imported.outputDir, "frame.png")),
    await readFile(path.join(preview.outputDir, preview.manifest.samples[1].filename)));
  assert.equal(await readFile(path.join(imported.outputDir, "source.txt"), "utf8"), SOURCE);

  stage = "narrow layout";
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator("#frame-stage").scrollIntoViewIfNeeded();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1);
  assert.equal(overflow, false, "review interface must not overflow a narrow viewport");
  assertRegion(await numericRegion(page), keyboardRegion, "resize preserves normalized region");
  await assertOverlay(page, keyboardRegion, "narrow visible rectangle matches exported note");
  screenshots.narrow = path.join(root, "review-narrow.png");
  await page.screenshot({ path: screenshots.narrow, fullPage: true });
  assert.deepEqual(networkRequests, [], "file review must work without network requests");
  assert.deepEqual(errors, []);
  await context.close();

  stage = "failed image decode";
  const broken = await feedbackCommand([
    "review", "--preview", preview.manifestPath, "--source", "fixture_scene.py", "--output", "review-broken-image",
  ]);
  await writeFile(path.join(broken.outputDir, "frame-002.png"), "not an image");
  const brokenContext = await browser.newContext({ viewport: { width: 1000, height: 900 } });
  await brokenContext.setOffline(true);
  page = await brokenContext.newPage();
  await page.goto(pathToFileURL(broken.indexPath).href);
  await page.locator('#frame-list [data-sample-index="2"]').click();
  await page.waitForFunction(() => {
    const image = document.querySelector("#frame-image");
    return image.complete && image.naturalWidth === 0;
  });
  await enterNote(page);
  await page.waitForFunction(() => document.querySelector("#image-status").dataset.state === "error");
  assert.match(await page.locator("#image-status").textContent(), /fail|cannot|could not|unavailable|unable/i);
  assert.equal(await page.locator("#download-note").isDisabled(), true, "failed image decode must prevent saving feedback");
  screenshots.failedDecode = path.join(root, "review-image-error.png");
  await page.screenshot({ path: screenshots.failedDecode, fullPage: true });
  await brokenContext.close();

  Object.assign(report, {
    ok: true, sourceSha256: hash(SOURCE), frameSha256: expectedFrame.sha256,
    importedFeedbackPath: imported.feedbackPath, downloadedNotePath: notePath,
    boundaryFeedbackPath: boundaryImported.feedbackPath,
    annotationStatus: imported.feedback.status, screenshots,
    checks: ["offline-file-view", "embedded-data", "reverse-drag", "letterbox", "escape-cancel", "edge-clamp",
      "overlay-geometry", "keyboard-region", "frame-draft-isolation", "download-import", "narrow-layout", "image-decode-failure"],
  });
  await writeFile(path.join(root, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify(report)}\n`);
} catch (error) {
  if (page && !page.isClosed()) {
    await page.screenshot({ path: path.join(root, "failure.png"), fullPage: true }).catch(() => {});
  }
  Object.assign(report, { ok: false, failedStage: stage, error: String(error?.stack ?? error), screenshots });
  await writeFile(path.join(root, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
  process.stderr.write(`${JSON.stringify(report)}\n`);
  process.exitCode = 1;
} finally {
  await browser?.close();
}
