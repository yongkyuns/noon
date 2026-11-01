// Compact sampled motion through real Python, the canonical authoring worker,
// retained historical playback, both renderers/transports, and actual Web Audio.
// The original Fourier source is preserved; the variant changes native_poses only.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { observeAudio } from "./browser-audio-observation.mjs";
import { waitForBrowserObservation } from "./playground-browser-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.resolve(process.env.NOON_SAMPLED_MOTION_ARTIFACTS ??
  path.join(root, "browser-smoke-artifacts/sampled-motion"));
const backends = (process.env.NOON_SAMPLED_MOTION_BACKENDS ?? "webgl,webgpu").split(",");
const transports = (process.env.NOON_SAMPLED_MOTION_TRANSPORTS ?? "transferable,shared").split(",");
const selected = (process.env.NOON_SAMPLED_MOTION_CASES ?? "paired,original,sampled").split(",");
assert.ok(backends.length && backends.every(x => ["webgl", "webgpu"].includes(x)));
assert.ok(transports.length && transports.every(x => ["transferable", "shared"].includes(x)));
assert.ok(selected.length && selected.every(x => ["paired", "original", "sampled"].includes(x)));
const admission = "normal";
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const json = value => JSON.stringify(value, (_, x) => typeof x === "bigint" ? String(x) : x, 2) + "\n";
const near = (actual, expected, name, tolerance = 1e-6) =>
  assert.ok(Number.isFinite(actual) && Math.abs(actual - expected) <= tolerance,
    name + ": " + actual + " != " + expected + " (tolerance " + tolerance + ")");
const pointNear = (actual, expected, name, tolerance = 2e-5) => {
  assert.ok(Array.isArray(actual) && actual.length === 2, name + " is a point");
  near(actual[0], expected[0], name + " x", tolerance);
  near(actual[1], expected[1], name + " y", tolerance);
};
const positionTolerance = 2e-5;
const angleTolerance = 4e-5;
const revealTolerance = 2e-7;
const python = process.env.NOON_SAMPLED_MOTION_PYTHON ?? process.env.CODEX_PRIMARY_RUNTIME_PYTHON ?? "python3";
const sourceComparison = JSON.parse(execFileSync(python, [path.join(root, "scripts/sampled-motion-fixture-check.py"), "--oracle"],
  { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }));
assert.equal(sourceComparison.status, "passed");
const fixtureDirectory = path.join(root, "tests/fixtures/fourier_audio");
const sources = {
  original: await readFile(path.join(fixtureDirectory, "fourier_epicycles.py"), "utf8"),
  sampled: await readFile(path.join(fixtureDirectory, "fourier_epicycles_sampled.py"), "utf8"),
  paired: await readFile(path.join(root, "web/python/examples/sampled_motion.py"), "utf8"),
};
assert.equal(hash(sources.original), sourceComparison.original_sha256);
assert.equal(hash(sources.sampled), sourceComparison.compact_sha256);
for (const name of ["original", "sampled"]) {
  const firstPlay = "        self.play(FadeIn(title)";
  const insertion = "        self.audio(AudioSource.small_discovery(), duration=34.65, gain=0.5, fade_in=1.5, fade_out=2.5)\n";
  assert.equal(sources[name].split(firstPlay).length, 2);
  const original = sources[name];
  sources[name] = original.replace(firstPlay, insertion + firstPlay);
  assert.equal(sources[name].replace(insertion, ""), original);
}
const pairedAudio = "        self.audio(AudioSource.small_discovery(), duration=2.3, gain=0.5)\n";
assert.equal(sources.paired.split("        self.play(").length, 2);
sources.paired = sources.paired.replace("        self.play(", pairedAudio + "        self.play(");
const workerSource = await readFile(path.join(root, "web/python-worker.source.js"), "utf8");
const pinnedPyodide = workerSource.match(/from "(https:\/\/cdn\.jsdelivr\.net\/pyodide\/v[^/]+\/full\/pyodide\.mjs)"/)?.[1];
assert.ok(pinnedPyodide, "production worker must pin Pyodide");
const cache = createPyodideResourceCache(workerSource);
const report = {
  status: "running",
  scope: "sampled motion fidelity and structural construction savings; real Python/WASM/render workers/Web Audio; no physical-device performance claim",
  revision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  sourceDirty: execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim() !== "",
  harnessSha256: hash(await readFile(fileURLToPath(import.meta.url))),
  configuration: { backends, transports, selected, admission, python },
  tolerances: { positionSceneUnits: positionTolerance, rotationRadians: angleTolerance, reveal: revealTolerance },
  sources: Object.fromEntries(Object.entries(sources).map(([name, source]) => [name, { sha256: hash(source), bytes: Buffer.byteLength(source) }])),
  sourceComparison: { ...sourceComparison, stages: sourceComparison.stages.map(({ oracle, ...stage }) => stage) },
  pinnedPyodide, localPyodideResources: {}, browserArguments: {}, cases: [],
};
await mkdir(output, { recursive: true });
for (const [name, source] of Object.entries(sources)) await writeFile(path.join(output, name + "-audio.py"), source);
await writeFile(path.join(output, "source-comparison.json"), json(sourceComparison));
let browser, server;
const comparisons = new Map();

