import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const start = main.indexOf("async function updateWorkerMetrics() {");
const end = main.indexOf('document.addEventListener("visibilitychange"', start);
assert.ok(start >= 0 && end > start);
const source = main.slice(start, end);
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const flush = async () => { for (let i = 0; i < 16; i += 1) await Promise.resolve(); };
const report = count => ({
  metrics: { objectCount: count, drawCalls: 1, bytesUploaded: 0, instancesDrawn: count,
    geometryCacheMisses: 0, presentedFrames: 1 },
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
    document: { visibilityState: "visible" },
    busyDepth: 1, activeSourceContinuation: {}, playerNeedsRestart: false,
    generations: { diagnostics: { runGeneration: 1 } },
    status: { dataset: {} }, METRICS_POLL_MS: 500, PLAYBACK_POLL_MS: 100,
    performance: { now: () => wall },
    setPlaybackRuntimeStatus() {}, formatBytes: String,
    playbackControls: { observe: value => observations.push({ ...value }) },
    showError: error => errors.push(error), clearTimeout() {},
  };
  for (const key of ["metricObjects", "metricDraws", "metricUpload", "metricTime"]) context[key] = { value: "—" };
  const api = vm.runInNewContext(`${source}\n({ poll: updateWorkerMetrics, stop: stopMetricsPolling })`, context);
  return { context, api, initial, makePlayer, observations, errors, telemetry,
    setWall(value) { wall = value; } };
}

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
