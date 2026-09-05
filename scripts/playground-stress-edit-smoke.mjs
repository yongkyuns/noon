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
      patchSequence: patch?.dataset.sequence ?? "",
      exampleId: patch?.dataset.exampleId ?? "",
      objectCount: document.querySelector("#metric-objects")?.value ?? "",
      editorHeight: pane?.getBoundingClientRect().height ?? 0,
      bodyHeight: document.body.scrollHeight,
      editorScrollHeight: scroller?.scrollHeight ?? 0,
      editorClientHeight: scroller?.clientHeight ?? 0,
      enhanced: document.querySelector("#scene-editor-panel .python-code-editor[data-editor-ready='true']") !== null,
      textareaHidden: document.querySelector("#python-scene-source")?.hidden ?? false,
    };
  });
}

async function waitForPreloadedScene(page) {
  await page.waitForFunction(
    () => {
      const status = document.querySelector("#status");
      const patch = document.querySelector("#patch-status");
      return (
        status?.dataset.liveAuthoring === "ready" &&
        status?.dataset.state === "running" &&
        Number(status?.dataset.presentedFrames ?? "0") > 0 &&
        patch?.dataset.state === "applied" &&
        !document.querySelector("#replace-scene")?.disabled
      );
    },
    null,
    { timeout: 120_000 },
  );
  const state = await snapshot(page);
  assert.equal(
    state.runtimeStartup,
    "started-on-demand",
    "stress playground must preload the existing execution owner without an explicit Run",
  );
  assert.equal(state.liveAuthoring, "ready");
  assert.ok(state.presentedFrames > 0, "stress preload must present a frame");
  assert.equal(state.patchState, "applied", `stress preload must succeed: ${state.patchText}`);
  return state;
}

async function waitForAutomaticRun(page, previousObjectCount) {
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

  diagnostics.snapshots.baseline = await waitForPreloadedScene(page);
  assert.equal(diagnostics.snapshots.baseline.exampleId, "manim-parity-stress-grid");
  assert.equal(diagnostics.snapshots.baseline.executionMode, "retained");
  assert.match(diagnostics.snapshots.baseline.patchText, /Scene rebuilt atomically/);

  const source = await page.evaluate(
    () => document.querySelector("#python-scene-source")?.value ?? "",
  );
  assert.match(source, /rows = 20/);
  let editedSource = source;
  let previousObjectCount = diagnostics.snapshots.baseline.objectCount;
  for (const rows of [5, 7, 20]) {
    const nextSource = editedSource.replace(/rows = \d+/, `rows = ${rows}`);
    assert.notEqual(nextSource, editedSource);

    // Use the real CodeMirror input path on every edit so identity stabilization and the
    // latest-source debounce are exercised across consecutive automatic authoring results.
    await editor.click();
    await page.keyboard.press(selectAllShortcut);
    await page.keyboard.insertText(nextSource);
    await page.waitForFunction(
      (expected) => document.querySelector("#python-scene-source")?.value === expected,
      nextSource,
      { timeout: 15_000 },
    );
    diagnostics.snapshots[`rows${rows}Edited`] = await snapshot(page);
    assert.ok(
      Math.abs(
        diagnostics.snapshots[`rows${rows}Edited`].editorHeight -
          diagnostics.snapshots.loaded.editorHeight,
      ) <= 2,
      "editing the long source must not grow the editor pane",
    );

    diagnostics.snapshots[`rows${rows}Rerun`] = await waitForAutomaticRun(
      page,
      previousObjectCount,
    );
    assert.equal(diagnostics.snapshots[`rows${rows}Rerun`].executionMode, "retained");
    assert.match(
      diagnostics.snapshots[`rows${rows}Rerun`].patchText,
      /Scene rebuilt atomically/,
    );
    previousObjectCount = diagnostics.snapshots[`rows${rows}Rerun`].objectCount;
    editedSource = nextSource;
  }

  assert.deepEqual(diagnostics.pageErrors, [], `unhandled page errors: ${diagnostics.pageErrors.join("\n")}`);
  assert.deepEqual(
    diagnostics.consoleErrors,
    [],
    `valid repeated structural stress edits emitted console errors: ${diagnostics.consoleErrors.join("\n")}`,
  );

  diagnostics.serverOutput = serverOutput;
  await page.screenshot({ path: path.join(artifactDir, "stress-edited.png"), fullPage: true });
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  console.log(
    `playground live structural stress edits ok: ${diagnostics.snapshots.loaded.editorHeight}px editor, rows=5 -> 7 -> 20 applied`,
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
