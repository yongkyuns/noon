import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

import playwright from "playwright";

const { chromium } = playwright;
const port = Number(process.env.NOON_PLAYGROUND_AUTHORING_RECOVERY_PORT ?? "4185");
const baseUrl = `http://127.0.0.1:${port}`;
const presentationProfile = process.env.NOON_PLAYGROUND_AUTHORING_RECOVERY_PROFILE ?? "default";
assert.ok(["default", "live"].includes(presentationProfile));
const liveComparison = presentationProfile === "live";
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

class ExplicitRerun(Scene):
    def construct(self):
        square = Square()
        self.add(square)
        self.play(square.animate.shift(RIGHT), run_time=3)
`,
  explicitRerunFailure: `from noon import *

class ExplicitRerunFailure(Scene):
    def construct(self):
        raise ValueError("Intentional edited-draft failure")
`,
  taintMutation: `from noon import *

class TaintLibrary(Scene):
    def construct(self):
        Square.shift = lambda self, *args, **kwargs: self
        self.add(Square())
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
    const draft = document.querySelector("#draft-status");
    const executionDiagnostics = window.__noonExampleGallery?.executionDiagnostics ?? null;
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
      runGeneration: patch?.dataset.runGeneration ?? "",
      draftState: draft?.dataset.state ?? "",
      draftText: draft?.value ?? draft?.textContent ?? "",
      activeDraftRunGeneration: draft?.dataset.activeRunGeneration ?? "",
      executionDiagnostics,
      interpreterGeneration: executionDiagnostics?.interpreterGeneration ?? null,
      runRequestId: executionDiagnostics?.runRequestId ?? null,
      authoringRunCount: window.__recoveryAuthoringRuns?.length ?? 0,
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

async function workerEvidence(page) {
  return page.evaluate(() => ({
    workers: window.__recoveryWorkerEvents ?? [],
    runs: window.__recoveryAuthoringRuns ?? [],
  }));
}

function assertRunEvidence(evidence, beforeCount, source, observed, previousRunGeneration) {
  assert.equal(evidence.runs.length, beforeCount + 1, "one explicit Run must submit source exactly once");
  const run = evidence.runs.at(-1);
  assert.equal(run.source, source, "Run must capture the latest editor draft");
  assert.equal(Number(observed.runRequestId), run.requestId,
    "diagnostic trace must belong to the source request submitted by this Run");
  assert.equal(observed.executionDiagnostics.context.exampleId, observed.exampleId);
  assert.ok(Number(observed.executionDiagnostics.context.runGeneration) > previousRunGeneration,
    "diagnostics must be bound to a newer explicit Run generation");
  if (observed.patchState === "applied") {
    assert.equal(Number(observed.executionDiagnostics.context.runGeneration), Number(observed.runGeneration),
      "an adopted source trace must match the active Run generation");
  }
  assert.equal(observed.executionDiagnostics.requestedProfile, run.preparedPlayback ? "incremental" : "observed");
  assert.equal(Number(observed.interpreterGeneration) >= 0, true,
    "diagnostic trace must identify the interpreter generation");
  const messages = evidence.workers.filter(message => message.workerId === run.workerId &&
    message.requestId === run.requestId);
  const ready = evidence.workers.find(message => message.workerId === run.workerId && message.type === "ready");
  const proof = messages.find(message => message.type === "execution_diagnostic" &&
    ["source-proof", "interpreter-trust"].includes(message.event?.stage));
  const settled = messages.find(message => message.type === "result" || message.type === "error");
  const failed = messages.find(message => message.type === "execution_diagnostic" && message.event?.stage === "failed");
  assert.ok(ready && ready.libraryTrust === "pristine" && ready.sequence < run.sequence,
    "worker must report pristine before accepting the explicit source Run");
  const decision = proof ?? failed;
  assert.ok(decision && decision.sequence > run.sequence && settled && decision.sequence < settled.sequence,
    "run must report its source proof/trust or source failure before settling");
  return { run, ready, proof: decision };
}