async function bounded(promise, label, timeout = 30_000) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(label + " timed out")), timeout);
    })]);
  } finally { clearTimeout(timer); }
}

async function installResources(context) {
  await cache.install(context);
  const local = process.env.NOON_PYODIDE_ROOT;
  if (!local) return;
  const base = new URL(".", pinnedPyodide).href;
  // Optional byte cache only: URLs, version, source worker and import behavior
  // remain the production ones. Every used byte identity is retained in evidence.
  await context.route(url => url.href.startsWith(base), async route => {
    const name = new URL(route.request().url()).pathname.split("/").at(-1);
    assert.equal(path.basename(name), name);
    try {
      const bytes = await readFile(path.join(local, name));
      report.localPyodideResources[name] = { bytes: bytes.length, sha256: hash(bytes) };
      await route.fulfill({ status: 200, body: bytes, headers: {
        "access-control-allow-origin": "*", "cross-origin-resource-policy": "cross-origin",
        "content-type": name.endsWith(".mjs") ? "text/javascript" :
          name.endsWith(".wasm") ? "application/wasm" :
            name.endsWith(".json") ? "application/json" : "application/octet-stream",
      } });
    } catch (error) {
      if (error.code === "ENOENT") return route.fallback();
      throw error;
    }
  });
}

async function installObservers(page) {
  await page.evaluate(async () => {
    const { WasmWorkerAudioOutput } = await import("./pkg/noon_web.js");
    const { WorkerAudioDevice } = await import("./worker-audio-device.js");
    audioQualification.anchors = [];
    audioQualification.boundedStarts = [];
    audioQualification.devices = [];
    const apply = WorkerAudioDevice.prototype.applyAnchor;
    WorkerAudioDevice.prototype.applyAnchor = function (anchor, session) {
      if (!audioQualification.devices.includes(this)) audioQualification.devices.push(this);
      const before = audioQualification.starts.length;
      const result = apply.call(this, anchor, session);
      audioQualification.anchors.push({
        anchor: structuredClone(anchor), result,
        sources: audioQualification.starts.slice(before).map(x => x.node),
      });
      return result;
    };
    const bounded = WasmWorkerAudioOutput.prototype.startBounded;
    if (typeof bounded !== "function") throw new Error("candidate lacks bounded audio output");
    WasmWorkerAudioOutput.prototype.startBounded = function (session, time, sampledEpochMs, receivedEpochMs, availableUntil) {
      const before = audioQualification.starts.length;
      const result = bounded.call(this, session, time, sampledEpochMs, receivedEpochMs, availableUntil);
      audioQualification.boundedStarts.push({
        session, time, sampledEpochMs, receivedEpochMs, availableUntil,
        sources: audioQualification.starts.slice(before).map(x => x.node),
      });
      return result;
    };
  });
}

async function launchSource(page, source, transportMode) {
  await bounded(page.evaluate(async ({ source, transportMode }) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const { PlaygroundPlaybackControls } = await import("./playground-playback-controls.js");
    const host = { authoring: new PythonAuthoringClient(new Worker("./python-worker.js", { type: "module" })),
      errors: [], registration: null, outcome: null, attached: false };
    globalThis.preparedAudio = host;
    const canvas = document.querySelector("#scene");
    canvas.width = 960; canvas.height = 540; canvas.style.width = "960px"; canvas.style.height = "540px";
    host.execution = new AuthoringExecutionClient(canvas, {
      onError: error => host.errors.push(String(error)),
      onRecoverableError: error => host.errors.push(String(error)),
    });
    host.run = host.authoring.run(source, {}, {
      async onSemanticContinuation(registration) {
        host.registration = registration;
        await host.execution.startSemanticExecution(registration.semanticExecution, {
          authoringClient: host.authoring, transportMode, initiallyPaused: true,
          loopDurationSeconds: Math.max(1, registration.duration),
        });
        const panel = document.createElement("section"); panel.id = "prepared-controls"; document.body.append(panel);
        host.controls = new PlaygroundPlaybackControls(host.execution, panel, {
          onError: error => host.errors.push(String(error)),
        });
        host.controls.observe(await host.execution.state());
        host.attached = true;
      },
    });
    void host.run.then(
      result => { host.outcome = { status: "finished", result }; },
      error => { host.outcome = { status: "failed", error: String(error) }; },
    );
  }, { source, transportMode }), "normal source setup");
  await page.waitForFunction(() => preparedAudio.attached || preparedAudio.outcome?.status === "failed");
  assert.equal(await page.evaluate(() => preparedAudio.attached), true,
    json(await page.evaluate(() => preparedAudio.outcome)));
  await bounded(waitForBrowserObservation(page, async () => {
    const { metrics } = await preparedAudio.execution.metrics();
    return metrics.ready && metrics.presentedFrames > 0 && !metrics.needsPresent && metrics.bufferedDeltas === 0;
  }), "initial renderer observation", 90_000);
}

