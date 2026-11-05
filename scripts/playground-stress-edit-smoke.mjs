import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { installStressObservation } from "./playground-stress-observation.mjs";
import { seekPausedGallery, waitForPublishedGalleryFrame } from "./showcase-playback.mjs";

const { chromium } = playwright;
const port = Number(process.env.NOON_PLAYGROUND_STRESS_EDIT_PORT ?? "0");
const selectAllShortcut = process.platform === "darwin" ? "Meta+A" : "Control+A";
const runShortcut = process.platform === "darwin" ? "Meta+Enter" : "Control+Enter";
const SOURCE_OWNED_DURATION_SECONDS = 600;
const artifactDir = path.resolve(
  process.env.NOON_PLAYGROUND_STRESS_EDIT_ARTIFACTS ??
    "browser-smoke-artifacts/playground-stress-edit",
);

await mkdir(artifactDir, { recursive: true });
const server = await serveRepository(path.resolve("."), port);
const { baseUrl } = server;

async function snapshot(page, { reset = false } = {}) {
  return page.evaluate((reset) => window.__noonStressObservation.snapshot({ reset }), reset);
}

async function waitForPreloadedRuntime(page) {
  const deferred = await page.evaluate(() => window.__noonStressInitialDeferred ?? null);
  assert.deepEqual(
    deferred,
    { runtimeStartup: "deferred", rendererBackend: "", presentedFrames: 0 },
    "stress playground must be deferred before its first explicit Run",
  );
  await page.waitForFunction(
    () => {
      const status = document.querySelector("#status");
      const patch = document.querySelector("#patch-status");
      if (status?.dataset.state === "error" || patch?.dataset.state === "error") {
        return true;
      }
      return (
        status?.dataset.liveAuthoring === "ready" &&
        Number(status?.dataset.presentedFrames ?? "0") > 0 &&
        patch?.dataset.state === "applied" &&
        !document.querySelector("#replace-scene")?.disabled
      );
    },
    null,
    { timeout: 120_000 },
  );
  const state = await snapshot(page);
  assert.notEqual(state.patchState, "error", `stress Run must succeed: ${state.patchText}`);
  assert.notEqual(state.runtimeState, "error", "stress runtime must start successfully");
  assert.equal(
    state.runtimeStartup,
    "started-on-demand",
    "stress playground must start its execution owner from the first explicit Run",
  );
  assert.equal(state.liveAuthoring, "ready");
  assert.ok(state.presentedFrames > 0, "stress Run must present a frame");
  assert.equal(state.patchState, "applied", `stress Run must succeed: ${state.patchText}`);
  return state;
}

async function waitForLongSourcePlayback(page, expectedGeneration, previousObjectCount) {
  await page.waitForFunction(
    ({ generation, previousCount }) => {
      const patch = document.querySelector("#patch-status");
      const run = document.querySelector("#replace-scene");
      return (
        window.__noonExampleGallery?.runInFlight === true &&
        window.__noonExampleGallery?.generationDiagnostics.runGeneration === generation &&
        patch?.dataset.state === "running" &&
        run?.disabled === false &&
        document.querySelector("#metric-objects")?.value !== previousCount
      );
    },
    { generation: expectedGeneration, previousCount: previousObjectCount },
    { timeout: 120_000 },
  );
  const state = await snapshot(page);
  assert.equal(state.runInFlight, true);
  assert.equal(state.runGeneration, expectedGeneration);
  assert.equal(state.runDisabled, false, "Run must stay available during the long source continuation");
  assert.equal(state.patchState, "running");
  assert.notEqual(state.objectCount, previousObjectCount, "the explicit source replacement must publish its row count");
  assert.equal(state.patchRunGeneration, String(expectedGeneration));
  return state;
}

