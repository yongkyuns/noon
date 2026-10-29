import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";
import { PresentationRate } from "./frame-metrics.js";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const start = main.indexOf("async function updateWorkerMetrics() {");
const end = main.indexOf('document.addEventListener("visibilitychange"', start);
assert.ok(start >= 0 && end > start);
// Status owns the playing/paused/buffering decision consumed by telemetry.
// Exercise those production functions too; a no-op status mock would bypass
// the very observation that decides whether frame-rate evidence is current.
const statusStart = main.indexOf("function setRuntimeStatus(");
const statusEnd = main.indexOf("function setBusy(", statusStart);
assert.ok(statusStart >= 0 && statusEnd > statusStart);
const source = `${main.slice(statusStart, statusEnd)}\n${main.slice(start, end)}`;
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const flush = async () => { for (let i = 0; i < 16; i += 1) await Promise.resolve(); };
const report = count => ({
  metrics: { objectCount: count, drawCalls: 1, bytesUploaded: 0, instancesDrawn: count,
    geometryCacheMisses: 0, presentedFrames: 1, presentationIntervalMs: null },
  engineMetrics: { host: { missedDeadlines: 0, droppedLateResults: 0 } },
  executionMode: "semantic",
});
function fixture() {
  let wall = 0;
  const observations = [], errors = [], telemetry = [];
  function makePlayer() {
    let state = { time: 0.2, playing: true };
    let reads = 0;
    return {
      mode: "semantic", rendererBackend: "WebGPU",
      state: async () => { reads += 1; return state; },
      metrics() { const pending = deferred(); telemetry.push(pending); return pending.promise; },
      setState(value) { state = value; },
      get reads() { return reads; },
    };
  }
  const initial = makePlayer();
  const context = {
    player: initial, metricsPending: null, playbackPending: null, metricsNextPollAt: 0,
    metricsEpoch: 0, metricsTimer: null, rendererBackend: "WebGPU",
    presentationRate: new PresentationRate(), patchStatus: { dataset: { state: "running" } },
    document: { visibilityState: "visible" },
    busyDepth: 1, activeSourceContinuation: {}, playerNeedsRestart: false,
    sceneRunPromise: {}, playbackDurationSeconds: 4, preparedPlaybackPlayer: null,
    generations: { diagnostics: { runGeneration: 1 } },
    status: { dataset: {} }, statusText: { textContent: "" }, METRICS_POLL_MS: 500, PLAYBACK_POLL_MS: 100,
    performance: { now: () => wall },
    formatBytes: String,
    playbackControls: { observe: value => observations.push({ ...value }) },
    showError: error => errors.push(error), clearTimeout() {},
  };
  for (const key of ["metricObjects", "metricDraws", "metricUpload", "metricTime", "metricFps", "metricFrameGap"]) context[key] = { value: "—" };
  const api = vm.runInNewContext(`${source}\n({ poll: updateWorkerMetrics, stop: stopMetricsPolling })`, context);
  return { context, api, initial, makePlayer, observations, errors, telemetry,
    setWall(value) { wall = value; } };
}

test("frame gap is independent of FPS warmup and clears on pause and polling retirement", async () => {
  const f = fixture();
  async function sample(wall, presentedFrames, sampledAtMs, presentationIntervalMs = null) {
    f.setWall(wall);
    await f.api.poll();
    const value = report(600);
    Object.assign(value.metrics, { ready: true, presentedFrames, sampledAtMs, presentationIntervalMs });
    f.telemetry.at(-1).resolve(value);
    await flush();
    assert.deepEqual(f.errors, []);
    return { fps: f.context.metricFps.value, frameGap: f.context.metricFrameGap.value };
  }
  assert.deepEqual(await sample(0, 10, 100, { p95: 16.7, max: 33.4 }),
    { fps: "—", frameGap: "16.7 / 33.4 ms" }, "frame gap does not wait for FPS warmup");
  assert.deepEqual(await sample(3000, 70, 1100, { p95: 20, max: 41 }),
    { fps: "60.0", frameGap: "20.0 / 41.0 ms" }, "message delay is not frame time");
  assert.deepEqual(await sample(3500, 73, 1150),
    { fps: "60.0", frameGap: "—" }, "a missing continuous-cadence summary clears the gap");
  f.context.activeSourceContinuation = null;
  f.context.busyDepth = 0;
  f.initial.setState({ time: 1, playing: false });
  f.setWall(3600);
  await f.api.poll();
  assert.equal(f.context.metricFps.value, "—", "pause clears before the next telemetry poll");
  assert.equal(f.context.metricFrameGap.value, "—", "pause clears the last continuous frame gap");
  f.initial.setState({ time: 1, playing: true });
  assert.deepEqual(await sample(4000, 75, 3000), { fps: "—", frameGap: "—" });
  assert.deepEqual(await sample(5000, 105, 4000), { fps: "30.0", frameGap: "—" });
  f.api.stop();
  assert.equal(f.context.metricFps.value, "—", "hidden or retired sessions cannot leave a stale rate");
  assert.equal(f.context.metricFrameGap.value, "—", "hidden or retired sessions cannot leave a stale gap");
});