async function snapshot(page) {
  return bounded(page.evaluate(async () => {
    const transport = await preparedAudio.execution.state();
    preparedAudio.controls?.observe(transport);
    return { ...audioQualification.snapshot(), transport, outcome: preparedAudio.outcome,
      errors: [...preparedAudio.errors], anchors: audioQualification.anchors,
      boundedStarts: audioQualification.boundedStarts };
  }), "prepared reader observation");
}
async function waitState(page, predicate, timeout = 15_000, observe = null) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const value = await snapshot(page);
    observe?.(value);
    assert.deepEqual(value.errors, []);
    assert.notEqual(value.outcome?.status, "failed", json(value.outcome));
    if (predicate(value)) return value;
    await page.waitForTimeout(30);
  }
  const last = await snapshot(page);
  assert.fail("prepared audio state did not settle: " + json({
    transport: last.transport, outcome: last.outcome, errors: last.errors,
  }));
}
async function quiet(page) {
  await page.waitForTimeout(180);
  const values = await page.evaluate(async () => {
    const windows = [];
    for (let i = 0; i < 6; i++) {
      windows.push(audioQualification.sample());
      await new Promise(resolve => setTimeout(resolve, 25));
    }
    return windows;
  });
  assert.ok(values.every(samples => samples.every(x => x.state !== "running" || x.peak <= 1e-7)),
    "actual device samples must remain silent");
  return values;
}
async function audible(page) {
  await page.waitForFunction(() => audioQualification.sample().some(x =>
    x.state === "running" && x.rms > 1e-5 && x.peak > 1e-4), null, { timeout: 10_000 });
}
async function play(page) {
  await snapshot(page);
  await page.getByRole("button", { name: "Play animation", exact: true }).click();
  await waitState(page, x => x.transport.playing);
  await audible(page);
}
async function pause(page) {
  await bounded(page.evaluate(() => preparedAudio.execution.pause()), "pause control");
  const state = await snapshot(page);
  assert.equal(state.transport.playing, false);
  await quiet(page);
  near((await snapshot(page)).transport.time, state.transport.time, "paused reader is stable", 0);
  return state;
}
async function seek(page, time) {
  await bounded(page.evaluate(time => preparedAudio.execution.seek(time), time), "seek control");
  const state = await snapshot(page);
  near(state.transport.time, time, "exact reader seek", 0);
  const metrics = await bounded(page.evaluate(async () =>
    (await preparedAudio.execution.metrics()).metrics), "seek presentation receipt");
  near(Number(metrics.time), time, "seek completes after corresponding picture", 0);
  return state;
}
async function picture(page, name) {
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const bytes = await page.locator("#scene").screenshot();
  await writeFile(path.join(output, name + ".png"), bytes);
  return PNG.sync.read(bytes).data;
}
function pairedPixelEvidence(pixels) {
  let bluePixels = 0, redPixels = 0;
  for (let index = 0; index < pixels.length; index += 4) {
    const [red, green, blue, alpha] = pixels.subarray(index, index + 4);
    if (alpha > 0 && blue > 100 && blue > red + 40 && green > 60) bluePixels++;
    if (alpha > 0 && red > 100 && red > green + 40 && red > blue + 40) redPixels++;
  }
  assert.ok(bluePixels > 10, "actual paired pixels contain the blue line");
  assert.ok(redPixels > 10, "actual paired pixels contain the red square");
  return { bluePixels, redPixels };
}
function assertBounded(snapshot) {
  assert.ok(snapshot.boundedStarts.length > 0, "native device used bounded prepared plans");
  for (const anchor of snapshot.boundedStarts) {
    assert.ok(anchor.availableUntil >= anchor.time);
    const phase = Math.min(anchor.availableUntil,
      anchor.time + Math.max(0, anchor.receivedEpochMs - anchor.sampledEpochMs) / 1000);
    for (const id of anchor.sources) {
      const voice = snapshot.starts.find(x => x.node === id);
      assert.ok(voice, "bounded output must identify each real source");
      near(voice.args[1], phase, "device clip phase follows accepted authored-time sample");
      assert.ok(voice.args[2] > 0, "no zero-duration device source");
      assert.ok(voice.args[1] + voice.args[2] <= anchor.availableUntil + 1e-6,
        "device source cannot outlive adopted visual availability");
    }
  }
}
function assertNaturalClock(anchors) {
  const accepted = anchors.filter(x => x.result !== false).map(x => x.anchor);
  for (let index = 1; index < accepted.length; index++) {
    const previous = accepted[index - 1], current = accepted[index];
    assert.equal(current.session, previous.session);
    assert.ok(current.time >= previous.time, "uninterrupted playback never rewinds authored time");
    const elapsed = (current.sampledEpochMs - previous.sampledEpochMs) / 1000;
    assert.ok(current.time - previous.time <= elapsed + 0.05,
      "new prefix cannot inherit expired preparation wall time: " + json({ previous, current }));
  }
}

