import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

import playwright from "playwright";

const { chromium } = playwright;
const port = Number(process.env.NOON_PLAYGROUND_AUTHORING_RECOVERY_PORT ?? "4185");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  process.env.NOON_PLAYGROUND_AUTHORING_RECOVERY_ARTIFACTS ??
    "browser-smoke-artifacts/playground-authoring-recovery",
);

const SOURCES = {
  syntaxError: `from noon import *

class BrokenScene(Scene)
    def construct(self):
        self.add(Square())
`,
  twoObjects: `from noon import *

class RecoveryScene(Scene):
    def construct(self):
        self.add(Square())
        self.add(Circle().shift(RIGHT * 2))
`,
  runtimeError: `from noon import *

class RuntimeFailure(Scene):
    def construct(self):
        self.add(Square())
        label = "😀한글"; self.add(MissingShape())
`,
  oneObject: `from noon import *

class FinalRecovery(Scene):
    def construct(self):
        self.add(Circle())
`,
  explicitRerun: `from noon import *
import builtins

class ExplicitRerun(Scene):
    def construct(self):
        attempt = getattr(builtins, "_noon_test_preview_rerun", 0) + 1
        builtins._noon_test_preview_rerun = attempt
        if attempt == 2:
            raise ValueError("Intentional explicit rerun failure")
        square = Square()
        self.add(square)
        self.play(square.animate.shift(RIGHT), run_time=3)
`,
};

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
      playhead: document.querySelector("#metric-time")?.value ?? "",
      runDisabled: document.querySelector("#replace-scene")?.disabled ?? true,
      hasExecution: window.__noonExampleGallery?.executionMode != null,
      playing: status?.dataset.playbackPlaying === "true",
      canvasHidden: getComputedStyle(document.querySelector("canvas")).visibility === "hidden",
      traceback: document.querySelector("#python-traceback")?.textContent ?? "",
      problemVisible: document.querySelector("#authoring-problem")?.hidden === false,
      problemTitle: document.querySelector("#authoring-problem strong")?.textContent ?? "",
      detailsOpen: document.querySelector("#python-diagnostics")?.open ?? false,
    };
  });
}

async function assertSourceDiagnostic(page, observed, { type, line, selectedText = null }) {
  assert.equal(observed.problemVisible, true, "the production error panel must be visible");
  assert.equal(observed.problemTitle, `Python raised ${type}`);
  assert.equal(observed.detailsOpen, false, "technical details must be collapsed by default");
  assert.match(observed.traceback, /noon-s\d+-r\d+\.py/, "raw traceback must name the executed revision");
  await page.getByRole("button", { name: `Go to line ${line}`, exact: true }).click();
  const selection = await page.evaluate(() => {
    const view = document.querySelector("#python-scene-source").editorView;
    const { from, to } = view.state.selection.main;
    return { line: view.state.doc.lineAt(from).number, text: view.state.sliceDoc(from, to) };
  });
  assert.equal(selection.line, line, "source link must target the user's actual line");
  if (selectedText !== null) assert.equal(selection.text, selectedText);
}

function assertDiagnosticCleared(observed) {
  assert.equal(observed.problemVisible, false, "edited/corrected source must clear the old panel");
  assert.equal(observed.traceback, "", "old technical details must not survive into the next run");
}

async function assertStoppedPreview(page, observed) {
  assert.equal(observed.playing, false, "invalid edits must pause the previous execution");
  if (!observed.hasExecution) return;
  assert.equal(observed.canvasHidden, true, "a retained renderer must not display stale source");
  // Allow any already submitted frame to finish, then prove that retaining the
  // worker does not keep the old animation presenting in the background.
  await page.waitForTimeout(100);
  const frames = () => page.evaluate(async () =>
    (await window.__noonExampleGallery.executionMetrics()).metrics.presentedFrames);
  const before = await frames();
  await page.waitForTimeout(250);
  assert.equal(await frames(), before, "paused source must settle with no further presentations");
}

