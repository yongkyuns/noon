import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

// Run the production attachment function, not a copy of its lifecycle policy.
// DOM, worker endpoints and source-token freshness are deterministic doubles.
const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const start = main.indexOf("async function ensureRuntimeReady({");
const end = main.indexOf("async function ensureExecutionReady()", start);
assert.ok(start >= 0 && end > start, "runtime attachment boundary must exist");
const attachment = main.slice(start, end);

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

async function until(predicate) {
  for (let i = 0; i < 100; i++) {
    if (predicate()) return;
    await Promise.resolve();
  }
  assert.fail("attachment did not reach the expected boundary");
}

function fixture({ preparedPlayback = true, continuationGeneration = 1,
  replaySupported = true } = {}) {
  const preparationGate = deferred(), startGate = deferred(), stateGate = deferred();
  const calls = [], token = {}, client = {};
  const initialState = { preparedPlayback, replaySupported, time: 0.25, playing: false,
    preparedFrontier: 1, durationSeconds: null, buffering: false, nextPatchSequence: 2 };
  const descriptor = { contextId: "source", continuationGeneration, preparedPlayback };
  const candidate = {
    mode: "semantic",
    async startSemanticExecution(...args) { calls.push(["start", ...args]); return startGate.promise; },
    async state() { calls.push(["state"]); return stateGate.promise; },
    terminate() { calls.push(["terminate", this]); },
  };
  const preparation = { candidate, ready: preparationGate.promise, startedAtMs: 1, readyAtMs: 2 };
  const env = { calls, token, currentToken: token, preparation, controlsError: null };
  const api = new Function("env", `
    let player = null, preparedPlaybackPlayer = null, playbackControls = null;
    let runtimePreparation = env.preparation, runtimeStartPromise = null;
    let playerNeedsRestart = true, rendererBackend = "";
    const status = { dataset: {} }, patchStatus = { dataset: {} };
    const isCurrentRun = token => token === env.currentToken;
    const ensureRuntimePreparation = () => runtimePreparation;
    const setRuntimeStatus = (...args) => env.calls.push(["status", ...args]);
    const adoptRuntimeCanvas = owner => env.calls.push(["adopt", owner]);
    const startMetricsPolling = () => env.calls.push(["poll"]);
    const updatePlaybackControls = options => {
      env.calls.push(["controls", options]);
      playbackControls = { destroy: () => env.calls.push(["destroy-controls"]) };
      if (env.controlsError) throw env.controlsError;
    };
    ${attachment}
    return {
      attach: ensureRuntimeReady,
      inspect: () => ({ player, preparedPlaybackPlayer, playbackControls,
        runtimePreparation, runtimeStartPromise, playerNeedsRestart }),
      supersede: () => { env.currentToken = {}; },
      replace: (owner, prepared, controls, pending = null) => {
        player = owner; preparedPlaybackPlayer = owner; playbackControls = controls;
        runtimePreparation = prepared; runtimeStartPromise = pending;
      },
    };
  `)(env);
  return {
    ...api, env, calls, candidate, preparation, descriptor, initialState,
    preparationGate, startGate, stateGate,
    options: { preparation, semanticExecution: descriptor, authoringClient: client,
      loopDurationSeconds: 4, runToken: token },
    resolveAll() {
      preparationGate.resolve();
      startGate.resolve({ render: { backend: "WebGL2" } });
      stateGate.resolve(initialState);
    },
    count(name) { return calls.filter(call => call[0] === name).length; },
  };
}

for (const [name, profile, supported] of [
  ["prepared continuation", {}, true],
  ["source-owned continuation", { preparedPlayback: false }, false],
  ["ordinary replay", { preparedPlayback: false, continuationGeneration: null }, true],
  ["unavailable replay", { preparedPlayback: false, continuationGeneration: null,
    replaySupported: false }, false],
]) {
  test(`${name} publishes only after the first state observation`, async () => {
    const f = fixture(profile);
    const result = f.attach(f.options);
    assert.equal(f.inspect().player, null);
    f.preparationGate.resolve();
    await until(() => f.count("start") === 1);
    assert.equal(f.inspect().player, null);
    assert.equal(f.count("controls"), 0);
    f.startGate.resolve({ render: { backend: "WebGL2" } });
    await until(() => f.count("state") === 1);
    assert.equal(f.inspect().player, null);
    assert.equal(f.count("poll"), 0);
    f.stateGate.resolve(f.initialState);
    const ready = await result;
    assert.equal(f.inspect().player, f.candidate);
    assert.equal(f.inspect().preparedPlaybackPlayer, f.initialState.preparedPlayback ? f.candidate : null);
    assert.equal(f.inspect().runtimePreparation, null);
    assert.equal(f.inspect().runtimeStartPromise, null);
    assert.equal(f.inspect().playerNeedsRestart, false);
    assert.equal(f.count("poll"), 1);
    assert.equal(f.count("terminate"), 0);
    const options = f.calls.find(call => call[0] === "controls")[1];
    assert.equal(options.supported, supported);
    assert.equal(options.playbackState, f.initialState);
    assert.equal(options.player, f.candidate);
    assert.equal(ready.time, 0.25);
    assert.equal(ready.playing, false);
    assert.equal(ready.rebuilt, true);
  });
}

