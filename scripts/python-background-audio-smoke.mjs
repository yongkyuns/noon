// Actual Pyodide authoring, shared Rust execution, normal gallery controls, and
// native Web Audio output. Test observations do not supply a scene or clock.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { observeAudio } from "./browser-audio-observation.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { holdPlaygroundPreload, playgroundLaunchOptions, waitForBrowserObservation } from "./playground-browser-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { layoutReplayViewport, replayViewport } from "./showcase-viewport.mjs";
import { assertRetainedPoster } from "./showcase-posters.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const browserName = process.env.NOON_PYTHON_AUDIO_BROWSER ?? "chromium";
const backend = process.env.NOON_PYTHON_AUDIO_BACKEND ?? "webgl";
const artifacts = path.resolve(process.env.NOON_PYTHON_AUDIO_ARTIFACTS ??
  path.join(root, "browser-smoke-artifacts/python-background-audio", `${browserName}-${backend}`));
assert.ok(["chromium", "firefox", "webkit"].includes(browserName));
assert.ok(["webgl", "webgpu"].includes(backend));
assert.ok(browserName === "chromium" || backend === "webgl", "this browser profile qualifies WebGL2");
const manifest = JSON.parse(await readFile(path.join(root, "web/python/examples/noon_showcase_manifest.json"), "utf8"));
const entry = manifest.entries.find(entry => entry.id === "showcase-background-music");
assert.ok(entry);
const source = await readFile(path.join(root, "web", entry.path));
const fourierMusicPreview = await readFile(path.join(root, "scripts/fixtures/noon_fourier_music_preview.py"));
const fourierEpicycles = await readFile(path.join(root, "scripts/fixtures/noon_fourier_epicycles.py"));
const fourierAudioCall = "        self.audio(AudioSource.small_discovery(), duration=34.65, gain=0.5, fade_in=1.5, fade_out=2.5)\n";
const fourierWithAudio = Buffer.from(fourierEpicycles.toString().replace(
  "    def construct(self):\n", "    def construct(self):\n" + fourierAudioCall));
assert.equal(fourierWithAudio.toString().replace(fourierAudioCall, ""), fourierEpicycles.toString(),
  "combined Fourier fixture differs only by the documented first-statement soundtrack insertion");
assert.notEqual(fourierWithAudio.toString(), fourierEpicycles.toString(), "combined fixture adds only the documented soundtrack call");
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const json = value => `${JSON.stringify(value, (_, item) => typeof item === "bigint" ? String(item) : item, 2)}\n`;
const report = {
  scope: "finite-python-gallery-shared-rust-real-web-audio", status: "running", browserName, backend,
  revision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  sourceDirty: execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim() !== "",
  sourcePath: entry.path, sourceSha256: hash(source), cases: [], browserErrors: [], consoleErrors: [],
  failedResponses: [],
};
await mkdir(artifacts, { recursive: true });
let browser, server, page, context, cache;
let poster, posterRecord;
let fourierReferencePixels = null;
const near = (actual, expected, label, tolerance = 1e-6) =>
  assert.ok(Math.abs(actual - expected) <= tolerance, `${label}: ${actual} != ${expected}`);

async function record(name, action) {
  const result = { name, status: "running" };
  report.cases.push(result);
  try {
    Object.assign(result, await action(), { status: "passed" });
    console.log(`[PASS] Python background audio: ${name}`);
  } catch (error) {
    Object.assign(result, { status: "failed", error: String(error.stack ?? error) });
    throw error;
  }
}

async function state() {
  return page.evaluate(async () => ({
    ...audioQualification.snapshot(),
    anchors: audioQualification.anchors,
    transport: await audioQualification.execution.state(),
  }));
}

async function command(method, ...args) {
  return page.evaluate(({ method, args }) => audioQualification.execution[method](...args), { method, args });
}

async function waitReady() {
  await page.waitForFunction(() => {
    const patch = document.querySelector("#patch-status");
    if (patch?.dataset.state === "error") throw new Error(patch.value || patch.textContent);
    return patch?.dataset.state === "applied" && !window.__noonExampleGallery?.runInFlight;
  }, null, { timeout: 180_000 });
  await waitForBrowserObservation(page, async () => {
    if (!audioQualification.execution) return false;
    const result = await audioQualification.execution.state();
    return result.hasAudio === true && result.durationSeconds === 30;
  });
}

async function audible() {
  await page.waitForFunction(() => audioQualification.sample().some(output =>
    output.state === "running" && output.rms > 1e-5 && output.peak > 1e-4), null, { timeout: 10_000 });
  return state();
}

async function quiet() {
  // Drain the analyser's real prior samples before requiring sustained silence.
  await page.waitForTimeout(180);
  const windows = await page.evaluate(async () => {
    const values = [];
    for (let index = 0; index < 6; index++) {
      values.push(audioQualification.sample());
      await new Promise(resolve => setTimeout(resolve, 25));
    }
    return values;
  });
  assert.ok(windows.every(outputs => outputs.length > 0 && outputs.every(output =>
    output.state !== "running" || output.peak <= 1e-7)), "actual device output must stay silent");
  return windows;
}

async function play() {
  await page.getByRole("button", { name: "Play animation", exact: true }).click();
  await waitForBrowserObservation(page, async () => (await audioQualification.execution.state()).playing);
  return audible();
}

async function runFourierSource(label, fixture, expectedAudio) {
  const before = await state();
  const beforeAttachments = await page.evaluate(() => audioQualification.attachments);
  const traceStart = await page.evaluate(() => audioQualification.sourceTrace.length);
  const runKey = `${label}Run`;
  await page.evaluate(({ fixture, runKey, attachments, traceStart }) => {
    const editor = document.querySelector("#python-scene-source");
    if (!(editor instanceof HTMLTextAreaElement)) throw new Error("ordinary Python editor is unavailable");
    window.audioQualification[`${runKey}Error`] = null;
    window.audioQualification[`${runKey}Started`] = false;
    window.audioQualification[`${runKey}BeforeAttachments`] = attachments;
    window.audioQualification[`${runKey}TraceStart`] = traceStart;
    editor.value = fixture;
    window.__noonExampleGallery.run().catch(error => {
      window.audioQualification[`${runKey}Error`] = String(error?.stack ?? error);
    });
    window.audioQualification[`${runKey}Started`] = true;
  }, { fixture: fixture.toString(), runKey, attachments: beforeAttachments, traceStart });
  await waitForBrowserObservation(page, key => {
    const observation = window.audioQualification;
    if (observation[`${key}Error`]) throw new Error(observation[`${key}Error`]);
    return observation[`${key}Started`] && window.__noonExampleGallery?.runInFlight === true;
  }, runKey, { timeout: 90_000 });
  try {
    await waitForBrowserObservation(page, async ({ key, expectedAudio: hasAudio }) => {
      const observation = window.audioQualification;
      if (observation[`${key}Error`]) throw new Error(observation[`${key}Error`]);
      if (!observation.sourceTrace.slice(observation[`${key}TraceStart`]).some(event =>
        event.phase === "source-run-start")) return false;
      try {
        const playback = await observation.execution.state();
        return playback.preparedPlayback === true && playback.durationSeconds === null &&
          playback.hasAudio === hasAudio && playback.preparedFrontier > playback.time;
      } catch { return false; }
    }, { key: runKey, expectedAudio }, { timeout: 180_000 });
  } catch (error) {
    const diagnostics = await page.evaluate(async key => {
      const observation = window.audioQualification;
      const playback = await observation.execution?.state?.().catch(failure => ({ unavailable: String(failure) })) ?? null;
      return { runError: observation[`${key}Error`], runStarted: observation[`${key}Started`],
        attachments: observation.attachments, traceStart: observation[`${key}TraceStart`],
        runInFlight: window.__noonExampleGallery?.runInFlight,
        trace: observation.sourceTrace.slice(-12), playback };
    }, runKey);
    throw new Error(`${String(error.stack ?? error)}\nFourier attachment diagnostics: ${JSON.stringify(diagnostics)}`);
  }
  const attached = await state();
  assert.equal(attached.starts.length, before.starts.length, "device output stays silent before explicit Play");
  assert.equal(attached.transport.hasAudio, expectedAudio);
  return { before, attached, traceStart, runKey };
}