// A readback of the displayed placeholder, independent of the preview cache.
// The negative control (a blank or missing image) cannot satisfy this reference.
async function previewPixels(page, frozen = false) {
  return page.evaluate(async (frozen) => {
    const image = document.querySelector(frozen ? "#last-successful-preview img" : "#scene");
    const width = frozen ? image.naturalWidth : image.width;
    const height = frozen ? image.naturalHeight : image.height;
    const scale = Math.min(1, 1536 / width, 1536 / height, Math.sqrt(1048576 / width / height));
    const copy = document.createElement("canvas");
    copy.width = Math.max(1, Math.floor(width * scale));
    copy.height = Math.max(1, Math.floor(height * scale));
    const context = copy.getContext("2d");
    context.drawImage(image, 0, 0, copy.width, copy.height);
    const bytes = context.getImageData(0, 0, copy.width, copy.height).data;
    let different = 0;
    for (let i = 4; i < bytes.length; i += 4) {
      if (bytes[i] !== bytes[0] || bytes[i + 1] !== bytes[1] || bytes[i + 2] !== bytes[2] || bytes[i + 3] !== bytes[3]) different += 1;
    }
    const digest = [...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))]
      .map(byte => byte.toString(16).padStart(2, "0")).join("");
    return { width: copy.width, height: copy.height, different, digest };
  }, frozen);
}

async function pauseAndRecordPreview(page) {
  const toggle = page.locator(".playback-toggle");
  if (await toggle.getAttribute("aria-label") === "Pause animation") await toggle.click();
  // Observe the compositor's placeholder update after the acknowledged pause.
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const pixels = await previewPixels(page);
  assert.ok(pixels.different > 50, "the successful scene must have visible nonuniform pixels");
  return { pixels, runGeneration: await page.locator("#patch-status").getAttribute("data-run-generation") };
}

async function assertFrozenPreview(page, reference) {
  await page.waitForFunction(() => {
    const overlay = document.querySelector("#last-successful-preview");
    const image = overlay?.querySelector("img");
    return overlay?.hidden === false && image?.complete && image.naturalWidth > 0;
  }, null, { timeout: 10_000 });
  const overlay = page.locator("#last-successful-preview");
  assert.match(await overlay.textContent(), /Previous successful preview.*Edited source failed/);
  assert.equal(await overlay.getAttribute("data-run-generation"), reference.runGeneration);
  assert.deepEqual(await previewPixels(page, true), reference.pixels, "invalid edits must preserve actual displayed pixels");
  assert.equal(await page.locator(".playback-toggle").isDisabled(), true,
    "the old execution must not resume behind its frozen preview");
  assert.equal(await page.locator("canvas").count(), 1, "the frozen image must not create a second live canvas");
}

// Source-owned construct/play has finished when its result is applied. Restart
// the accepted timeline through public controls, then explicitly resume it.
// Replay controls do not execute Python again or increment the fixture counter.
async function startReplayForExplicitRerun(page) {
  const restart = page.locator(".playback-restart");
  const toggle = page.locator(".playback-toggle");
  assert.equal(await restart.isEnabled(), true, "replay restart must be available");
  await restart.click();
  await page.waitForFunction(() => {
    const controls = document.querySelector(".playback-controls");
    const button = document.querySelector(".playback-toggle");
    return controls?.dataset.busy === "false" && button?.disabled === false;
  });
  const label = await toggle.getAttribute("aria-label");
  if (label === "Play animation") await toggle.click();
  else assert.equal(label, "Pause animation");
  await page.waitForFunction(() => {
    const controls = document.querySelector(".playback-controls");
    const button = document.querySelector(".playback-toggle");
    return controls?.dataset.busy === "false" && controls.dataset.playing === "true" &&
      button?.disabled === false && button.getAttribute("aria-label") === "Pause animation";
  });
}

async function waitForInitialScene(page) {
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  await page.waitForSelector("#scene-editor-panel .python-code-editor[data-editor-ready='true'] .cm-content", {
    timeout: 30_000,
  });
  await page.waitForFunction(
    () => {
      const status = document.querySelector("#status");
      const patch = document.querySelector("#patch-status");
      return (
        status?.dataset.liveAuthoring === "ready" &&
        status?.dataset.rendererBackend === "WebGL2" &&
        Number(status?.dataset.presentedFrames ?? "0") > 0 &&
        patch?.dataset.state === "applied" &&
        !document.querySelector("#replace-scene")?.disabled
      );
    },
    null,
    { timeout: 60_000 },
  );

  const preloaded = await snapshot(page);
  assert.equal(
    preloaded.runtimeStartup,
    "started-on-demand",
    "live authoring preload must start the existing execution owner before an explicit Run",
  );
  assert.equal(preloaded.liveAuthoring, "ready", "live authoring preload must reach ready state");
  assert.equal(preloaded.rendererBackend, "WebGL2");
  assert.ok(preloaded.presentedFrames > 0, "preload must present the initial scene");

  const editor = page.locator("#scene-editor-panel .cm-content");
  await editor.focus();
  assert.equal(
    await page.locator("#replace-scene").isEnabled(),
    true,
    "explicit Run must remain available after preload",
  );
}