async function assertDraftOnly(page, before, expectedSource) {
  const after = await snapshot(page);
  const evidence = await workerEvidence(page);
  assert.equal(await page.evaluate(() => document.querySelector("#python-scene-source").value), expectedSource);
  assert.equal(after.runGeneration, before.runGeneration, "draft edits cannot advance the Run generation");
  assert.equal(after.interpreterGeneration, before.interpreterGeneration,
    "typing, IME and Reset cannot replace the interpreter");
  assert.equal(evidence.runs.length, before.authoringRunCount, "draft edits cannot invoke Python");
  assert.equal(after.canvasHidden, before.canvasHidden, "draft edits cannot hide or replace the active preview");
  assert.equal(after.exampleId, before.exampleId, "draft edits cannot change the selected example");
  assert.equal(after.patchText, before.patchText, "draft edits cannot replace active execution status");
  assert.equal(after.patchState, before.patchState);
  assert.equal(after.runDisabled, before.runDisabled, "editing cannot disable the explicit Run action");
  return after;
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
  const duration = Number(await page.locator(".playback-scrubber").getAttribute("max"));
  if (duration === 0) {
    assert.equal(await toggle.isDisabled(), true, "an exact-zero scene has no play interval");
    assert.equal(await page.locator(".playback-scrubber").isDisabled(), true);
    assert.equal(Number(await page.locator(".playback-controls").getAttribute("data-elapsed-seconds")), 0);
  } else {
    assert.equal(await toggle.isEnabled(), true, "a positive replay interval must remain controllable");
    if (await toggle.getAttribute("aria-label") === "Pause animation") await toggle.click();
  }
  // Observe the compositor after pause or exact-zero completion.
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

async function waitForRunResult(page, { expectedState, expectedObjectCount = null, runGenerationUnchanged = null }) {
  await page.waitForFunction(
    ({ state, objectCount, runGenerationUnchanged }) => {
      const patch = document.querySelector("#patch-status");
      const run = document.querySelector("#replace-scene");
      if (patch?.dataset.state !== state || run?.disabled) return false;
      const diagnostics = window.__noonExampleGallery?.executionDiagnostics;
      if (runGenerationUnchanged !== null &&
          String(diagnostics?.context?.runGeneration) === runGenerationUnchanged) return false;
      if (state === "applied" &&
          patch.dataset.runGeneration !== String(diagnostics?.context?.runGeneration)) return false;
      if (objectCount === null) return true;
      return document.querySelector("#metric-objects")?.value === String(objectCount);
    },
    { state: expectedState, objectCount: expectedObjectCount, runGenerationUnchanged },
    { timeout: 60_000 },
  );
  return snapshot(page);
}

async function runEditedSource(page, options) {
  const source = await page.evaluate(() => document.querySelector("#python-scene-source").value);
  const before = await workerEvidence(page);
  const generationBefore = Number((await snapshot(page)).executionDiagnostics?.context?.runGeneration ?? 0);
  await page.locator("#replace-scene").click();
  await page.waitForFunction(count => (window.__recoveryAuthoringRuns?.length ?? 0) === count + 1,
    before.runs.length, { timeout: 30_000 });
  const result = await waitForRunResult(page, { ...options, runGenerationUnchanged: String(generationBefore) });
  const evidence = await workerEvidence(page);
  const { run, ready, proof } = assertRunEvidence(evidence, before.runs.length, source, result, generationBefore);
  result.runEvidence = { run: { workerId: run.workerId, requestId: run.requestId,
    sequence: run.sequence, sourceLength: run.source.length }, ready, proof };
  return result;
}

async function selectGalleryExample(page, id, title) {
  const trigger = page.locator("#example-browser-trigger");
  if (await trigger.getAttribute("aria-expanded") !== "true") await trigger.click();
  const search = page.locator('.example-browser-layer input[type="search"]');
  await search.fill(title);
  const card = page.locator(`.example-card[data-example-id="${id}"]`);
  await card.waitFor({ state: "visible" });
  const before = await workerEvidence(page);
  const beforeState = await snapshot(page);
  await card.click();
  await page.waitForFunction(({ count, id }) =>
    (window.__recoveryAuthoringRuns?.length ?? 0) === count + 1 &&
    window.__noonExampleGallery?.selectedExampleId === id,
  { count: before.runs.length, id }, { timeout: 30_000 });
  await page.waitForFunction(previousGeneration => !window.__noonExampleGallery?.runInFlight &&
    document.querySelector("#patch-status")?.dataset.state === "applied" &&
    document.querySelector("#replace-scene")?.disabled === false &&
    String(window.__noonExampleGallery?.executionDiagnostics?.context?.runGeneration) !== previousGeneration &&
    document.querySelector("#patch-status")?.dataset.runGeneration ===
      String(window.__noonExampleGallery?.executionDiagnostics?.context?.runGeneration),
  String(beforeState.executionDiagnostics?.context?.runGeneration ?? 0), { timeout: 120_000 });
  const result = await snapshot(page);
  const evidence = await workerEvidence(page);
  const run = evidence.runs.at(-1);
  assert.equal(evidence.runs.length, before.runs.length + 1, "gallery Run must submit source exactly once");
  assert.equal(Number(result.runRequestId), run.requestId);
  assert.equal(Number(result.interpreterGeneration) >= 0, true);
  assert.equal(result.executionDiagnostics.context.exampleId, result.exampleId);
  assert.equal(Number(result.executionDiagnostics.context.runGeneration), Number(result.runGeneration));
  assert.equal(result.executionDiagnostics.requestedProfile, run.preparedPlayback ? "incremental" : "observed");
  const messages = evidence.workers.filter(message => message.workerId === run.workerId &&
    message.requestId === run.requestId);
  const ready = evidence.workers.find(message => message.workerId === run.workerId && message.type === "ready");
  const proof = messages.find(message => message.type === "execution_diagnostic" &&
    ["source-proof", "interpreter-trust"].includes(message.event?.stage));
  const settled = messages.find(message => message.type === "result" || message.type === "error");
  assert.ok(ready && ready.libraryTrust === "pristine" && ready.sequence < run.sequence);
  assert.ok(proof && proof.sequence > run.sequence && settled && proof.sequence < settled.sequence);
  assert.equal(await page.evaluate(() => document.querySelector("#python-scene-source").value), run.source,
    "gallery Run must capture the selected example's source bytes");
  result.runEvidence = { run: { workerId: run.workerId, requestId: run.requestId,
    sequence: run.sequence, sourceLength: run.source.length }, ready, proof };
  return result;
}

async function trackFullNavigations(targetPage) {
  let documentStart = await targetPage.evaluate(() => performance.timeOrigin);
  targetPage.on("framenavigated", frame => {
    if (frame !== targetPage.mainFrame()) return;
    void targetPage.evaluate(() => performance.timeOrigin).then(nextStart => {
      if (nextStart !== documentStart) diagnostics.unexpectedNavigations += 1;
      documentStart = nextStart;
    }).catch(() => {});
  });
}

async function waitForObjectCount(page, count) {
  await page.waitForFunction(
    (expected) => document.querySelector("#metric-objects")?.value === String(expected),
    count,
    { timeout: 10_000 },
  );
}

const diagnostics = {
  presentationProfile,
  browser: null,
  viewport: { width: 1280, height: 820 },
  devicePixelRatio: 1,
  snapshots: {},
  pageErrors: [],
  consoleErrors: [],
  failedRequests: [],
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
  await context.addInitScript(({ preparedPlayback }) => {
    window.__recoveryAuthoringRuns = [];
    window.__recoveryWorkerEvents = [];
    let sequence = 0, workerId = 0;
    window.Worker = new Proxy(window.Worker, {
      construct(target, args, newTarget) {
        const id = workerId++;
        const workerUrl = new URL(args[0], window.location.href).href;
        const worker = Reflect.construct(target, args, newTarget);
        const originalPostMessage = worker.postMessage.bind(worker);
        worker.postMessage = (data, ...rest) => {
          if (data?.channel === "noon.authoring" && data.type === "run") {
            window.__recoveryAuthoringRuns.push({ workerId: id, requestId: data.requestId,
              source: data.source, preparedPlayback: window.__recoveryPreparedPlayback, sequence: sequence++ });
          }
          return originalPostMessage(data, ...rest);
        };
        worker.addEventListener("message", ({ data }) => {
          if (data?.channel !== "noon.authoring" || !["ready", "execution_diagnostic", "result", "error"].includes(data.type)) return;
          window.__recoveryWorkerEvents.push({ workerId: id, workerUrl: workerUrl.slice(0, 400),
            type: data.type, requestId: data.requestId ?? null, libraryTrust: data.libraryTrust ?? null,
            event: data.event ?? null, sequence: sequence++ });
          if (window.__recoveryWorkerEvents.length > 1000) window.__recoveryWorkerEvents.shift();
        });
        return worker;
      },
    });
    window.__recoveryPreparedPlayback = preparedPlayback;
    window.__NOON_PLAYGROUND_TEST_HOOKS__ = {
      authoringRunOptions: () => ({ preparedPlayback: window.__recoveryPreparedPlayback }),
    };
  }, { preparedPlayback: !liveComparison });
  page = await context.newPage();
  page.on("pageerror", (error) => diagnostics.pageErrors.push(String(error)));
  page.on("requestfailed", request => diagnostics.failedRequests.push({
    phase, url: request.url(), error: request.failure()?.errorText ?? "request failed",
  }));
  page.on("console", (message) => {
    if (message.type() === "error") diagnostics.consoleErrors.push({ phase, text: message.text() });
  });

  diagnostics.browser = await browser.version();
  await page.goto(`${baseUrl}/web/index.html?example=parity-create-circle`, { waitUntil: "load" });
  await waitForInitialScene(page);
  await waitForObjectCount(page, 1);
  await trackFullNavigations(page);

  phase = "baseline";
  diagnostics.snapshots.baseline = await snapshot(page);
  assert.notEqual(diagnostics.snapshots.baseline.runtimeState, "error");
  assert.equal(diagnostics.snapshots.baseline.liveAuthoring, "ready");
  assert.equal(diagnostics.snapshots.baseline.rendererBackend, "WebGL2");
  assert.equal(diagnostics.snapshots.baseline.exampleId, "parity-create-circle");
  assert.notEqual(diagnostics.snapshots.baseline.patchSequence, "");
  await page.screenshot({ path: path.join(artifactDir, "baseline.png"), fullPage: true });

  diagnostics.previewBaseline = await pauseAndRecordPreview(page);
  const draftBaseline = await snapshot(page);
  const canonicalSource = await page.evaluate(() => document.querySelector("#python-scene-source").value);
  phase = "draft-only-ime-reset";
  await page.evaluate(() => document.querySelector("#python-scene-source").dispatchEvent(
    new CompositionEvent("compositionstart", { bubbles: true })));
  await setEditorSource(page, SOURCES.twoObjects);
  let edited = await assertDraftOnly(page, draftBaseline, SOURCES.twoObjects);
  await page.evaluate(() => document.querySelector("#python-scene-source").dispatchEvent(
    new CompositionEvent("compositionend", { bubbles: true })));
  edited = await assertDraftOnly(page, draftBaseline, SOURCES.twoObjects);
  assert.equal(edited.draftState, "unapplied");
  assert.match(edited.draftText, /draft not applied.*Run to apply/);
  await page.locator(".reset-example").click();
  await page.waitForFunction(expected => document.querySelector("#python-scene-source").value === expected,
    canonicalSource, { timeout: 10_000 });
  await assertDraftOnly(page, draftBaseline, canonicalSource);

  phase = "syntax-error";
  await setEditorSource(page, SOURCES.syntaxError);
  edited = await assertDraftOnly(page, draftBaseline, SOURCES.syntaxError);
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
  edited = await assertDraftOnly(page, diagnostics.snapshots.syntaxError, SOURCES.twoObjects);
  assertDiagnosticCleared(edited);
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
  edited = await assertDraftOnly(page, diagnostics.snapshots.syntaxRecovered, SOURCES.runtimeError);
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
  edited = await assertDraftOnly(page, diagnostics.snapshots.runtimeError, SOURCES.oneObject);
  assertDiagnosticCleared(edited);
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

  phase = "taint-mutation";
  await page.evaluate(() => { window.__recoveryPreparedPlayback = false; });
  await setEditorSource(page, SOURCES.taintMutation);
  edited = await assertDraftOnly(page, diagnostics.snapshots.runtimeRecovered, SOURCES.taintMutation);
  diagnostics.snapshots.taintMutation = await runEditedSource(page, {
    expectedState: "applied", expectedObjectCount: 1,
  });
  diagnostics.previewTainted = await pauseAndRecordPreview(page);
  assert.equal(diagnostics.snapshots.taintMutation.executionDiagnostics.requestedProfile, "observed");
  assert.equal(diagnostics.snapshots.taintMutation.runEvidence.proof.libraryTrust, "tainted",
    "opaque source execution must report monotonic interpreter taint");

  phase = "taint-recovery";
  await page.evaluate(() => { window.__recoveryPreparedPlayback = true; });
  await setEditorSource(page, SOURCES.oneObject);
  edited = await assertDraftOnly(page, diagnostics.snapshots.taintMutation, SOURCES.oneObject);
  diagnostics.snapshots.taintRecovered = await runEditedSource(page, {
    expectedState: "applied", expectedObjectCount: 1,
  });
  assert.ok(diagnostics.snapshots.taintRecovered.interpreterGeneration >
    diagnostics.snapshots.taintMutation.interpreterGeneration,
  "a pure Run after opaque mutation must use a fresh interpreter generation");
  assert.notEqual(diagnostics.snapshots.taintRecovered.runEvidence.run.workerId,
    diagnostics.snapshots.taintMutation.runEvidence.run.workerId);
  assert.equal(diagnostics.snapshots.taintRecovered.runEvidence.ready.libraryTrust, "pristine");
  assert.equal(diagnostics.snapshots.taintRecovered.runEvidence.proof.libraryTrust, "pristine");
  assert.equal(diagnostics.snapshots.taintRecovered.runEvidence.proof.event.admitted, true,
    "the fresh interpreter must prove the clean source before dependencies execute");
  assert.ok(diagnostics.snapshots.taintRecovered.executionDiagnostics.events.some(event =>
    event.stage === "attached"), "the fresh source must attach to its actual Rust-owned profile");
  diagnostics.previewTaintRecovered = await pauseAndRecordPreview(page);
  assert.notEqual(diagnostics.previewTaintRecovered.pixels.digest, diagnostics.previewTainted.pixels.digest,
    "the fresh Run must display its own scene, not reactivate the tainted worker's scene");

  phase = "same-page-pixels-to-first-scene";
  await page.close();
  page = await context.newPage();
  page.on("pageerror", (error) => diagnostics.pageErrors.push(String(error)));
  page.on("requestfailed", request => diagnostics.failedRequests.push({
    phase, url: request.url(), error: request.failure()?.errorText ?? "request failed",
  }));
  page.on("console", (message) => {
    if (message.type() === "error") diagnostics.consoleErrors.push({ phase, text: message.text() });
  });
  await page.goto(`${baseUrl}/web/index.html?example=showcase-first-scene`, { waitUntil: "load" });
  await waitForInitialScene(page);
  await trackFullNavigations(page);
  diagnostics.snapshots.pixels = await selectGalleryExample(page, "showcase-raster-images", "Pixels in the scene");
  assert.equal(diagnostics.snapshots.pixels.exampleId, "showcase-raster-images");
  const pixelProof = diagnostics.snapshots.pixels.runEvidence.proof;
  assert.equal(pixelProof.libraryTrust, "tainted",
    "Pixels in the scene must execute through an opaque source proof on this baseline");
  assert.equal(pixelProof.event.admitted, false);
  const pixelGeneration = diagnostics.snapshots.pixels.interpreterGeneration;
  diagnostics.snapshots.firstSceneAfterPixels = await selectGalleryExample(page, "showcase-first-scene", "Your first scene");
  assert.equal(diagnostics.snapshots.firstSceneAfterPixels.exampleId, "showcase-first-scene");
  assert.ok(diagnostics.snapshots.firstSceneAfterPixels.interpreterGeneration > pixelGeneration,
    "same-page Pixels → Your first scene must not reuse opaque interpreter state");
  assert.equal(diagnostics.snapshots.firstSceneAfterPixels.runEvidence.ready.libraryTrust, "pristine");
  assert.equal(diagnostics.snapshots.firstSceneAfterPixels.runEvidence.proof.libraryTrust, "pristine");
  assert.equal(diagnostics.snapshots.firstSceneAfterPixels.runEvidence.proof.event.admitted, true);
  assert.ok(diagnostics.snapshots.firstSceneAfterPixels.executionDiagnostics.events.some(event =>
    event.stage === "attached" && event.profile === "incremental"));

  // Replaying a retained timeline does not rerun source. A later explicit Run
  // applies the latest draft once while the prior playback is active.
  phase = "explicit-rerun-baseline";
  await page.evaluate(() => { window.__recoveryPreparedPlayback = true; });
  await setEditorSource(page, SOURCES.explicitRerun);
  diagnostics.snapshots.rerunBaseline = await runEditedSource(page, {
    expectedState: "applied", expectedObjectCount: 1,
  });
  const acceptedGeneration = await page.locator("#patch-status").getAttribute("data-run-generation");
  await startReplayForExplicitRerun(page);
  diagnostics.snapshots.rerunReady = await snapshot(page);
  assert.equal(await page.locator("#patch-status").getAttribute("data-run-generation"), acceptedGeneration,
    "replaying the timeline must not rerun Python");
  // CodeMirror exposes the source through a main-world textarea getter.
  // Playwright inputValue reads the isolated world's stale native fallback.
  assert.equal(await page.evaluate(() => document.querySelector("#python-scene-source").value), SOURCES.explicitRerun);
  assert.equal(diagnostics.snapshots.rerunReady.authoringRunCount,
    diagnostics.snapshots.rerunBaseline.authoringRunCount,
  "Replay controls must not invoke source again");
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
  await setEditorSource(page, SOURCES.explicitRerunFailure);
  edited = await assertDraftOnly(page, diagnostics.snapshots.rerunReady, SOURCES.explicitRerunFailure);
  assert.equal(edited.playing, true, "editing the replaying draft cannot pause the active preview");
  diagnostics.snapshots.rerunError = await runEditedSource(page, { expectedState: "error" });
  diagnostics.rerunClick = await page.evaluate(() => window.__noonPreviewRerunClick);
  assert.deepEqual(diagnostics.rerunClick, { playing: true, busy: false, runGeneration: acceptedGeneration },
    "the rerun must begin while the retained replay is actively playing");
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
  await setEditorSource(page, SOURCES.oneObject);
  edited = await assertDraftOnly(page, diagnostics.snapshots.rerunError, SOURCES.oneObject);
  diagnostics.snapshots.rerunRecovered = await runEditedSource(page, {
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
  assert.ok(rerunConsole.some((entry) => /Intentional edited-draft failure/.test(entry.text)),
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
