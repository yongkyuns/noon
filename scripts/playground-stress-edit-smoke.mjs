import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

import playwright from "playwright";

const { chromium } = playwright;
const port = Number(process.env.NOON_PLAYGROUND_STRESS_EDIT_PORT ?? "4191");
const baseUrl = `http://127.0.0.1:${port}`;
const selectAllShortcut = process.platform === "darwin" ? "Meta+A" : "Control+A";
const artifactDir = path.resolve(
  process.env.NOON_PLAYGROUND_STRESS_EDIT_ARTIFACTS ??
    "browser-smoke-artifacts/playground-stress-edit",
);

await mkdir(artifactDir, { recursive: true });

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", "."],
  { stdio: ["ignore", "pipe", "pipe"] },
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
  throw new Error(`Playground server did not start: ${lastError}\n${serverOutput}`);
}

async function snapshot(page) {
  return page.evaluate(() => {
    const status = document.querySelector("#status");
    const patch = document.querySelector("#patch-status");
    const pane = document.querySelector(".editor-pane");
    const scroller = document.querySelector("#scene-editor-panel .cm-scroller");
    return {
      runtimeState: status?.dataset.state ?? "",
      runtimeStartup: status?.dataset.runtimeStartup ?? "",
      liveAuthoring: status?.dataset.liveAuthoring ?? "",
      rendererBackend: status?.dataset.rendererBackend ?? "",
      executionMode: status?.dataset.executionMode ?? "",
      presentedFrames: Number(status?.dataset.presentedFrames ?? "0"),
      patchState: patch?.dataset.state ?? "",
      patchText: patch?.value ?? patch?.textContent ?? "",
      patchOperation: patch?.dataset.operation ?? "",
      patchSequence: patch?.dataset.sequence ?? "",
      exampleId: patch?.dataset.exampleId ?? "",
      objectCount: document.querySelector("#metric-objects")?.value ?? "",
      runInFlight: window.__noonExampleGallery?.runInFlight ?? false,
      runDisabled: document.querySelector("#replace-scene")?.disabled ?? true,
      editorHeight: pane?.getBoundingClientRect().height ?? 0,
      bodyHeight: document.body.scrollHeight,
      editorScrollHeight: scroller?.scrollHeight ?? 0,
      editorClientHeight: scroller?.clientHeight ?? 0,
      enhanced: document.querySelector("#scene-editor-panel .python-code-editor[data-editor-ready='true']") !== null,
      textareaHidden: document.querySelector("#python-scene-source")?.hidden ?? false,
    };
  });
}

async function waitForPreloadedRuntime(page) {
  const deferred = await page.evaluate(() => window.__noonStressInitialDeferred ?? null);
  assert.deepEqual(
    deferred,
    { runtimeStartup: "deferred", rendererBackend: "", presentedFrames: 0 },
    "stress playground must be deferred before its first source-owned Run",
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
    "stress playground must start its execution owner from the first source-owned Run",
  );
  assert.equal(state.liveAuthoring, "ready");
  assert.ok(state.presentedFrames > 0, "stress Run must present a frame");
  assert.equal(state.patchState, "applied", `stress Run must succeed: ${state.patchText}`);
  return state;
}

async function waitForSourceOwnedPlayback(page) {
  await page.waitForFunction(
    () => {
      const patch = document.querySelector("#patch-status");
      const run = document.querySelector("#replace-scene");
      return (
        window.__noonExampleGallery?.runInFlight === true &&
        patch?.dataset.state === "running" &&
        (patch?.value ?? patch?.textContent ?? "").includes("edit freely or Run again") &&
        run?.disabled === false
      );
    },
    null,
    { timeout: 120_000 },
  );
  const state = await snapshot(page);
  assert.equal(state.runInFlight, true);
  assert.equal(state.runDisabled, false, "Run must stay available during source-owned playback");
  assert.match(state.patchText, /edit freely or Run again/);
  return state;
}

async function waitForAppliedRun(page, previousObjectCount) {
  await page.waitForFunction(
    (priorObjectCount) => {
      const patch = document.querySelector("#patch-status");
      const run = document.querySelector("#replace-scene");
      if (run?.disabled) return false;
      if (patch?.dataset.state === "error") return true;
      return (
        patch?.dataset.state === "applied" &&
        document.querySelector("#metric-objects")?.value !== priorObjectCount
      );
    },
    previousObjectCount,
    { timeout: 120_000 },
  );
  const state = await snapshot(page);
  assert.equal(state.patchState, "applied", `stress scene must rerun successfully: ${state.patchText}`);
  assert.notEqual(
    state.objectCount,
    previousObjectCount,
    "successful structural edit must publish a different object count",
  );
  return state;
}