async function finishFourierSource(run, expectedAudio) {
  const { runKey, traceStart } = run;
  await waitForBrowserObservation(page, async ({ key, start }) => {
    const observation = window.audioQualification;
    if (observation[`${key}Error`]) throw new Error(observation[`${key}Error`]);
    const trace = observation.sourceTrace.slice(start);
    return window.__noonExampleGallery?.runInFlight === false &&
      trace.some(event => event.phase === "source-run-result");
  }, { key: runKey, start: traceStart }, { timeout: 360_000 });
  let sourceState = await state();
  if (sourceState.transport.loopDuration !== null && sourceState.transport.loopCount < 1) {
    await waitForBrowserObservation(page, async firstLoop => {
      const playback = await audioQualification.execution.state();
      return playback.completed === true || playback.loopCount > firstLoop;
    }, sourceState.transport.loopCount, { timeout: 120_000 });
    sourceState = await state();
  } else if (sourceState.transport.completed !== true && sourceState.transport.loopDuration === null) {
    await waitForBrowserObservation(page, async () => {
      const playback = await audioQualification.execution.state();
      return playback.completed === true;
    }, null, { timeout: 120_000 });
    sourceState = await state();
  }
  if (sourceState.transport.playing) await command("pause");
  const finalSnapshot = await state();
  const events = await page.evaluate(start => audioQualification.sourceTrace.slice(start), traceStart);
  const starts = events.filter(event => event.phase === "source-run-start");
  const results = events.filter(event => event.phase === "source-run-result");
  assert.equal(starts.length, 1, "each fixture is authored once");
  assert.equal(results.length, 1, "the original authoring run reaches its natural result");
  assert.equal(starts[0].requestedProfile, "incremental");
  assert.equal(results[0].generation, starts[0].generation, "source completion stays on its original worker generation");
  assert.equal(sourceState.transport.preparedPlayback, true);
  assert.equal(sourceState.transport.durationSeconds, 34.65, "the terminal Rust history owns the authored endpoint");
  assert.equal(sourceState.transport.preparedFrontier, 34.65);
  assert.equal(sourceState.transport.buffering, false);
  assert.equal(sourceState.transport.replaySupported, true);
  assert.equal(sourceState.transport.hasAudio, expectedAudio);
  assert.equal(finalSnapshot.transport.playing, false, "the completed source can be paused for retained seeking");
  return { finalState: finalSnapshot, sourceState, events };
}

async function pausedSeek(time) {
  await command("pause");
  const result = await command("seek", time);
  near(result.time, time, "exact seek", 0);
  assert.equal(result.playing, false);
  // The public command completes after its corresponding renderer publication.
  const metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
  near(Number(metrics.metrics.time), time, "published picture time", 0);
  return result;
}

function assertEnvelope(snapshot, start, expected) {
  assert.ok(Number.isInteger(start.gainParameter), "source must feed the real authored gain node");
  const events = snapshot.parameters[start.gainParameter].events;
  assert.equal(events.length, expected.length);
  for (const [index, [offset, gain]] of expected.entries()) {
    near(events[index].args[0], gain, "authored gain");
    near(events[index].args[1] - start.args[0], offset, "authored gain time");
  }
}

function correctedPhase(snapshot, start, requested) {
  const anchor = snapshot.anchors.find(anchor => anchor.sources.includes(start.node));
  assert.ok(anchor, "actual device start must be associated with an accepted worker anchor");
  assert.ok(anchor.time >= requested, "worker anchor must not precede the requested authored time");
  const phase = anchor.time + (anchor.receivedEpochMs - anchor.sampledEpochMs) / 1000;
  near(start.args[1], phase, "shared clip phase includes exact transport age");
  near(start.args[2], 30 - phase, "finite source ends at the shared endpoint");
  return phase;
}

function pointCentroid(bytes, y, color, expectedX) {
  const image = PNG.sync.read(bytes);
  const scale = image.height / 8;
  const expectedY = image.height / 2 - y * scale;
  let count = 0, x = 0, rowSum = 0;
  for (let row = Math.max(0, Math.floor(expectedY - 0.2 * scale)); row <= Math.ceil(expectedY + 0.2 * scale); row++) {
    for (let column = Math.floor(image.width / 2 - 3.3 * scale); column < image.width; column++) {
      const offset = (row * image.width + column) * 4;
      const [red, green, blue] = image.data.subarray(offset, offset + 3);
      const matches = color === "blue" ? red < 130 && green > 140 && blue > 160
        : red > 180 && green > 140 && blue < 160;
      if (matches) { count++; x += column; rowSum += row; }
    }
  }
  assert.ok(count > 25, `actual ${color} point is missing from the number line`);
  const centroid = { x: x / count, y: rowSum / count, pixels: count };
  near(centroid.x, image.width / 2 + expectedX * scale - 0.5, `${color} input/output position`, 1.5);
  near(centroid.y, expectedY - 0.5, `${color} number-line height`, 1.5);
  return centroid;
}

async function currentPicture(input, filename) {
  const bytes = await page.locator("#scene").screenshot({ path: path.join(artifacts, filename) });
  return {
    bytes,
    points: {
      input: pointCentroid(bytes, 0.75, "blue", 1 + input),
      output: pointCentroid(bytes, -1.1, "gold", 1 + 2 * input),
    },
  };
}

async function picture(time, input, filename) {
  await pausedSeek(time);
  return currentPicture(input, filename);
}