function lerpKeys(keys, values, phase) {
  if (phase <= keys[0]) return [...values[0]];
  if (phase >= keys.at(-1)) return [...values.at(-1)];
  const upper = keys.findIndex(key => key >= phase), lower = upper - 1;
  const alpha = (phase - keys[lower]) / (keys[upper] - keys[lower]);
  return values[lower].map((value, axis) => value + (values[upper][axis] - value) * alpha);
}
function translation(object) {
  return [object.transform.translation.x, object.transform.translation.y];
}
function present(frame) {
  assert.ok(frame && Array.isArray(frame.objects), "inspect the shared retained reader frame");
  return frame.objects.filter(object => object.present);
}
function pairedObservation(frame, phase) {
  const rows = present(frame);
  assert.equal(rows.length, 2, "paired fixture has exactly two visible objects");
  const keys = [0, 0.13, 0.4, 0.73, 1];
  const centers = [[-2, 1], [-1, 2], [1, 1], [2, 2], [2, 0]];
  const expected = lerpKeys(keys, centers, phase);
  const result = { phase, objects: [] };
  for (const [index, sign, initial, radians] of [[0, 1, 0.25, -6 * Math.PI], [1, -1, -0.4, 8 * Math.PI]]) {
    const object = rows[index], center = [expected[0], sign * expected[1]];
    pointNear(object.center, center, "paired bounds center");
    pointNear(translation(object), center, "paired affine translation");
    const theta = initial + radians * phase;
    near(object.transform.rotation, theta, "paired signed unwrapped angle", angleTolerance);
    if (index === 0) {
      assert.ok(Array.isArray(object.path_endpoints), "direct line endpoint observation is available");
      pointNear(object.path_endpoints[0], [center[0] - Math.cos(theta) / 2, center[1] - Math.sin(theta) / 2], "paired line start");
      pointNear(object.path_endpoints[1], [center[0] + Math.cos(theta) / 2, center[1] + Math.sin(theta) / 2], "paired line end");
    }
    result.objects.push({ center: object.center, translation: translation(object),
      angle: object.transform.rotation, endpoints: object.path_endpoints });
  }
  return result;
}
function fourierObservation(frame, stage, phase) {
  const { oracle, terms } = stage, count = terms - 1;
  const rows = present(frame);
  // _manim_compat._manim_stroke_width and _manim_semantic_handles convert
  // authored widths with MANIM_CAIRO_LINE_WIDTH_MULTIPLE before Rust stores them.
  const manimCairoLineWidthMultiple = 0.01;
  const stroke = authoredWidth => rows.filter(object => object.stroke !== null &&
    Math.abs(object.stroke_width - authoredWidth * manimCairoLineWidthMultiple) < 1e-6);
  const circles = stroke(1), vectors = stroke(1.6), traces = stroke(3.2);
  assert.equal(circles.length, count, "every original epicycle circle is present");
  assert.equal(vectors.length, count, "every original rotating vector is present");
  const marker = diameter => rows.filter(object => object.fill !== null && object.bounds !== null &&
    Math.abs(object.bounds.width - diameter) < 1e-5 && Math.abs(object.bounds.height - diameter) < 1e-5);
  const halos = marker(0.26), tips = marker(0.09);
  assert.equal(halos.length, 1); assert.equal(tips.length, 1);
  const chain = oracle.chain[0].map((_, column) =>
    lerpKeys(oracle.keys, oracle.chain.map(row => row[column]), phase));
  const result = { terms, phase, circles: [], vectors: [], halo: halos[0].center, tip: tips[0].center, trace: null,
    objectIds: { circles: circles.map(x => x.id), vectors: vectors.map(x => x.id), halo: halos[0].id, tip: tips[0].id },
    presentObjectCount: rows.length, maxima: { centerError: 0, angleError: 0, tracePenGap: 0 } };
  const centerCheck = (actual, expected, label) => {
    pointNear(actual, expected, label, positionTolerance);
    result.maxima.centerError = Math.max(result.maxima.centerError, Math.hypot(actual[0] - expected[0], actual[1] - expected[1]));
  };
  for (let index = 0; index < count; index++) {
    centerCheck(circles[index].center, chain[index], "circle center " + index);
    const midpoint = chain[index].map((x, axis) => (x + chain[index + 1][axis]) / 2);
    centerCheck(vectors[index].center, midpoint, "vector midpoint " + index);
    const theta = 2 * Math.PI * oracle.frequencies[index] * phase;
    const angle = vectors[index].transform.rotation;
    near(angle, theta, "signed vector angle " + index, angleTolerance);
    result.maxima.angleError = Math.max(result.maxima.angleError, Math.abs(angle - theta));
    const [real, imaginary] = oracle.coefficients[index], cosine = Math.cos(theta), sine = Math.sin(theta);
    const offset = [real * cosine - imaginary * sine, real * sine + imaginary * cosine];
    assert.ok(Array.isArray(vectors[index].path_endpoints), "actual retained vector endpoints are observable");
    pointNear(vectors[index].path_endpoints[0], midpoint.map((x, axis) => x - offset[axis] / 2), "vector start " + index);
    pointNear(vectors[index].path_endpoints[1], midpoint.map((x, axis) => x + offset[axis] / 2), "vector end " + index);
    result.circles.push(circles[index].center);
    result.vectors.push({ center: vectors[index].center, angle, endpoints: vectors[index].path_endpoints });
  }
  centerCheck(halos[0].center, chain.at(-1), "halo follows sampled endpoint");
  centerCheck(tips[0].center, chain.at(-1), "tip follows sampled endpoint");
  assert.ok(traces.length === 1 || (phase === 0 && traces.length === 0), "one continuous retained trace");
  if (traces.length) {
    const trace = traces[0];
    near(trace.reveal, phase, "trace uses the same elapsed phase", revealTolerance);
    if (phase > 0) {
      assert.ok(Array.isArray(trace.path_endpoints), "actual revealed retained trace endpoints are observable");
      pointNear(trace.path_endpoints[1], tips[0].center, "trace end coincides with the pen", positionTolerance);
      result.maxima.tracePenGap = Math.hypot(...trace.path_endpoints[1].map((x, axis) => x - tips[0].center[axis]));
    }
    result.trace = { reveal: trace.reveal, endpoints: trace.path_endpoints };
    result.objectIds.trace = trace.id;
  }
  return result;
}

