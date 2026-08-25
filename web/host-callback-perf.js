import initNoonWeb, { HostScenePlayer } from "./pkg/noon_web.js";
import { PythonAuthoringClient } from "./authoring-client.js";
import { BrowserJankMonitor } from "./browser-jank.js";
import { FrameMetrics, SampleWindow } from "./frame-metrics.js";

const parameters = new URLSearchParams(location.search);
const objectCount = positiveInteger("objects", 1_000);
const activeCount = positiveInteger("active", 1);
const warmupFrames = positiveInteger("warmup", 30);
const measuredFrames = positiveInteger("frames", 300);
if (activeCount > objectCount) throw new Error("active updater count cannot exceed object count");

const status = document.querySelector("#status");
const output = document.querySelector("#json");
const encoder = new TextEncoder();
let client = null;

try {
  await initNoonWeb();
  client = new PythonAuthoringClient();
  await client.ready();

  const native = await author(nativeSource(objectCount, activeCount));
  const host = await author(hostSource(objectCount, activeCount));
  if (native.callbacks !== null) throw new Error("native comparison scene unexpectedly registered callbacks");
  if (host.callbacks === null) throw new Error("host comparison scene did not register callbacks");

  status.value = `Native baseline · ${activeCount}/${objectCount} active…`;
  const nativeResult = await measureNative(native.document);
  status.value = `Python updater · ${activeCount}/${objectCount} active…`;
  const hostResult = await measureHost(host);

  const report = {
    schemaVersion: 1,
    benchmark: "Noon native versus Python host callback frame cost",
    generatedAt: new Date().toISOString(),
    workload: { objects: objectCount, active: activeCount, warmupFrames, measuredFrames },
    environment: {
      userAgent: navigator.userAgent,
      hardwareConcurrency: navigator.hardwareConcurrency ?? null,
      devicePixelRatio: window.devicePixelRatio || 1,
    },
    native: nativeResult,
    host: hostResult,
    overhead: {
      frameWorkP95Ms: difference(hostResult.frameWorkMs?.p95, nativeResult.frameWorkMs?.p95),
      cadenceP95Ms: difference(hostResult.frameIntervalMs?.p95, nativeResult.frameIntervalMs?.p95),
      callbackRoundTripP95Ms: hostResult.callbackRoundTripMs?.p95 ?? null,
      callbackSnapshotBytesP50: hostResult.callbackSnapshotBytes?.p50 ?? null,
      patchBytesP50: hostResult.patchBytes?.p50 ?? null,
    },
  };
  window.__NOON_HOST_CALLBACK_PERF__ = report;
  output.textContent = JSON.stringify(report, null, 2);
  status.value =
    `Complete · native work p95 ${format(nativeResult.frameWorkMs?.p95)} ms · ` +
    `host p95 ${format(hostResult.frameWorkMs?.p95)} ms · ` +
    `callback ${format(hostResult.callbackRoundTripMs?.p95)} ms`;
  status.dataset.state = "complete";
  console.log("NOON_HOST_CALLBACK_PERF", report);
} catch (error) {
  console.error(error);
  status.value = `Host callback profile failed: ${error}`;
  status.dataset.state = "error";
} finally {
  client?.terminate();
}

async function author(source) {
  const result = await client.run(source);
  if (result.kind !== "scene_document") throw new Error("performance source did not return a scene");
  return result;
}

async function measureNative(document) {
  const player = new HostScenePlayer(JSON.stringify(document), "[]");
  try {
    for (let frame = 0; frame < warmupFrames; frame += 1) {
      player.advanceTo((frame + 1) / 60);
    }
    const cadence = new FrameMetrics();
    const work = new SampleWindow(measuredFrames);
    const jank = new BrowserJankMonitor();
    let firstTimestamp = null;
    const start = performance.now();
    jank.start();
    for (let frame = 0; frame < measuredFrames; frame += 1) {
      const timestamp = await nextAnimationFrame();
      firstTimestamp ??= timestamp;
      const semanticTime = warmupFrames / 60 + (timestamp - firstTimestamp) / 1000 + 1 / 60;
      const began = performance.now();
      player.advanceTo(semanticTime);
      const elapsed = performance.now() - began;
      cadence.record(timestamp, elapsed);
      work.record(elapsed);
    }
    const end = performance.now();
    jank.stop();
    const frame = cadence.summary();
    return {
      frameWorkMs: work.summary(),
      frameIntervalMs: frame.interval,
      cadence: frame.cadence,
      longTasks: jank.summary(start, end),
    };
  } finally {
    player.free();
  }
}