test("a slow renderer reply cannot hold or freeze current Rust clock observations", async () => {
  const f = fixture();
  void f.api.poll(); await flush();
  assert.equal(f.observations.at(-1)?.time, 0.2, "publish engine time before renderer metrics resolve");
  f.initial.setState({ time: 0.3, playing: true }); f.setWall(100);
  void f.api.poll(); await flush();
  assert.equal(f.observations.at(-1)?.time, 0.3, "continue observing while telemetry is pending");
  assert.equal(f.telemetry.length, 1, "one slow metrics request must not create a backlog");
  f.telemetry[0].resolve(report(626)); await flush();
  assert.equal(f.context.metricObjects.value, "626");
  assert.equal(f.observations.at(-1)?.time, 0.3, "old telemetry must never restore an older playhead");
});

test("renderer telemetry remains at the slower cadence without slowing the playhead", async () => {
  const f = fixture();
  void f.api.poll(); await flush(); f.telemetry[0].resolve(report(626)); await flush();
  for (const wall of [100, 200, 300, 400]) {
    f.setWall(wall); f.initial.setState({ time: wall / 1000, playing: true });
    void f.api.poll(); await flush();
  }
  assert.equal(f.initial.reads, 5);
  assert.equal(f.telemetry.length, 1);
  f.setWall(500); void f.api.poll(); await flush();
  assert.equal(f.telemetry.length, 2);
  f.telemetry[1].resolve(report(626)); await flush();
});

test("retired clock and telemetry requests cannot overwrite or block a replacement", async () => {
  const f = fixture(), oldClock = deferred();
  f.initial.state = () => oldClock.promise;
  void f.api.poll(); await flush();
  f.api.stop();
  f.context.player = f.makePlayer();
  f.context.generations.diagnostics.runGeneration += 1;
  f.context.player.setState({ time: 0.05, playing: true });
  void f.api.poll(); await flush();
  assert.equal(f.observations.at(-1)?.time, 0.05);
  assert.equal(f.telemetry.length, 2);
  f.telemetry[1].resolve(report(2)); await flush();
  oldClock.resolve({ time: 9, playing: false });
  f.telemetry[0].reject(new Error("old renderer retired")); await flush();
  assert.equal(f.context.metricObjects.value, "2");
  assert.equal(f.observations.at(-1)?.time, 0.05);
  assert.deepEqual(f.errors, []);
});

test("hidden and unready hosts do not request either clock or renderer work", async () => {
  const f = fixture(); f.context.document.visibilityState = "hidden";
  await f.api.poll();
  f.context.document.visibilityState = "visible"; f.context.playerNeedsRestart = true;
  await f.api.poll();
  f.context.playerNeedsRestart = false; f.context.activeSourceContinuation = null;
  await f.api.poll();
  assert.equal(f.initial.reads, 0); assert.equal(f.telemetry.length, 0);
});

for (const invalidation of ["generation", "epoch"]) {
  test(`${invalidation} changes retire observations even when the player is reused`, async () => {
    const f = fixture(), oldClock = deferred(), freshClock = deferred();
    f.initial.state = () => oldClock.promise;
    void f.api.poll(); await flush();
    if (invalidation === "generation") f.context.generations.diagnostics.runGeneration += 1;
    else f.api.stop();
    f.setWall(500);
    f.initial.state = () => freshClock.promise;
    void f.api.poll(); await flush();
    assert.equal(f.telemetry.length, 2);
    const pending = f.context.playbackPending;
    oldClock.reject(new Error("retired clock"));
    f.telemetry[0].resolve(report(999)); await flush();
    assert.equal(f.context.playbackPending, pending, "old completion cannot clear the current request");
    assert.equal(f.context.metricObjects.value, "—");
    freshClock.resolve({ time: 0.05, playing: true });
    f.telemetry[1].resolve(report(2)); await flush();
    assert.equal(f.observations.at(-1)?.time, 0.05);
    assert.equal(f.context.metricObjects.value, "2");
    assert.deepEqual(f.errors, []);
  });
}

