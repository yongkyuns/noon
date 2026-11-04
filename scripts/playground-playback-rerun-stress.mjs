import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { createProcessTreeRssSampler } from "./playground-cold-start-memory.mjs";

const { chromium } = playwright;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.env.NOON_PLAYGROUND_RERUN_STRESS_PORT ?? "4188");
const baseUrl = `http://127.0.0.1:${port}`;
const profile = process.env.NOON_PLAYGROUND_RERUN_STRESS_PROFILE ?? "default";
assert.ok(["default", "live"].includes(profile));
const liveComparison = profile === "live";
const memoryEnabled = process.env.NOON_PLAYGROUND_RERUN_STRESS_MEMORY === "1";
const wasmAccountingEnabled = process.env.NOON_PLAYGROUND_RERUN_STRESS_WASM_ACCOUNTING === "1";
assert.ok(!wasmAccountingEnabled || memoryEnabled,
  "WASM accounting requires NOON_PLAYGROUND_RERUN_STRESS_MEMORY=1");
const exampleId = process.env.NOON_PLAYGROUND_RERUN_STRESS_EXAMPLE_ID ?? "parity-create-circle";
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_PLAYGROUND_RERUN_STRESS_ARTIFACTS ??
    "browser-smoke-artifacts/playground-playback-rerun-stress",
);

await mkdir(artifactDir, { recursive: true });

const sha256 = value => createHash("sha256").update(value).digest("hex");
const workerIds = new WeakMap();
let nextWorkerId = 0;
function workerIdentity(worker) {
  if (!workerIds.has(worker)) workerIds.set(worker, ++nextWorkerId);
  return { id: workerIds.get(worker), url: worker.url() };
}
async function memoryBuildIdentity() {
  const manifestNames = ["manim_tutorial_manifest.json", "manim_stress_manifest.json", "noon_showcase_manifest.json"];
  let sourceEntry = null, manifestHash = null;
  for (const name of manifestNames) {
    const bytes = await readFile(path.join(repoRoot, "web/python/examples", name));
    const entry = JSON.parse(bytes).entries.find(item => item.id === exampleId);
    if (entry) { sourceEntry = entry; manifestHash = sha256(bytes); break; }
  }
  assert.ok(sourceEntry, `no tracked Manim fixture found for ${exampleId}`);
  const sourceBytes = await readFile(path.join(repoRoot, "web", sourceEntry.path));
  const identityBytes = await readFile(path.join(repoRoot, "web/runtime-build-identity.json"));
  const identity = JSON.parse(identityBytes);
  return { exampleId, sourceFixture: sourceEntry.path, sourceFixtureSha256: sha256(sourceBytes),
    sourceManifestSha256: manifestHash, buildIdentitySha256: sha256(identityBytes),
    sourceRevision: identity.sourceRevision, buildId: identity.buildId, packageAssets: identity.files };
}

const memory = memoryEnabled ? { enabled: true, build: await memoryBuildIdentity(), marks: [],
  status: "running", errors: [],
  ...(wasmAccountingEnabled ? { wasmCapacityDiagnostic: {
    enabled: true, label: "invasive Noon wasm-bindgen linear-memory capacity diagnostic",
    workerEntrypoints: {},
    comparisonCaveat: "Do not compare timing or RSS with an uninstrumented cohort.",
    metricCaveat: "linear-memory byteLength is capacity, not live allocation or total memory.",
  } } : {}),
  unaccounted: ["JavaScript live heap", "GPU/out-of-process allocations", "WASM live allocator",
    "Pyodide WASM linear-memory capacity and heap",
    ...(!wasmAccountingEnabled ? ["Noon wasm-bindgen linear-memory capacity"] : [])],
  caveat: "Rust retained charges are category-local logical bytes; sampled Chromium process-tree RSS is separate and may double-count shared pages. The opt-in observer covers Noon wasm-bindgen memory only, not Pyodide memory." }
  : null;

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
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
  throw new Error(`Playground rerun stress server did not start: ${lastError}\n${serverOutput}`);
}