async function setEditorSource(page, source) {
  const editor = page.locator("#scene-editor-panel .cm-content");
  await editor.click();
  await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A");
  await page.keyboard.insertText(source);
  await page.waitForFunction(
    (expected) => document.querySelector("#python-scene-source")?.value === expected,
    source,
    { timeout: 10_000 },
  );
  assert.equal(
    await page.evaluate(() => document.activeElement?.classList.contains("cm-content") ?? false),
    true,
    "real CodeMirror editor should retain keyboard focus after user input",
  );
}

async function waitForRunResult(page, { expectedState, expectedObjectCount = null }) {
  await page.waitForFunction(
    ({ state, objectCount }) => {
      const patch = document.querySelector("#patch-status");
      const run = document.querySelector("#replace-scene");
      if (patch?.dataset.state !== state || run?.disabled) return false;
      if (objectCount === null) return true;
      return document.querySelector("#metric-objects")?.value === String(objectCount);
    },
    { state: expectedState, objectCount: expectedObjectCount },
    { timeout: 60_000 },
  );
  return snapshot(page);
}

async function runEditedSource(page, options) {
  return waitForRunResult(page, options);
}

async function waitForObjectCount(page, count) {
  await page.waitForFunction(
    (expected) => document.querySelector("#metric-objects")?.value === String(expected),
    count,
    { timeout: 10_000 },
  );
}

const diagnostics = {
  browser: null,
  viewport: { width: 1280, height: 820 },
  devicePixelRatio: 1,
  snapshots: {},
  pageErrors: [],
  consoleErrors: [],
  unexpectedNavigations: 0,
  serverOutput: "",
};

