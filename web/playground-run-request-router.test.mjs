import assert from "node:assert/strict";
import test from "node:test";

import { createRunRequestRouter } from "./playground-run-request-router.js";

function deferred() {
  let resolve;
  const promise = new Promise((resolvePromise) => { resolve = resolvePromise; });
  return { promise, resolve };
}

async function flush() {
  for (let index = 0; index < 8; index += 1) await Promise.resolve();
}

function harness({ source = "first source", activeContinuation = false, cancellation = null } = {}) {
  let currentSource = source;
  let active = null;
  let continuation = activeContinuation ? {} : null;
  let nextToken = 0;
  let supersedes = 0;
  let queued = 0;
  const starts = [];
  const run = () => {
    const gate = deferred();
    const request = { token: ++nextToken, source: currentSource };
    active = { gate, request };
    starts.push({ gate, source: currentSource, request });
    gate.promise.finally(() => {
      if (active?.request === request) active = null;
    });
    return gate.promise;
  };
  const router = createRunRequestRouter({
    currentSource: () => currentSource,
    currentRun: () => active?.gate.promise ?? null,
    activeSourceContinuation: () => continuation,
    activeRunRequest: () => active?.request ?? null,
    isActiveRunCurrent: (token) => active?.request.token === token,
    run,
    async supersede() {
      if (continuation === null) return false;
      supersedes += 1;
      continuation = null;
      await cancellation?.promise;
      return true;
    },
    onQueued() { queued += 1; },
  });
  return {
    router, starts,
    setSource(value) { currentSource = value; },
    setContinuation(value) { continuation = value ? {} : null; },
    resolveActive() { active?.gate.resolve(); },
    get supersedes() { return supersedes; },
    get queued() { return queued; },
  };
}

test("duplicate unchanged Run before continuation registration executes once", async () => {
  const state = harness();
  const first = state.router.request();
  const duplicate = state.router.request();
  assert.equal(state.starts.length, 1);
  state.resolveActive();
  await Promise.all([first, duplicate]);
  assert.equal(state.starts.length, 1);
  assert.equal(state.queued, 0);
});

test("changed source during a build queues exactly one replacement using the latest source", async () => {
  const state = harness();
  const first = state.router.request();
  state.setSource("second source");
  const changed = state.router.request();
  state.setSource("latest source");
  const duplicateChanged = state.router.request();
  await flush();
  assert.equal(state.starts.length, 1);
  assert.equal(state.queued, 1);
  state.resolveActive();
  await flush();
  assert.equal(state.starts.length, 2);
  assert.equal(state.starts[1].source, "latest source");
  state.resolveActive();
  await Promise.all([first, changed, duplicateChanged]);
  assert.equal(state.starts.length, 2);
});

test("unchanged Run during an active continuation supersedes once", async () => {
  const state = harness({ activeContinuation: true });
  const first = state.router.request();
  const duplicate = state.router.request();
  assert.equal(state.starts.length, 1);
  assert.equal(state.supersedes, 1);
  state.resolveActive();
  await flush();
  assert.equal(state.starts.length, 2);
  state.resolveActive();
  await Promise.all([first, duplicate]);
  assert.equal(state.starts.length, 2);
});

test("requests during cancellation converge without a stale replacement or deadlock", async () => {
  const cancellation = deferred();
  const state = harness({ activeContinuation: true, cancellation });
  const first = state.router.request();
  const second = state.router.request();
  const third = state.router.request();
  assert.equal(state.supersedes, 1);
  assert.equal(state.starts.length, 1);
  cancellation.resolve();
  state.resolveActive();
  await flush();
  assert.equal(state.starts.length, 2);
  state.resolveActive();
  await Promise.all([first, second, third]);
  assert.equal(state.starts.length, 2);
});

test("an edited Run supersedes a replacement after that replacement owns its continuation", async () => {
  const state = harness();
  const initial = state.router.request();
  state.setContinuation(true);
  const replacement = state.router.request();
  state.resolveActive();
  await flush();
  assert.equal(state.starts.length, 2);

  state.setContinuation(true);
  state.setSource("edited replacement source");
  const editedReplacement = state.router.request();
  assert.equal(state.supersedes, 2, "the active replacement must receive its own explicit supersession");
  state.resolveActive();
  await flush();
  assert.equal(state.starts.length, 3);
  assert.equal(state.starts[2].source, "edited replacement source");

  state.resolveActive();
  await Promise.all([initial, replacement, editedReplacement]);
  assert.equal(state.starts.length, 3);
});


test("every request joining cancellation awaits the replacement result", async () => {
  const cancellation = deferred();
  const state = harness({ activeContinuation: true, cancellation });
  const initial = state.router.request();
  const replacement = state.router.request();
  let joinedSettled = false;
  const joined = state.router.request().then((result) => {
    joinedSettled = true;
    return result;
  });
  cancellation.resolve();
  state.resolveActive();
  await flush();
  assert.equal(state.starts.length, 2);
  assert.equal(joinedSettled, false, "joined request resolved before replacement completed");
  state.starts[1].gate.resolve("replacement result");
  assert.deepEqual(await Promise.all([replacement, joined]), ["replacement result", "replacement result"]);
  await initial;
});


test("a fresh Run joining an invalidated cancellation still starts exactly one replacement", async () => {
  const cancellation = deferred();
  const state = harness({ activeContinuation: true, cancellation });
  const initial = state.router.request();
  let current = true;
  const superseded = state.router.request(() => current);
  current = false;
  state.setSource("newest explicitly requested source");
  const latest = state.router.request(() => true);
  cancellation.resolve();
  state.resolveActive();
  await flush();
  assert.equal(state.starts.length, 2);
  assert.equal(state.supersedes, 1);
  assert.equal(state.starts[1].source, "newest explicitly requested source");
  state.starts[1].gate.resolve("latest result");
  assert.deepEqual(await Promise.all([superseded, latest]), ["latest result", "latest result"]);
  await initial;
});