async function waitForAppliedRun(page, previousObjectCount, expectedGeneration) {
  await page.waitForFunction(
    ({ previousCount, generation }) => {
      const patch = document.querySelector("#patch-status");
      const run = document.querySelector("#replace-scene");
      if (patch?.dataset.state === "error") return true;
      return patch?.dataset.state === "applied" && !run?.disabled &&
        patch.dataset.runGeneration === String(generation) &&
        document.querySelector("#metric-objects")?.value !== previousCount;
    },
    { previousCount: previousObjectCount, generation: expectedGeneration },
    { timeout: 120_000 },
  );
  const state = await snapshot(page);
  assert.equal(state.patchState, "applied", `explicit stress Run must succeed: ${state.patchText}`);
  assert.notEqual(state.objectCount, previousObjectCount, "the explicit Run must publish its structural change");
  assert.equal(state.patchRunGeneration, String(expectedGeneration));
  assert.equal(state.runGeneration, expectedGeneration);
  return state;
}

async function assertDraftOnlyEdit(page, edited, active, label) {
  assert.equal(edited.runGeneration, active.runGeneration, `${label} must not create a run`);
  assert.equal(edited.runInFlight, active.runInFlight, `${label} must preserve source execution state`);
  assert.equal(edited.runDisabled, active.runDisabled, `${label} must preserve explicit Run availability`);
  if (!active.playbackPlaying) {
    assert.equal(edited.objectCount, active.objectCount, `${label} must not publish the draft`);
  }
  assert.equal(edited.patchState, active.patchState, `${label} must preserve active source status`);
  assert.equal(edited.playbackPlaying, active.playbackPlaying, `${label} must preserve play intent`);
  assert.equal(edited.playbackPhase, active.playbackPhase, `${label} must preserve playback phase`);
}

async function replaceSource(editor, page, nextSource) {
  await page.evaluate((source) => window.__noonStressObservation.expectSourceEdit(source), nextSource);
  await editor.click();
  await page.keyboard.press(selectAllShortcut);
  await page.keyboard.insertText(nextSource);
  await page.waitForFunction(
    (expected) => document.querySelector("#python-scene-source")?.value === expected,
    nextSource,
    { timeout: 15_000 },
  );
  return page.evaluate(() => window.__noonStressObservation.editedSnapshot());
}

const diagnostics = {
  browser: null,
  viewport: { width: 1280, height: 820 },
  pageErrors: [],
  consoleErrors: [],
  snapshots: {},
  serverOutput: "",
};