try {
  server = await serveRepository(root, 0);
  const launch = browserName === "chromium" ? {
    headless: true,
    ...(process.env.NOON_AUDIO_CHROMIUM ? { executablePath: process.env.NOON_AUDIO_CHROMIUM } : {}),
    args: [...browserArgs(backend), "--autoplay-policy=user-gesture-required"],
  } : playgroundLaunchOptions(browserName);
  browser = await playwright[browserName].launch(launch);
  report.browserVersion = browser.version();
  context = await browser.newContext({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 });
  cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.js"), "utf8"));
  await cache.install(context);
  await context.addInitScript(observeAudio);
  await context.addInitScript(() => {
    // Observe the actual Window-host surface policy without changing requests.
    // Worker contexts live in their own realm; tiny capability probes are not
    // product surfaces and are deliberately excluded from this observation.
    const getContext = OffscreenCanvas.prototype.getContext;
    const contexts = [];
    window.noonAudioWebglContexts = contexts;
    OffscreenCanvas.prototype.getContext = function (...args) {
      const result = getContext.apply(this, args);
      if (args[0] === "webgl2" && result && this.width > 2 && this.height > 2 &&
          !contexts.includes(result)) {
        contexts.push(result);
      }
      return result;
    };
  });
  if (backend === "webgl") await context.addInitScript(() => Object.defineProperty(navigator, "gpu", { value: undefined }));
  page = await context.newPage();
  page.setDefaultTimeout(120_000);
  page.on("pageerror", error => report.browserErrors.push(String(error)));
  page.on("console", message => {
    if (message.type() === "error") report.consoleErrors.push({ text: message.text(), location: message.location() });
  });
  page.on("response", response => {
    if (response.status() >= 400) report.failedResponses.push({ url: response.url(), status: response.status() });
  });
  const release = await holdPlaygroundPreload(page);
  await page.goto(`${server.baseUrl}/web/index.html?example=${entry.id}`, { waitUntil: "domcontentloaded" });
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  await page.evaluate(async () => {
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { ExecutionWorkerClient } = await import("./execution-worker-client.js");
    const { WorkerAudioDevice } = await import("./worker-audio-device.js");
    audioQualification.audioResourceSessions = [];
    const installAudioDevice = WorkerAudioDevice.prototype.install;
    WorkerAudioDevice.prototype.install = async function (bytes) {
      if (bytes instanceof Uint8Array && bytes.byteLength >= 4) {
        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        const metadataLength = view.getUint32(0, true);
        const metadata = JSON.parse(new TextDecoder().decode(bytes.subarray(4, 4 + metadataLength)));
        audioQualification.audioResourceSessions.push(metadata.session);
        audioQualification.audioResourceSession = metadata.session;
      }
      return installAudioDevice.call(this, bytes);
    };
    audioQualification.pointerViews = [];
    audioQualification.audioDeviceAnchors = [];
    audioQualification.audioApplyAnchors = [];
    audioQualification.devicePauses = [];
    const extendDeviceAnchor = WorkerAudioDevice.prototype.extendAnchor;
    WorkerAudioDevice.prototype.extendAnchor = function (anchor, session) {
      const startsBefore = audioQualification.starts.length;
      const receivedEpochMs = performance.timeOrigin + performance.now();
      const accepted = extendDeviceAnchor.call(this, anchor, session);
      audioQualification.audioDevice = this;
      audioQualification.audioDeviceAnchors.push({ anchor, session, accepted,
        receivedEpochMs, starts: audioQualification.starts.slice(startsBefore) });
      return accepted;
    };
    const applyDeviceAnchor = WorkerAudioDevice.prototype.applyAnchor;
    WorkerAudioDevice.prototype.applyAnchor = function (anchor, session) {
      const receivedEpochMs = performance.timeOrigin + performance.now();
      const result = applyDeviceAnchor.call(this, anchor, session);
      audioQualification.audioApplyAnchors.push({ anchor, session, receivedEpochMs });
      return result;
    };
    const pauseDevice = WorkerAudioDevice.prototype.pause;
    WorkerAudioDevice.prototype.pause = function (...args) {
      audioQualification.devicePauses.push({ method: "pause", receivedEpochMs: performance.timeOrigin + performance.now(),
        stack: new Error().stack });
      return pauseDevice.apply(this, args);
    };
    const stopDevice = WorkerAudioDevice.prototype.stop;
    WorkerAudioDevice.prototype.stop = function (...args) {
      audioQualification.devicePauses.push({ method: "stop", receivedEpochMs: performance.timeOrigin + performance.now(),
        stack: new Error().stack });
      return stopDevice.apply(this, args);
    };
    const registerPointerView = ExecutionWorkerClient.prototype.setBrowserPointerView;
    ExecutionWorkerClient.prototype.setBrowserPointerView = function (revision, width, height) {
      audioQualification.pointerViews.push({ revision, width, height });
      return registerPointerView.call(this, revision, width, height);
    };
    const original = AuthoringExecutionClient.prototype.startSemanticExecution;
    audioQualification.attachments = 0;
    audioQualification.sourceTrace = [];
    audioQualification.readyFailureCount = 0;
    audioQualification.stateFailureCount = 0;
    const trace = event => {
      if (audioQualification.sourceTrace.length < 128) audioQualification.sourceTrace.push(event);
      else if (audioQualification.sourceTrace.at(-1)?.phase !== "trace-truncated") {
        audioQualification.sourceTrace.push({ phase: "trace-truncated" });
      }
    };
    for (const method of ["terminate", "retireExecutionSource"]) {
      const originalMethod = ExecutionWorkerClient.prototype[method];
      ExecutionWorkerClient.prototype[method] = function (...args) {
        trace({ phase: `execution-${method}`, diagnostics: this.diagnostics,
          callsite: new Error().stack });
        return originalMethod.apply(this, args);
      };
    }
    const originalReady = ExecutionWorkerClient.prototype.ready;
    ExecutionWorkerClient.prototype.ready = function (...args) {
      try { return originalReady.apply(this, args); }
      catch (error) {
        audioQualification.readyFailureCount++;
        if (audioQualification.readyFailureCount === 1) {
          trace({ phase: "execution-ready-missing",
            error: String(error?.stack ?? error), diagnostics: this.diagnostics, callsite: new Error().stack });
        }
        throw error;
      }
    };
    AuthoringExecutionClient.prototype.startSemanticExecution = async function (...args) {
      trace({ phase: "runtime-attach-start" });
      const result = await original.apply(this, args);
      audioQualification.execution = this;
      audioQualification.attachments++;
      trace({ phase: "runtime-attach-ready", attachments: audioQualification.attachments });
      return result;
    };
    const originalRun = PythonAuthoringClient.prototype.run;
    PythonAuthoringClient.prototype.run = async function (source, context, options = {}) {
      const item = { phase: "source-run-start", generation: this.interpreterGeneration,
        requestedProfile: options.preparedPlayback === true ? "incremental" : "observed" };
      trace(item);
      const onContinuation = options.onSemanticContinuation;
      const tracedOptions = onContinuation === null || onContinuation === undefined ? options : {
        ...options,
        async onSemanticContinuation(registration) {
          trace({ phase: "continuation-registered", generation: item.generation,
            contextId: registration.semanticExecution.contextId,
            continuationGeneration: registration.generation, duration: registration.duration });
          try {
            const result = await onContinuation(registration);
            trace({ phase: "continuation-adopted", generation: item.generation });
            return result;
          } catch (error) {
            trace({ phase: "continuation-adoption-failed", generation: item.generation,
              error: String(error?.stack ?? error) });
            throw error;
          }
        },
      };
      try {
        const result = await originalRun.call(this, source, context, tracedOptions);
        trace({ phase: "source-run-result", generation: item.generation,
          contextId: result.semanticExecution?.contextId ?? null,
          continuationGeneration: result.semanticExecution?.continuationGeneration ?? null,
          diagnostics: this.executionDiagnostics });
        return result;
      } catch (error) {
        trace({ phase: "source-run-failed", generation: item.generation,
          error: String(error?.stack ?? error), diagnostics: this.executionDiagnostics,
          client: this.diagnostics });
        throw error;
      }
    };
    const originalState = AuthoringExecutionClient.prototype.state;
    AuthoringExecutionClient.prototype.state = async function (...args) {
      try { return await originalState.apply(this, args); }
      catch (error) {
        audioQualification.stateFailureCount++;
        if (audioQualification.stateFailureCount === 1) {
          trace({ phase: "authoring-state-failed",
            error: String(error?.stack ?? error), diagnostics: this.executionDiagnostics });
        }
        throw error;
      }
    };
    const { WasmWorkerAudioOutput } = await import("./pkg/noon_web.js");
    audioQualification.deviceTransitions = [];
    audioQualification.captureDeviceTransitions = false;
    const captureDeviceTransition = (kind, node, context) => {
      if (!audioQualification.captureDeviceTransitions) return;
      const transition = { kind, node, deviceTime: context.currentTime,
        requestStartedEpochMs: performance.timeOrigin + performance.now(), state: "pending" };
      audioQualification.deviceTransitions.push(transition);
      const execution = audioQualification.execution;
      if (!execution) { transition.state = "unavailable"; return; }
      execution.state().then(playback => Object.assign(transition, {
        requestFinishedEpochMs: performance.timeOrigin + performance.now(),
        state: "captured", time: playback.time, playing: playback.playing,
        buffering: playback.buffering ?? null, frontier: playback.preparedFrontier ?? null,
        duration: playback.durationSeconds ?? null,
      }), error => Object.assign(transition, { requestFinishedEpochMs: performance.timeOrigin + performance.now(),
        state: "error", error: String(error) }));
    };
    const nativeSourceStart = AudioBufferSourceNode.prototype.start;
    AudioBufferSourceNode.prototype.start = function (...args) {
      const value = nativeSourceStart.apply(this, args);
      const node = audioQualification.starts.at(-1)?.node ?? null;
      captureDeviceTransition("start", node, this.context);
      this.addEventListener("ended", () => captureDeviceTransition("ended", node, this.context), { once: true });
      return value;
    };
    const start = WasmWorkerAudioOutput.prototype.start;
    audioQualification.anchors = [];
    WasmWorkerAudioOutput.prototype.start = function (session, time, sampledEpochMs, receivedEpochMs) {
      const before = audioQualification.starts.length;
      const entry = { operation: "start", session, resourceSession: audioQualification.audioResourceSession,
        time, sampledEpochMs, receivedEpochMs };
      try {
        const result = start.call(this, session, time, sampledEpochMs, receivedEpochMs);
        entry.sources = audioQualification.starts.slice(before).map(source => source.node);
        return result;
      } catch (error) { entry.error = String(error?.stack ?? error); throw error; }
      finally { audioQualification.anchors.push(entry); }
    };
    const startUntil = WasmWorkerAudioOutput.prototype.startUntil;
    WasmWorkerAudioOutput.prototype.startUntil = function (session, time, authorizedUntil, sampledEpochMs, receivedEpochMs) {
      const before = audioQualification.starts.length;
      const entry = { operation: "startUntil", session, resourceSession: audioQualification.audioResourceSession,
        time, authorizedUntil, sampledEpochMs, receivedEpochMs };
      try {
        const result = startUntil.call(this, session, time, authorizedUntil, sampledEpochMs, receivedEpochMs);
        entry.sources = audioQualification.starts.slice(before).map(source => source.node);
        return result;
      } catch (error) { entry.error = String(error?.stack ?? error); throw error; }
      finally { audioQualification.anchors.push(entry); }
    };
    const extendUntil = WasmWorkerAudioOutput.prototype.extendUntil;
    WasmWorkerAudioOutput.prototype.extendUntil = function (session, time, authorizedUntil, sampledEpochMs, receivedEpochMs) {
      const before = audioQualification.starts.length;
      const entry = { operation: "extendUntil", session, resourceSession: audioQualification.audioResourceSession,
        time, authorizedUntil, sampledEpochMs, receivedEpochMs };
      try {
        const result = extendUntil.call(this, session, time, authorizedUntil, sampledEpochMs, receivedEpochMs);
        entry.sources = audioQualification.starts.slice(before).map(source => source.node);
        return result;
      } catch (error) { entry.error = String(error?.stack ?? error); throw error; }
      finally { audioQualification.anchors.push(entry); }
    };
  });
  release();
  await waitReady();
  const identity = await context.request.get(`${server.baseUrl}/web/runtime-build-identity.json`);
  assert.ok(identity.ok(), "capture requires the actually served build identity");
  report.runtimeBuildIdentity = await identity.json();
  if (report.runtimeBuildIdentity.sourceRevision !== null) {
    assert.equal(report.runtimeBuildIdentity.sourceRevision, report.revision);
  } else assert.equal(report.sourceDirty, true, "an unattested source revision must be reported as dirty");
  const metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
  report.renderHost = metrics.renderHost;
  report.rendererBackend = metrics.rendererBackend ?? await page.locator("#status").getAttribute("data-renderer-backend");
  assert.equal(report.rendererBackend, backend === "webgl" ? "WebGL2" : "WebGPU");
  report.windowWebglContexts = await page.evaluate(() => noonAudioWebglContexts.map(context => ({
    width: context.canvas.width, height: context.canvas.height, attributes: context.getContextAttributes(),
  })));
  if (backend === "webgl" && report.renderHost === "main-thread") {
    assert.equal(report.windowWebglContexts.length, 1, "the fallback owns one real Window WebGL surface");
    assert.equal(report.windowWebglContexts[0].attributes.antialias, false);
    assert.equal(report.windowWebglContexts[0].attributes.preserveDrawingBuffer, true,
      "the Window WebGL host must preserve its final presented frame");
  }

  await record("original Python lesson prepares silently at its finite beginning", async () => {
    const initial = await state();
    assert.equal(initial.starts.length, 0);
    assert.ok(initial.output.every(output => output.peak === 0), "prepared device output must stay silent");
    assert.equal(initial.transport.hasAudio, true);
    assert.equal(initial.transport.playing, false);
    near(initial.transport.time, 0, "initial authored time", 0);
    assert.deepEqual(initial.transport.sections.map(section => [section.name, section.time]), [
      ["Double a positive input", 0], ["Keep the sign", 10], ["Return to zero", 20],
    ]);
    await page.waitForTimeout(300);
    near((await state()).transport.time, 0, "no autoplay", 0);
    await page.screenshot({ path: path.join(artifacts, "gallery-page.png"), fullPage: true });
    return { initial };
  });

  await record("trusted Play produces original stereo music with authored fades", async () => {
    const running = await play();
    assert.equal(running.transport.audioReady, true);
    assert.equal(running.output.length, 1);
    assert.equal(running.starts.length, 1);
    assert.equal(running.buffers.length, 1);
    assert.deepEqual(running.buffers[0], { context: 0, channels: 2, frames: 1_920_000, sampleRate: 24_000, duration: 80 });
    const source = running.starts[0];
    const phase = correctedPhase(running, source, 0);
    assert.ok(phase < 1.5, "initial playback must begin within the authored entrance fade");
    assert.equal(source.loop, false);
    assertEnvelope(running, source, [[0, phase / 3], [1.5 - phase, 0.5], [27.5 - phase, 0.5], [30 - phase, 0]]);
    await command("pause");
    const paused = await state();
    assert.ok(paused.transport.time > 0);
    await quiet();
    await page.waitForTimeout(100);
    near((await state()).transport.time, paused.transport.time, "pause freezes authored time", 0);
    return { running, paused };
  });

  await record("exact seek preserves native input/output bindings and a real gallery poster", async () => {
    const starts = (await state()).starts.length;
    const canvas = page.locator("#scene");
    await page.getByRole("slider", { name: "Animation playhead", exact: true }).fill("8");
    await waitForBrowserObservation(page, async () => {
      const result = await audioQualification.execution.state();
      return result.time === 8 && !result.playing &&
        document.querySelector('[aria-label="Animation time"]').textContent === "8.00 / 30.00 s";
    });
    await page.screenshot({ path: path.join(artifacts, "gallery-page-at-8s.png"), fullPage: true });
    await layoutReplayViewport(canvas, { width: 960, height: 540 });
    await pausedSeek(0);
    await replayViewport(canvas, { width: 960, height: 540 });
    const samples = [];
    for (const [time, input] of [[5, 1], [13, 1], [25, -1]]) {
      const capture = await picture(time, input, `lesson-${time}.png`);
      samples.push({ time, points: capture.points, pngSha256: hash(capture.bytes) });
    }
    await pausedSeek(0);
    const forwardSamples = [];
    for (const [time, input] of [[5, 1], [8, 1.75], [25, -1], [30, 0]]) {
      const result = await command("advanceTo", time);
      near(result.time, time, "exact forward sample", 0);
      const capture = await currentPicture(input, `lesson-forward-${time}.png`);
      forwardSamples.push({ time, points: capture.points, pngSha256: hash(capture.bytes) });
    }
    const completedMetrics = await command("metrics");
    await quiet();
    const held = await currentPicture(0, "lesson-final-held.png");
    const heldMetrics = await command("metrics");
    assert.equal(heldMetrics.metrics.presentedFrames, completedMetrics.metrics.presentedFrames,
      "a held final picture must stay correct without an extra draw");
    const capture = await picture(8, 1.75, "showcase-background-music.png");
    poster = capture.bytes;
    const image = PNG.sync.read(poster);
    posterRecord = {
      id: entry.id, captureBuildId: report.runtimeBuildIdentity.buildId,
      sourceSha256: report.sourceSha256, thumbnailTime: entry.thumbnail_time,
      image: { width: image.width, height: image.height, pngSha256: hash(poster), pixelSha256: hash(image.data) },
      sample: { requestedTime: 8, publishedTime: 8, authoredDuration: 30, sourceCompleted: true, sourceState: "completed", completionProbe: false },
    };
    assertRetainedPoster(entry, posterRecord, source, poster, report.runtimeBuildIdentity);
    assert.equal((await state()).starts.length, starts, "paused seeks must not start device sources");
    await quiet();
    return {
      samples, forwardSamples,
      held: { time: 30, points: held.points, pngSha256: hash(held.bytes),
        presentedFrames: heldMetrics.metrics.presentedFrames },
      poster: { ...posterRecord, points: capture.points },
    };
  });

  await record("section and restart commands use the authored finite section table", async () => {
    await pausedSeek(13);
    const previous = await command("seekSection", true);
    const next = await command("seekSection", false);
    const restart = await command("restartPlayback");
    near(previous.time, 10, "previous section", 0);
    near(next.time, 20, "next section", 0);
    near(restart.time, 0, "restart", 0);
    assert.ok([previous, next, restart].every(result => result.playing === false));
    await pausedSeek(13);
    const running = await play();
    const source = running.starts.at(-1);
    const phase = correctedPhase(running, source, 13);
    assertEnvelope(running, source, [[0, 0.5], [27.5 - phase, 0.5], [30 - phase, 0]]);
    return { previous, next, restart, running };
  });

  await record("viewer volume and mute affect the device without restarting the music", async () => {
    const before = await state();
    await command("setAudioVolume", 0);
    await quiet();
    const zero = await state();
    assert.equal(zero.transport.audioVolume, 0);
    await command("setAudioVolume", 0.4);
    await audible();
    await command("setAudioMuted", true);
    await quiet();
    const muted = await state();
    assert.equal(muted.transport.audioMuted, true);
    await command("setAudioMuted", false);
    const restored = await audible();
    assert.equal(restored.transport.audioMuted, false);
    assert.equal(restored.transport.audioVolume, 0.4);
    assert.equal(restored.starts.length, before.starts.length, "viewer changes preserve source phase");
    assert.ok(restored.transport.time > before.transport.time);
    return { before, zero, muted, restored };
  });

  await record("hidden-page pause retires audio and does not resume on visibility return", async () => {
    const before = await state();
    await page.evaluate(async () => {
      const saved = new Map(["hidden", "visibilityState"].map(name => [name, Object.getOwnPropertyDescriptor(document, name)]));
      try {
        Object.defineProperty(document, "hidden", { configurable: true, value: true });
        Object.defineProperty(document, "visibilityState", { configurable: true, value: "hidden" });
        document.dispatchEvent(new Event("visibilitychange"));
        const deadline = performance.now() + 5_000;
        while ((await audioQualification.execution.state()).playing) {
          if (performance.now() > deadline) throw new Error("hidden page did not pause");
          await new Promise(resolve => setTimeout(resolve, 25));
        }
      } finally {
        for (const [name, descriptor] of saved) {
          if (descriptor) Object.defineProperty(document, name, descriptor);
          else delete document[name];
        }
        document.dispatchEvent(new Event("visibilitychange"));
      }
    });
    const paused = await state();
    assert.equal(paused.transport.playing, false);
    assert.ok(paused.stops.length > before.stops.length);
    await quiet();
    const settled = await state();
    near(settled.transport.time, paused.transport.time, "hidden authored time remains fixed", 0);
    assert.equal(settled.starts.length, paused.starts.length, "visibility return must stay silent");
    return { before, paused, settled };
  });

  await record("the final fade reaches silence at the exact 30-second endpoint", async () => {
    await command("setAudioVolume", 1);
    await pausedSeek(28.75);
    const running = await play();
    const source = running.starts.at(-1);
    const phase = correctedPhase(running, source, 28.75);
    assertEnvelope(running, source, [[0, (30 - phase) / 5], [30 - phase, 0]]);
    await waitForBrowserObservation(page, async () => {
      const result = await audioQualification.execution.state();
      return !result.playing && result.time === 30;
    }, undefined, { timeout: 10_000 });
    await quiet();
    return { running, completed: await state() };
  });

  await record("Run retires the old voice and prepares the same source without autoplay", async () => {
    await pausedSeek(8);
    await play();
    const before = await state();
    await page.evaluate(() => window.__noonExampleGallery.run());
    await waitReady();
    const replaced = await state();
    assert.equal(replaced.transport.playing, false);
    near(replaced.transport.time, 0, "replacement begins at zero", 0);
    assert.equal(replaced.starts.length, before.starts.length, "replacement preparation must stay silent");
    assert.ok(replaced.stops.some(stop => stop.node === before.starts.at(-1).node), "old voice must stop");
    await quiet();
    return { before, replaced };
  });

  await record("decoded Python PCM crosses once and drives overlapping trimmed loops", async () => {
    const fixture = `from array import array
from math import sin, tau
from noon import AudioSource, Circle, Scene

pcm = array("f", (0.1 * sin(tau * 440 * index / 8000) for index in range(8000)))
source = AudioSource.from_interleaved(pcm, sample_rate=8000, channels=1)
assert (source.sample_rate, source.channels, source.frames) == (8000, 1, 8000)
# Mutation of the original Python buffer cannot change retained Rust PCM.
for index in range(len(pcm)):
    pcm[index] = 0

result = Scene()
result.add(Circle(0.5))
result.declare_wait(30)
first = result.audio(source, source_start_frame=1000, source_end_frame=3000,
                     duration=30, loop=True, gain=0.4, fade_in=0.25, fade_out=0.25)
second = result.audio(source, start_time=1, source_start_frame=1000, source_end_frame=3000,
                      duration=29, loop=True, gain=0.2, fade_in=0.25, fade_out=0.25)
assert (first.start_time, first.end_time, first.duration) == (0, 30, 30)
assert (second.start_time, second.end_time, second.duration) == (1, 30, 29)
`;
    const before = await state();
    await page.evaluate(async source => {
      const editor = document.querySelector("#python-scene-source");
      if (!(editor instanceof HTMLTextAreaElement)) throw new Error("ordinary Python editor is unavailable");
      // Invoke the normal explicit Run once; input would also queue an auto-run.
      editor.value = source;
      await window.__noonExampleGallery.run();
    }, fixture);
    await waitReady();
    const prepared = await state();
    assert.equal(prepared.transport.playing, false);
    near(prepared.transport.time, 0, "decoded PCM preparation starts paused", 0);
    assert.equal(prepared.starts.length, before.starts.length, "PCM preparation must stay silent");
    assert.equal(prepared.buffers.length, before.buffers.length + 1, "both clips share one immutable device buffer");
    assert.deepEqual(prepared.buffers.at(-1), {
      context: prepared.output.length - 1, channels: 1, frames: 8000, sampleRate: 8000, duration: 1,
    });
    await quiet();
    await pausedSeek(2.1);
    const running = await play();
    const sources = running.starts.slice(before.starts.length);
    assert.equal(sources.length, 2);
    assert.equal(sources[0].buffer, sources[1].buffer, "overlap reuses the exact retained buffer");
    const anchor = running.anchors.find(anchor => anchor.sources.includes(sources[0].node));
    assert.ok(anchor, "overlapping sources must share an accepted worker clock sample");
    assert.deepEqual(anchor.sources, sources.map(source => source.node));
    const phase = anchor.time + (anchor.receivedEpochMs - anchor.sampledEpochMs) / 1000;
    assert.ok(phase >= 2.1 && phase < 29.75);
    for (const [index, source] of sources.entries()) {
      assert.equal(source.loop, true);
      near(source.loopStart, 0.125, "trimmed loop beginning", 0);
      near(source.loopEnd, 0.375, "trimmed loop end", 0);
      near(source.args[1], 0.125 + (phase - index) % 0.25, "shared loop phase after a direct seek");
      near(source.args[2], 30 - phase, "overlapping clip finite remainder");
      const gain = index === 0 ? 0.4 : 0.2;
      assertEnvelope(running, source, [[0, gain], [29.75 - phase, gain], [30 - phase, 0]]);
    }
    await command("pause");
    await quiet();
    return { fixtureSha256: hash(fixture), prepared, running };
  });

  await record("original 34.65-second Fourier soundtrack preview uses the finite viewer route", async () => {
    assert.equal(hash(fourierMusicPreview), "573d4e94e81707925632f158bdb5d99f0c044eaf86ab0d25a4420fc4735013d4",
      "the Fourier music preview stays byte-for-byte identical to the original issue fixture");
    const before = await state();
    await page.evaluate(async fixture => {
      const editor = document.querySelector("#python-scene-source");
      if (!(editor instanceof HTMLTextAreaElement)) throw new Error("ordinary Python editor is unavailable");
      window.audioQualification.previewRunError = null;
      window.audioQualification.previewRunDone = false;
      editor.value = fixture;
      try {
        await window.__noonExampleGallery.run();
      } catch (error) {
        window.audioQualification.previewRunError = String(error?.stack ?? error);
      } finally {
        window.audioQualification.previewRunDone = true;
      }
    }, fourierMusicPreview.toString());
    await page.waitForFunction(() => window.audioQualification.previewRunDone === true, null,
      { timeout: 180_000 });
    const runError = await page.evaluate(() => window.audioQualification.previewRunError);
    assert.equal(runError, null, runError ?? undefined);
    const preview = await state();
    assert.equal(preview.transport.hasAudio, true);
    assert.equal(preview.transport.durationSeconds, 34.65);
    assert.equal(preview.transport.playing, false, "finite immutable preparation remains silent before Play");
    assert.equal(preview.starts.length, before.starts.length, "Run does not autoplay the original soundtrack");
    assert.equal(preview.transport.preparedPlayback, undefined,
      "the standalone source retains its finite immutable route instead of claiming open-prefix playback");
    assert.equal("preparedFrontier" in preview.transport, false,
      "the immutable result does not report an incremental reader frontier");
    assert.equal(await page.locator(".playback-audio-controls").isVisible(), true);
    near(Number(await page.locator(".playback-scrubber").getAttribute("max")), 34.65,
      "finite immutable soundtrack endpoint", 0);
    const poster = await page.locator("#scene").screenshot();
    assert.ok(poster.length > 0, "the Fourier preview picture is available before playback");
    await pausedSeek(2.0);
    const running = await play();
    const voices = running.starts.slice(before.starts.length);
    assert.ok(voices.length > 0, "ordinary Play starts the retained original soundtrack");
    assert.equal(running.transport.hasAudio, true);
    assert.equal(running.transport.durationSeconds, 34.65);
    await command("pause");
    await quiet();
    return {
      sourceSha256: hash(fourierMusicPreview),
      profile: { preparedPlayback: preview.transport.preparedPlayback ?? null,
        replaySupported: preview.transport.replaySupported, durationSeconds: preview.transport.durationSeconds },
      startsBeforePlay: preview.starts.length - before.starts.length,
      voice: voices[0], pictureBytes: poster.length,
    };
  });

  await record("open prepared Python audio starts from the displayed scene and extends its admitted voice", async () => {
    const fixture = `from noon import *

class AudioWithOrdinaryPlay(Scene):
    def construct(self):
        self.audio(
            AudioSource.small_discovery(),
            duration=3.0,
            gain=0.5,
            fade_in=0.5,
            fade_out=0.5,
        )
        self.play(Create(Circle(radius=0.5)), run_time=1.0)
        self.wait(2.0)
`;
    assert.equal(hash(fixture), "a898089e0534e31a5a149daaff4fec95ffc7ba5d108125d8ba93b4e252e5a60b",
      "the open audio qualification uses the original issue fixture bytes");
    const before = await state();
    await page.waitForFunction(() => !window.__noonExampleGallery?.runInFlight &&
      document.querySelector("#patch-status")?.dataset.state === "applied", null, { timeout: 90_000 });
    await page.evaluate(source => {
      const editor = document.querySelector("#python-scene-source");
      if (!(editor instanceof HTMLTextAreaElement)) throw new Error("ordinary Python editor is unavailable");
      editor.value = source;
      void window.__noonExampleGallery.run().catch(error => {
        window.audioQualification.sourceRunError = String(error?.stack ?? error);
      });
    }, fixture);
    await page.waitForFunction(() => {
      if (window.audioQualification.sourceRunError) throw new Error(window.audioQualification.sourceRunError);
      return window.__noonExampleGallery?.runInFlight === true;
    }, null, { timeout: 90_000 });
    await page.waitForFunction(() => {
      const patch = document.querySelector("#patch-status");
      if (window.audioQualification.sourceRunError) throw new Error(window.audioQualification.sourceRunError);
      if (patch?.dataset.state === "error") throw new Error(patch.value || patch.textContent);
      return patch?.dataset.state === "applied";
    }, null, { timeout: 180_000 });
    await waitForBrowserObservation(page, async () => {
      try {
        const result = await audioQualification.execution.state();
        return result.hasAudio === true && result.preparedPlayback === true;
      } catch {
        return false;
      }
    });
    const initial = await state();
    assert.equal(initial.transport.durationSeconds, null, "an open prepared source has no finite endpoint");
    assert.equal(initial.transport.playing, false);
    assert.equal(initial.starts.length, before.starts.length, "prepared audio must stay silent before Play");
    assert.equal(initial.transport.audioReady, true, "the normal gesture path exposes the prepared device");
    assert.ok(initial.transport.preparedFrontier > initial.transport.time);
    const audioControls = page.locator(".playback-audio-controls");
    assert.equal(await audioControls.isVisible(), true, "audio controls remain available for an open prepared source");
    assert.equal(await page.locator(".playback-mute").count(), 1);

    const canvas = page.locator("#scene");
    await page.getByRole("button", { name: "Play animation", exact: true }).click();
    await waitForBrowserObservation(page, async startCount => audioQualification.starts.length > startCount,
      before.starts.length);
    const first = await state();
    const firstVoice = first.starts.at(-1);
    assert.equal(firstVoice.args.length, 2, "prepared voice remains extendable with an explicit stop");
    const voiceStops = first.stops.filter(stop => stop.node === firstVoice.node && stop.args.length > 0);
    assert.ok(voiceStops.length > 0 && voiceStops[0].args[0] > voiceStops[0].observedAt);
    assert.equal(first.transport.playing, true);
    const shown = await canvas.screenshot();
    assert.ok(shown.length > 0, "the prepared scene is displayed during playback");

    await page.waitForFunction(node => audioQualification.stops.filter(stop => stop.node === node && stop.args.length > 0).length > 1,
      firstVoice.node, { timeout: 30_000 });
    const extended = await state();
    const laterStops = extended.stops.filter(stop => stop.node === firstVoice.node && stop.args.length > 0);
    assert.ok(laterStops.at(-1).args[0] > voiceStops[0].args[0],
      "Rust prefix growth extends the already playing voice instead of restarting it");
    assert.equal(extended.starts.filter(start => start.node === firstVoice.node).length, 1);
    await command("pause");
    await quiet();
    return {
      fixtureSha256: hash(fixture),
      initial: { time: initial.transport.time, frontier: initial.transport.preparedFrontier,
        durationSeconds: initial.transport.durationSeconds, startsBeforePlay: initial.starts.length - before.starts.length },
      voice: { node: firstVoice.node, firstStop: voiceStops[0].args[0], extendedStop: laterStops.at(-1).args[0] },
      displayedSceneBytes: shown.length,
    };
  });

  await record("original Fourier scene reaches finite source completion and replays retained frames", async () => {
    assert.equal(hash(fourierEpicycles), "7a09cda6039efdc9901a3241b0c514f634996187a0ea9b1f316e34dbbd1feed9");
    const run = await runFourierSource("fourier", fourierEpicycles, false);
    assert.ok(run.attached.transport.preparedFrontier > run.attached.transport.time);
    assert.equal(run.attached.transport.durationSeconds, null, "the open prefix is not reported as terminal");
    const completed = await finishFourierSource(run, false);
    const canvas = page.locator("#scene");
    const canvasStyle = await canvas.getAttribute("style");
    await layoutReplayViewport(canvas, { width: 960, height: 540 });
    await replayViewport(canvas, { width: 960, height: 540 });
    const pixelsAt = async (time, filename) => {
      const result = await pausedSeek(time);
      const viewport = await replayViewport(canvas, { width: 960, height: 540 });
      const render = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
      const view = await canvas.evaluate(element => {
        const style = getComputedStyle(element);
        const rect = element.getBoundingClientRect();
        return { bounds: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
          style: { position: style.position, width: style.width, height: style.height,
            transform: style.transform, filter: style.filter, opacity: style.opacity,
            objectFit: style.objectFit, borderRadius: style.borderRadius,
            imageRendering: style.imageRendering } };
      });
      const pointerViews = await page.evaluate(() => audioQualification.pointerViews.slice(-4));
      const backingUrl = await canvas.evaluate(element => element.toDataURL("image/png"));
      const backingRaster = PNG.sync.read(Buffer.from(backingUrl.split(",", 2)[1], "base64"));
      const bytes = await page.locator("#scene").screenshot({ path: path.join(artifacts, filename) });
      const raster = PNG.sync.read(bytes);
      return { time: result.time, pixels: hash(raster.data), width: raster.width,
        height: raster.height, bytes: bytes.length, viewport, view,
        backingPixels: hash(backingRaster.data), backingWidth: backingRaster.width,
        backingHeight: backingRaster.height,
        receipt: result.presentationReceipt ?? null, pointerViews,
        render: { time: render.metrics.time, session: render.metrics.presentedSession,
          backend: render.metrics.backend, objectCount: render.metrics.objectCount,
          drawCalls: render.metrics.drawCalls, instancesDrawn: render.metrics.instancesDrawn,
          bytesUploaded: render.metrics.bytesUploaded, geometryCacheMisses: render.metrics.geometryCacheMisses,
          outlineCacheMisses: render.metrics.outlineCacheMisses, rendererRebuilds: render.metrics.rendererRebuilds,
          gpuGeneration: render.metrics.gpuGeneration, needsPresent: render.metrics.needsPresent } };
    };
    const firstStage = await pixelsAt(4, "fourier-original-stage-1.png");
    const secondStage = await pixelsAt(15, "fourier-original-stage-2.png");
    const thirdStage = await pixelsAt(27, "fourier-original-stage-3.png");
    assert.notEqual(firstStage.pixels, secondStage.pixels, "the retained scene changes across authored stages");
    assert.notEqual(secondStage.pixels, thirdStage.pixels, "the final epicycle stage has its own observed frame");
    const repeated = await pixelsAt(15, "fourier-original-stage-2-repeat.png");
    assert.equal(repeated.pixels, secondStage.pixels, "seeking the same authored time reproduces the same pixels");
    fourierReferencePixels = secondStage;
    await canvas.evaluate((element, style) => style === null ? element.removeAttribute("style") : element.setAttribute("style", style), canvasStyle);
    await pausedSeek(15);
    const runCountBeforeReplay = completed.events.filter(event => event.phase === "source-run-start").length;
    await page.locator(".playback-restart").click();
    await waitForBrowserObservation(page, async () => {
      const result = await audioQualification.execution.state();
      return result.time === 0 && result.playing === false;
    });
    await page.getByRole("button", { name: "Play animation", exact: true }).click();
    await waitForBrowserObservation(page, async () => (await audioQualification.execution.state()).playing);
    await waitForBrowserObservation(page, async () => (await audioQualification.execution.state()).time > 0.1);
    const replayed = await command("pause");
    const replayMetrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
    near(Number(replayMetrics.metrics.time), replayed.time, "replayed frame receipt", 0);
    const replayEvents = await page.evaluate(start => audioQualification.sourceTrace.slice(start), run.traceStart);
    assert.equal(replayEvents.filter(event => event.phase === "source-run-start").length, runCountBeforeReplay,
      "transport replay never executes Python source again");
    assert.equal((await state()).starts.length, run.before.starts.length, "visual-only replay remains silent");
    const sourceResult = completed.events.find(event => event.phase === "source-run-result");
    const terminalEvent = sourceResult?.diagnostics?.events?.find(event => event.stage === "terminal-adopted");
    return { fixtureSha256: hash(fourierEpicycles), initial: run.attached.transport,
      sourceComplete: completed.sourceState.transport, completed: completed.finalState.transport,
      publication: terminalEvent?.publication ?? null,
      stages: [firstStage, secondStage, thirdStage], repeated,
      replayed: { time: replayed.time, metricsTime: Number(replayMetrics.metrics.time) } };
  });

  await record("Fourier plus the documented soundtrack shares one finite scene clock", async () => {
    assert.equal(hash(fourierEpicycles), "7a09cda6039efdc9901a3241b0c514f634996187a0ea9b1f316e34dbbd1feed9");
    assert.equal(hash(fourierWithAudio), "d23682aff80b5e9a50e0c0b0c3444bb8ff94fa73218278a07e51077fcccac9d9");
    const run = await runFourierSource("combinedFourier", fourierWithAudio, true);
    assert.ok(run.attached.transport.preparedFrontier > run.attached.transport.time);
    assert.equal(run.attached.transport.durationSeconds, null, "open audio preparation does not invent the final endpoint");
    assert.equal(run.attached.transport.playing, false, "audio-bearing source waits for the normal Play gesture");
    assert.equal(run.attached.transport.audioReady, true);
    assert.equal(await page.locator(".playback-audio-controls").isVisible(), true);
    await quiet();
    const startsBeforePlay = run.before.starts.length;
    await page.evaluate(() => {
      audioQualification.deviceTransitions = [];
      audioQualification.audioDeviceAnchors = [];
      audioQualification.audioApplyAnchors = [];
      audioQualification.devicePauses = [];
      audioQualification.captureDeviceTransitions = true;
    });
    await page.getByRole("button", { name: "Play animation", exact: true }).click();
    await waitForBrowserObservation(page, async count => audioQualification.starts.length > count, startsBeforePlay);
    const firstPass = await audible();
    const voice = firstPass.starts.at(-1);
    const anchor = firstPass.anchors.find(item => item.sources.includes(voice.node));
    assert.ok(anchor, "the audio voice uses the accepted shared Rust time anchor");
    const phase = anchor.time + (anchor.receivedEpochMs - anchor.sampledEpochMs) / 1000;
    near(voice.args[1], phase, "initial prepared soundtrack sample offset", 0.02);
    assert.equal(voice.args.length, 2, "the open prefix schedules a stoppable voice without a premature duration");
    const initialStop = firstPass.stops.filter(stop => stop.node === voice.node && stop.args.length > 0).at(-1);
    assert.ok(initialStop && initialStop.args[0] > initialStop.observedAt,
      "the source is initially limited by the admitted visual prefix");
    assert.equal(firstPass.transport.hasAudio, true);
    assert.equal(firstPass.transport.playing, true);
    const completed = await finishFourierSource(run, true);
    await page.evaluate(() => { audioQualification.captureDeviceTransitions = false; });
    await waitForBrowserObservation(page, () => audioQualification.deviceTransitions.every(item => item.state !== "pending"));
    const finalState = completed.finalState;
    assert.equal(finalState.transport.durationSeconds, 34.65);
    assert.equal(finalState.transport.preparedFrontier, 34.65);
    assert.ok(completed.sourceState.transport.loopCount >= 1 || completed.sourceState.transport.completed,
      "source completion is followed by one natural visual traversal");
    const deviceAnchorHistory = (await page.evaluate(() => audioQualification.audioDeviceAnchors))
      .filter(item => item.session === anchor.session);
    const bufferedIndex = deviceAnchorHistory.findIndex(item => item.anchor.buffering === true);
    assert.ok(bufferedIndex >= 0, "Rust buffering holds the already-started audio device");
    const heldAnchor = deviceAnchorHistory[bufferedIndex];
    assert.equal(heldAnchor.accepted, true);
    assert.equal(heldAnchor.starts.length, 0, "buffer entry does not start another audio source");
    const bufferResumeAnchor = deviceAnchorHistory.slice(bufferedIndex + 1)
      .find(item => item.anchor.buffering === false && item.accepted === true);
    assert.ok(bufferResumeAnchor, "new Rust history resumes the held audio device");
    assert.ok(bufferResumeAnchor.anchor.sampledEpochMs > heldAnchor.anchor.sampledEpochMs);
    assert.ok(bufferResumeAnchor.starts.length > 0, "buffer recovery starts from the newly admitted Rust anchor");
    const bufferResumeAnchorPhase = bufferResumeAnchor.anchor.time +
      (bufferResumeAnchor.receivedEpochMs - bufferResumeAnchor.anchor.sampledEpochMs) / 1000;
    near(bufferResumeAnchor.starts[0].args[1], bufferResumeAnchorPhase,
      "audio buffer recovery uses the refreshed Rust sample offset", 0.02);
    const extendedStops = finalState.stops.filter(stop => stop.node === voice.node && stop.args.length > 0);
    assert.ok(extendedStops.at(-1).args[0] > initialStop.args[0],
      "terminal history extends the same active device voice to its full authored interval");
    assert.equal(finalState.starts.filter(start => start.node === voice.node).length, 1);
    assertEnvelope(finalState, voice, [
      [0, Math.min(0.5, 0.5 * phase / 1.5)],
      [1.5 - phase, 0.5], [32.15 - phase, 0.5], [34.65 - phase, 0],
    ]);
    const canvas = page.locator("#scene");
    const canvasStyle = await canvas.getAttribute("style");
    await layoutReplayViewport(canvas, { width: 960, height: 540 });
    await replayViewport(canvas, { width: 960, height: 540 });
    const combinedSeek = await pausedSeek(15);
    await replayViewport(canvas, { width: 960, height: 540 });
    const combinedView = await canvas.evaluate(element => {
      const style = getComputedStyle(element);
      const rect = element.getBoundingClientRect();
      return { bounds: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
        style: { position: style.position, width: style.width, height: style.height,
          transform: style.transform, filter: style.filter, opacity: style.opacity,
          objectFit: style.objectFit, borderRadius: style.borderRadius,
          imageRendering: style.imageRendering } };
    });
    const combinedPointerViews = await page.evaluate(() => audioQualification.pointerViews.slice(-4));
    const combinedStage = await page.locator("#scene").screenshot({ path: path.join(artifacts, "fourier-with-audio-stage-2.png") });
    const combinedRaster = PNG.sync.read(combinedStage);
    const combinedPixels = hash(combinedRaster.data);
    const combinedBackingUrl = await canvas.evaluate(element => element.toDataURL("image/png"));
    const combinedBacking = PNG.sync.read(Buffer.from(combinedBackingUrl.split(",", 2)[1], "base64"));
    const combinedBackingPixels = hash(combinedBacking.data);
    const combinedRender = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
    const combinedViewport = await replayViewport(canvas, { width: 960, height: 540 });
    const combinedReceipt = combinedSeek.presentationReceipt ?? null;
    await page.evaluate(diagnostic => { audioQualification.pixelDiagnostic = diagnostic; }, {
      time: combinedSeek.time, pixels: combinedPixels, viewport: combinedViewport,
      backingPixels: combinedBackingPixels, backingWidth: combinedBacking.width, backingHeight: combinedBacking.height,
      view: combinedView, receipt: combinedReceipt, pointerViews: combinedPointerViews,
      render: { time: combinedRender.metrics.time, session: combinedRender.metrics.presentedSession,
        backend: combinedRender.metrics.backend, objectCount: combinedRender.metrics.objectCount,
        drawCalls: combinedRender.metrics.drawCalls, instancesDrawn: combinedRender.metrics.instancesDrawn,
        bytesUploaded: combinedRender.metrics.bytesUploaded,
        geometryCacheMisses: combinedRender.metrics.geometryCacheMisses,
        outlineCacheMisses: combinedRender.metrics.outlineCacheMisses,
        rendererRebuilds: combinedRender.metrics.rendererRebuilds,
        gpuGeneration: combinedRender.metrics.gpuGeneration, needsPresent: combinedRender.metrics.needsPresent },
    });
    await pausedSeek(15);
    await replayViewport(canvas, { width: 960, height: 540 });
    const combinedRepeat = await page.locator("#scene").screenshot({ path: path.join(artifacts, "fourier-with-audio-stage-2-repeat.png") });
    assert.equal(hash(PNG.sync.read(combinedRepeat).data), combinedPixels,
      "the combined scene reproduces pixels for a repeated seek at the same authored time");
    assert.deepEqual({ width: combinedRaster.width, height: combinedRaster.height },
      { width: fourierReferencePixels.width, height: fourierReferencePixels.height });
    assert.equal(combinedPixels, fourierReferencePixels.pixels,
      "the documented soundtrack insertion leaves the matching authored scene pixels unchanged");
    await canvas.evaluate((element, style) => style === null ? element.removeAttribute("style") : element.setAttribute("style", style), canvasStyle);
    await pausedSeek(15);
    const seek = await pausedSeek(2);
    assert.equal(seek.playing, false);
    const startsBeforeSeekPlay = (await state()).starts.length;
    assert.equal(startsBeforeSeekPlay, finalState.starts.length, "seek does not start another device source");
    await page.getByRole("button", { name: "Play animation", exact: true }).click();
    await waitForBrowserObservation(page, async count => audioQualification.starts.length > count, startsBeforeSeekPlay);
    const resumed = await audible();
    const resumedVoice = resumed.starts.at(-1);
    const resumedAnchor = resumed.anchors.find(item => item.sources.includes(resumedVoice.node));
    assert.ok(resumedAnchor);
    const resumedPhase = resumedAnchor.time + (resumedAnchor.receivedEpochMs - resumedAnchor.sampledEpochMs) / 1000;
    near(resumedVoice.args[1], resumedPhase, "seeked soundtrack sample offset", 0.02);
    assert.equal(resumedVoice.args.length, 2, "prepared seek uses the bounded source stop path");
    const resumedStop = resumed.stops.filter(stop => stop.node === resumedVoice.node && stop.args.length > 0).at(-1);
    assert.ok(resumedStop, "the seeked voice has an explicit Rust-authorized end");
    near(resumedStop.args[0] - resumedVoice.args[0], 34.65 - resumedPhase,
      "seeked soundtrack remaining duration", 0.02);
    assert.equal(resumed.transport.hasAudio, true);
    assertEnvelope(resumed, resumedVoice, [[0, 0.5], [32.15 - resumedPhase, 0.5], [34.65 - resumedPhase, 0]]);
    await page.waitForTimeout(180);
    const paused = await command("pause");
    const pauseMetrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
    near(Number(pauseMetrics.metrics.time), paused.time, "paused renderer receipt", 0);
    await quiet();
    const afterPause = await state();
    const startsAfterPause = afterPause.starts.length;
    assert.ok(afterPause.stops.some(stop => stop.node === resumedVoice.node && stop.args.length === 0),
      "pause retires the seeked audio voice immediately");
    const staleExtension = await page.evaluate(() => {
      const latest = audioQualification.audioDeviceAnchors.at(-1);
      const before = audioQualification.starts.length;
      return { accepted: audioQualification.audioDevice.extendAnchor(latest.anchor, latest.session),
        startsBefore: before, startsAfter: audioQualification.starts.length };
    });
    assert.equal(staleExtension.accepted, false, "an old buffer extension cannot resume after explicit Pause");
    assert.equal(staleExtension.startsAfter, staleExtension.startsBefore,
      "a stale audio anchor after Pause creates no device source");
    const replaySeek = await pausedSeek(33);
    assert.equal(replaySeek.playing, false);
    assert.equal((await state()).starts.length, startsAfterPause, "a later seek remains silent until Play");
    await page.getByRole("button", { name: "Play animation", exact: true }).click();
    await waitForBrowserObservation(page, async count => audioQualification.starts.length > count, startsAfterPause);
    const fadePlay = await audible();
    assert.equal(fadePlay.transport.time >= 33, true);
    const fadeVoice = fadePlay.starts.at(-1);
    const fadeAnchor = fadePlay.anchors.find(item => item.sources.includes(fadeVoice.node));
    assert.ok(fadeAnchor, "the fade-out voice also uses a Rust transport anchor");
    const fadePhase = fadeAnchor.time + (fadeAnchor.receivedEpochMs - fadeAnchor.sampledEpochMs) / 1000;
    assert.ok(fadePhase >= 33 && fadePhase < 34.65);
    const fadeGain = 0.5 * Math.max(0, Math.min(1, (34.65 - fadePhase) / 2.5));
    assertEnvelope(fadePlay, fadeVoice, [[0, fadeGain], [34.65 - fadePhase, 0]]);
    const priorLoopCount = fadePlay.transport.loopCount;
    await waitForBrowserObservation(page, async firstLoop => {
      const playback = await audioQualification.execution.state();
      return playback.completed === true || playback.loopCount > firstLoop;
    }, priorLoopCount, { timeout: 10_000 });
    if ((await state()).transport.playing) await command("pause");
    const ended = await state();
    assert.ok(ended.ended.some(item => item.node === fadeVoice.node),
      "the finite schedule retires its final device source after the authored fade");
    await quiet();
    const sourceEvents = await page.evaluate(start => audioQualification.sourceTrace.slice(start), run.traceStart);
    assert.equal(sourceEvents.filter(event => event.phase === "source-run-start").length, 1,
      "seeking and replaying never rerun the combined Python source");
    return { originalSha256: hash(fourierEpicycles), combinedSha256: hash(fourierWithAudio),
      insertion: fourierAudioCall.trim(), initial: run.attached.transport, completed: finalState.transport,
      sourceComplete: completed.sourceState.transport,
      publication: completed.events.filter(event => event.phase === "terminal-adopted").at(-1)?.publication ?? null,
      render: { time: combinedRender.metrics.time, session: combinedRender.metrics.presentedSession,
        backend: combinedRender.metrics.backend, objectCount: combinedRender.metrics.objectCount,
        drawCalls: combinedRender.metrics.drawCalls, instancesDrawn: combinedRender.metrics.instancesDrawn,
        rendererRebuilds: combinedRender.metrics.rendererRebuilds, pointerView: combinedRender.pointerView ?? null },
      viewport: combinedViewport,
      initialVoice: { node: voice.node, anchor: anchor.time, phase, initialStop: initialStop.args[0],
        finalStop: extendedStops.at(-1).args[0] },
      deviceBuffering: { held: heldAnchor.anchor, resumed: bufferResumeAnchor.anchor,
        resumedNode: bufferResumeAnchor.starts[0].node, resumedPhase: bufferResumeAnchorPhase },
      seek: seek.time, resumedVoice: { node: resumedVoice.node, phase: resumedPhase,
        duration: resumedVoice.args[2] },
      paused: { time: paused.time, starts: startsAfterPause }, fadePlay: { time: fadePlay.transport.time,
        anchor: fadeAnchor.time, phase: fadePhase, gain: fadeGain },
      deviceTransitions: await page.evaluate(() => audioQualification.deviceTransitions),
      ended: ended.transport };
  });

  assert.deepEqual(report.browserErrors, [], "no uncaught browser/WASM errors");
  assert.deepEqual(report.consoleErrors, [], "no browser audio/GPU errors");
  report.status = "passed";
  const evidence = {
    schema: 1, publication: "captured-preview", captureRevision: report.runtimeBuildIdentity.sourceRevision,
    checkoutRevision: report.revision,
    backend, runtimeBuildIdentity: report.runtimeBuildIdentity,
    qualification: { script: "scripts/python-background-audio-smoke.mjs", browserName,
      browserVersion: report.browserVersion, status: report.status, sourceDirty: report.sourceDirty },
    posters: [posterRecord],
  };
  await writeFile(path.join(artifacts, "capture-evidence.json"), json(evidence));
  // Retain only a source-bound real capture after the complete browser profile passes.
  // Opt-in avoids one backend silently overwriting another qualification's poster.
  if (process.env.NOON_PYTHON_AUDIO_RETAIN_POSTER === "1") {
    assert.equal(report.sourceDirty, false, "retaining a publishable poster requires a clean checkout");
    assert.equal(report.runtimeBuildIdentity.sourceRevision, report.revision,
      "retaining a publishable poster requires a clean, source-attested runtime build");
    await writeFile(path.join(root, "web", entry.thumbnail), poster);
    await writeFile(path.join(root, "web/thumbnails/showcase/showcase-background-music-evidence.json"), json(evidence));
  }
} catch (error) {
  report.status = "failed";
  report.error = String(error.stack ?? error);
  process.exitCode = 1;
  console.error(report.error);
  const hasExecution = await page?.evaluate(() => Boolean(window.audioQualification?.execution)).catch(() => false);
  report.failureState = hasExecution ? await state().catch(error => ({ unavailable: String(error) }))
    : { unavailable: "the authoring execution owner was not ready" };
  if (hasExecution) {
    report.failureLifecycle = await page.evaluate(() => ({
      sourceTrace: window.audioQualification.sourceTrace.slice(-16),
      deviceTransitions: window.audioQualification.deviceTransitions ?? [],
      deviceAnchorHistory: window.audioQualification.audioDeviceAnchors ?? [],
      audioApplyAnchors: window.audioQualification.audioApplyAnchors ?? [],
      devicePauses: window.audioQualification.devicePauses ?? [],
      documentHidden: document.hidden,
      visibilityState: document.visibilityState,
      audioDeviceReady: window.audioQualification.audioDevice?.ready ?? null,
      pixelDiagnostic: window.audioQualification.pixelDiagnostic ?? null,
    })).catch(error => ({ unavailable: String(error) }));
    report.failureFrame = await command("debugFrame").catch(error => ({ unavailable: String(error) }));
    report.failureMetrics = await command("metrics").catch(error => ({ unavailable: String(error) }));
  }
  await page?.screenshot({ path: path.join(artifacts, "failure.png"), fullPage: true }).catch(() => {});
} finally {
  report.runtimeResources = cache?.stats() ?? null;
  await browser?.close();
  await server?.close();
  await writeFile(path.join(artifacts, "report.json"), json(report));
}