async function waitForApplied(page, exampleId, timeout = 60_000) {
  await page.waitForFunction(
    (id) => {
      const patch = document.querySelector("#patch-status");
      return (
        patch?.dataset.exampleId === id &&
        (patch.dataset.state === "applied" || patch.dataset.state === "error")
      );
    },
    exampleId,
    { timeout },
  );
  const patch = await page.evaluate(() => ({
    state: document.querySelector("#patch-status")?.dataset.state ?? "",
    text:
      document.querySelector("#patch-status")?.value ??
      document.querySelector("#patch-status")?.textContent ??
      "",
  }));
  assert.equal(patch.state, "applied", `${exampleId} failed: ${patch.text}`);
}

async function snapshot(page) {
  return page.evaluate(() => {
    const controls = document.querySelector(".playback-controls");
    const scrubber = document.querySelector(".playback-scrubber");
    const patch = document.querySelector("#patch-status");
    const status = document.querySelector("#status");
    const canvas = document.querySelector("#scene");
    const metricTime =
      document.querySelector("#metric-time")?.value ??
      document.querySelector("#metric-time")?.textContent ??
      "";
    return {
      playing: controls?.dataset.playing ?? null,
      controllable: controls?.dataset.controllable ?? null,
      scrubberDisabled: scrubber?.disabled ?? true,
      busy: controls?.dataset.busy ?? null,
      scrubberValue: Number(scrubber?.value ?? NaN),
      scrubberMax: Number(scrubber?.max ?? NaN),
      patchState: patch?.dataset.state ?? "",
      patchExample: patch?.dataset.exampleId ?? "",
      metricTime,
      canvasIdentity: canvas?.dataset.rerunStressIdentity ?? null,
      canvasCount: document.querySelectorAll("canvas").length,
      rendererBackend: status?.dataset.rendererBackend ?? null,
      runtimeState: status?.dataset.state ?? null,
      runtimeStartup: status?.dataset.runtimeStartup ?? "",
      playbackAvailability: status?.dataset.playbackControls ?? null,
      generationDiagnostics: window.__noonExampleGallery?.generationDiagnostics ?? null,
      runInFlight: window.__noonExampleGallery?.runInFlight ?? false,
      authoringCount: window.__noonRerunStress?.authoringCount ?? 0,
      sourceOwnedTransitions: window.__noonRerunStress?.sourceOwnedTransitions?.length ?? 0,
      durationKnown: controls?.dataset.durationKnown,
      buffering: controls?.dataset.buffering,
      elapsedSeconds: Number(controls?.dataset.elapsedSeconds),
      seekEnd: Number(scrubber?.max),
      seekStatus: controls?.querySelector(".playback-live-status")?.textContent ?? "",
    };
  });
}

async function memoryMark(page, phase) {
  if (!memoryEnabled) return;
  const state = await page.evaluate(async () => {
    const report = await window.__noonExampleGallery?.executionMetrics();
    const metrics = report?.metrics ?? {};
    const retention = report?.engineMetrics?.preparedRetention ?? null;
    const diagnostics = window.__noonExampleGallery?.executionDiagnostics;
    return { epochMs: performance.timeOrigin + performance.now(), executionIdentity: {
      runRequestId: diagnostics?.runRequestId ?? null,
      interpreterGeneration: diagnostics?.interpreterGeneration ?? null,
      runGeneration: window.__noonExampleGallery?.generationDiagnostics?.runGeneration ?? null },
    renderer: Object.fromEntries(["presentedFrames", "needsPresent", "bufferedDeltas", "bytesUploaded",
      "geometryCacheMisses"].map(name => [name, metrics[name] ?? null])), retention };
  });
  if (state.retention === null) memory.errors.push(`prepared retention metrics missing at ${phase}`);
  const workers = page.workers().map(workerIdentity);
  const wasmAccounting = wasmAccountingEnabled ? await wasmAccountingSnapshot(page.workers()) : undefined;
  if (wasmAccountingEnabled && (wasmAccounting.length === 0 ||
      wasmAccounting.some(item => item.accounting === null || item.error))) {
    memory.errors.push(`WASM capacity probe missing at ${phase}`);
  }
  memory.marks.push({ phase, ...state, workers, ...(wasmAccountingEnabled ? { wasmAccounting } : {}) });
}

