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

// Only this qualification knows the authored button coordinates. Production
// input still crosses the normal collector, displayed-frame receipt and Rust
// picker; no test supplies a semantic target or dispatches its action directly.
const sceneControls = { previous: -3.6, play: -1.8, next: 0, restart: 1.8, mute: 3.6 };
async function scenePosition(x, y = -3.45) {
  const canvas = page.locator("#scene");
  await canvas.scrollIntoViewIfNeeded();
  assert.notEqual(await canvas.evaluate(element => getComputedStyle(element).pointerEvents), "none",
    "physical interaction qualification requires the canvas's normal pointer-event policy");
  const box = await canvas.boundingBox();
  assert.ok(box, "scene canvas must have a visible surface");
  return { x: box.x + box.width / 2 + x * box.height / 8,
    y: box.y + box.height / 2 - y * box.height / 8 };
}

async function hoverScenePosition(x, y = -3.45) {
  const position = await scenePosition(x, y);
  await page.mouse.move(position.x, position.y);
  // Hover allows a real viewport change to complete its ordinary presentation
  // handshake before this test begins a new physical press/release gesture.
  await page.waitForFunction(() => audioQualification.pointerPlayer?.pointerPresentation != null &&
    audioQualification.pointerInputs.every(input => input.status !== "pending"));
  return position;
}

async function pointerCompletion(first, kind = "release") {
  await page.waitForFunction(({ first, kind }) => {
    const inputs = audioQualification.pointerInputs.slice(first);
    return inputs.some(input => input.input.kind === kind) &&
      inputs.every(input => input.status !== "pending");
  }, { first, kind });
  const inputs = await page.evaluate(first => audioQualification.pointerInputs.slice(first), first);
  assert.ok(inputs.every(input => input.status === "fulfilled"),
    `physical pointer delivery failed: ${JSON.stringify(inputs)}`);
  return inputs;
}

async function clickSceneControl(name, { settle = true, y = -3.45 } = {}) {
  assert.ok(Object.hasOwn(sceneControls, name), "known authored scene control");
  const position = await hoverScenePosition(sceneControls[name], y);
  const first = await page.evaluate(() => audioQualification.pointerInputs.length);
  await page.mouse.click(position.x, position.y);
  return settle ? pointerCompletion(first) : first;
}

async function scenePlay() {
  await clickSceneControl("play");
  await waitForBrowserObservation(page, async () => (await audioQualification.execution.state()).playing);
  return audible();
}

async function authoredRevisions() {
  const { publication } = await command("debugFrame");
  assert.notEqual(publication?.scene_revision, undefined);
  assert.notEqual(publication?.execution_revision, undefined);
  return { scene: publication.scene_revision, execution: publication.execution_revision };
}