function compareObservations(actual, expected) {
  assert.equal(actual.terms, expected.terms); assert.equal(actual.phase, expected.phase);
  assert.equal(actual.presentObjectCount, expected.presentObjectCount);
  for (const [index, center] of actual.circles.entries()) pointNear(center, expected.circles[index], "original/compact circle");
  for (const [index, vector] of actual.vectors.entries()) {
    pointNear(vector.center, expected.vectors[index].center, "original/compact vector midpoint");
    near(vector.angle, expected.vectors[index].angle, "original/compact angle", angleTolerance);
    for (const end of [0, 1]) pointNear(vector.endpoints[end], expected.vectors[index].endpoints[end], "original/compact vector endpoint");
  }
  pointNear(actual.halo, expected.halo, "original/compact halo");
  pointNear(actual.tip, expected.tip, "original/compact tip");
  assert.equal(actual.trace === null, expected.trace === null);
  if (actual.trace) {
    near(actual.trace.reveal, expected.trace.reveal, "original/compact trace reveal", revealTolerance);
    if (actual.phase > 0) pointNear(actual.trace.endpoints[1], expected.trace.endpoints[1], "original/compact trace endpoint");
  }
}

const pairedPhases = [0, 0.065, 0.13, 0.13 + 1e-7, 0.3, 0.4, 0.6, 0.73, 0.91, 1,
  0.73, 0, 0.91, 0.13, 0.6, 0.3, 1, 0.065];
function fourierSamples() {
  const samples = [];
  for (const [stageIndex, stage] of sourceComparison.stages.entries()) {
    const keys = stage.oracle.keys;
    const selectedKey = keys[Math.floor(keys.length * 0.67)];
    for (const phase of [0, 0.137, 0.503, selectedKey - 1e-7, selectedKey,
      selectedKey + 1e-7, 0.913, 1]) samples.push({ stageIndex, phase });
  }
  // Explicit cross-stage random and reverse order exercises membership/history.
  samples.push({ stageIndex: 2, phase: 0.73 }, { stageIndex: 0, phase: 0.137 },
    { stageIndex: 1, phase: 0.503 }, { stageIndex: 2, phase: 0.37 },
    { stageIndex: 0, phase: 0 }, { stageIndex: 2, phase: 1 },
    { stageIndex: 1, phase: 0.913 }, { stageIndex: 0, phase: 1 });
  return samples;
}
async function debugFrame(page) {
  return bounded(page.evaluate(() => preparedAudio.execution.debugFrame()), "read-only retained frame observation");
}
async function rendererMetrics(page) {
  return bounded(page.evaluate(async () => (await preparedAudio.execution.metrics()).metrics), "renderer metrics");
}
async function assertIdle(page, entry) {
  await page.waitForTimeout(150);
  const before = await rendererMetrics(page);
  await page.waitForTimeout(250);
  const after = await rendererMetrics(page);
  assert.equal(after.presentedFrames, before.presentedFrames, "settled paused picture has no redundant presentation");
  assert.equal(after.rendererRebuilds, before.rendererRebuilds);
  assert.equal(after.bufferedDeltas, 0); assert.equal(after.needsPresent, false);
  entry.idle = { before, after };
}