let browser = null;
let context = null;
let page = null;
try {
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
  context = await browser.newContext({ viewport: diagnostics.viewport, deviceScaleFactor: 1 });
  page = await context.newPage();
  page.on("pageerror", (error) => diagnostics.pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") diagnostics.consoleErrors.push(message.text());
  });
  diagnostics.browser = await browser.version();

  await page.addInitScript(installStressObservation);
  await page.addInitScript(() => {
    window.addEventListener("DOMContentLoaded", () => {
      const status = document.querySelector("#status");
      if (!status) return;
      const capture = () => {
        if (status.dataset.runtimeStartup !== "deferred") return false;
        window.__noonStressInitialDeferred = {
          runtimeStartup: status.dataset.runtimeStartup,
          rendererBackend: status.dataset.rendererBackend ?? "",
          presentedFrames: Number(status.dataset.presentedFrames ?? "0"),
        };
        return true;
      };
      if (capture()) return;
      const observer = new MutationObserver(() => {
        if (capture()) observer.disconnect();
      });
      observer.observe(status, { attributes: true });
    }, { once: true });
  });

  await page.goto(`${baseUrl}/web/index.html?example=manim-parity-stress-grid`, {
    waitUntil: "load",
  });
  await page.waitForFunction(
    () => window.__noonExampleGallery?.selectedExampleId === "manim-parity-stress-grid",
    null,
    { timeout: 30_000 },
  );
  // Highlighting must arrive without requiring the user to focus/click the source first.
  await page.waitForSelector(
    "#scene-editor-panel .python-code-editor[data-editor-ready='true'] .cm-content",
    { timeout: 30_000 },
  );
  await page.locator("#replace-scene").click();
  diagnostics.snapshots.loaded = await snapshot(page);
  assert.equal(diagnostics.snapshots.loaded.enhanced, true);
  assert.equal(diagnostics.snapshots.loaded.textareaHidden, true);
  assert.ok(
    diagnostics.snapshots.loaded.editorHeight >= 500 && diagnostics.snapshots.loaded.editorHeight <= 650,
    `desktop editor pane must stay viewport-bounded, got ${diagnostics.snapshots.loaded.editorHeight}px`,
  );
  assert.ok(
    diagnostics.snapshots.loaded.editorScrollHeight > diagnostics.snapshots.loaded.editorClientHeight,
    "long Python source must scroll inside the editor rather than expanding the workspace",
  );

  const editor = page.locator("#scene-editor-panel .cm-content");
  await editor.click();
  const focused = await snapshot(page);
  assert.ok(
    Math.abs(focused.editorHeight - diagnostics.snapshots.loaded.editorHeight) <= 2,
    `focusing CodeMirror must not expand the editor pane (${diagnostics.snapshots.loaded.editorHeight} -> ${focused.editorHeight})`,
  );

  diagnostics.snapshots.baseline = await waitForPreloadedRuntime(page);
  await page.waitForFunction(() => window.__noonExampleGallery?.runInFlight === false, null,
    { timeout: 120_000 });
  diagnostics.snapshots.baseline = await snapshot(page);
  assert.equal(diagnostics.snapshots.baseline.exampleId, "manim-parity-stress-grid");
  assert.equal(diagnostics.snapshots.baseline.executionMode, "semantic");

  const source = await page.evaluate(
    () => document.querySelector("#python-scene-source")?.value ?? "",
  );
  assert.match(source, /rows = 20/);
  // Preserve the stress source and put the long ownership interval after its
  // first play. Draft changes must leave that 600-second continuation running.
  const firstStressPlayBoundary = "            run_time=0.35,\n        )\n";
  assert.ok(
    source.includes(firstStressPlayBoundary),
    "stress source must retain the canonical first play before the ownership wait",
  );
  const rows5Source = source
    .replace(/rows = \d+/, "rows = 5")
    .replace(
      firstStressPlayBoundary,
      `${firstStressPlayBoundary.replace("0.35", "2.0")}        self.wait(${SOURCE_OWNED_DURATION_SECONDS}.0)\n`,
    );
  assert.match(rows5Source, /self\.wait\(600\.0\)/);
  const rows7Source = source.replace(/rows = \d+/, "rows = 7");
  const rows20Source = rows7Source.replace(/rows = \d+/, "rows = 20");
  diagnostics.snapshots.rows5Draft = await replaceSource(editor, page, rows5Source);
  assert.equal(diagnostics.snapshots.rows5Draft.draftState, "unapplied");
  // The active timeline can keep changing object counts while the draft is edited.
  assert.equal(diagnostics.snapshots.rows5Draft.playbackPlaying,
    diagnostics.snapshots.baseline.playbackPlaying);
  assert.equal(diagnostics.snapshots.rows5Draft.runInFlight, false);
  assert.equal(diagnostics.snapshots.rows5Draft.runGeneration, diagnostics.snapshots.baseline.runGeneration);

  await page.locator("#replace-scene").click();
  const rows5Generation = diagnostics.snapshots.rows5Draft.runGeneration + 1;
  diagnostics.snapshots.rows5Playing = await waitForLongSourcePlayback(
    page, rows5Generation, diagnostics.snapshots.baseline.objectCount,
  );
  const sourceOwnedGeneration = diagnostics.snapshots.rows5Playing.runGeneration;

  await page.waitForFunction(() => Number(document.querySelector(".playback-controls")?.dataset.elapsedSeconds) > 2.05);
  const activeRows5 = await snapshot(page);

  // Reset and typing only change the draft. The original long continuation
  // stays active until an explicit Run captures a replacement source.
  diagnostics.snapshots.resetDuringRows5 = await snapshot(page, { reset: true });
  assert.equal(await page.locator("#python-scene-source").inputValue(), source);
  await assertDraftOnlyEdit(page, diagnostics.snapshots.resetDuringRows5, activeRows5, "Reset example");
  await page.waitForFunction(
    ({ generation, elapsed }) => window.__noonExampleGallery?.generationDiagnostics.runGeneration === generation &&
      window.__noonExampleGallery?.runInFlight === true &&
      Number(document.querySelector(".playback-controls")?.dataset.elapsedSeconds) > elapsed + 0.1,
    { generation: sourceOwnedGeneration, elapsed: diagnostics.snapshots.resetDuringRows5.elapsedSeconds },
    { timeout: 15_000 },
  );

  diagnostics.snapshots.rows7EditedDuringRows5 = await replaceSource(editor, page, rows7Source);
  await assertDraftOnlyEdit(page, diagnostics.snapshots.rows7EditedDuringRows5, activeRows5, "rows=7 edit");
  assert.equal(diagnostics.snapshots.rows7EditedDuringRows5.draftState, "unapplied");

  await page.locator("#replace-scene").click();
  const rows7Generation = sourceOwnedGeneration + 2;
  diagnostics.snapshots.rows7Rerun = await waitForAppliedRun(
    page, diagnostics.snapshots.rows5Playing.objectCount, rows7Generation,
  );
  await page.waitForFunction(() => window.__noonExampleGallery?.runInFlight === false, null,
    { timeout: 120_000 });
  diagnostics.snapshots.rows7Rerun = await snapshot(page);
  assert.equal(diagnostics.snapshots.rows7Rerun.draftState, "applied");

  // The authored fades change membership while playing. Compare draft output at
  // one confirmed paused frame; the long-source edits above cover continued play.
  const duration = Number(await page.locator(".playback-scrubber").getAttribute("max"));
  const pausedTime = await seekPausedGallery(page, duration, 1);
  const pausedFrame = await waitForPublishedGalleryFrame(page, pausedTime, duration);
  await page.waitForFunction(count => document.querySelector("#metric-objects")?.value === String(count),
    pausedFrame.metrics.objectCount);
  diagnostics.snapshots.rows7Paused = await snapshot(page);
  assert.equal(diagnostics.snapshots.rows7Paused.playbackPlaying, false);
  const rows7ObjectCount = diagnostics.snapshots.rows7Paused.objectCount;
  diagnostics.snapshots.rows20Edited = await replaceSource(editor, page, rows20Source);
  assert.equal(diagnostics.snapshots.rows20Edited.objectCount, rows7ObjectCount);
  await assertDraftOnlyEdit(page, diagnostics.snapshots.rows20Edited, diagnostics.snapshots.rows7Paused,
    "rows=20 edit");
  assert.equal(diagnostics.snapshots.rows20Edited.draftState, "unapplied");
  await page.keyboard.press(runShortcut);
  diagnostics.snapshots.rows20Rerun = await waitForAppliedRun(
    page, rows7ObjectCount, rows7Generation + 1,
  );
  assert.equal(diagnostics.snapshots.rows20Rerun.draftState, "applied");
  assert.equal(diagnostics.snapshots.rows20Rerun.executionMode, "semantic");

  assert.deepEqual(diagnostics.pageErrors, [], `unhandled page errors: ${diagnostics.pageErrors.join("\n")}`);
  assert.deepEqual(
    diagnostics.consoleErrors,
    [],
    `explicit stress Runs emitted console errors: ${diagnostics.consoleErrors.join("\n")}`,
  );

  await page.screenshot({ path: path.join(artifactDir, "stress-edited.png"), fullPage: true });
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  console.log(
    `playground draft-only stress edits ok: ${diagnostics.snapshots.loaded.editorHeight}px editor, explicit rows=5 → rows=7 → rows=20 Runs`,
  );
} catch (error) {
  diagnostics.failure = error instanceof Error ? error.stack ?? error.message : String(error);
  if (page !== null) {
    try {
      diagnostics.snapshots.failure = await snapshot(page);
      await page.screenshot({ path: path.join(artifactDir, "failure.png"), fullPage: true });
    } catch (screenshotError) {
      diagnostics.screenshotFailure = String(screenshotError);
    }
  }
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  throw error;
} finally {
  await context?.close().catch(() => {});
  await browser?.close().catch(() => {});
  await server.close();
}