test("a current telemetry failure does not prevent subsequent clock observations", async () => {
  const f = fixture();
  void f.api.poll(); await flush();
  f.telemetry[0].reject(new Error("metrics failed")); await flush();
  assert.equal(f.errors.length, 1);
  f.initial.setState({ time: 0.3, playing: true }); f.setWall(100);
  void f.api.poll(); await flush();
  assert.equal(f.observations.at(-1)?.time, 0.3);
  assert.equal(f.telemetry.length, 1, "an error must not trigger a telemetry retry storm");
});

const preparedState = overrides => ({
  preparedPlayback: true, time: 0.5, playing: true, preparedFrontier: 4,
  durationSeconds: null, buffering: false, ...overrides,
});
const readyReport = (presentedFrames, sampledAtMs) => {
  const value = report(600);
  Object.assign(value.metrics, { ready: true, presentedFrames, sampledAtMs,
    presentationIntervalMs: { p95: 16.7, max: 33.4 } });
  return value;
};

for (const [label, state, phase] of [
  ["source-owned wait", { time: 0.5, playing: false }, "playing"],
  ["prepared running", preparedState({}), "playing"],
  ["prepared paused", preparedState({ playing: false }), "paused"],
  ["prepared buffering", preparedState({ buffering: true }), "buffering"],
  ["prepared paused while buffering", preparedState({ playing: false, buffering: true }), "paused"],
  ["prepared exact terminal", preparedState({ time: 4, durationSeconds: 4 }), "finished"],
]) {
  test(`${label} publishes the actual phase before delayed renderer telemetry`, async () => {
    const f = fixture();
    f.initial.setState(state);
    await f.api.poll();
    // Keep the source pending in every case. Preparation still being active
    // must not force an independent paused/buffering/finished reader to play.
    assert.equal(f.context.status.dataset.playbackPhase, phase);
    assert.equal(f.context.status.dataset.playbackPlaying, String(phase === "playing"));
    assert.match(f.context.statusText.textContent, new RegExp(`^${phase}`, "i"));
    assert.deepEqual(f.observations.at(-1), state);
    f.telemetry[0].resolve(readyReport(10, 100));
    await flush();
    assert.equal(f.context.metricFrameGap.value, phase === "playing" ? "16.7 / 33.4 ms" : "—");
    assert.equal(f.context.metricFps.value, "—", "one telemetry sample is not an FPS measurement");
    assert.deepEqual(f.errors, []);
  });
}

for (const [phase, state] of [
  ["paused", preparedState({ playing: false })],
  ["buffering", preparedState({ buffering: true })],
  ["finished", preparedState({ time: 4, durationSeconds: 4 })],
]) {
  test(`late renderer telemetry cannot restore a rate after prepared playback is ${phase}`, async () => {
    const f = fixture();
    f.initial.setState(preparedState({}));
    await f.api.poll();
    f.telemetry[0].resolve(readyReport(10, 100));
    await flush();
    f.setWall(500);
    await f.api.poll();
    f.telemetry[1].resolve(readyReport(70, 1100));
    await flush();
    assert.equal(f.context.metricFps.value, "60.0");
    assert.equal(f.context.metricFrameGap.value, "16.7 / 33.4 ms");

    f.setWall(1000);
    await f.api.poll(); // Start a slow render reply while the reader is playing.
    f.initial.setState(state);
    f.setWall(1100);
    await f.api.poll(); // The ready Rust observation must not wait for telemetry.
    assert.equal(f.telemetry.length, 3, "one pending render request, not a backlog");
    assert.equal(f.context.status.dataset.playbackPhase, phase);
    assert.equal(f.context.status.dataset.playbackPlaying, "false");
    assert.equal(f.context.metricFps.value, "—");
    assert.equal(f.context.metricFrameGap.value, "—");
    f.telemetry[2].resolve(readyReport(130, 2100));
    await flush();
    assert.equal(f.context.metricFps.value, "—");
    assert.equal(f.context.metricFrameGap.value, "—");
    assert.deepEqual(f.observations.at(-1), state);
    assert.deepEqual(f.errors, []);
  });
}