async function installWasmCapacityProbe(page) {
  const entries = new Set(["/web/python-worker.js", "/web/execution-render-worker.js"]);
  memory.wasmCapacityDiagnostic.probeSha256 = sha256(
    await readFile(path.join(repoRoot, "web/playground-cold-start-wasm-accounting.js")));
  await page.route(url => entries.has(new URL(url).pathname), async route => {
    if (!["script", "worker"].includes(route.request().resourceType())) return route.continue();
    const response = await route.fetch();
    const original = await response.text();
    const executed = `import "./playground-cold-start-wasm-accounting.js";\n${original}`;
    const pathname = new URL(route.request().url()).pathname;
    memory.wasmCapacityDiagnostic.workerEntrypoints[pathname] = {
      originalSha256: sha256(original), executedSha256: sha256(executed),
    };
    const headers = { ...response.headers() };
    delete headers["content-length"]; delete headers["content-encoding"];
    await route.fulfill({ response, headers, body: executed });
  });
}

async function wasmAccountingSnapshot(workers) {
  const entries = workers.filter(worker => /\/(python-worker|execution-render-worker)\.js(?:\?|$)/.test(worker.url()));
  return Promise.all(entries.map(async worker => {
    const identity = workerIdentity(worker);
    try {
      const accounting = await worker.evaluate(() => {
        const probe = globalThis.__noonColdStartWasmAccounting;
        return probe ? { records: probe.records, linearMemory: probe.snapshot() } : null;
      });
      if (accounting === null || accounting.records.length === 0 || accounting.linearMemory.length === 0 ||
          accounting.linearMemory.some(memory => !memory.available || memory.samplingError !== null)) {
        return { ...identity, accounting, error: "probe or capacity sample unavailable" };
      }
      return { ...identity, accounting };
    } catch (error) { return { ...identity, error: String(error) }; }
  }));
}

async function pausedRendererWindow(page) {
  const read = () => page.evaluate(async () => {
    const report = await window.__noonExampleGallery?.executionMetrics();
    const metrics = report?.metrics ?? {};
    return Object.fromEntries(["presentedFrames", "needsPresent", "bufferedDeltas", "bytesUploaded",
      "geometryCacheMisses"].map(name => [name, metrics[name] ?? null]));
  });
  const before = await read();
  await page.waitForTimeout(1_000);
  const after = await read();
  const deltas = Object.fromEntries(["presentedFrames", "bufferedDeltas", "bytesUploaded", "geometryCacheMisses"]
    .map(name => [name, Number.isFinite(before[name]) && Number.isFinite(after[name])
      ? after[name] - before[name] : null]));
  memory.pausedRendererWindow = { durationMs: 1_000, before, after, deltas,
    claim: "renderer counters only; no CPU-idle claim" };
}

async function waitForInitialReplay(page, exampleId) {
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  // Default startup performs one intentional authoring preload while keeping
  // the renderer deferred. Retain that source pass as the initial replay.
  await waitForApplied(page, exampleId);
  await page.waitForFunction(() =>
    document.querySelector("#status")?.dataset.liveAuthoring === "ready" &&
    window.__noonExampleGallery?.runInFlight === false, null, { timeout: 60_000 });
  const deferred = await page.evaluate(() => window.__noonRerunStress.initialDeferred);
  assert.ok(deferred, "rerun stress never observed deferred startup");
  assert.equal(deferred.runtimeStartup, "deferred", "rerun stress must begin without a runtime");
  assert.equal(deferred.rendererBackend, null, "deferred rerun stress must not initialize a renderer");
  assert.equal(deferred.playing, null, "deferred rerun stress must not allocate playback controls");
  return deferred;
}

async function holdNextRun(page, exampleId) {
  await page.evaluate((id) => {
    const stress = window.__noonRerunStress;
    stress.holdExample = id;
    stress.release = null;
    stress.holdReached = false;
  }, exampleId);
}

async function waitForHeldRun(page) {
  await page.waitForFunction(() => window.__noonRerunStress?.holdReached === true, null, {
    timeout: 30_000,
  });
}

