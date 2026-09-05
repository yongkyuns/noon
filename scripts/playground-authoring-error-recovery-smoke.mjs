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
        self.add(MissingShape())
`,
  oneObject: `from noon import *

class FinalRecovery(Scene):
    def construct(self):
        self.add(Circle())
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
    };
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
        status?.dataset.state === "running" &&
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
  await page.keyboard.press("Control+A");
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

async function waitForAutomaticRun(page, { expectedState, expectedObjectCount = null }) {
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
  assert.equal(diagnostics.snapshots.baseline.runtimeState, "running");
  assert.equal(diagnostics.snapshots.baseline.liveAuthoring, "ready");
  assert.equal(diagnostics.snapshots.baseline.rendererBackend, "WebGL2");
  assert.equal(diagnostics.snapshots.baseline.exampleId, "parity-create-circle");
  assert.notEqual(diagnostics.snapshots.baseline.patchSequence, "");
  await page.screenshot({ path: path.join(artifactDir, "baseline.png"), fullPage: true });

  phase = "syntax-error";
  await setEditorSource(page, SOURCES.syntaxError);
  diagnostics.snapshots.syntaxError = await waitForAutomaticRun(page, { expectedState: "error" });
  assert.match(diagnostics.snapshots.syntaxError.patchText, /Python failed:/);
  assert.equal(diagnostics.snapshots.syntaxError.runtimeState, "running");
  assert.equal(diagnostics.snapshots.syntaxError.rendererBackend, diagnostics.snapshots.baseline.rendererBackend);
  assert.equal(
    diagnostics.snapshots.syntaxError.patchSequence,
    diagnostics.snapshots.baseline.patchSequence,
    "syntax failure must not consume the current semantic generation's patch sequence",
  );
  assert.equal(
    diagnostics.snapshots.syntaxError.objectCount,
    diagnostics.snapshots.baseline.objectCount,
    "syntax failure must leave the last good scene visible",
  );
  await page.screenshot({ path: path.join(artifactDir, "syntax-error.png"), fullPage: true });

  phase = "syntax-recovery";
  await setEditorSource(page, SOURCES.twoObjects);
  diagnostics.snapshots.syntaxRecovered = await waitForAutomaticRun(page, {
    expectedState: "applied",
    expectedObjectCount: 2,
  });
  assert.match(diagnostics.snapshots.syntaxRecovered.patchText, /2 objects/);
  assert.equal(diagnostics.snapshots.syntaxRecovered.runtimeState, "running");
  assert.equal(diagnostics.snapshots.syntaxRecovered.rendererBackend, "WebGL2");

  phase = "runtime-error";
  await setEditorSource(page, SOURCES.runtimeError);
  diagnostics.snapshots.runtimeError = await waitForAutomaticRun(page, { expectedState: "error" });
  assert.match(diagnostics.snapshots.runtimeError.patchText, /Python failed:/);
  assert.equal(diagnostics.snapshots.runtimeError.runtimeState, "running");
  assert.equal(diagnostics.snapshots.runtimeError.rendererBackend, "WebGL2");
  assert.equal(
    diagnostics.snapshots.runtimeError.patchSequence,
    diagnostics.snapshots.syntaxRecovered.patchSequence,
    "execution-time authoring failure must not consume the current semantic generation's patch sequence",
  );
  assert.equal(
    diagnostics.snapshots.runtimeError.objectCount,
    "2",
    "execution-time authoring failure must preserve the last successful two-object scene",
  );
  await page.screenshot({ path: path.join(artifactDir, "runtime-error.png"), fullPage: true });

  phase = "runtime-recovery";
  await setEditorSource(page, SOURCES.oneObject);
  diagnostics.snapshots.runtimeRecovered = await waitForAutomaticRun(page, {
    expectedState: "applied",
    expectedObjectCount: 1,
  });
  assert.match(diagnostics.snapshots.runtimeRecovered.patchText, /1 objects/);
  assert.equal(diagnostics.snapshots.runtimeRecovered.runtimeState, "running");
  assert.equal(diagnostics.snapshots.runtimeRecovered.rendererBackend, "WebGL2");
  await page.screenshot({ path: path.join(artifactDir, "recovered.png"), fullPage: true });

  const syntaxConsole = diagnostics.consoleErrors.filter((entry) => entry.phase === "syntax-error");
  const runtimeConsole = diagnostics.consoleErrors.filter((entry) => entry.phase === "runtime-error");
  const unexpectedConsole = diagnostics.consoleErrors.filter(
    (entry) => entry.phase !== "syntax-error" && entry.phase !== "runtime-error",
  );
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
