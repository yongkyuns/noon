import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

// Execute the real host polling functions with an injected timer queue. The
// observed engine time is not fabricated or interpolated by this scheduler.
const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const start = main.indexOf("function stopMetricsPolling() {");
const end = main.indexOf('document.addEventListener("visibilitychange"', start);
assert.ok(start >= 0 && end > start, "the host polling lifecycle must remain discoverable");
const source = main.slice(start, end);
function fixture() {
  const timers = new Map();
  let id = 0;
  let reads = 0;
  const context = {
    metricsEpoch: 0, metricsTimer: null, player: {},
    document: { visibilityState: "visible" },
    status: { dataset: { playbackPlaying: "false" } },
    PLAYBACK_POLL_MS: 100, METRICS_POLL_MS: 500,
    setTimeout(callback, delay) { const next = ++id; timers.set(next, { callback, delay }); return next; },
    clearTimeout(timer) { timers.delete(timer); },
    async updateWorkerMetrics() { reads += 1; },
  };
  const api = vm.runInNewContext(`${source}\n({ start: startMetricsPolling, stop: stopMetricsPolling })`, context);
  return {
    context, api, timers,
    get reads() { return reads; },
    get delay() { assert.equal(timers.size, 1); return [...timers.values()][0].delay; },
    fire() { const [key, timer] = timers.entries().next().value; timers.delete(key); return timer.callback(); },
  };
}

test("first observation uses playback cadence before a playing-state poll has arrived", async () => {
  const f = fixture();
  f.api.start();
  assert.equal(f.delay, 100, "startup must not freeze an already-visible wait clock for 500 ms");
  f.context.status.dataset.playbackPlaying = "true";
  await f.fire();
  assert.equal(f.reads, 1);
  assert.equal(f.delay, 100);
  f.context.status.dataset.playbackPlaying = "false";
  await f.fire();
  assert.equal(f.reads, 2);
  assert.equal(f.delay, 500, "settled playback must retain the existing lower polling cadence");
});

test("polling has one timer, sleeps when hidden, and resumes with a prompt observation", () => {
  const f = fixture();
  f.api.start(); f.api.start();
  assert.equal(f.timers.size, 1);
  f.api.stop();
  assert.equal(f.timers.size, 0);
  f.context.document.visibilityState = "hidden";
  f.api.start();
  assert.equal(f.timers.size, 0);
  f.context.document.visibilityState = "visible";
  f.api.start();
  assert.equal(f.delay, 100);
  f.api.stop();
  f.context.player = null;
  f.api.start();
  assert.equal(f.timers.size, 0);
});

test("a retired in-flight poll cannot create a second loop after restart", async () => {
  const f = fixture();
  let release;
  f.context.updateWorkerMetrics = () => new Promise(resolve => { release = resolve; });
  f.api.start();
  const oldPoll = f.fire();
  f.api.stop(); f.api.start();
  const replacement = [...f.timers.keys()][0];
  release(); await oldPoll;
  assert.deepEqual([...f.timers.keys()], [replacement]);
  assert.equal(f.delay, 100);
});