async function releaseHeldRun(page) {
  await page.evaluate(() => {
    const release = window.__noonRerunStress?.release;
    window.__noonRerunStress.release = null;
    release?.();
  });
}

async function waitForCompletedReplay(page, exampleId) {
  await waitForApplied(page, exampleId);
  await page.waitForFunction(
    ({ id, live }) => {
      const events = window.__noonExampleGallery?.executionDiagnostics?.events ?? [];
      const controls = document.querySelector(".playback-controls")?.dataset;
      const disposition = events.findLast(event => event.stage === "disposition")?.disposition;
      return document.querySelector("#status")?.dataset.playbackControls === "available" &&
        window.__noonExampleGallery?.selectedExampleId === id &&
        window.__noonExampleGallery?.runInFlight === false && controls?.busy === "false" &&
        controls?.durationKnown === "true" && controls?.buffering === "false" &&
        (live ? disposition === "observed-complete" :
          events.some(event => event.stage === "source-complete") &&
          events.some(event => event.stage === "terminal-adopted") &&
          events.some(event => event.stage === "disposition" && event.disposition === "looped" &&
            event.loopCount >= 1));
    },
    { id: exampleId, live: liveComparison },
  );
}

async function waitForSourceOwnedPass(page, previousTransitions) {
  await page.waitForFunction(
    (previous) =>
      window.__noonRerunStress?.sourceOwnedTransitions?.slice(previous).some(transition =>
        transition.runGeneration === window.__noonRerunStress.requestedGeneration &&
        transition.controllable !== "true" && transition.scrubberDisabled && transition.canvasHidden),
    previousTransitions,
    { timeout: 30_000 },
  );
}

let browser = null;
let browserServer = null;
let rssSampler = null;
let exampleContext = null;

async function pauseReplay(page) {
  const toggle = page.locator(".playback-toggle");
  if (await toggle.getAttribute("aria-label") === "Pause animation") await toggle.click();
  await page.waitForFunction(() => {
    const controls = document.querySelector(".playback-controls")?.dataset;
    return controls?.playing === "false" && controls.busy === "false";
  });
}

async function startMovingReplay(page) {
  await page.locator(".playback-restart").click();
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "false");
  if (await page.locator(".playback-toggle").getAttribute("aria-label") === "Play animation") {
    await page.locator(".playback-toggle").click();
  }
  await page.waitForFunction(() => {
    const controls = document.querySelector(".playback-controls")?.dataset;
    const time = Number(controls?.elapsedSeconds), end = Number(document.querySelector(".playback-scrubber")?.max);
    return controls?.playing === "true" && controls.busy === "false" && time > 0 && time < end / 2;
  });
  const first = await snapshot(page);
  await page.waitForFunction(before => {
    const time = Number(document.querySelector(".playback-controls")?.dataset.elapsedSeconds);
    return time > before && time < Number(document.querySelector(".playback-scrubber")?.max);
  }, first.elapsedSeconds);
}

function assertHeldControls(held) {
  assert.equal(held.playbackAvailability, liveComparison ? "unavailable" : "available");
  assert.equal(held.controllable, liveComparison ? "false" : "true");
  if (liveComparison) assert.equal(held.scrubberDisabled, true);
  else {
    assert.equal(held.buffering, "false");
    if (held.durationKnown === "true") {
      assert.equal(held.scrubberDisabled, false);
      assert.ok(held.elapsedSeconds >= 0 && held.elapsedSeconds <= held.scrubberMax);
    } else {
      assert.equal(held.durationKnown, "false");
      assert.equal(held.scrubberDisabled, false);
      assert.match(held.seekStatus, /Available history:/);
      assert.ok(held.elapsedSeconds >= 0 && held.elapsedSeconds <= held.seekEnd);
    }
  }
}
let page = null;
const pageErrors = [];
const consoleErrors = [];
const diagnostics = { profile, phases: [] };