async function workerCase(page, entry, name, transportMode) {
  const duration = name === "paired" ? 2.3 : 34.65;
  entry.setupStartedEpochMs = Date.now();
  await launchSource(page, sources[name], transportMode);
  entry.firstPictureObservedEpochMs = Date.now();
  const initial = await snapshot(page);
  assert.equal(initial.transport.preparedPlayback, true); assert.equal(initial.transport.hasAudio, true);
  assert.equal(initial.transport.playing, false); assert.equal(initial.transport.time, 0);
  assert.equal(initial.starts.length, 0, "preparation never emits sound");
  await quiet(page);
  entry.initial = initial;
  entry.initialRenderer = await rendererMetrics(page);
  entry.checks.push("normal-ordinary-audio-admission", "silent-preparation", "first-picture-at-zero");
  await play(page);
  await waitState(page, value => value.transport.time > 0.12);
  await pause(page);
  const paused = await snapshot(page);
  assertBounded(paused);
  await seek(page, 0);
  const anchorsBegin = (await snapshot(page)).anchors.length;
  entry.naturalPlayback = { startedEpochMs: Date.now(), samples: [] };
  let lastProgress = 0;
  await play(page);
  const completion = await waitState(page, value =>
    value.outcome?.status === "finished" && !value.transport.playing &&
      Math.abs(value.transport.time - duration) <= 1e-6,
  duration * 1000 + 45_000, value => {
    const now = Date.now(), previous = entry.naturalPlayback.samples.at(-1);
    if (!previous || now - previous.observedEpochMs >= 500 || value.transport.buffering !== previous.buffering) {
      entry.naturalPlayback.samples.push({ observedEpochMs: now, time: value.transport.time,
        frontier: value.transport.preparedFrontier, buffering: value.transport.buffering });
    }
    if (now - lastProgress > 10_000) {
      console.log("[PLAYBACK] " + entry.id + ": " + value.transport.time + " / " + duration);
      lastProgress = now;
    }
  });
  entry.naturalPlayback.completedEpochMs = Date.now();
  near(completion.outcome.result.duration, duration, "unchanged source duration");
  assertNaturalClock(completion.anchors.slice(anchorsBegin)); assertBounded(completion);
  await quiet(page);
  entry.completion = completion;
  entry.checks.push("complete-natural-playback", "displayed-time-audio", "terminal-silence", "pause-freezes-clock");
  entry.observations = [];
  const repeat = new Map(), identities = new Map();
  const warmRenderer = await rendererMetrics(page);
  if (name === "paired") {
    for (const phase of pairedPhases) {
      await seek(page, phase * duration);
      const frame = await debugFrame(page), observation = pairedObservation(frame, phase);
      const signature = hash(json(observation));
      if (repeat.has(phase)) assert.equal(signature, repeat.get(phase), "paired reverse seek restores exact effective values");
      repeat.set(phase, signature);
      const metrics = await rendererMetrics(page);
      assert.equal(metrics.rendererRebuilds, warmRenderer.rendererRebuilds, "paired historical motion reuses the renderer");
      assert.equal(metrics.geometryCacheMisses, 0, "paired warm seek reuses immutable geometry");
      entry.observations.push({ ...observation,
        renderer: { bytesUploaded: metrics.bytesUploaded, geometryCacheMisses: metrics.geometryCacheMisses,
          outlineCacheMisses: metrics.outlineCacheMisses, rendererRebuilds: metrics.rendererRebuilds } });
    }
    await seek(page, 0.6 * duration);
    entry.pixels = pairedPixelEvidence(await picture(page, entry.id + "-midpoint"));
    const visible = await rendererMetrics(page);
    assert.ok(visible.drawCalls > 0, "paired renderer submits real draws");
    assert.ok(visible.instancesDrawn >= 2, "paired renderer draws both object instances");
    entry.checks.push("nonuniform-keys", "signed-unwrapped-multiturn-rotation", "vector-world-endpoints",
      "warm-geometry-cache-reuse", "actual-blue-line-and-red-square-pixels");
  } else {
    const baselineKey = entry.backend + "/" + transportMode;
    const baseline = comparisons.get(baselineKey) ?? new Map();
    for (const { stageIndex, phase } of fourierSamples()) {
      const stage = sourceComparison.stages[stageIndex];
      const time = stage.oracle.draw_start_seconds + phase * stage.oracle.draw_duration_seconds;
      await seek(page, time);
      const frame = await debugFrame(page);
      near(frame.time, time, "debug frame belongs to exact selected time", 0);
      let observation;
      try { observation = fourierObservation(frame, stage, phase); }
      catch (error) {
        entry.failedFourierObservation = { time, stageIndex, terms: stage.terms, phase, frame };
        throw error;
      }
      const key = stageIndex + "/" + phase;
      const signature = hash(json(observation));
      if (repeat.has(key)) assert.equal(signature, repeat.get(key), "Fourier reverse seek restores exact historical geometry");
      repeat.set(key, signature);
      const stableIds = { ...observation.objectIds }; delete stableIds.trace;
      if (identities.has(stageIndex)) assert.deepEqual(stableIds, identities.get(stageIndex), "seek does not allocate new rig identities");
      identities.set(stageIndex, stableIds);
      if (name === "original") baseline.set(key, observation);
      else if (selected.includes("original")) {
        assert.ok(baseline.has(key), "paired original observation is required");
        compareObservations(observation, baseline.get(key));
      }
      const metrics = await rendererMetrics(page);
      assert.equal(metrics.rendererRebuilds, warmRenderer.rendererRebuilds, "historical motion reuses the renderer");
      assert.equal(metrics.geometryCacheMisses, 0, "warm motion seek does not rebuild immutable geometry");
      entry.observations.push({ time, ...observation,
        renderer: { bytesUploaded: metrics.bytesUploaded, geometryCacheMisses: metrics.geometryCacheMisses,
          outlineCacheMisses: metrics.outlineCacheMisses, rendererRebuilds: metrics.rendererRebuilds } });
    }
    if (name === "original") comparisons.set(baselineKey, baseline);
    entry.stagePictures = [];
    for (const [terms, time] of [[7, 10.2], [21, 20.8], [61, 31.4]]) {
      await seek(page, time);
      const name = entry.id + "-" + terms + "-terms";
      const pixels = await picture(page, name);
      entry.stagePictures.push({ terms, time, name: name + ".png", pixelSha256: hash(pixels) });
    }
    assert.equal(new Set(entry.stagePictures.map(value => value.pixelSha256)).size, 3);
    entry.checks.push("all-cumulative-centers", "signed-unwrapped-multiturn-rotation", "vector-world-endpoints",
      "trace-pen-phase-agreement", "nonuniform-key-neighborhoods", "cross-stage-membership-restoration",
      "stable-rig-identity", "warm-geometry-cache-reuse");
    if (name === "sampled" && selected.includes("original")) entry.checks.push("original-compact-runtime-fidelity");
  }
  await quiet(page);
  entry.checks.push("forward-random-reverse-seek", "exact-repeat-observation");
  await assertIdle(page, entry);
  await bounded(page.evaluate(() => preparedAudio.execution.restartPlayback()), "retained-reader restart");
  const restarted = await snapshot(page);
  assert.equal(restarted.transport.time, 0); assert.equal(restarted.transport.playing, false);
  await quiet(page);
  entry.checks.push("restart-retains-history-and-silence");
  entry.renderer = await rendererMetrics(page);
}