async function measureHost(authored) {
  const player = new HostScenePlayer(
    JSON.stringify(authored.document),
    JSON.stringify(authored.callbacks.slots),
  );
  try {
    let sequence = 0;
    for (let frame = 0; frame < warmupFrames; frame += 1) {
      player.advanceTo((frame + 1) / 60);
      const snapshot = JSON.parse(player.callbackFrameJson());
      const batch = await client.runCallbackPhase(authored.callbacks.session_id, snapshot, sequence);
      player.commitPatchBatch(JSON.stringify(batch));
      sequence += 1;
    }

    const cadence = new FrameMetrics();
    const windows = {
      work: new SampleWindow(measuredFrames),
      advance: new SampleWindow(measuredFrames),
      snapshot: new SampleWindow(measuredFrames),
      parse: new SampleWindow(measuredFrames),
      callback: new SampleWindow(measuredFrames),
      serialize: new SampleWindow(measuredFrames),
      commit: new SampleWindow(measuredFrames),
      snapshotBytes: new SampleWindow(measuredFrames),
      patchBytes: new SampleWindow(measuredFrames),
      patchCount: new SampleWindow(measuredFrames),
    };
    const jank = new BrowserJankMonitor();
    let firstTimestamp = null;
    const start = performance.now();
    jank.start();
    for (let frame = 0; frame < measuredFrames; frame += 1) {
      const timestamp = await nextAnimationFrame();
      firstTimestamp ??= timestamp;
      const semanticTime = warmupFrames / 60 + (timestamp - firstTimestamp) / 1000 + 1 / 60;
      const frameStarted = performance.now();

      let began = performance.now();
      player.advanceTo(semanticTime);
      windows.advance.record(performance.now() - began);

      began = performance.now();
      const snapshotJson = player.callbackFrameJson();
      windows.snapshot.record(performance.now() - began);
      windows.snapshotBytes.record(encoder.encode(snapshotJson).byteLength);

      began = performance.now();
      const snapshot = JSON.parse(snapshotJson);
      windows.parse.record(performance.now() - began);

      began = performance.now();
      const batch = await client.runCallbackPhase(authored.callbacks.session_id, snapshot, sequence);
      windows.callback.record(performance.now() - began);

      began = performance.now();
      const patchJson = JSON.stringify(batch);
      windows.serialize.record(performance.now() - began);
      windows.patchBytes.record(encoder.encode(patchJson).byteLength);
      windows.patchCount.record(batch.patches.length);

      began = performance.now();
      player.commitPatchBatch(patchJson);
      windows.commit.record(performance.now() - began);
      sequence += 1;

      const elapsed = performance.now() - frameStarted;
      windows.work.record(elapsed);
      cadence.record(timestamp, elapsed);
    }
    const end = performance.now();
    jank.stop();
    const frame = cadence.summary();
    return {
      frameWorkMs: windows.work.summary(),
      frameIntervalMs: frame.interval,
      cadence: frame.cadence,
      advanceMs: windows.advance.summary(),
      callbackSnapshotMs: windows.snapshot.summary(),
      mainThreadParseMs: windows.parse.summary(),
      callbackRoundTripMs: windows.callback.summary(),
      patchSerializeMs: windows.serialize.summary(),
      patchCommitMs: windows.commit.summary(),
      callbackSnapshotBytes: windows.snapshotBytes.summary(),
      patchBytes: windows.patchBytes.summary(),
      patchCount: windows.patchCount.summary(),
      longTasks: jank.summary(start, end),
    };
  } finally {
    player.free();
  }
}

function nativeSource(objects, active) {
  return `
from noon import Circle, Scene, Vec2
scene = Scene()
dots = [Circle(0.1) for _ in range(${objects})]
for index, dot in enumerate(dots):
    scene.add(dot, key=f"dot.{index}")
for index, dot in enumerate(dots[:${active}]):
    scene.animate_position(dot, Vec2(0.0, 0.0), Vec2(1.0, 0.0), duration=60.0, key=f"move.{index}")
result = scene
`;
}

function hostSource(objects, active) {
  return `
from noon import Circle, RIGHT, Scene
scene = Scene()
dots = [Circle(0.1) for _ in range(${objects})]
for index, dot in enumerate(dots):
    scene.add(dot, key=f"dot.{index}")

def move(mobject, dt):
    mobject.shift(RIGHT * (0.1 * dt))

for dot in dots[:${active}]:
    dot.add_updater(move)
result = scene
`;
}

function nextAnimationFrame() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

function positiveInteger(name, fallback) {
  const value = parameters.get(name);
  if (value === null) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`);
  return parsed;
}

function difference(left, right) {
  return Number.isFinite(left) && Number.isFinite(right) ? left - right : null;
}

function format(value) {
  return Number.isFinite(value) ? Number(value).toFixed(3) : "—";
}