async function replaceSource(editor, page, nextSource) {
  await editor.click();
  await page.keyboard.press(selectAllShortcut);
  await page.keyboard.insertText(nextSource);
  await page.waitForFunction(
    (expected) => document.querySelector("#python-scene-source")?.value === expected,
    nextSource,
    { timeout: 15_000 },
  );
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
  context = await browser.newContext({ viewport: diagnostics.viewport, deviceScaleFactor: 1 });
  page = await context.newPage();
  page.on("pageerror", (error) => diagnostics.pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") diagnostics.consoleErrors.push(message.text());
  });
  diagnostics.browser = await browser.version();

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

  await page.waitForSelector(
    "#scene-editor-panel .python-code-editor[data-editor-ready='true'] .cm-content",
    { timeout: 30_000 },
  );
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
  assert.equal(diagnostics.snapshots.baseline.exampleId, "manim-parity-stress-grid");
  assert.equal(diagnostics.snapshots.baseline.executionMode, "semantic");
  assert.equal(diagnostics.snapshots.baseline.patchOperation, "Scene rebuilt atomically");

  const source = await page.evaluate(
    () => document.querySelector("#python-scene-source")?.value ?? "",
  );
  assert.match(source, /rows = 20/);
  const rows5Source = source.replace(/rows = \d+/, "rows = 5");
  const rows7Source = rows5Source.replace(/rows = \d+/, "rows = 7");
  const rows20Source = rows7Source.replace(/rows = \d+/, "rows = 20");
  const baselineObjectCount = diagnostics.snapshots.baseline.objectCount;

  // Editing is now deliberately non-destructive. Prove that the existing replay and
  // object count remain untouched until explicit Run.
  await replaceSource(editor, page, rows5Source);
  diagnostics.snapshots.rows5Edited = await snapshot(page);
  assert.match(diagnostics.snapshots.rows5Edited.patchText, /current preview continues · Run to apply/);
  assert.equal(diagnostics.snapshots.rows5Edited.objectCount, baselineObjectCount);
  await page.waitForTimeout(350);
  diagnostics.snapshots.rows5StillIdle = await snapshot(page);
  assert.equal(
    diagnostics.snapshots.rows5StillIdle.objectCount,
    baselineObjectCount,
    "editing alone must not autorun or replace the current scene",
  );

  // Start rows=5 and catch it while Python still owns the active animation. Then edit
  // again and press Run before that animation completes. This is the product contract:
  // edit leaves playback alone; Run is the explicit supersession boundary.
  await page.locator("#replace-scene").click();
  diagnostics.snapshots.rows5Playing = await waitForSourceOwnedPlayback(page);
  await replaceSource(editor, page, rows7Source);
  diagnostics.snapshots.rows7EditedDuringRows5 = await snapshot(page);
  assert.equal(diagnostics.snapshots.rows7EditedDuringRows5.runtimeState, "running");
  assert.match(
    diagnostics.snapshots.rows7EditedDuringRows5.patchText,
    /current preview continues · Run to apply/,
  );
  assert.equal(
    diagnostics.snapshots.rows7EditedDuringRows5.runDisabled,
    false,
    "editing during an active source-owned animation must leave Run available",
  );

  await page.locator("#replace-scene").click();
  diagnostics.snapshots.rows7Rerun = await waitForAppliedRun(page, baselineObjectCount);
  assert.equal(diagnostics.snapshots.rows7Rerun.executionMode, "semantic");
  assert.equal(diagnostics.snapshots.rows7Rerun.patchOperation, "Scene rebuilt atomically");

  // One more ordinary explicit edit/run verifies the post-supersession session remains reusable.
  const rows7ObjectCount = diagnostics.snapshots.rows7Rerun.objectCount;
  await replaceSource(editor, page, rows20Source);
  diagnostics.snapshots.rows20Edited = await snapshot(page);
  assert.equal(diagnostics.snapshots.rows20Edited.objectCount, rows7ObjectCount);
  assert.match(diagnostics.snapshots.rows20Edited.patchText, /current preview continues · Run to apply/);
  await page.locator("#replace-scene").click();
  diagnostics.snapshots.rows20Rerun = await waitForAppliedRun(page, rows7ObjectCount);
  assert.equal(diagnostics.snapshots.rows20Rerun.executionMode, "semantic");
  assert.equal(diagnostics.snapshots.rows20Rerun.patchOperation, "Scene rebuilt atomically");

  assert.deepEqual(diagnostics.pageErrors, [], `unhandled page errors: ${diagnostics.pageErrors.join("\n")}`);
  assert.deepEqual(
    diagnostics.consoleErrors,
    [],
    `valid explicit structural reruns emitted console errors: ${diagnostics.consoleErrors.join("\n")}`,
  );

  diagnostics.serverOutput = serverOutput;
  await page.screenshot({ path: path.join(artifactDir, "stress-edited.png"), fullPage: true });
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  console.log(
    `playground explicit structural reruns ok: ${diagnostics.snapshots.loaded.editorHeight}px editor, edit-inert rows=5 then mid-animation rows=7 supersession and rows=20 replay`,
  );
} catch (error) {
  diagnostics.failure = error instanceof Error ? error.stack ?? error.message : String(error);
  diagnostics.serverOutput = serverOutput;
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
  server.kill("SIGTERM");
}