async function directCase(page, entry) {
  await page.evaluate(async () => {
    const wasm = await import("./pkg/noon_web.js"); await wasm.default();
    const NativeWorker = globalThis.Worker;
    globalThis.Worker = class { constructor() { throw new Error("direct typed Rust fixture attempted worker transport"); } };
    const canvas = document.querySelector("#scene"); canvas.width = 960; canvas.height = 540;
    canvas.style.width = "960px"; canvas.style.height = "540px";
    const renderer = await wasm.createDirectSampledMotionSmokeRenderer(canvas.transferControlToOffscreen());
    globalThis.directMotion = { renderer, NativeWorker };
    renderer.render();
  });
  entry.observations = [];
  const repeat = new Map();
  for (const phase of pairedPhases) {
    const result = await bounded(page.evaluate(phase => {
      const r = directMotion.renderer;
      r.seekDirect(phase * 2.3); r.render();
      return { frame: JSON.parse(r.debugSelectionFrameJson()), backend: r.rendererBackend(), time: r.time(),
        objectCount: r.objectCount(), geometryCacheMisses: r.lastGeometryCacheMisses(), bytesUploaded: r.lastBytesUploaded(),
        drawCalls: r.lastDrawCalls(), instancesDrawn: r.lastInstancesDrawn() };
    }, phase), "direct typed Rust seek and observation");
    near(result.time, phase * 2.3, "direct exact authored time", 0);
    assert.equal(result.objectCount, 2);
    assert.equal(result.backend, entry.backend === "webgl" ? "WebGL2" : "WebGPU");
    assert.ok(result.drawCalls > 0, "direct renderer submits real draws");
    assert.ok(result.instancesDrawn >= 2, "direct renderer draws both object instances");
    const observation = pairedObservation(result.frame, phase);
    const signature = hash(json(observation));
    if (repeat.has(phase)) assert.equal(signature, repeat.get(phase), "direct reverse seek agrees exactly");
    repeat.set(phase, signature);
    entry.observations.push(observation);
    entry.renderer = { backend: result.backend, objectCount: result.objectCount,
      geometryCacheMisses: result.geometryCacheMisses, bytesUploaded: result.bytesUploaded,
      drawCalls: result.drawCalls, instancesDrawn: result.instancesDrawn };
  }
  entry.pixels = pairedPixelEvidence(await picture(page, entry.id + "-final"));
  entry.checks.push("shared-rust-fixture", "direct-typed-engine-path", "no-worker-transport",
    "nonuniform-keys", "signed-unwrapped-angles", "world-endpoints", "forward-random-reverse-seek",
    "actual-blue-line-and-red-square-pixels");
  await page.evaluate(() => directMotion.renderer.free());
}

