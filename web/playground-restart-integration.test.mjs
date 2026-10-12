import assert from "node:assert/strict";
import test from "node:test";
import { createSourceRestart } from "./playground-source-restart.js";
import { createRunRequestRouter } from "./playground-run-request-router.js";

function deferred() {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
}
async function flush() {
  for (let i = 0; i < 30; i += 1) await Promise.resolve();
}
function fixture() {
  let source = "original";
  let selection = "one";
  let active = null;
  const cancellation = deferred();
  const starts = [];
  const timers = new Map();
  const errors = [];
  let timerId = 0;
  const run = () => {
    const gate = deferred();
    const request = { token: {}, source };
    active = { gate, request };
    starts.push({ gate, source });
    return gate.promise.finally(() => { if (active?.gate === gate) active = null; });
  };
  const router = createRunRequestRouter({
    currentSource: () => source,
    currentRun: () => active?.gate.promise ?? null,
    activeSourceContinuation: () => active,
    activeRunRequest: () => active?.request,
    isActiveRunCurrent: (token) => token === active?.request.token,
    run,
    supersede: () => cancellation.promise.then(() => true),
    onQueued() {},
  });
  const restart = createSourceRestart({
    stop: () => Promise.all([cancellation.promise, active?.gate.promise]),
    run: (isCurrent) => router.request(isCurrent),
    currentSelection: () => selection,
    onError: (error) => errors.push(error),
    setTimer(callback) { const id = ++timerId; timers.set(id, callback); return id; },
    clearTimer(id) { timers.delete(id); },
  });
  return {
    restart, starts, errors,
    edit() { source = "edited"; restart.edited(); },
    select() { selection = "two"; restart.cancel(); },
    retire() { cancellation.resolve(); active?.gate.resolve(); },
    tick() { const pending = [...timers.values()]; timers.clear(); for (const callback of pending) callback(); },
  };
}

test("an edit during an explicit Run's cancellation cannot bypass the new typing debounce", async () => {
  const f = fixture();
  const initial = f.restart.runNow(); await flush();
  const explicit = f.restart.runNow(); await flush();
  f.edit();
  f.retire(); await flush();
  assert.equal(f.starts.length, 1, "the obsolete explicit request must not restart before the edit timer");
  f.tick(); await flush();
  assert.deepEqual(f.starts.map((entry) => entry.source), ["original", "edited"]);
  f.retire();
  await Promise.all([initial, explicit]);
  assert.deepEqual(f.errors, []);
});

test("selection change and disposal prevent a queued explicit Run from reviving source", async () => {
  for (const dispose of [false, true]) {
    const f = fixture();
    const initial = f.restart.runNow(); await flush();
    const explicit = f.restart.runNow(); await flush();
    if (dispose) f.restart.dispose(); else f.select();
    f.retire(); await flush();
    assert.equal(f.starts.length, 1);
    await Promise.all([initial, explicit]);
    assert.deepEqual(f.errors, []);
  }
});