try {
  await waitForServer();
  const launchOptions = {
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
  };
  if (memoryEnabled) {
    browserServer = await chromium.launchServer(launchOptions);
    browser = await chromium.connect(browserServer.wsEndpoint());
    rssSampler = createProcessTreeRssSampler(browserServer.process().pid);
    memory.rssClockOriginEpochMs = Date.now();
    rssSampler.start();
  } else browser = await chromium.launch(launchOptions);
  if (memoryEnabled) {
    exampleContext = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    page = await exampleContext.newPage();
  } else page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  if (wasmAccountingEnabled) await installWasmCapacityProbe(page);
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });

  await page.addInitScript(({ liveComparison }) => {
    window.__noonRerunStress = {
      authoringCount: 0,
      initialDeferred: null,
      holdExample: null,
      holdReached: false,
      release: null,
      sourceOwnedTransitions: [],
      requestedGeneration: null,
      runTriggers: [],
      beforeRun(playing) {
        const controls = document.querySelector(".playback-controls")?.dataset;
        const trigger = { playing: controls?.playing === "true", time: Number(controls?.elapsedSeconds),
          end: Number(document.querySelector(".playback-scrubber")?.max), generation: this.requestedGeneration };
        this.runTriggers.push(trigger);
        if (trigger.playing !== playing || (playing && !(trigger.time > 0 && trigger.time < trigger.end))) {
          throw new Error("Run trigger lost its paused or actively moving replay precondition");
        }
      },
    };
    window.__NOON_PLAYGROUND_TEST_HOOKS__ = {
      authoringRunOptions(payload) {
        window.__noonRerunStress.requestedGeneration = payload.runGeneration;
        return liveComparison ? { preparedPlayback: false } : undefined;
      },
      afterAuthoring(payload) {
        const stress = window.__noonRerunStress;
        stress.authoringCount += 1;
        if (stress.holdExample !== payload.exampleId) return undefined;
        stress.holdExample = null;
        stress.holdReached = true;
        return new Promise((resolve) => {
          stress.release = resolve;
        });
      },
    };
    const observeSourceOwnership = () => {
      const status = document.querySelector("#status");
      const stress = window.__noonRerunStress;
      if (status?.dataset.runtimeStartup === "deferred" && stress.initialDeferred === null) {
        stress.initialDeferred = {
          runtimeStartup: status.dataset.runtimeStartup,
          rendererBackend: status.dataset.rendererBackend ?? null,
          playing: document.querySelector(".playback-controls")?.dataset.playing ?? null,
        };
      }
      if (
        status?.dataset.playbackControls === "unavailable" &&
        window.__noonExampleGallery?.runInFlight === true
      ) {
        const latest = stress.sourceOwnedTransitions.at(-1);
        const runGeneration = stress.requestedGeneration;
        if (latest?.runGeneration !== runGeneration) {
          stress.sourceOwnedTransitions.push({ runGeneration, at: performance.now(),
            controllable: document.querySelector(".playback-controls")?.dataset.controllable ?? null,
            scrubberDisabled: document.querySelector(".playback-scrubber")?.disabled ?? true,
            canvasHidden: document.querySelector("#scene")?.style.visibility === "hidden" });
        }
      }
    };
    const sourceOwnershipObserver = new MutationObserver(observeSourceOwnership);
    sourceOwnershipObserver.observe(document, {
      attributes: true,
      subtree: true,
      attributeFilter: ["data-playback-controls", "data-runtime-startup"],
    });
    observeSourceOwnership();
  }, { liveComparison });

  await page.goto(`${baseUrl}/web/index.html?example=${exampleId}`, { waitUntil: "load" });
  diagnostics.deferred = await waitForInitialReplay(page, exampleId);
  await waitForCompletedReplay(page, exampleId);
  await pauseReplay(page);
  await page.waitForSelector(".playback-controls");
  await page.waitForFunction(() => document.querySelector(".playback-controls")?.dataset.busy === "false");
  await page.evaluate(() => {
    document.querySelector("#scene").dataset.rerunStressIdentity = "original";
  });

  const initial = await snapshot(page);
  const initialTrace = await page.evaluate(() => window.__noonExampleGallery.executionDiagnostics);
  diagnostics.initialTrace = initialTrace;
  diagnostics.phases.push({ phase: "initial", ...initial });
  assert.equal(initial.authoringCount, 1, "automatic preload must author the initial scene exactly once");
  assert.equal(initial.playing, "false", "explicit Pause must settle before the paused case");
  if (memoryEnabled) {
    await memoryMark(page, "initial-paused");
    await pausedRendererWindow(page);
  }
  assert.equal(initial.playbackAvailability, "available");
  const initialEvents = initialTrace?.events ?? [];
  assert.ok(initialEvents.some(event => event.stage === "source-proof" && event.admitted === true));
  assert.ok(initialEvents.some(event => event.stage === "attached" &&
    event.profile === (liveComparison ? "observed" : "incremental")));
  assert.ok(liveComparison
    ? initialEvents.some(event => event.stage === "disposition" && event.disposition === "observed-complete")
    : initialEvents.some(event => event.stage === "source-complete") &&
      initialEvents.some(event => event.stage === "terminal-adopted") &&
      initialEvents.some(event => event.stage === "disposition" && event.disposition === "looped" &&
        event.loopCount >= 1), "initial preload must reach its Rust completion/loop disposition");
  assert.equal(initial.canvasIdentity, "original");
  assert.equal(initial.canvasCount, 1);
  assert.ok(initial.rendererBackend === "WebGL2" || initial.rendererBackend === "WebGPU");
  assert.ok(Number.isFinite(initial.scrubberMax) && initial.scrubberMax > 0);

  // A completed replay is seekable while paused. The next explicit Run must
  // retire its old source before exposing a new admitted reader, without
  // replacing the canvas or wedging controls.
  const pausedBaselineCount = (await snapshot(page)).authoringCount;
  const pausedBaselineTransitions = (await snapshot(page)).sourceOwnedTransitions;
  await memoryMark(page, "before-paused-rerun");
  await holdNextRun(page, exampleId);
  const finalScrubTarget = await page.locator(".playback-scrubber").evaluate((input) => {
    const max = Number(input.max);
    let last = 0;
    for (let index = 0; index < 40; index += 1) {
      last = max * (((index * 17) % 37) + 1) / 38;
      input.value = String(last);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }
    window.__noonRerunStress.beforeRun(false);
    window.__pausedRunA = window.__noonExampleGallery.run();
    window.__pausedRunB = window.__noonExampleGallery.run();
    return last;
  });
  await waitForSourceOwnedPass(page, pausedBaselineTransitions);
  await waitForHeldRun(page);
  const pausedHeld = await snapshot(page);
  diagnostics.phases.push({ phase: "paused-held", target: finalScrubTarget, ...pausedHeld });
  assert.equal(pausedHeld.authoringCount, pausedBaselineCount + 1, "duplicate paused Run was not coalesced");
  assertHeldControls(pausedHeld);
  assert.equal(pausedHeld.runInFlight, true, "held source completion must retain its active Run");
  assert.equal(pausedHeld.canvasIdentity, "original");
  assert.equal(pausedHeld.canvasCount, 1);

  await releaseHeldRun(page);
  await page.evaluate(async () => Promise.all([window.__pausedRunA, window.__pausedRunB]));
  await waitForCompletedReplay(page, exampleId);
  await pauseReplay(page);
  const pausedSettled = await snapshot(page);
  diagnostics.phases.push({ phase: "paused-settled", ...pausedSettled });
  assert.equal(pausedSettled.playing, "false", "Pause must acknowledge the new completed reader");
  assert.equal(pausedSettled.playbackAvailability, "available");
  assert.equal(pausedSettled.canvasIdentity, "original");
  assert.equal(pausedSettled.canvasCount, 1);
  assert.equal(pausedSettled.runtimeState, "running");
  await memoryMark(page, "after-paused-rerun");
  assert.equal(pausedSettled.patchExample, exampleId);
  assert.ok(
    pausedSettled.scrubberValue >= 0 && pausedSettled.scrubberValue <= pausedSettled.scrubberMax,
    "settled scrubber escaped the authored duration",
  );

  // Resume the completed replay, then verify an explicit Run enters exclusive
  // source ownership again and returns another completed replay lease.
  await startMovingReplay(page);
  const runningBaselineCount = (await snapshot(page)).authoringCount;
  const runningBaselineTransitions = (await snapshot(page)).sourceOwnedTransitions;
  await memoryMark(page, "before-running-rerun");
  await holdNextRun(page, exampleId);
  await page.evaluate(() => {
    window.__noonRerunStress.beforeRun(true);
    window.__runningRunA = window.__noonExampleGallery.run();
    window.__runningRunB = window.__noonExampleGallery.run();
  });
  await waitForSourceOwnedPass(page, runningBaselineTransitions);
  await waitForHeldRun(page);
  const runningHeld = await snapshot(page);
  diagnostics.phases.push({ phase: "running-held", ...runningHeld });
  assert.equal(runningHeld.authoringCount, runningBaselineCount + 1, "duplicate running Run was not coalesced");
  assertHeldControls(runningHeld);
  assert.equal(runningHeld.runInFlight, true);
  assert.equal(runningHeld.canvasIdentity, "original");

  await releaseHeldRun(page);
  await page.evaluate(async () => Promise.all([window.__runningRunA, window.__runningRunB]));
  await waitForCompletedReplay(page, exampleId);
  await pauseReplay(page);
  const runningSettled = await snapshot(page);
  diagnostics.phases.push({ phase: "running-settled", ...runningSettled });
  assert.equal(runningSettled.playing, "false", "Pause must acknowledge the replacement reader");
  assert.equal(runningSettled.playbackAvailability, "available");
  assert.equal(runningSettled.canvasIdentity, "original");
  assert.equal(runningSettled.canvasCount, 1);
  assert.equal(runningSettled.runtimeState, "running");
  await memoryMark(page, "after-running-rerun");

  // Repeat state transitions without hooks to catch command-queue ordering bugs.
  for (let iteration = 0; iteration < 8; iteration += 1) {
    const shouldPause = iteration % 2 === 0;
    if (shouldPause) await pauseReplay(page);
    else await startMovingReplay(page);
    await page.waitForFunction(
      (paused) => document.querySelector(".playback-controls")?.dataset.playing === (paused ? "false" : "true"),
      shouldPause,
    );
    if (shouldPause) {
      await page.locator(".playback-scrubber").evaluate((input, seed) => {
        const max = Number(input.max);
        for (let index = 0; index < 12; index += 1) {
          input.value = String(max * (((seed + index * 7) % 23) + 1) / 24);
          input.dispatchEvent(new Event("input", { bubbles: true }));
        }
      }, iteration);
    }
    const before = await snapshot(page);
    const beforeCount = before.authoringCount;
    const beforeTransitions = before.sourceOwnedTransitions;
    await memoryMark(page, `before-iteration-${iteration}`);
    await page.evaluate(async (shouldPause) => {
      window.__noonRerunStress.beforeRun(!shouldPause);
      await Promise.all([window.__noonExampleGallery.run(), window.__noonExampleGallery.run()]);
    }, shouldPause);
    await waitForSourceOwnedPass(page, beforeTransitions);
    await waitForCompletedReplay(page, exampleId);
    await pauseReplay(page);
    const settled = await snapshot(page);
    diagnostics.phases.push({ phase: `iteration-${iteration}`, ...settled });
    assert.equal(settled.authoringCount, beforeCount + 1, `iteration ${iteration}: duplicate Run was not coalesced`);
    assert.equal(settled.playing, "false", "Pause must settle after each completed replay");
    assert.equal(settled.playbackAvailability, "available");
    assert.equal(settled.canvasIdentity, "original");
    assert.equal(settled.canvasCount, 1);
    assert.equal(settled.patchState, "applied");
    assert.equal(settled.patchExample, exampleId);
    assert.equal(settled.runtimeState, "running");
    await memoryMark(page, `after-iteration-${iteration}`);
  }

  assert.deepEqual(pageErrors, [], `page errors: ${pageErrors.join("\n")}`);
  assert.deepEqual(consoleErrors, [], `console errors: ${consoleErrors.join("\n")}`);
  diagnostics.pageErrors = pageErrors;
  diagnostics.consoleErrors = consoleErrors;
  diagnostics.serverOutput = serverOutput;
  diagnostics.runTriggers = await page.evaluate(() => window.__noonRerunStress.runTriggers);
  await page.screenshot({ path: path.join(artifactDir, "playground.png"), fullPage: true });
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  console.log("✓ replay rerun stress: source ownership, seek queues, duplicate Runs and canvas reuse remain coherent");
} catch (error) {
  if (memoryEnabled) memory.errors.push(`rerun qualification: ${String(error)}`);
  diagnostics.pageErrors = pageErrors;
  diagnostics.consoleErrors = consoleErrors;
  diagnostics.serverOutput = serverOutput;
  diagnostics.error = error instanceof Error ? { name: error.name, message: error.message, stack: error.stack } : String(error);
  if (page !== null) {
    try {
      diagnostics.failure = await snapshot(page);
      diagnostics.runTriggers = await page.evaluate(() => window.__noonRerunStress.runTriggers);
      await page.screenshot({ path: path.join(artifactDir, "failure.png"), fullPage: true });
    } catch {
      // Preserve the original failure.
    }
  }
  await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`);
  throw error;
} finally {
  if (memoryEnabled) {
    const workers = page?.workers() ?? [];
    if (exampleContext === null) memory.errors.push("browser context was unavailable for retirement observation");
    else try {
      const closedAt = Date.now();
      await exampleContext.close();
      memory.contextCloseMs = Date.now() - closedAt;
      memory.workersAfterContextClose = await Promise.all(workers.map(async worker => ({ ...workerIdentity(worker),
        retired: await Promise.race([worker.evaluate(() => true).then(() => false, () => true),
          new Promise(resolve => setTimeout(() => resolve(null), 500))]) })));
      await new Promise(resolve => setTimeout(resolve, 750));
    } catch (error) { memory.errors.push(`context retirement observation: ${String(error)}`); }
    if (rssSampler === null) memory.errors.push("process RSS sampler was not started");
    else try {
      const rss = await rssSampler.stop();
      memory.processTreeRss = { ...rss, clockOriginEpochMs: memory.rssClockOriginEpochMs,
        samples: rss.samples.map(sample => ({ ...sample, epochMs: memory.rssClockOriginEpochMs + sample.atMs })) };
      if (rss.sampleCount === 0 || rss.peakSampledRssBytes === null || rss.samplingError !== null || rss.truncated) {
        memory.errors.push("process RSS samples were missing, errored, or truncated");
      }
    } catch (error) { memory.errors.push(`process RSS sampler stop: ${String(error)}`); }
    if (!Array.isArray(memory.workersAfterContextClose) || memory.workersAfterContextClose.length === 0 ||
        memory.workersAfterContextClose.some(worker => worker.retired !== true)) {
      memory.errors.push("one or more browser workers were not confirmed retired");
    }
    if (wasmAccountingEnabled) {
      const sampledWorkerIds = new Set(memory.marks.flatMap(mark =>
        (mark.wasmAccounting ?? []).filter(item => item.accounting !== null && !item.error).map(item => item.id)));
      const observedWorkerIds = new Set(memory.marks.flatMap(mark => mark.workers.map(worker => worker.id)));
      if (observedWorkerIds.size === 0 || [...observedWorkerIds].some(id => !sampledWorkerIds.has(id))) {
        memory.errors.push("WASM capacity probe did not produce a valid sample for every observed worker");
      }
    }
    if (!Number.isFinite(memory.pausedRendererWindow?.before?.presentedFrames) ||
        !Number.isFinite(memory.pausedRendererWindow?.after?.presentedFrames)) {
      memory.errors.push("paused renderer counters were unavailable");
    }
    memory.status = memory.errors.length === 0 ? "observed" : "failed";
    diagnostics.memory = memory;
    try { await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`); }
    catch (error) { memory.status = "failed"; memory.errors.push(`memory report write: ${String(error)}`); }
  }
  await browser?.close().catch(() => {});
  await browserServer?.close().catch(() => {});
  server.kill("SIGTERM");
  if (memoryEnabled && memory.status === "failed" && diagnostics.error === undefined) {
    diagnostics.error = { name: "MemoryObservationError", message: memory.errors.join("; ") };
    await writeFile(path.join(artifactDir, "diagnostics.json"), `${JSON.stringify(diagnostics, null, 2)}\n`).catch(() => {});
    throw new Error(`memory observation failed: ${memory.errors.join("; ")}`);
  }
}
