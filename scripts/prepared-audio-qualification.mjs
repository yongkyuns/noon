// Real ordinary Python source -> canonical authoring worker -> prepared reader ->
// render worker and native Web Audio. Observers never supply a scene or clock.
// The original #2358 source bytes are immutable inputs. The combined Fourier
// input differs only by its requested soundtrack declaration.
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
import { holdPlaygroundPreload, waitForBrowserObservation } from "./playground-browser-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.resolve(process.env.NOON_PREPARED_AUDIO_ARTIFACTS ??
  path.join(root, "browser-smoke-artifacts/prepared-audio"));
const backends = (process.env.NOON_PREPARED_AUDIO_BACKENDS ?? "webgl,webgpu").split(",");
const transports = (process.env.NOON_PREPARED_AUDIO_TRANSPORTS ?? "transferable,shared").split(",");
const selected = (process.env.NOON_PREPARED_AUDIO_CASES ?? "minimal,fourier,buffering,lifecycle,gallery").split(",");
const admission = process.env.NOON_PREPARED_AUDIO_ADMISSION ?? "normal";
assert.ok(["normal", "explicit-prepared"].includes(admission));
assert.ok(backends.length && backends.every(x => ["webgl", "webgpu"].includes(x)));
assert.ok(transports.length && transports.every(x => ["transferable", "shared"].includes(x)));
assert.ok(selected.length && selected.every(x => ["minimal", "fourier", "buffering", "lifecycle", "gallery"].includes(x)));
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const json = value => JSON.stringify(value, (_, x) => typeof x === "bigint" ? String(x) : x, 2) + "\n";
const near = (actual, expected, name, tolerance = 1e-6) =>
  assert.ok(Math.abs(actual - expected) <= tolerance, name + ": " + actual + " != " + expected);
const fixtureDirectory = path.join(root, "tests/fixtures/fourier_audio");
const originalHashes = {
  "audio_ordinary_play.py": "a898089e0534e31a5a149daaff4fec95ffc7ba5d108125d8ba93b4e252e5a60b",
  "fourier_epicycles.py": "7a09cda6039efdc9901a3241b0c514f634996187a0ea9b1f316e34dbbd1feed9",
  "music_preview.py": "573d4e94e81707925632f158bdb5d99f0c044eaf86ab0d25a4420fc4735013d4",
};
const originals = {};
for (const [name, expected] of Object.entries(originalHashes)) {
  originals[name] = await readFile(path.join(fixtureDirectory, name), "utf8");
  assert.equal(hash(originals[name]), expected, "preserve original source " + name);
}
const insertion = "        self.audio(AudioSource.small_discovery(), duration=34.65, gain=0.5, fade_in=1.5, fade_out=2.5)\n";
const firstPlay = "        self.play(FadeIn(title)";
assert.equal(originals["fourier_epicycles.py"].split(firstPlay).length, 2);
const fourier = originals["fourier_epicycles.py"].replace(firstPlay, insertion + firstPlay);
assert.equal(fourier.replace(insertion, ""), originals["fourier_epicycles.py"]);
// The independent buffering case has an actual asynchronous Python preparation
// delay. It does not alter either acceptance source or synthesize reader demand.
const delayed = [
  "from noon import *",
  "import asyncio",
  "from pyodide.ffi import run_sync",
  "class DelayedAudioSource(Scene):",
  "    def construct(self):",
  "        self.audio(AudioSource.small_discovery(), duration=4, gain=0.5)",
  "        self.add(Circle(radius=0.5))",
  "        self.wait(1)",
  "        run_sync(asyncio.sleep(2))",
  "        self.wait(3)",
  "",
].join("\n");
// This companion blocks the real authoring worker's event loop. The old prefix
// can expire while Python is computing, so its wall clock must be settled before
// newly prepared history becomes available. An asynchronous sleep cannot expose
// that ordering error.
const cpuDelayed = [
  "from noon import *",
  "from time import perf_counter",
  "class ComputingAudioSource(Scene):",
  "    def construct(self):",
  "        self.audio(AudioSource.small_discovery(), duration=4, gain=0.5)",
  "        self.play(Create(Circle(radius=0.5)), run_time=1)",
  "        deadline = perf_counter() + 2",
  "        while perf_counter() < deadline:",
  "            pass",
  "        self.wait(3)",
  "",
].join("\n");
const activeCpuDelayed = [
  "from noon import *",
  "from time import perf_counter",
  "class ComputingDuringAnimation(Scene):",
  "    def construct(self):",
  "        self.audio(AudioSource.small_discovery(), duration=4, gain=0.5)",
  "        self.play(Create(Circle(radius=0.5)), run_time=2)",
  "        deadline = perf_counter() + 0.3",
  "        while perf_counter() < deadline:",
  "            pass",
  "        self.wait(2)",
  "",
].join("\n");
const workerSource = await readFile(path.join(root, "web/python-worker.source.js"), "utf8");
const pinnedPyodide = workerSource.match(/from "(https:\/\/cdn\.jsdelivr\.net\/pyodide\/v[^/]+\/full\/pyodide\.mjs)"/)?.[1];
assert.ok(pinnedPyodide, "production worker must pin Pyodide");
const cache = createPyodideResourceCache(workerSource);
const report = {
  status: "running",
  scope: "ordinary Python source, production prepared worker, displayed reader and real Web Audio; no physical DAC or performance claim",
  revision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  sourceDirty: execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim() !== "",
  harnessSha256: hash(await readFile(fileURLToPath(import.meta.url))), nodeVersion: process.version,
  originalHashes, combinedFourierSha256: hash(fourier), combinedDurationSeconds: 34.65,
  pinnedPyodide, localPyodideResources: {}, browserArguments: {}, cases: [],
  admission,
};
await mkdir(output, { recursive: true });
await writeFile(path.join(output, "combined_fourier_audio.py"), fourier);
let browser, server;

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