try {
  report.wasmSha256 = hash(await readFile(path.join(root, "web/pkg/noon_web_bg.wasm")));
  report.bindingsSha256 = hash(await readFile(path.join(root, "web/pkg/noon_web.js")));
  report.workerSha256 = hash(await readFile(path.join(root, "web/python-worker.js")));
  const executablePath = process.env.NOON_SAMPLED_MOTION_CHROMIUM ?? process.env.NOON_AUDIO_CHROMIUM;
  if (executablePath) report.browserExecutableSha256 = hash(await readFile(executablePath));
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of backends) {
    report.browserArguments[backend] = [...browserArgs(backend), "--autoplay-policy=user-gesture-required"];
    browser = await playwright.chromium.launch({ headless: true,
      ...(executablePath ? { executablePath } : { channel: "chromium" }),
      ignoreDefaultArgs: ["--mute-audio"], args: report.browserArguments[backend],
      ...(process.env.NOON_BROWSER_PROXY ? { proxy: { server: process.env.NOON_BROWSER_PROXY, bypass: "127.0.0.1,localhost" } } : {}) });
    report.browserVersion = browser.version();
    const cases = [{ case: "direct", transportMode: "none" },
      ...transports.flatMap(transportMode => selected.map(name => ({ case: name, transportMode })))];
    for (const descriptor of cases) {
      const entry = { id: backend + "-" + descriptor.transportMode + "-" + descriptor.case,
        backend, ...descriptor, status: "running", checks: [], pageErrors: [], consoleErrors: [], failedResponses: [] };
      report.cases.push(entry);
      const context = await browser.newContext({ viewport: { width: 1000, height: 700 }, deviceScaleFactor: 1 });
      await installResources(context);
      await context.addInitScript(observeAudio);
      const page = await context.newPage(); page.setDefaultTimeout(180_000);
      page.on("pageerror", error => entry.pageErrors.push(String(error)));
      page.on("console", message => { if (message.type() === "error") entry.consoleErrors.push(message.text()); });
      page.on("response", response => {
        if (response.status() >= 400) entry.failedResponses.push({ url: response.url(), status: response.status() });
      });
      try {
        await page.goto(server.baseUrl + "/web/execution-worker-smoke.html");
        if (entry.case === "direct") await directCase(page, entry);
        else {
          await installObservers(page);
          await workerCase(page, entry, entry.case, entry.transportMode);
          assert.equal(entry.renderer.backend, backend === "webgl" ? "WebGL2" : "WebGPU");
          assert.equal(entry.renderer.transportMode, entry.transportMode);
        }
        assert.deepEqual(entry.pageErrors, []); assert.deepEqual(entry.consoleErrors, []);
        assert.deepEqual(entry.failedResponses, []);
        entry.status = "passed"; console.log("[PASS] " + entry.id);
      } catch (error) {
        entry.status = "failed"; entry.error = String(error.stack ?? error);
        if (entry.case !== "direct") entry.failure = await bounded(snapshot(page), "failure observation", 5_000).catch(() => null);
        await page.screenshot({ path: path.join(output, entry.id + "-failure.png"), timeout: 10_000 }).catch(() => {});
        throw error;
      } finally {
        try { await bounded(context.close(), "browser context teardown"); }
        catch (error) {
          entry.teardownError = String(error.stack ?? error);
          if (!entry.error) { entry.status = "failed"; entry.error = entry.teardownError; throw error; }
        }
      }
      await writeFile(path.join(output, "report.json"), json(report));
    }
    await bounded(browser.close(), "backend browser teardown"); browser = null;
  }
  report.status = "passed";
} catch (error) {
  report.status = "failed"; report.error = String(error.stack ?? error); process.exitCode = 1;
} finally {
  report.resources = cache.stats();
  const cleanup = await Promise.allSettled([
    bounded(browser?.close() ?? Promise.resolve(), "browser teardown"),
    bounded(server?.close() ?? Promise.resolve(), "server teardown"),
  ]);
  report.teardownErrors = cleanup.filter(value => value.status === "rejected").map(value => String(value.reason));
  if (report.teardownErrors.length && report.status !== "failed") {
    report.status = "failed"; report.error = report.teardownErrors.join("\n"); process.exitCode = 1;
  }
  await writeFile(path.join(output, "report.json"), json(report));
}
console.log(json({ status: report.status, cases: report.cases.map(value => ({ id: value.id, status: value.status })), error: report.error }));