for (const runToken of ["current", "unscoped"]) {
  for (const phase of ["prepare", "start", "state"]) {
    test(`${runToken} ${phase} failure retires the candidate and adopts its replacement canvas`, async () => {
      const f = fixture();
      if (runToken === "unscoped") f.options.runToken = null;
      const error = new Error(`${phase} failed`);
      const result = f.attach(f.options);
      const rejected = assert.rejects(result, value => value === error);
      if (phase === "prepare") f.preparationGate.reject(error);
      else {
        f.preparationGate.resolve();
        await until(() => f.count("start") === 1);
        if (phase === "start") f.startGate.reject(error);
        else {
          f.startGate.resolve({ render: { backend: "WebGL2" } });
          await until(() => f.count("state") === 1);
          f.stateGate.reject(error);
        }
      }
      await rejected;
      assert.equal(f.inspect().player, null);
      assert.equal(f.inspect().preparedPlaybackPlayer, null);
      assert.equal(f.inspect().runtimePreparation, null);
      assert.equal(f.inspect().runtimeStartPromise, null);
      assert.equal(f.count("controls"), 0);
      assert.equal(f.count("poll"), 0);
      assert.deepEqual(f.calls.filter(call => ["terminate", "adopt"].includes(call[0])),
        [["terminate", f.candidate], ["adopt", f.candidate]]);
    });
  }
}

for (const boundary of ["preparation", "first state"]) {
  test(`supersession during ${boundary} cannot publish or replace the newer owner's canvas`, async () => {
    const f = fixture();
    const result = f.attach(f.options);
    const rejected = assert.rejects(result, /superseded source run/);
    if (boundary === "first state") {
      f.preparationGate.resolve();
      await until(() => f.count("start") === 1);
      f.startGate.resolve({ render: { backend: "WebGL2" } });
      await until(() => f.count("state") === 1);
    }
    f.supersede();
    const newerPlayer = {}, newerPreparation = {}, newerPending = Promise.resolve();
    const newerControls = { destroy() { assert.fail("new owner's controls were destroyed"); } };
    f.replace(newerPlayer, newerPreparation, newerControls, newerPending);
    f.resolveAll();
    await rejected;
    assert.equal(f.inspect().player, newerPlayer);
    assert.equal(f.inspect().preparedPlaybackPlayer, newerPlayer);
    assert.equal(f.inspect().runtimePreparation, newerPreparation);
    assert.equal(f.inspect().runtimeStartPromise, newerPending);
    assert.equal(f.inspect().playbackControls, newerControls);
    assert.equal(f.count("terminate"), 1);
    assert.equal(f.count("adopt"), 0);
    assert.equal(f.count("controls"), 0);
    assert.equal(f.count("poll"), 0);
    assert.equal(f.count("start"), boundary === "preparation" ? 0 : 1);
  });
}

for (const preparedPlayback of [true, false]) {
  test(`rejects ${preparedPlayback ? "a prepared" : "an ordinary"} source attached to the wrong profile`, async () => {
    const f = fixture({ preparedPlayback });
    f.initialState.preparedPlayback = !preparedPlayback;
    f.resolveAll();
    await assert.rejects(f.attach(f.options), /profile does not match/);
    assert.equal(f.inspect().player, null);
    assert.equal(f.inspect().preparedPlaybackPlayer, null);
    assert.equal(f.count("controls"), 0);
    assert.equal(f.count("poll"), 0);
    assert.equal(f.count("terminate"), 1);
    assert.equal(f.count("adopt"), 1);
  });
}

test("failure after publication clears only the failed owner's controls and profile", async () => {
  const f = fixture();
  const error = new Error("controls failed");
  f.env.controlsError = error;
  f.resolveAll();
  await assert.rejects(f.attach(f.options), value => value === error);
  assert.equal(f.inspect().player, null);
  assert.equal(f.inspect().preparedPlaybackPlayer, null);
  assert.equal(f.inspect().playbackControls, null);
  assert.equal(f.inspect().runtimeStartPromise, null);
  assert.equal(f.count("destroy-controls"), 1);
  assert.equal(f.count("terminate"), 1);
  assert.equal(f.count("adopt"), 1);
  assert.equal(f.count("poll"), 0);
});

test("concurrent attachment callers join one startup and an attached player is not rebuilt", async () => {
  const f = fixture();
  const first = f.attach(f.options), second = f.attach(f.options);
  f.resolveAll();
  const results = await Promise.all([first, second]);
  assert.equal(results[0], results[1]);
  assert.equal(f.count("start"), 1);
  assert.equal(f.count("state"), 1);
  assert.equal(f.count("controls"), 1);
  assert.equal(await f.attach(f.options), null);
  assert.equal(f.count("start"), 1);
});
