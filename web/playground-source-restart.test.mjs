import assert from "node:assert/strict";
import test from "node:test";
import { createSourceRestart } from "./playground-source-restart.js";

const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const flush = async () => { for (let i = 0; i < 12; i += 1) await Promise.resolve(); };
function harness() {
  let selection = "one";
  let source = "initial";
  const timers = new Map();
  const stops = [];
  const runs = [];
  const errors = [];
  let timerId = 0;
  const controller = createSourceRestart({
    stop() { const gate = deferred(); stops.push(gate); return gate.promise; },
    run() { const gate = deferred(); runs.push({ source, selection, gate }); return gate.promise; },
    currentSelection: () => selection,
    onError: (error) => errors.push(error),
    setTimer(callback) { const id = ++timerId; timers.set(id, callback); return id; },
    clearTimer(id) { timers.delete(id); },
  });
  return {
    controller, stops, runs, errors,
    edit(text, options) { source = text; controller.edited(options); },
    select(id) { selection = id; },
    tick() { const callbacks = [...timers.values()]; timers.clear(); for (const callback of callbacks) callback(); },
    get timers() { return timers.size; },
  };
}

test("editing stops synchronously; a typing burst submits only the latest source after retirement", async () => {
  const h = harness();
  h.edit("first");
  assert.equal(h.stops.length, 1);
  assert.equal(h.runs.length, 0);
  h.edit("second");
  h.edit("latest");
  assert.equal(h.stops.length, 1);
  assert.equal(h.timers, 1);
  h.tick(); await flush();
  assert.equal(h.runs.length, 0, "replacement must await old context retirement");
  h.stops[0].resolve(); await flush();
  assert.deepEqual(h.runs.map((run) => run.source), ["latest"]);
  h.runs[0].gate.resolve(); await flush();
  h.tick(); await flush();
  assert.equal(h.runs.length, 1);
});

test("an edit during cancellation cannot start at an older debounce deadline", async () => {
  const h = harness();
  h.edit("first"); h.tick();
  h.edit("latest");
  h.stops[0].resolve(); await flush();
  assert.equal(h.runs.length, 0);
  h.tick(); await flush();
  assert.equal(h.runs[0].source, "latest");
  h.runs[0].gate.resolve();
});

test("editing an active replacement stops it without waiting for the whole animation", async () => {
  const h = harness();
  h.edit("first"); h.stops[0].resolve(); h.tick(); await flush();
  h.edit("second");
  assert.equal(h.stops.length, 2);
  h.stops[1].resolve(); h.tick(); await flush();
  assert.deepEqual(h.runs.map((run) => run.source), ["first", "second"]);
  h.runs[0].gate.resolve(); h.runs[1].gate.resolve();
});

test("Run now flushes pending edits without a duplicate delayed Run", async () => {
  const h = harness();
  h.edit("draft");
  const explicit = h.controller.runNow();
  assert.equal(h.timers, 0);
  h.stops[0].resolve(); await flush();
  assert.equal(h.runs[0].source, "draft");
  h.tick(); await flush(); assert.equal(h.runs.length, 1);
  h.runs[0].gate.resolve("done");
  assert.equal(await explicit, "done");
});

test("IME composition stops the old preview but submits only committed text", async () => {
  const h = harness();
  h.edit("partial", { composing: true });
  assert.equal(h.stops.length, 1); assert.equal(h.timers, 0);
  h.tick(); h.stops[0].resolve(); await flush();
  assert.equal(h.runs.length, 0);
  h.edit("committed"); h.stops[1].resolve(); h.tick(); await flush();
  assert.equal(h.runs[0].source, "committed");
  h.runs[0].gate.resolve();
});

test("a selection change or disposal retires pending source submissions", async () => {
  for (const action of ["select", "dispose"]) {
    const h = harness();
    h.edit("draft"); h.tick();
    if (action === "select") h.select("two"); else h.controller.dispose();
    h.stops[0].resolve(); await flush();
    assert.equal(h.runs.length, 0);
  }
});

test("stale run failures do not overwrite newer edit status, and a later run recovers", async () => {
  const h = harness();
  h.edit("first"); h.stops[0].resolve(); h.tick(); await flush();
  h.edit("second"); h.stops[1].resolve(); h.tick(); await flush();
  h.runs[0].gate.reject(new Error("stale failure"));
  h.runs[1].gate.resolve(); await flush();
  assert.deepEqual(h.errors, []);
  h.edit("bad syntax"); h.stops[2].resolve(); h.tick(); await flush();
  h.runs[2].gate.reject(new Error("SyntaxError")); await flush();
  assert.equal(h.errors.length, 1);
  h.edit("repaired"); h.stops[3].resolve(); h.tick(); await flush();
  assert.equal(h.runs[3].source, "repaired");
  h.runs[3].gate.resolve();
});

test("duplicate Run callers both await the same replacement result", async () => {
  const h = harness();
  h.edit("draft");
  const first = h.controller.runNow();
  const second = h.controller.runNow();
  h.stops[0].resolve(); await flush();
  assert.equal(h.runs.length, 1);
  h.runs[0].gate.resolve("latest result");
  assert.deepEqual(await Promise.all([first, second]), ["latest result", "latest result"]);
});

test("retirement failure is reported once and never starts an overlapping run", async () => {
  const h = harness();
  h.edit("draft"); h.tick();
  h.stops[0].reject(new Error("retirement failed")); await flush();
  assert.equal(h.errors.length, 1);
  assert.equal(h.runs.length, 0);
});