async function pausedSceneTime(time) {
  await waitForBrowserObservation(page, async time => {
    const result = await audioQualification.execution.state();
    return result.time === time && !result.playing;
  }, time);
  const metrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
  near(Number(metrics.metrics.time), time, "scene command's retained picture time", 0);
  return state();
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
    const original = AuthoringExecutionClient.prototype.startSemanticExecution;
    AuthoringExecutionClient.prototype.startSemanticExecution = function (...args) {
      audioQualification.execution = this;
      return original.apply(this, args);
    };
    const { ExecutionWorkerClient } = await import("./execution-worker-client.js");
    const submitPointer = ExecutionWorkerClient.prototype.submitBrowserPointerInput;
    audioQualification.pointerInputs = [];
    ExecutionWorkerClient.prototype.submitBrowserPointerInput = function (input) {
      audioQualification.pointerPlayer = this;
      const observation = { input: structuredClone(input),
        presentation: structuredClone(this.pointerPresentation), status: "pending" };
      audioQualification.pointerInputs.push(observation);
      // Observation only: invoke the production method in this same gesture turn
      // so browser activation is neither delayed nor synthesized by the harness.
      const result = submitPointer.call(this, input);
      return result.then(value => {
        observation.status = "fulfilled";
        observation.result = structuredClone(value);
        return value;
      }, error => {
        observation.status = "rejected";
        observation.error = String(error);
        throw error;
      });
    };
    const { WasmWorkerAudioOutput } = await import("./pkg/noon_web.js");
    const start = WasmWorkerAudioOutput.prototype.start;
    audioQualification.anchors = [];
    WasmWorkerAudioOutput.prototype.start = function (session, time, sampledEpochMs, receivedEpochMs) {
      const before = audioQualification.starts.length;
      const result = start.call(this, session, time, sampledEpochMs, receivedEpochMs);
      audioQualification.anchors.push({ session, time, sampledEpochMs, receivedEpochMs,
        sources: audioQualification.starts.slice(before).map(source => source.node) });
      return result;
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

  await record("a superseded scene Play cannot resume after its delayed device unlock", async () => {
    const revision = await authoredRevisions();
    await page.evaluate(() => { audioQualification.holdNextResume = true; });
    const first = await clickSceneControl("play", { settle: false });
    await page.waitForFunction(() => audioQualification.pendingResumes.length === 1);
    const pending = await state();
    assert.equal(pending.transport.playing, false, "device preparation cannot advance the worker");
    assert.equal(pending.starts.length, 0);
    await command("pause");
    await page.evaluate(() => { for (const resolve of audioQualification.pendingResumes.splice(0)) resolve(); });
    const inputs = await pointerCompletion(first);
    const cancelled = await pausedSceneTime(0);
    assert.equal(cancelled.starts.length, 0, "superseded unlock must not start a source");
    assert.ok(inputs.some(input => input.input.kind === "release" &&
      input.result.sceneControlApplied === false), "the held gesture must report nonfatal cancellation");
    assert.deepEqual(await authoredRevisions(), revision);
    await quiet();
    return { revision, pending, cancelled, inputs };
  });

  await record("trusted scene Play produces original stereo music with authored fades", async () => {
    const running = await scenePlay();
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
    const pointerPolicy = await canvas.evaluate(element => ({
      value: element.style.getPropertyValue("pointer-events"),
      priority: element.style.getPropertyPriority("pointer-events"),
    }));
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
    // The capture-only layout intentionally disables hit testing. Restore the
    // exact prior policy before subsequent real scene-button qualification.
    await canvas.evaluate((element, policy) => {
      if (policy.value) element.style.setProperty("pointer-events", policy.value, policy.priority);
      else element.style.removeProperty("pointer-events");
    }, pointerPolicy);
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

  await record("scene section buttons seek the actual picture and music with preserved play state", async () => {
    const revision = await authoredRevisions();
    await clickSceneControl("play");
    const paused = await state();
    assert.equal(paused.transport.playing, false);
    await quiet();
    await page.waitForTimeout(100);
    near((await state()).transport.time, paused.transport.time, "scene pause freezes authored time", 0);
    await clickSceneControl("restart");
    await pausedSceneTime(0);
    await clickSceneControl("previous");
    await pausedSceneTime(0);
    const starts = (await state()).starts.length;
    await clickSceneControl("next");
    const positive = await pausedSceneTime(10);
    const positivePicture = await currentPicture(2, "scene-next-10.png");
    await clickSceneControl("next");
    const negative = await pausedSceneTime(20);
    const negativePicture = await currentPicture(-2, "scene-next-20.png");
    await clickSceneControl("next");
    await pausedSceneTime(20);
    await clickSceneControl("previous");
    const previous = await pausedSceneTime(10);
    assert.equal(previous.starts.length, starts, "paused scene navigation stays silent");
    await quiet();
    const running = await scenePlay();
    correctedPhase(running, running.starts.at(-1), 10);
    const nextInputs = await clickSceneControl("next");
    const nextWhilePlaying = await audible();
    assert.equal(nextWhilePlaying.transport.playing, true);
    assert.equal(nextWhilePlaying.starts.length, running.starts.length + 1);
    correctedPhase(nextWhilePlaying, nextWhilePlaying.starts.at(-1), 20);
    const restartInputs = await clickSceneControl("restart");
    const restarted = await audible();
    assert.equal(restarted.transport.playing, true);
    assert.equal(restarted.starts.length, nextWhilePlaying.starts.length + 1);
    assert.ok(correctedPhase(restarted, restarted.starts.at(-1), 0) < 1.5);
    assert.equal(restarted.buffers.length, 1, "scene controls reuse the original immutable PCM");
    assert.deepEqual(await authoredRevisions(), revision);
    return { revision, paused, positive, negative, previous, running, nextWhilePlaying, restarted,
      pictures: { positive: positivePicture.points, negative: negativePicture.points },
      nextInputs, restartInputs };
  });

  await record("scene Sound changes only viewer gain and keeps the same device source", async () => {
    const revision = await authoredRevisions();
    const before = await state();
    await clickSceneControl("mute");
    await quiet();
    const muted = await state();
    assert.equal(muted.transport.audioMuted, true);
    assert.equal(muted.transport.playing, true);
    assert.equal(muted.starts.length, before.starts.length);
    assert.ok(muted.transport.time > before.transport.time);
    await clickSceneControl("mute");
    const restored = await audible();
    assert.equal(restored.transport.audioMuted, false);
    assert.equal(restored.starts.length, before.starts.length, "scene mute must not restart clip phase");
    assert.equal(restored.transport.audioVolume, before.transport.audioVolume);
    assert.deepEqual(await authoredRevisions(), revision);
    return { revision, before, muted, restored };
  });

  await record("background, dragged and cancelled gestures cannot activate a scene button", async () => {
    await clickSceneControl("play");
    await clickSceneControl("restart");
    const before = await pausedSceneTime(0);
    const revision = await authoredRevisions();
    const background = await hoverScenePosition(5.8, 2);
    let first = await page.evaluate(() => audioQualification.pointerInputs.length);
    await page.mouse.click(background.x, background.y);
    const backgroundInputs = await pointerCompletion(first);
    const afterBackground = await pausedSceneTime(0);
    const button = await hoverScenePosition(sceneControls.play);
    first = await page.evaluate(() => audioQualification.pointerInputs.length);
    await page.mouse.down();
    await pointerCompletion(first, "press");
    await page.mouse.move(button.x + 30, button.y, { steps: 3 });
    await page.mouse.move(button.x, button.y, { steps: 3 });
    await page.mouse.up();
    const dragInputs = await pointerCompletion(first);
    const afterDrag = await pausedSceneTime(0);
    first = await page.evaluate(() => audioQualification.pointerInputs.length);
    await page.mouse.down();
    const presses = await pointerCompletion(first, "press");
    const pointerId = presses.find(input => input.input.kind === "press").input.pointer_id;
    // Supply only the browser cancellation occurrence, never a target/action.
    // The later physical release must not reconstruct a successful gesture.
    await page.locator("#scene").dispatchEvent("pointercancel", {
      pointerId, pointerType: "mouse", isPrimary: true, button: 0, buttons: 0,
      clientX: button.x, clientY: button.y,
    });
    await page.mouse.up();
    const cancelInputs = await pointerCompletion(first, "cancel");
    const afterCancel = await pausedSceneTime(0);
    for (const snapshot of [afterBackground, afterDrag, afterCancel]) {
      assert.equal(snapshot.starts.length, before.starts.length);
      assert.equal(snapshot.transport.playing, false);
    }
    assert.deepEqual(await authoredRevisions(), revision);
    await quiet();
    const fresh = await scenePlay();
    assert.equal(fresh.starts.length, before.starts.length + 1,
      "a fresh complete click must still work after cancellation");
    return { revision, before, afterBackground, afterDrag, afterCancel, fresh,
      backgroundInputs, dragInputs, cancelInputs };
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

  await record("Run retires a delayed scene Play without reviving either audio owner", async () => {
    const before = await state();
    await page.evaluate(async () => {
      await audioQualification.contexts.at(-1).context.suspend();
      audioQualification.holdNextResume = true;
    });
    const first = await clickSceneControl("play", { settle: false });
    await page.waitForFunction(() => audioQualification.pendingResumes.length === 1);
    assert.equal((await state()).transport.playing, false);
    await page.evaluate(() => window.__noonExampleGallery.run());
    await waitReady();
    await page.evaluate(() => { for (const resolve of audioQualification.pendingResumes.splice(0)) resolve(); });
    await page.waitForFunction(first => audioQualification.pointerInputs.slice(first)
      .every(input => input.status !== "pending"), first);
    const replaced = await pausedSceneTime(0);
    assert.equal(replaced.starts.length, before.starts.length,
      "old unlock completion cannot start old or replacement output");
    assert.equal(replaced.buffers.length, before.buffers.length + 1,
      "replacement owns its independently prepared immutable device buffer");
    assert.ok(replaced.output.slice(0, -1).every(output => output.state === "closed"),
      "every retired audio owner must close its context");
    await quiet();
    const inputs = await page.evaluate(first => audioQualification.pointerInputs.slice(first), first);
    return { before, replaced, inputs };
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

  await record("finite scene controls also work without allocating an audio device", async () => {
    const fixture = `from noon import BLUE, PlaybackControl, Rectangle, Scene

result = Scene()
for x, action in [(-3.6, PlaybackControl.PREVIOUS_SECTION),
                  (-1.8, PlaybackControl.TOGGLE_PLAYBACK),
                  (0, PlaybackControl.NEXT_SECTION),
                  (1.8, PlaybackControl.RESTART),
                  (3.6, PlaybackControl.TOGGLE_AUDIO_MUTED)]:
    button = Rectangle(width=1.55, height=0.48).set_fill(BLUE, opacity=1)
    result.add(button.move_to((x, 0, 0)))
    result.on_click_playback(button, action)
for name in ["First", "Second", "Third"]:
    result.next_section(name)
    result.declare_wait(2)
`;
    const before = await state();
    await page.evaluate(async source => {
      const editor = document.querySelector("#python-scene-source");
      if (!(editor instanceof HTMLTextAreaElement)) throw new Error("ordinary Python editor is unavailable");
      editor.value = source;
      await window.__noonExampleGallery.run();
    }, fixture);
    await page.waitForFunction(() => {
      const patch = document.querySelector("#patch-status");
      if (patch?.dataset.state === "error") throw new Error(patch.value || patch.textContent);
      return patch?.dataset.state === "applied" && !window.__noonExampleGallery?.runInFlight;
    });
    // Keep this fixture independent of the visual-only viewer's autoplay policy.
    await command("pause");
    await command("seek", 0);
    const prepared = await pausedSceneTime(0);
    assert.notEqual(prepared.transport.hasAudio, true);
    const revision = await authoredRevisions();
    await command("seek", 3);
    await clickSceneControl("next", { y: 0 });
    const next = await pausedSceneTime(4);
    await clickSceneControl("previous", { y: 0 });
    const previous = await pausedSceneTime(2);
    await clickSceneControl("restart", { y: 0 });
    const restarted = await pausedSceneTime(0);
    await clickSceneControl("play", { y: 0 });
    await waitForBrowserObservation(page, async () => {
      const result = await audioQualification.execution.state();
      return result.playing && result.time > 0;
    });
    await clickSceneControl("play", { y: 0 });
    const paused = await state();
    assert.equal(paused.transport.playing, false);
    await page.waitForTimeout(100);
    near((await state()).transport.time, paused.transport.time, "silent scene pause freezes time", 0);
    await command("seek", 5.75);
    await clickSceneControl("play", { y: 0 });
    await pausedSceneTime(6);
    await clickSceneControl("play", { y: 0 });
    await waitForBrowserObservation(page, async () => {
      const result = await audioQualification.execution.state();
      return result.playing && result.time >= 0 && result.time < 2;
    });
    await clickSceneControl("play", { y: 0 });
    const final = await state();
    assert.equal(final.transport.playing, false);
    assert.equal(final.buffers.length, before.buffers.length, "visual-only controls create no PCM buffer");
    assert.equal(final.output.length, before.output.length, "visual-only controls create no AudioContext");
    assert.equal(final.starts.length, before.starts.length);
    assert.deepEqual(await authoredRevisions(), revision);
    return { fixtureSha256: hash(fixture), revision, prepared, next, previous, restarted, paused, final };
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
    report.failureFrame = await command("debugFrame").catch(error => ({ unavailable: String(error) }));
    report.failureMetrics = await command("metrics").catch(error => ({ unavailable: String(error) }));
    report.failurePointerInput = await page.evaluate(() => ({
      inputs: audioQualification.pointerInputs,
      presentation: audioQualification.pointerPlayer?.pointerPresentation ?? null,
      canvasPointerEvents: getComputedStyle(document.querySelector("#scene")).pointerEvents,
    })).catch(error => ({ unavailable: String(error) }));
  }
  await page?.screenshot({ path: path.join(artifacts, "failure.png"), fullPage: true }).catch(() => {});
} finally {
  report.runtimeResources = cache?.stats() ?? null;
  await browser?.close();
  await server?.close();
  await writeFile(path.join(artifacts, "report.json"), json(report));
}