let browser = null;
let context = null;
let page = null;
let phase = "startup";
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
    if (message.type() === "error") diagnostics.consoleErrors.push({ phase, text: message.text() });
  });

  diagnostics.browser = await browser.version();
  await page.goto(`${baseUrl}/web/index.html?example=parity-create-circle`, { waitUntil: "load" });
  await waitForInitialScene(page);
  await waitForObjectCount(page, 1);
  page.on("framenavigated", (frame) => {
    if (frame === page.mainFrame()) diagnostics.unexpectedNavigations += 1;
  });

  phase = "baseline";
  diagnostics.snapshots.baseline = await snapshot(page);
  assert.notEqual(diagnostics.snapshots.baseline.runtimeState, "error");
  assert.equal(diagnostics.snapshots.baseline.liveAuthoring, "ready");
  assert.equal(diagnostics.snapshots.baseline.rendererBackend, "WebGL2");
  assert.equal(diagnostics.snapshots.baseline.exampleId, "parity-create-circle");
  assert.notEqual(diagnostics.snapshots.baseline.patchSequence, "");
  await page.screenshot({ path: path.join(artifactDir, "baseline.png"), fullPage: true });

  diagnostics.previewBaseline = await pauseAndRecordPreview(page);
  phase = "syntax-error";
  await setEditorSource(page, SOURCES.syntaxError);
  let edited = await snapshot(page);
  assert.match(edited.patchText, /restarting/);
  assert.equal(edited.objectCount, diagnostics.snapshots.baseline.objectCount);
  diagnostics.snapshots.syntaxError = await runEditedSource(page, { expectedState: "error" });
  assert.equal(diagnostics.snapshots.syntaxError.patchText, "Python raised SyntaxError");
  assert.notEqual(diagnostics.snapshots.syntaxError.runtimeState, "error");
  assert.equal(diagnostics.snapshots.syntaxError.rendererBackend, diagnostics.snapshots.baseline.rendererBackend);
  assert.equal(
    diagnostics.snapshots.syntaxError.patchSequence,
    diagnostics.snapshots.baseline.patchSequence,
    "syntax failure must not consume the current semantic generation's patch sequence",
  );
  assert.equal(
    diagnostics.snapshots.syntaxError.objectCount,
    diagnostics.snapshots.baseline.objectCount,
    "syntax failure must retain the last successful object-count diagnostic",
  );
  await assertStoppedPreview(page, diagnostics.snapshots.syntaxError);
  await assertFrozenPreview(page, diagnostics.previewBaseline);
  assert.match(diagnostics.snapshots.syntaxError.traceback, /SyntaxError/);
  await assertSourceDiagnostic(page, diagnostics.snapshots.syntaxError, { type: "SyntaxError", line: 3 });
  await page.screenshot({ path: path.join(artifactDir, "syntax-error.png"), fullPage: true });

  phase = "syntax-recovery";
  await setEditorSource(page, SOURCES.twoObjects);
  edited = await snapshot(page);
  assertDiagnosticCleared(edited);
  assert.match(edited.patchText, /restarting/);
  diagnostics.snapshots.syntaxRecovered = await runEditedSource(page, {
    expectedState: "applied",
    expectedObjectCount: 2,
  });
  assertDiagnosticCleared(diagnostics.snapshots.syntaxRecovered);
  assert.match(diagnostics.snapshots.syntaxRecovered.patchText, /2 objects/);
  assert.notEqual(diagnostics.snapshots.syntaxRecovered.runtimeState, "error");
  assert.equal(diagnostics.snapshots.syntaxRecovered.rendererBackend, "WebGL2");

  assert.equal(await page.locator("#last-successful-preview").isHidden(), true);
  diagnostics.previewRecovered = await pauseAndRecordPreview(page);
  phase = "runtime-error";
  await setEditorSource(page, SOURCES.runtimeError);
  edited = await snapshot(page);
  assert.match(edited.patchText, /restarting/);
  assert.equal(edited.objectCount, "2");
  diagnostics.snapshots.runtimeError = await runEditedSource(page, { expectedState: "error" });
  assert.equal(diagnostics.snapshots.runtimeError.patchText, "Python raised NameError");
  assert.notEqual(diagnostics.snapshots.runtimeError.runtimeState, "error");
  assert.equal(diagnostics.snapshots.runtimeError.rendererBackend, "WebGL2");
  assert.equal(
    diagnostics.snapshots.runtimeError.patchSequence,
    diagnostics.snapshots.syntaxRecovered.patchSequence,
    "execution-time authoring failure must not consume the current semantic generation's patch sequence",
  );
  assert.equal(
    diagnostics.snapshots.runtimeError.objectCount,
    "2",
    "authoring failure must retain the last successful object-count diagnostic",
  );
  await assertStoppedPreview(page, diagnostics.snapshots.runtimeError);
  await assertFrozenPreview(page, diagnostics.previewRecovered);
  assert.match(diagnostics.snapshots.runtimeError.traceback, /MissingShape/);
  await assertSourceDiagnostic(page, diagnostics.snapshots.runtimeError, {
    type: "NameError", line: 6, selectedText: "MissingShape",
  });
  await page.screenshot({ path: path.join(artifactDir, "runtime-error.png"), fullPage: true });

  phase = "runtime-recovery";
  await setEditorSource(page, SOURCES.oneObject);
  edited = await snapshot(page);
  assertDiagnosticCleared(edited);
  assert.match(edited.patchText, /restarting/);
  diagnostics.snapshots.runtimeRecovered = await runEditedSource(page, {
    expectedState: "applied",
    expectedObjectCount: 1,
  });
  assertDiagnosticCleared(diagnostics.snapshots.runtimeRecovered);
  assert.match(diagnostics.snapshots.runtimeRecovered.patchText, /1 objects/);
  assert.notEqual(diagnostics.snapshots.runtimeRecovered.runtimeState, "error");
  assert.equal(diagnostics.snapshots.runtimeRecovered.rendererBackend, "WebGL2");
  assert.equal(await page.locator("#last-successful-preview").isHidden(), true);
  await page.screenshot({ path: path.join(artifactDir, "recovered.png"), fullPage: true });

  // Rerunning the same valid source can fail on a later invocation. No edit
  // event pauses the previous replay here: Run itself must stop it first.
  phase = "explicit-rerun-baseline";
  await setEditorSource(page, SOURCES.explicitRerun);
  diagnostics.snapshots.rerunBaseline = await runEditedSource(page, {
    expectedState: "applied", expectedObjectCount: 1,
  });
  const acceptedGeneration = await page.locator("#patch-status").getAttribute("data-run-generation");
  await startReplayForExplicitRerun(page);
  diagnostics.snapshots.rerunPlaying = await snapshot(page);
  assert.equal(await page.locator("#patch-status").getAttribute("data-run-generation"), acceptedGeneration,
    "replaying the timeline must not rerun Python");
  assert.equal(await page.locator("#python-scene-source").inputValue(), SOURCES.explicitRerun);
  // Record the precondition at the actual Run event, before its handler pauses
  // playback. A slow test that lets the replay finish must not pass vacuously.
  await page.evaluate(() => {
    document.querySelector("#replace-scene").addEventListener("click", () => {
      const controls = document.querySelector(".playback-controls");
      window.__noonPreviewRerunClick = {
        playing: controls?.dataset.playing === "true",
        busy: controls?.dataset.busy === "true",
        runGeneration: document.querySelector("#patch-status")?.dataset.runGeneration,
      };
    }, { capture: true, once: true });
  });
  phase = "explicit-rerun-error";
  await page.locator("#replace-scene").click();
  diagnostics.rerunClick = await page.evaluate(() => window.__noonPreviewRerunClick);
  assert.deepEqual(diagnostics.rerunClick, { playing: true, busy: false, runGeneration: acceptedGeneration },
    "the rerun failure must start from active replay, not an already-finished timeline");
  diagnostics.snapshots.rerunError = await waitForRunResult(page, { expectedState: "error" });
  assert.equal(diagnostics.snapshots.rerunError.problemTitle, "Python raised ValueError");
  await assertStoppedPreview(page, diagnostics.snapshots.rerunError);
  await page.waitForFunction(() => {
    const overlay = document.querySelector("#last-successful-preview");
    const image = overlay?.querySelector("img");
    return overlay?.hidden === false && image?.complete && image.naturalWidth > 0;
  });
  const rerunPixels = await previewPixels(page, true);
  assert.ok(rerunPixels.different > 50, "the failed rerun must retain visible scene pixels");
  // Running-frame capture is not compared to a different live instant. Check
  // its accepted source identity and stability; earlier paused cases compare
  // against independent exact displayed-pixel references.
  await assertFrozenPreview(page, { pixels: rerunPixels, runGeneration: acceptedGeneration });
  phase = "explicit-rerun-recovery";
  await page.locator("#replace-scene").click();
  diagnostics.snapshots.rerunRecovered = await waitForRunResult(page, {
    expectedState: "applied", expectedObjectCount: 1,
  });
  assertDiagnosticCleared(diagnostics.snapshots.rerunRecovered);
  assert.equal(await page.locator("#last-successful-preview").isHidden(), true);

  const syntaxConsole = diagnostics.consoleErrors.filter((entry) => entry.phase === "syntax-error");
  const runtimeConsole = diagnostics.consoleErrors.filter((entry) => entry.phase === "runtime-error");
  const rerunConsole = diagnostics.consoleErrors.filter((entry) => entry.phase === "explicit-rerun-error");
  const unexpectedConsole = diagnostics.consoleErrors.filter(
    (entry) => entry.phase !== "syntax-error" && entry.phase !== "runtime-error" && entry.phase !== "explicit-rerun-error",
  );
  assert.ok(rerunConsole.some((entry) => /Intentional explicit rerun failure/.test(entry.text)),
    `the explicit rerun should expose its actual exception: ${JSON.stringify(rerunConsole)}`);
  assert.ok(
    syntaxConsole.some((entry) => /SyntaxError|invalid syntax|expected ':'/i.test(entry.text)),
    `syntax failure should be diagnostic in the console: ${JSON.stringify(syntaxConsole)}`,
  );
  assert.ok(
    runtimeConsole.some((entry) => /NameError|MissingShape|not defined/i.test(entry.text)),
    `runtime failure should be diagnostic in the console: ${JSON.stringify(runtimeConsole)}`,
  );
  assert.deepEqual(unexpectedConsole, [], `valid runs emitted console errors: ${JSON.stringify(unexpectedConsole)}`);
  assert.deepEqual(diagnostics.pageErrors, [], `unhandled page errors: ${diagnostics.pageErrors.join("\n")}`);
  assert.equal(diagnostics.unexpectedNavigations, 0, "failure/recovery must not reload the Playground");

  diagnostics.serverOutput = serverOutput;
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  console.log(
    `playground live authoring recovery ok: ${diagnostics.snapshots.baseline.objectCount} -> ` +
      `${diagnostics.snapshots.syntaxRecovered.objectCount} -> ${diagnostics.snapshots.runtimeRecovered.objectCount} objects`,
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