async function launchSource(page, source, transportMode, replace = false, preparationLookaheadSeconds = 1) {
  await bounded(page.evaluate(async ({ source, transportMode, replace, admission, preparationLookaheadSeconds }) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const { PlaygroundPlaybackControls } = await import("./playground-playback-controls.js");
    let host = globalThis.preparedAudio;
    if (!replace) {
      host = { authoring: new PythonAuthoringClient(new Worker("./python-worker.js", { type: "module" })),
        errors: [], outcomes: [], generation: 0 };
      globalThis.preparedAudio = host;
    } else {
      // Like normal gallery replacement, acknowledge cooperative source
      // cancellation before closing its transport, preserving the warm VM.
      if (host.registration && host.outcome === null) {
        await host.authoring.cancelSemanticContinuation(
          host.registration.semanticExecution.contextId, host.registration.generation, "acceptance fresh Run");
      }
      await Promise.race([host.run.catch(() => {}), new Promise((_, reject) =>
        setTimeout(() => reject(new Error("source did not settle after cancellation")), 10_000))]);
      host.execution.terminate();
      host.controls?.destroy();
      document.querySelector("#prepared-controls")?.remove();
      const old = document.querySelector("#scene");
      const canvas = document.createElement("canvas"); canvas.id = "scene"; old.replaceWith(canvas);
    }
    const generation = ++host.generation;
    host.registration = null; host.outcome = null; host.attached = false;
    const canvas = document.querySelector("#scene");
    canvas.width = 960; canvas.height = 540; canvas.style.width = "960px"; canvas.style.height = "540px";
    host.execution = new AuthoringExecutionClient(canvas, {
      onError: error => host.errors.push(String(error)),
      onRecoverableError: error => host.errors.push(String(error)),
    });
    host.run = host.authoring.run(source, {}, {
      ...(admission === "explicit-prepared" ? { preparedPlayback: true } : {}),
      ...(preparationLookaheadSeconds !== 1 ? { preparationLookaheadSeconds } : {}),
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
      result => { if (host.generation === generation) host.outcome = { status: "finished", result }; },
      error => { if (host.generation === generation) host.outcome = { status: "failed", error: String(error) }; },
    );
  }, { source, transportMode, replace, admission, preparationLookaheadSeconds }), "source setup / ordered replacement");
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

async function ordinaryCase(page, entry, name, source, duration, transportMode) {
  await launchSource(page, source, transportMode);
  const initial = await snapshot(page);
  assert.equal(initial.transport.preparedPlayback, true);
  assert.equal(initial.transport.hasAudio, true);
  assert.equal(initial.transport.playing, false);
  assert.equal(initial.transport.time, 0);
  assert.equal(initial.starts.length, 0, "headless preparation starts no audio sources");
  await quiet(page);
  entry.initial = initial;
  entry.checks.push("unchanged-ordinary-source", "silent-preparation", "paused-reader-at-zero");

  const comparisonTime = name === "fourier" ? 0.35 : 0.5;
  await seek(page, comparisonTime);
  const baseline = await picture(page, entry.id + "-historical");
  const beforePlay = (await snapshot(page)).starts.length;
  assert.equal(beforePlay, 0, "paused seek never starts sound");
  await play(page);
  await waitState(page, x => x.transport.time > comparisonTime + 0.1);
  const paused = await pause(page);
  assert.ok(paused.starts.length > beforePlay);
  entry.checks.push("real-audible-output", "pause-stops-output-and-clock");
  await seek(page, comparisonTime);
  assert.ok(baseline.equals(await picture(page, entry.id + "-rewind")), "rewind restores exact historical pixels");

  // Replay from zero naturally through the whole authored interval. Seeking to
  // its end is not accepted as evidence that all 34.65 seconds played.
  await seek(page, 0);
  const naturalAnchorStart = (await snapshot(page)).anchors.length;
  entry.naturalPlayback = { startedEpochMs: Date.now(), samples: [] };
  let lastTraceTime = 0;
  const trace = value => {
    const now = Date.now(), samples = entry.naturalPlayback.samples, previous = samples.at(-1);
    if (!previous || now - previous.observedEpochMs >= 500 ||
        value.transport.preparedFrontier !== previous.frontier || value.transport.buffering !== previous.buffering ||
        value.transport.playing !== previous.playing) {
      samples.push({ observedEpochMs: now, time: value.transport.time,
        frontier: value.transport.preparedFrontier, buffering: value.transport.buffering,
        playing: value.transport.playing, output: value.output });
    }
    if (now - lastTraceTime >= 10_000) {
      console.log("[PLAYBACK] " + entry.id + ": time=" + value.transport.time +
        " frontier=" + value.transport.preparedFrontier + " buffering=" + value.transport.buffering);
      lastTraceTime = now;
    }
  };
  await play(page);
  const completed = await waitState(page,
    x => x.outcome?.status === "finished" && Math.abs(x.transport.durationSeconds - duration) < 1e-9 &&
      !x.transport.playing && Math.abs(x.transport.time - duration) < 1e-6,
    Math.ceil(duration * 1000) + 45_000, trace);
  entry.naturalPlayback.completedEpochMs = Date.now();
  assertNaturalClock(completed.anchors.slice(naturalAnchorStart));
  near(completed.outcome.result.duration, duration, "ordinary-source duration");
  await quiet(page);
  entry.completion = completed;
  entry.checks.push("full-interval-natural-playback", "natural-clock-never-skips-new-prefix",
    "source-terminal-adoption", "terminal-silence");
  assertBounded(completed);
  if (name === "fourier") {
    entry.completedStages = [];
    for (const [terms, time] of [[7, 10.2], [21, 20.8], [61, 31.4]]) {
      await seek(page, time);
      const pixels = await picture(page, entry.id + "-" + terms + "-terms");
      entry.completedStages.push({ terms, time, pixelSha256: hash(pixels) });
    }
    assert.equal(new Set(entry.completedStages.map(x => x.pixelSha256)).size, 3,
      "completed history contains all three distinct Fourier drawing stages");
    entry.checks.push("complete-seven-twentyone-sixtyone-term-pictures");
  }
  await seek(page, comparisonTime);
  assert.ok(baseline.equals(await picture(page, entry.id + "-completed-rewind")));
  await seek(page, duration - 0.4);
  await play(page);
  const nearEnd = await snapshot(page);
  assertBounded(nearEnd);
  await waitState(page, x => !x.transport.playing && Math.abs(x.transport.time - duration) < 1e-6);
  await quiet(page);
  await bounded(page.evaluate(() => preparedAudio.execution.restartPlayback()), "restart control");
  const restarted = await snapshot(page);
  near(restarted.transport.time, 0, "prepared restart selects beginning", 0);
  assert.equal(restarted.transport.playing, false);
  await quiet(page);
  entry.checks.push("completed-history-rewind", "seek-keeps-clip-phase", "restart-retains-reader");
  entry.final = restarted;
}

async function bufferingCase(page, entry, transportMode) {
  await launchSource(page, delayed, transportMode, false, 0.75);
  const initial = await snapshot(page);
  assert.equal(initial.transport.preparedFrontier, 1);
  assert.equal(initial.starts.length, 0);
  await play(page);
  const buffered = await waitState(page, x => x.transport.buffering === true);
  assert.equal(buffered.transport.preparedFrontier, 1);
  assert.equal(buffered.transport.durationSeconds, null);
  const held = buffered.transport.time;
  const bufferedAnchors = buffered.boundedStarts.length;
  entry.buffered = buffered;
  await quiet(page);
  const still = await snapshot(page);
  assert.equal(still.transport.buffering, true, "delayed producer leaves a measurable buffer wait");
  near(still.transport.time, held, "buffered wall time cannot advance reader", 0);
  assertBounded(still);
  await waitState(page, x => x.transport.preparedFrontier > 1 && !x.transport.buffering && x.transport.time > held);
  await audible(page);
  const extended = await snapshot(page);
  assertBounded(extended);
  const resumed = extended.boundedStarts.slice(bufferedAnchors).filter(x => x.sources.length > 0);
  assert.ok(resumed.length > 0, "a real bounded output resumes after delivery");
  assert.ok(resumed.every(x => x.time >= buffered.transport.preparedFrontier),
    "refill must not replay audio from the older retained static picture");
  entry.extended = extended;
  await waitState(page, x => x.transport.durationSeconds === 4 && x.transport.time === 4 && !x.transport.playing);
  await quiet(page);
  entry.checks.push("actual-asynchronous-source-delay", "frontier-bounded-audio", "buffer-wait-silence",
    "buffering-freezes-authored-time", "append-reanchors-and-resumes", "no-buffered-wall-time-charge");

  await launchSource(page, cpuDelayed, transportMode, true, 0.75);
  const cpuInitial = await snapshot(page);
  assert.equal(cpuInitial.transport.preparedFrontier, 1);
  assert.equal(cpuInitial.transport.time, 0);
  const firstAnchor = cpuInitial.anchors.length;
  const firstBounded = cpuInitial.boundedStarts.length;
  await play(page);
  const cpuExtended = await waitState(page, x => x.transport.preparedFrontier > 1 &&
    x.boundedStarts.slice(firstBounded).some(anchor => anchor.availableUntil > 1 && anchor.sources.length > 0));
  const cpuResumed = cpuExtended.boundedStarts.slice(firstBounded)
    .filter(x => x.availableUntil > 1 && x.sources.length > 0);
  assert.ok(cpuResumed.length > 0, "CPU-blocked source resumes real output after delivery");
  near(cpuResumed[0].time, 1, "CPU-blocked refill resumes from attained old frontier", 0.05);
  assert.ok(cpuExtended.transport.time < 1.3, "new interval cannot absorb blocked worker wall time");
  entry.cpuBlocked = { initial: cpuInitial, extended: cpuExtended };
  const cpuCompleted = await waitState(page, x => x.outcome?.status === "finished" &&
    x.transport.durationSeconds === 4 && x.transport.time === 4 && !x.transport.playing);
  assertNaturalClock(cpuCompleted.anchors.slice(firstAnchor));
  assertBounded(cpuCompleted);
  await quiet(page);
  entry.cpuBlocked.completion = cpuCompleted;
  entry.checks.push("real-synchronous-python-preparation", "expired-old-frontier-settled-before-append",
    "new-prefix-plays-without-skipping");

  // A shorter computation ends before the previous horizon. Its new anchor must
  // follow the current picture, not replay the last frame from before the stall.
  await launchSource(page, activeCpuDelayed, transportMode, true, 0.75);
  const activeInitial = await snapshot(page);
  assert.equal(activeInitial.transport.preparedFrontier, 2);
  const activeFirstAnchor = activeInitial.anchors.length;
  await play(page);
  const activeExtended = await waitState(page, x => x.anchors.slice(activeFirstAnchor)
    .some(receipt => receipt.result !== false && receipt.anchor.playing && receipt.anchor.availableUntil > 2));
  const activeAnchors = activeExtended.anchors.slice(activeFirstAnchor)
    .filter(receipt => receipt.result !== false && receipt.anchor.playing).map(receipt => receipt.anchor);
  const initialPlaying = activeAnchors[0];
  const refillPlaying = activeAnchors.find(anchor => anchor.availableUntil > 2);
  assert.equal(initialPlaying.availableUntil, 2);
  assert.ok(refillPlaying.time < 2, "short computation must finish before the old frontier");
  assert.equal(activeExtended.transport.buffering, false);
  const elapsed = (refillPlaying.sampledEpochMs - initialPlaying.sampledEpochMs) / 1000;
  near(refillPlaying.time - initialPlaying.time, elapsed,
    "unexpired reanchor follows elapsed authored time and current picture", 0.08);
  assertBounded(activeExtended);
  entry.cpuWithinFrontier = { initial: activeInitial, extended: activeExtended };
  const activeCompleted = await waitState(page, x => x.outcome?.status === "finished" &&
    x.transport.durationSeconds === 4 && x.transport.time === 4 && !x.transport.playing);
  assertNaturalClock(activeCompleted.anchors.slice(activeFirstAnchor));
  await quiet(page);
  entry.cpuWithinFrontier.completion = activeCompleted;
  entry.checks.push("unexpired-worker-stall-selects-current-picture", "active-frontier-reanchor-keeps-continuous-phase");
}

async function lifecycleCase(page, entry, transportMode) {
  await launchSource(page, originals["audio_ordinary_play.py"], transportMode);
  // Keep the live renderer's identity before explicit cancellation retires it.
  // A stopped endpoint is no longer a metrics-query service.
  entry.renderer = await bounded(page.evaluate(async () =>
    (await preparedAudio.execution.metrics()).metrics), "lifecycle renderer identity");
  await page.evaluate(() => { audioQualification.holdNextResume = true; });
  await page.getByRole("button", { name: "Play animation", exact: true }).click();
  await page.waitForFunction(() => audioQualification.pendingResumes.length === 1);
  assert.equal((await snapshot(page)).transport.playing, false);
  await bounded(page.evaluate(() => preparedAudio.execution.pause()), "superseding pause control");
  await page.evaluate(() => audioQualification.pendingResumes.shift()());
  await page.waitForTimeout(120);
  const superseded = await snapshot(page);
  assert.equal(superseded.transport.playing, false);
  assert.equal(superseded.starts.length, 0);
  await quiet(page);
  entry.checks.push("superseded-delayed-unlock-stays-silent");
  console.log("[CHECK] " + entry.id + ": superseded delayed unlock");
  // Any UI notice for an explicitly superseded command is recorded separately;
  // errors are never erased or ignored by the global qualification assertion.
  entry.supersededErrors = superseded.errors;
  assert.ok(superseded.errors.every(x => /supersed|cancel/i.test(x)));
  await page.evaluate(() => { preparedAudio.errors = []; });
  await play(page);
  const running = await snapshot(page);
  assertBounded(running);
  await pause(page);
  const stale = await page.evaluate(() => {
    const accepted = audioQualification.anchors.filter(x => x.anchor?.playing === true && x.result !== false).at(-1);
    const device = audioQualification.devices.at(-1);
    const before = audioQualification.starts.length;
    const result = device.applyAnchor(accepted.anchor, accepted.anchor.session);
    return { result, before, after: audioQualification.starts.length };
  });
  assert.equal(stale.result, false, "superseded sequence cannot restart audio");
  assert.equal(stale.before, stale.after);
  await quiet(page);
  entry.staleAnchor = stale;
  entry.checks.push("stale-anchor-does-not-revive-paused-output");
  console.log("[CHECK] " + entry.id + ": rejected stale anchor");

  const priorCount = (await snapshot(page)).starts.length;
  await launchSource(page, originals["audio_ordinary_play.py"], transportMode, true);
  console.log("[CHECK] " + entry.id + ": fresh Run attached");
  const fresh = await snapshot(page);
  assert.equal(fresh.transport.time, 0);
  assert.equal(fresh.transport.playing, false);
  assert.equal(fresh.starts.length, priorCount, "fresh Run must not revive predecessor sound");
  await quiet(page);
  await play(page);
  assert.ok((await snapshot(page)).starts.length > priorCount);
  await bounded(page.evaluate(async () => {
    const host = preparedAudio;
    await host.authoring.cancelSemanticContinuation(
      host.registration.semanticExecution.contextId, host.registration.generation, "acceptance cancellation");
  }), "explicit source cancellation");
  await page.waitForFunction(() => preparedAudio.outcome !== null);
  assert.equal(await page.evaluate(() => preparedAudio.outcome.status), "failed");
  const countAfterCancel = await page.evaluate(() => audioQualification.starts.length);
  await quiet(page);
  await page.waitForTimeout(150);
  assert.equal(await page.evaluate(() => audioQualification.starts.length), countAfterCancel);
  await page.evaluate(() => preparedAudio.execution.terminate());
  entry.checks.push("fresh-run-new-reader-silent", "explicit-new-play-required",
    "cancellation-retires-output", "no-late-revival");
}

async function galleryCase(page, entry, baseUrl) {
  assert.equal(admission, "normal", "gallery qualification must use the default Run path");
  const examples = ["audio-qualification-silent", "audio-qualification-ordinary"].map((id, order) => ({
    id, order, title: id, status: "ready", parity_status: "candidate",
    reuse: "manim-compatible-parity-v0.21", category: "qualification",
    path: "python/examples/" + id + ".py", thumbnail: "audio-qualification.svg",
    features: ["ordinary-audio"], summary: "Original ordinary audio acceptance",
  }));
  const silent = "from noon import *\nclass SilentSource(Scene):\n    def construct(self):\n        self.play(Create(Circle(radius=0.5)), run_time=0.2)\n        self.wait(0.1)\n";
  await page.route("**/python/examples/manim_*_manifest.json", route => route.fulfill({
    contentType: "application/json",
    body: JSON.stringify({ reference: { version: "0.21" },
      entries: route.request().url().endsWith("manim_tutorial_manifest.json") ? examples : [] }),
  }));
  for (const example of examples) await page.route("**/" + example.path, route => route.fulfill({
    contentType: "text/x-python",
    body: example.order === 0 ? silent : originals["audio_ordinary_play.py"],
  }));
  await page.route("**/audio-qualification.svg", route => route.fulfill({
    contentType: "image/svg+xml",
    body: '<svg xmlns="http://www.w3.org/2000/svg" width="160" height="90"><circle cx="80" cy="45" r="20" fill="blue"/></svg>',
  }));
  await page.addInitScript(() => {
    const NativeWorker = Worker;
    globalThis.galleryWorkers = [];
    globalThis.Worker = class extends NativeWorker {
      constructor(url, options) { super(url, options); galleryWorkers.push(options?.name ?? ""); }
    };
  });
  const release = await holdPlaygroundPreload(page);
  await page.goto(baseUrl + "/web/index.html?example=audio-qualification-silent", { waitUntil: "domcontentloaded" });
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  await installObservers(page);
  await page.evaluate(async () => {
    if (globalThis.__NOON_PLAYGROUND_TEST_HOOKS__ !== undefined) throw new Error("gallery run cannot use prepared test options");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const host = { execution: null, attached: false, attachmentGeneration: 0,
      outcome: null, errors: [], requests: [], descriptors: [], generation: 0 };
    globalThis.preparedAudio = host;
    const run = PythonAuthoringClient.prototype.run;
    PythonAuthoringClient.prototype.run = function (source, metadata, options) {
      const generation = ++host.generation;
      host.outcome = null; host.attached = false;
      host.requests.push({ source, preparedPlayback: options?.preparedPlayback ?? false });
      const promise = run.call(this, source, metadata, options);
      promise.then(
        result => { if (generation === host.generation) host.outcome = { status: "finished", result }; },
        error => { if (generation === host.generation) host.outcome = { status: "failed", error: String(error) }; },
      );
      return promise;
    };
    const start = AuthoringExecutionClient.prototype.startSemanticExecution;
    AuthoringExecutionClient.prototype.startSemanticExecution = async function (descriptor, options) {
      const generation = ++host.attachmentGeneration;
      host.execution = this;
      host.attached = false;
      host.descriptors.push(structuredClone(descriptor));
      const result = await start.call(this, descriptor, options);
      if (host.execution === this && host.attachmentGeneration === generation) host.attached = true;
      return result;
    };
  });
  release();
  await page.waitForFunction(() => {
    const status = document.querySelector("#patch-status");
    if (status?.dataset.state === "error") throw new Error(status.value);
    return status?.dataset.state === "applied" && !__noonExampleGallery.runInFlight;
  });
  const silentState = await snapshot(page);
  assert.notEqual(silentState.transport.preparedPlayback, true, "no-audio source keeps normal realtime route");
  assert.equal(silentState.starts.length, 0);
  await page.evaluate(() => { globalThis.previousGalleryCanvas = document.querySelector("#scene"); });
  await page.locator("#example-browser-trigger").click();
  const layer = page.locator(".example-browser-layer");
  await layer.waitFor({ state: "visible" });
  await layer.locator('.example-card[data-example-id="audio-qualification-ordinary"]').click();
  await layer.waitFor({ state: "hidden" });
  await bounded(waitForBrowserObservation(page, async source => {
    const status = document.querySelector("#patch-status");
    if (status?.dataset.state === "error") throw new Error(status.value);
    if (__noonExampleGallery.selectedExampleId !== "audio-qualification-ordinary" ||
        !preparedAudio.attached || preparedAudio.requests.at(-1)?.source !== source) return false;
    const state = await preparedAudio.execution.state();
    return state.preparedPlayback === true && state.hasAudio === true && !state.playing &&
      document.querySelector(".playback-controls")?.dataset.busy === "false";
  }, originals["audio_ordinary_play.py"]), "normal gallery audio attachment", 90_000);
  assert.equal(await page.evaluate(() => previousGalleryCanvas === document.querySelector("#scene")), false);
  const initial = await snapshot(page);
  assert.equal(initial.transport.time, 0);
  assert.equal(initial.starts.length, 0);
  assert.equal(await page.evaluate(() => preparedAudio.requests.every(x => x.preparedPlayback === false)), true);
  await quiet(page);
  entry.initial = initial;
  entry.checks.push("normal-gallery-run", "no-prepared-test-option", "realtime-to-audio-selection",
    "fresh-canvas-for-reader", "default-audio-preparation-silent");
  await play(page);
  await waitState(page, x => x.transport.time > 0.1);
  await page.getByRole("button", { name: "Pause animation", exact: true }).click();
  await waitState(page, x => !x.transport.playing);
  await quiet(page);
  assertBounded(await snapshot(page));
  entry.pictures = [entry.id + "-normal-ui-paused.png", entry.id + "-normal-ui-completed.png"];
  await page.screenshot({ path: path.join(output, entry.pictures[0]), fullPage: true, timeout: 10_000 });
  entry.checks.push("normal-ui-play-and-pause");
  const starts = (await snapshot(page)).starts.length;
  const edited = originals["audio_ordinary_play.py"] + "\n# Fresh Run keeps ordinary authoring syntax.\n";
  await page.evaluate(source => {
    globalThis.previousGalleryCanvas = document.querySelector("#scene");
    const editor = document.querySelector("#python-scene-source");
    editor.value = source;
    editor.dispatchEvent(new Event("input", { bubbles: true }));
  }, edited);
  await page.locator("#replace-scene").click();
  await bounded(waitForBrowserObservation(page, async source => {
    const status = document.querySelector("#patch-status");
    if (status?.dataset.state === "error") throw new Error(status.value);
    if (previousGalleryCanvas === document.querySelector("#scene") ||
        !preparedAudio.attached || preparedAudio.requests.at(-1)?.source !== source) return false;
    const state = await preparedAudio.execution.state();
    return state.preparedPlayback === true && state.hasAudio === true && state.time === 0 && !state.playing &&
      document.querySelector(".playback-controls")?.dataset.busy === "false";
  }, edited), "normal gallery edited Run", 90_000);
  const rerun = await snapshot(page);
  assert.equal(rerun.starts.length, starts);
  await quiet(page);
  await play(page);
  assert.ok((await snapshot(page)).starts.length > starts);
  await waitState(page, x => x.outcome?.status === "finished" && x.transport.time === 3 && !x.transport.playing);
  await quiet(page);
  entry.requests = await page.evaluate(() => preparedAudio.requests.map(x => ({
    sourceSha256Input: x.source, preparedPlayback: x.preparedPlayback,
  })));
  entry.requests = entry.requests.map(x => ({
    sourceSha256: hash(x.sourceSha256Input), preparedPlayback: x.preparedPlayback,
  }));
  assert.ok(entry.requests.every(x => x.preparedPlayback === false));
  assert.equal(entry.requests[0].sourceSha256, hash(silent));
  assert.ok(entry.requests.some(x => x.sourceSha256 === originalHashes["audio_ordinary_play.py"]),
    "normal gallery submits the exact original minimal source");
  assert.equal(entry.requests.at(-1).sourceSha256, hash(edited));
  assert.equal(await page.evaluate(() => galleryWorkers.filter(x => x === "noon-python-authoring").length), 1);
  entry.checks.push("actual-run-button-after-edit", "rerun-prepares-silently", "warm-interpreter-new-reader",
    "explicit-play-after-rerun", "normal-gallery-completes-three-seconds");
  entry.final = await snapshot(page);
  await page.screenshot({ path: path.join(output, entry.pictures[1]), fullPage: true, timeout: 10_000 });
}

try {
  report.wasmSha256 = hash(await readFile(path.join(root, "web/pkg/noon_web_bg.wasm")));
  report.bindingsSha256 = hash(await readFile(path.join(root, "web/pkg/noon_web.js")));
  report.workerSha256 = hash(await readFile(path.join(root, "web/python-worker.js")));
  const executablePath = process.env.NOON_PREPARED_AUDIO_CHROMIUM ?? process.env.NOON_AUDIO_CHROMIUM;
  if (executablePath) report.browserExecutableSha256 = hash(await readFile(executablePath));
  server = await serveRepository(root, 0, { crossOriginIsolated: true });
  for (const backend of backends) {
    report.browserArguments[backend] = [...browserArgs(backend), "--autoplay-policy=user-gesture-required"];
    browser = await playwright.chromium.launch({
      headless: true, ...(executablePath ? { executablePath } : { channel: "chromium" }),
      ignoreDefaultArgs: ["--mute-audio"],
      args: report.browserArguments[backend],
      ...(process.env.NOON_BROWSER_PROXY ? { proxy: { server: process.env.NOON_BROWSER_PROXY, bypass: "127.0.0.1,localhost" } } : {}),
    });
    report.browserVersion = browser.version();
    for (const transportMode of transports) for (const name of selected) {
      const entry = { id: backend + "-" + transportMode + "-" + name, backend, transportMode,
        case: name, preparationLookaheadSeconds: name === "buffering" ? 0.75 : 1,
        status: "running", checks: [], pageErrors: [], consoleErrors: [], failedResponses: [] };
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
        if (name === "gallery") {
          const galleryServer = await serveRepository(root, 0, { crossOriginIsolated: transportMode === "shared" });
          try { await galleryCase(page, entry, galleryServer.baseUrl); }
          finally { await galleryServer.close(); }
        }
        else {
          await page.goto(server.baseUrl + "/web/execution-worker-smoke.html");
          await installObservers(page);
        }
        if (name === "minimal" || name === "fourier") {
          await ordinaryCase(page, entry, name, name === "minimal" ? originals["audio_ordinary_play.py"] : fourier,
            name === "minimal" ? 3 : 34.65, transportMode);
        } else if (name === "buffering") await bufferingCase(page, entry, transportMode);
        else if (name === "lifecycle") await lifecycleCase(page, entry, transportMode);
        const metrics = entry.renderer ?? await bounded(page.evaluate(async () => {
          return (await preparedAudio.execution.metrics()).metrics;
        }), "final renderer identity");
        assert.ok(metrics, "qualification requires an actual renderer identity");
        assert.equal(metrics.backend, backend === "webgl" ? "WebGL2" : "WebGPU");
        assert.equal(metrics.transportMode, transportMode);
        entry.renderer = metrics;
        assert.deepEqual(entry.pageErrors, []);
        assert.deepEqual(entry.consoleErrors, []);
        assert.deepEqual(entry.failedResponses, []);
        entry.status = "passed";
        console.log("[PASS] " + entry.id);
      } catch (error) {
        entry.status = "failed"; entry.error = String(error.stack ?? error);
        entry.failure = await bounded(snapshot(page), "failure observation", 5_000).catch(() => null);
        await page.screenshot({ path: path.join(output, entry.id + "-failure.png"), timeout: 10_000 }).catch(() => {});
        throw error;
      } finally {
        try { await bounded(context.close(), "browser context teardown"); }
        catch (error) {
          entry.teardownError = String(error.stack ?? error);
          // A diagnostic or teardown failure must not replace the assertion or
          // source error that made this case fail in the first place.
          if (!entry.error) {
            entry.status = "failed"; entry.error = entry.teardownError;
            throw error;
          }
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
  report.teardownErrors = cleanup.filter(x => x.status === "rejected").map(x => String(x.reason));
  if (report.teardownErrors.length && report.status !== "failed") {
    report.status = "failed"; report.error = report.teardownErrors.join("\n"); process.exitCode = 1;
  }
  await writeFile(path.join(output, "report.json"), json(report));
}
console.log(json({ status: report.status, cases: report.cases.map(x => ({ id: x.id, status: x.status })), error: report.error }));
