import assert from "node:assert/strict";
import test from "node:test";
import { MessageChannel, MessagePort } from "node:worker_threads";
import { attachSemanticEngine, MAX_PENDING_SEMANTIC_CONTROLS } from "./semantic-engine-endpoint.js";
import { decodeTransferableExecutionDelta, SharedExecutionDeltaReader } from "./execution-transport.js";

globalThis.MessagePort = MessagePort;
const next = (port) => new Promise((resolve) => port.once("message", resolve));
const nextMatching = (port, predicate) => new Promise((resolve) => {
  const receive = (message) => {
    if (!predicate(message)) return;
    port.off("message", receive);
    resolve(message);
  };
  port.on("message", receive);
});
const turn = () => new Promise((resolve) => setImmediate(resolve));
const request = (port, type, requestId, fields = {}) => {
  const result = next(port);
  port.postMessage({ channel: "noon.engine", protocolVersion: 1, type, requestId, ...fields });
  return result;
};
function fixture(
  transportMode = "transferable",
  runRequiredCallbackPhase = null,
  continuation = null,
) {
  const control = new MessageChannel();
  const render = new MessageChannel();
  let time = 0, playing = true, sequence = 0, returned = 0, returnedPlayer = null, stopped = 0;
  let created = 0, resumed = 0, completedSegments = 0, drained = 0, committedPhases = 0;
  let initialSnapshots = 0, resourceBundles = 0;
  const nativeInputs = [];
  const continuationDriveTimes = [];
  const json = () => JSON.stringify({ channel: "noon.execution.retained", protocol_version: 4, session: 7, sequence: sequence++, snapshot: sequence === 1, time, objects: [] });
  const player = {
    initialDeltaJson: () => { initialSnapshots += 1; return json(); },
    initialCallbackPhaseJson: () => null,
    resourceBundleBytes: () => { resourceBundles += 1; return new Uint8Array([1]); },
    tickCallbackPhaseJson: () => null,
    advanceForwardToCallbackPhaseJson: (value) => {
      if (!Number.isFinite(value) || value < time) throw new Error("invalid forward time");
      time = value;
      return null;
    },
    drainDeltaJson: () => { drained += 1; return null; },
    commitCallbackPhaseJson: () => { committedPhases += 1; },
    failCallbackPhaseJson: () => {},
    callbackTerminationJson: () => null,
    tickDeltaJson: () => null,
    seekDeltaJson: (value) => { if (!Number.isFinite(value)) throw new Error("invalid time"); time = value; return json(); },
    setNativeStateInputJson: (value) => { nativeInputs.push({ type: "state", value: JSON.parse(value) }); },
    emitNativeEventJson: (value) => { nativeInputs.push({ type: "event", value: JSON.parse(value) }); },
    liveSegmentWake: () => ({
      presentNow: true,
      cadence: "animation_frame",
      timerAfterMilliseconds: undefined,
    }),
    driveLiveSegmentFromWallTime: (wallTime) => {
      continuationDriveTimes.push(wallTime);
      time = 1;
      return { callbackPhaseJson: null, reachedEndpoint: true };
    },
    completeLiveSegment: () => { completedSegments += 1; },
    setLoopDuration: () => {}, pause: () => { playing = false; }, resume: () => { playing = true; },
    time: () => time, isPlaying: () => playing,
  };
  const context = {
    createExecutionPlayer: () => { created += 1; return player; },
    resumeExecutionPlayer: () => { resumed += 1; return player; },
    returnExecutionPlayer: (value) => { returned += 1; returnedPlayer = value; },
    liveHandoffDuration: () => Math.max(time, 1),
  };
  return { control, render, player, stats: () => ({
    returned, returnedPlayer, stopped, nativeInputs, created, resumed, completedSegments,
    initialSnapshots, resourceBundles, continuationDriveTimes, drained, committedPhases,
  }),
    attach: () => attachSemanticEngine(context, {
      controlPort: control.port1, renderPort: render.port1, session: 7,
      loopDurationSeconds: 2, transportMode,
    }, () => { stopped += 1; }, runRequiredCallbackPhase, continuation),
    close: () => { control.port1.close(); control.port2.close(); render.port1.close(); render.port2.close(); },
  };
}

test("semantic producer installs mixed resources before its retained snapshot and supports controls", async () => {
  const f = fixture();
  try {
    const ready = next(f.control.port2);
    const resources = nextMatching(f.render.port2, (message) => message.type === "retained_resources");
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    const endpoint = await f.attach();
    assert.equal((await ready).type, "ready");
    const resourceBundle = await resources;
    assert.equal(resourceBundle.type, "retained_resources");
    assert.deepEqual([...resourceBundle.bytes], [1]);
    const delta = await initial;
    assert.equal(JSON.parse(decodeTransferableExecutionDelta(delta).json).snapshot, true);
    f.render.port2.postMessage({ type: "execution_ack", session: delta.session, sequence: delta.sequence });
    assert.equal((await request(f.control.port2, "pause", 1)).playing, false);
    assert.equal((await request(f.control.port2, "resume", 2)).playing, true);
    const changed = next(f.render.port2);
    assert.equal((await request(f.control.port2, "seek", 3, { time: 0.5 })).time, 0.5);
    assert.equal(JSON.parse(decodeTransferableExecutionDelta(await changed).json).time, 0.5);
    assert.equal((await request(f.control.port2, "apply_patch", 4)).type, "error");
    endpoint.stop(); endpoint.stop();
    assert.equal(f.stats().returned, 1);
    assert.equal(f.stats().returnedPlayer, f.player);
    assert.equal(f.stats().stopped, 1);
  } finally { f.close(); }
});

test("semantic continuation returns one completed player before resuming and retakes it later", async () => {
  const completions = [];
  const failures = [];
  const continuation = {
    generation: 9,
    onComplete: (generation) => { completions.push(generation); },
    onError: (generation, error) => { failures.push({ generation, error }); },
  };
  const f = fixture("transferable", null, continuation);
  let endpoint;
  try {
    const wakes = [];
    f.render.port2.on("message", (message) => {
      if (message.type === "execution_wake") wakes.push(message.cadence);
    });
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    endpoint = await f.attach();
    await ready;
    const initialDelta = await initial;
    f.render.port2.postMessage({
      type: "execution_presented",
      session: initialDelta.session,
      sequence: initialDelta.sequence,
    });
    const paused = await request(f.control.port2, "pause", 80);
    assert.equal(paused.type, "error");
    assert.match(paused.message, /Python source continuation owns execution/);
    const sought = await request(f.control.port2, "seek", 81, { time: 0.75 });
    assert.equal(sought.type, "error");
    assert.match(sought.message, /Python source continuation owns execution/);
    assert.equal(f.player.isPlaying(), true, "rejected pause must not change presentation state");
    assert.equal(f.player.time(), 0, "rejected seek must not bypass the live segment barrier");
    const foreignWorkerTimestamp = 9_000_000_000;
    f.render.port2.postMessage({ type: "tick", timestamp: foreignWorkerTimestamp });
    await turn();
    await turn();
    assert.deepEqual(completions, [9]);
    assert.deepEqual(failures, []);
    assert.equal(f.stats().completedSegments, 1);
    assert.equal(f.stats().returned, 1);
    assert.equal(f.stats().returnedPlayer, f.player);
    assert.notEqual(
      f.stats().continuationDriveTimes[0],
      foreignWorkerTimestamp,
      "render ticks must not supply the authoring worker's continuation clock",
    );
    assert.deepEqual(wakes, ["animation_frame", "idle"]);
    const idleState = await request(f.control.port2, "state", 90);
    assert.equal(idleState.time, 1);
    assert.equal(idleState.playing, false);
    const idleResume = await request(f.control.port2, "resume", 91);
    assert.equal(idleResume.type, "error");
    assert.match(idleResume.message, /Python source continuation owns execution/);

    endpoint.startContinuation(9);
    assert.equal(f.stats().created, 1, "only the first attachment may bootstrap transport");
    assert.equal(f.stats().resumed, 1, "later await must retake the returned player");
    assert.equal(f.stats().resourceBundles, 1);
    assert.equal(f.stats().initialSnapshots, 1);
    await turn();
    assert.equal(wakes.at(-1), "animation_frame");
    f.render.port2.postMessage({ type: "tick", timestamp: 32 });
    await turn();
    await turn();
    assert.deepEqual(completions, [9, 9]);
    assert.equal(f.stats().completedSegments, 2);
    assert.equal(f.stats().returned, 2);
    assert.equal(wakes.at(-1), "idle");
    assert.throws(() => endpoint.startContinuation(8), /stale semantic continuation generation/);
  } finally { endpoint?.stop(); f.close(); }
});

test("semantic continuation services Rust callback barriers before endpoint publication", async () => {
  const callbacks = [];
  const completions = [];
  const f = fixture("transferable", async (phase) => {
    callbacks.push(phase);
    return JSON.stringify({ token: phase.token, writes: [] });
  }, {
    generation: 17,
    onComplete: (generation) => completions.push(generation),
    onError: (_generation, error) => { throw error; },
  });
  let endpoint;
  try {
    let step = 0;
    f.player.driveLiveSegmentFromWallTime = (wallTime) => {
      f.stats().continuationDriveTimes.push(wallTime);
      if (step++ === 0) {
        return {
          callbackPhaseJson: JSON.stringify({
            token: { runtime: 3, publication: { scene: 1 }, sequence: 4 },
            invocations: [{ callback_id: 9 }],
          }),
          reachedEndpoint: false,
        };
      }
      return { callbackPhaseJson: null, reachedEndpoint: true };
    };
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    endpoint = await f.attach();
    await ready;
    const initialDelta = await initial;
    f.render.port2.postMessage({
      type: "execution_presented",
      session: initialDelta.session,
      sequence: initialDelta.sequence,
    });
    f.render.port2.postMessage({ type: "tick", timestamp: 1 });
    await turn();
    await turn();

    assert.equal(callbacks.length, 1);
    assert.equal(f.stats().committedPhases, 1);
    assert.equal(f.stats().completedSegments, 1);
    assert.deepEqual(completions, [17]);
    assert.equal(f.stats().drained, 2, "only ready endpoint and completion may publish");
    assert.equal(f.stats().continuationDriveTimes.length, 2);
    assert.equal(
      f.stats().continuationDriveTimes[0],
      f.stats().continuationDriveTimes[1],
      "every phase retry preserves the one captured wall timestamp",
    );
  } finally { endpoint?.stop(); f.close(); }
});

test("semantic continuation drives a pure wait only when its Rust deadline is due", async () => {
  const completions = [];
  const f = fixture("transferable", null, {
    generation: 4,
    onComplete: (generation) => { completions.push(generation); },
    onError: (_generation, error) => { throw error; },
  });
  let endpoint;
  try {
    let wakeCount = 0;
    f.player.liveSegmentWake = () => ({
      presentNow: false,
      cadence: "timer",
      timerAfterMilliseconds: wakeCount++ < 2 ? 1_000 : 0,
    });
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    const initialWake = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_wake",
    );
    endpoint = await f.attach();
    await ready;
    assert.deepEqual(await initialWake, {
      type: "execution_wake",
      cadence: "timer",
      timerAfterMilliseconds: 1_000,
    });
    const initialDelta = await initial;
    f.render.port2.postMessage({
      type: "execution_presented",
      session: initialDelta.session,
      sequence: initialDelta.sequence,
    });

    const rearmedWake = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_wake" && message.cadence === "timer",
    );
    f.render.port2.postMessage({ type: "tick", timestamp: 16 });
    await turn();
    assert.deepEqual(completions, []);
    assert.equal(f.stats().completedSegments, 0);
    assert.deepEqual(await rearmedWake, {
      type: "execution_wake",
      cadence: "timer",
      timerAfterMilliseconds: 1_000,
    });

    f.render.port2.postMessage({ type: "tick", timestamp: 1_016 });
    await turn();
    await turn();
    assert.deepEqual(completions, [4]);
    assert.equal(f.stats().completedSegments, 1);
  } finally { endpoint?.stop(); f.close(); }
});

test("stopping an incomplete semantic continuation never returns its player", async () => {
  const failures = [];
  const continuation = {
    generation: 12,
    onComplete: () => { throw new Error("incomplete continuation must not complete"); },
    onError: (generation, error) => { failures.push({ generation, error }); },
  };
  const f = fixture("transferable", null, continuation);
  try {
    const ready = next(f.control.port2);
    const endpoint = await f.attach();
    await ready;
    endpoint.stop();
    assert.equal(f.stats().returned, 0);
    assert.equal(f.stats().completedSegments, 0);
    assert.equal(f.stats().stopped, 1);
    assert.deepEqual(failures, []);
  } finally { f.close(); }
});

test("native state and event controls reach the leased player in accepted order", async () => {
  const f = fixture();
  let endpoint;
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach();
    await ready;

    const state = await request(f.control.port2, "native_state_input", 20, {
      source: { kind: "control", name: "opacity" },
      value: { kind: "scalar", value: 0.75 },
    });
    assert.equal(state.type, "native_state_input");
    const event = await request(f.control.port2, "native_event", 21, {
      source: { kind: "pointer_down", button: 0 },
    });
    assert.equal(event.type, "native_event");
    assert.deepEqual(f.stats().nativeInputs, [
      {
        type: "state",
        value: {
          source: { kind: "control", name: "opacity" },
          value: { kind: "scalar", value: 0.75 },
        },
      },
      { type: "event", value: { source: { kind: "pointer_down", button: 0 } } },
    ]);

    f.player.setNativeStateInputJson = () => { throw new Error("native value rejected"); };
    const rejected = await request(f.control.port2, "native_state_input", 22, {
      source: { kind: "control", name: "opacity" },
      value: { kind: "bool", value: true },
    });
    assert.equal(rejected.type, "error");
    assert.match(rejected.message, /native value rejected/);
    assert.equal(f.stats().nativeInputs.length, 2);
  } finally { endpoint?.stop(); f.close(); }
});

test("forward authored-time control commits its callback phase before its matching publication presents", async () => {
  let callbackFrames = 0;
  let committed = 0;
  const f = fixture("transferable", async (phase) => {
    callbackFrames += 1;
    assert.equal(phase.time, 1.0);
    return JSON.stringify({ token: phase.token, writes: [] });
  });
  let endpoint;
  try {
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    endpoint = await f.attach();
    await ready;
    const initialDelta = await initial;
    f.render.port2.postMessage({
      type: "execution_ack",
      session: initialDelta.session,
      sequence: initialDelta.sequence,
    });
    f.player.pause();
    const advanceForward = f.player.advanceForwardToCallbackPhaseJson;
    let advances = 0;
    f.player.advanceForwardToCallbackPhaseJson = (time) => {
      assert.equal(time, 1.0);
      advanceForward(time);
      if (advances++ === 0) {
        return JSON.stringify({ token: { sequence: "1" }, time });
      }
      return null;
    };
    f.player.commitCallbackPhaseJson = (batch) => {
      assert.equal(batch, '{"token":{"sequence":"1"},"writes":[]}');
      committed += 1;
    };
    f.player.drainDeltaJson = () => f.player.initialDeltaJson();
    const advancedDelta = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_delta" && message.sequence !== initialDelta.sequence,
    );
    const advance = request(f.control.port2, "advance_to", 30, { time: 1.0 });
    const delta = await advancedDelta;
    let settled = false;
    advance.then(() => { settled = true; });
    await turn();
    assert.equal(callbackFrames, 1);
    assert.equal(committed, 1);
    assert.equal(settled, false, "control must wait for renderer presentation");
    f.render.port2.postMessage({
      type: "execution_presented",
      session: delta.session,
      sequence: delta.sequence,
    });
    assert.equal((await advance).time, 1.0);
    endpoint.stop();
  } finally { endpoint?.stop(); f.close(); }
});

test("forward authored-time control crosses every required barrier before publishing its requested frame", async () => {
  const callbackTimes = [];
  const committedTokens = [];
  const f = fixture("transferable", async (phase) => {
    callbackTimes.push(phase.time);
    return JSON.stringify({ token: phase.token, writes: [] });
  });
  let endpoint;
  try {
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    endpoint = await f.attach();
    await ready;
    const initialDelta = await initial;
    f.render.port2.postMessage({
      type: "execution_ack",
      session: initialDelta.session,
      sequence: initialDelta.sequence,
    });
    f.player.pause();
    const advanceForward = f.player.advanceForwardToCallbackPhaseJson;
    const barriers = [1, 2];
    f.player.advanceForwardToCallbackPhaseJson = (requested) => {
      const barrier = barriers.shift();
      advanceForward(barrier ?? requested);
      return barrier === undefined
        ? null
        : JSON.stringify({ token: { sequence: String(barrier) }, time: barrier });
    };
    f.player.commitCallbackPhaseJson = (batch) => {
      committedTokens.push(JSON.parse(batch).token.sequence);
    };
    f.player.drainDeltaJson = () => f.player.initialDeltaJson();
    const finalDelta = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_delta" && message.sequence !== initialDelta.sequence,
    );
    const advanced = request(f.control.port2, "advance_to", 33, { time: 3 });
    const delta = await finalDelta;
    let settled = false;
    advanced.then(() => { settled = true; });
    await turn();
    assert.deepEqual(callbackTimes, [1, 2]);
    assert.deepEqual(committedTokens, ["1", "2"]);
    assert.equal(settled, false, "must not resolve at an earlier callback barrier");
    f.render.port2.postMessage({
      type: "execution_presented",
      session: delta.session,
      sequence: delta.sequence,
    });
    assert.equal((await advanced).time, 3);
    endpoint.stop();
  } finally { endpoint?.stop(); f.close(); }
});

test("forward authored-time control accepts an unchanged already coherent frame without a redraw", async () => {
  const f = fixture();
  let endpoint;
  try {
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    endpoint = await f.attach();
    await ready;
    const initialDelta = await initial;
    f.render.port2.postMessage({
      type: "execution_ack",
      session: initialDelta.session,
      sequence: initialDelta.sequence,
    });
    f.render.port2.postMessage({
      type: "execution_presented",
      session: initialDelta.session,
      sequence: initialDelta.sequence,
    });
    const advanceForward = f.player.advanceForwardToCallbackPhaseJson;
    f.player.advanceForwardToCallbackPhaseJson = (time) => {
      assert.equal(time, 0.5);
      advanceForward(time);
      return null;
    };
    f.player.drainDeltaJson = () => null;
    const whilePlaying = await request(f.control.port2, "advance_to", 31, { time: 0.5 });
    assert.equal(whilePlaying.type, "error");
    assert.match(whilePlaying.message, /pause semantic execution/);
    f.player.pause();
    assert.equal((await request(f.control.port2, "advance_to", 31, { time: 0.5 })).time, 0.5);
    endpoint.stop();
  } finally { endpoint?.stop(); f.close(); }
});

test("unchanged control waits for the initial publication's exact presentation", async () => {
  const f = fixture();
  let endpoint;
  try {
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    endpoint = await f.attach();
    await ready;
    const initialDelta = await initial;
    f.player.pause();
    f.player.drainDeltaJson = () => null;

    const result = nextMatching(f.control.port2, (message) => message.requestId === 34);
    f.control.port2.postMessage({
      channel: "noon.engine",
      protocolVersion: 1,
      type: "advance_to",
      requestId: 34,
      time: 0.5,
    });
    let settled = false;
    result.then(() => { settled = true; });
    await turn();
    assert.equal(settled, false, "unchanged control must retain the initial presentation barrier");

    const invalid = nextMatching(
      f.control.port2,
      (message) => message.type === "error" && message.requestId === null,
    );
    f.render.port2.postMessage({
      type: "execution_presented",
      session: initialDelta.session + 1,
      sequence: initialDelta.sequence,
    });
    assert.match((await invalid).message, /invalid execution publication/);
    await turn();
    assert.equal(settled, false, "a foreign-session presentation must not release the barrier");

    f.render.port2.postMessage({
      type: "execution_presented",
      session: initialDelta.session,
      sequence: initialDelta.sequence,
    });
    assert.equal((await result).time, 0.5);
  } finally { endpoint?.stop(); f.close(); }
});

test("unchanged control waits for a previously sent publication still pending presentation", async () => {
  const f = fixture();
  let endpoint;
  try {
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    endpoint = await f.attach();
    await ready;
    const initialDelta = await initial;
    f.render.port2.postMessage({
      type: "execution_ack",
      session: initialDelta.session,
      sequence: initialDelta.sequence,
    });
    f.render.port2.postMessage({
      type: "execution_presented",
      session: initialDelta.session,
      sequence: initialDelta.sequence,
    });

    const changed = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_delta" && message.sequence !== initialDelta.sequence,
    );
    const seek = request(f.control.port2, "seek", 35, { time: 0.5 });
    const changedDelta = await changed;
    assert.equal((await seek).time, 0.5);
    f.player.pause();
    f.player.drainDeltaJson = () => null;

    const advance = request(f.control.port2, "advance_to", 36, { time: 0.5 });
    let settled = false;
    advance.then(() => { settled = true; });
    await turn();
    assert.equal(settled, false, "unchanged control must wait for the preceding seek publication");
    f.render.port2.postMessage({
      type: "execution_presented",
      session: initialDelta.session,
      sequence: initialDelta.sequence,
    });
    await turn();
    assert.equal(settled, false, "an older presentation must not release the newer barrier");
    f.render.port2.postMessage({
      type: "execution_presented",
      session: changedDelta.session,
      sequence: changedDelta.sequence,
    });
    assert.equal((await advance).time, 0.5);
  } finally { endpoint?.stop(); f.close(); }
});

test("forward authored-time control rejects when the renderer fails before presentation", async () => {
  const f = fixture();
  let endpoint;
  try {
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    endpoint = await f.attach();
    await ready;
    const initialDelta = await initial;
    f.render.port2.postMessage({
      type: "execution_ack",
      session: initialDelta.session,
      sequence: initialDelta.sequence,
    });
    f.player.pause();
    f.player.drainDeltaJson = () => f.player.initialDeltaJson();
    const advancedDelta = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_delta" && message.sequence !== initialDelta.sequence,
    );
    const result = request(f.control.port2, "advance_to", 32, { time: 0.5 });
    await advancedDelta;
    f.render.port2.postMessage({ type: "render_error", message: "presentation failed" });
    const rejected = await result;
    assert.equal(rejected.type, "error");
    assert.match(rejected.message, /presentation failed/);
    endpoint.stop();
  } finally { endpoint?.stop(); f.close(); }
});

test("initial snapshot failure returns the exact player for retry", async () => {
  const f = fixture();
  f.player.initialDeltaJson = () => { throw new Error("snapshot failed"); };
  try {
    await assert.rejects(f.attach(), /snapshot failed/);
    assert.equal(f.stats().returned, 1);
    assert.equal(f.stats().returnedPlayer, f.player);
    assert.equal(f.stats().stopped, 1);
  } finally { f.close(); }
});

test("player construction failure closes both transferred ports", async () => {
  const control = new MessageChannel();
  const render = new MessageChannel();
  let closed = 0;
  for (const port of [control.port1, render.port1]) {
    const close = port.close.bind(port);
    port.close = () => { closed += 1; close(); };
  }
  let stopped = 0;
  await assert.rejects(attachSemanticEngine({
    createExecutionPlayer: () => { throw new Error("lowering failed"); },
    returnExecutionPlayer: () => { throw new Error("must not return an uncreated player"); },
  }, {
    controlPort: control.port1, renderPort: render.port1, session: 7,
    loopDurationSeconds: 2, transportMode: "transferable",
  }, () => { stopped += 1; }), /lowering failed/);
  assert.equal(closed, 2);
  assert.equal(stopped, 1);
  control.port2.close();
  render.port2.close();
});


test("shared setup cannot expose a snapshot before its resource bundle", async () => {
  const f = fixture("shared");
  let endpoint;
  try {
    const received = [];
    const setup = new Promise((resolve, reject) => {
      f.render.port2.on("message", (message) => {
        received.push(message.type);
        if (message.type !== "transport_setup") return;
        try {
          assert.deepEqual(received, ["retained_resources", "transport_setup"]);
          const reader = new SharedExecutionDeltaReader(message.mailbox);
          const snapshots = [];
          reader.drain((json) => { snapshots.push(JSON.parse(json)); return true; });
          assert.equal(snapshots.length, 1);
          assert.equal(snapshots[0].snapshot, true);
          resolve();
        } catch (error) { reject(error); }
      });
    });
    endpoint = await f.attach();
    await setup;
  } finally { endpoint?.stop(); f.close(); }
});

test("required initial callback withholds the first delta until its exact batch commits", async () => {
  let resolvePhase;
  const phase = new Promise((resolve) => { resolvePhase = resolve; });
  const f = fixture("transferable", () => phase);
  let committed = 0;
  try {
    f.player.initialCallbackPhaseJson = () => JSON.stringify({ token: { sequence: "0" } });
    f.player.commitCallbackPhaseJson = (batch) => {
      assert.equal(batch, "{\"token\":{\"sequence\":\"0\"},\"writes\":[]}");
      committed += 1;
    };
    const resources = nextMatching(f.render.port2, (message) => message.type === "retained_resources");
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    const attached = f.attach();
    await resources;
    let early = false;
    initial.then(() => { early = true; });
    await turn();
    assert.equal(early, false);
    assert.equal(committed, 0);
    resolvePhase("{\"token\":{\"sequence\":\"0\"},\"writes\":[]}");
    const endpoint = await attached;
    assert.equal(committed, 1);
    const delta = await initial;
    assert.equal(JSON.parse(decodeTransferableExecutionDelta(delta).json).snapshot, true);
    endpoint.stop();
  } finally { f.close(); }
});

test("stopping an attachment discards a late callback result before returning its player", async () => {
  let resolvePhase;
  let phaseStarted;
  const phase = new Promise((resolve) => { resolvePhase = resolve; });
  const began = new Promise((resolve) => { phaseStarted = resolve; });
  const f = fixture("transferable", () => {
    phaseStarted();
    return phase;
  });
  let committed = 0;
  let failed = 0;
  try {
    const ready = next(f.control.port2);
    const endpoint = await f.attach();
    await ready;
    const initial = await nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    f.render.port2.postMessage({ type: "execution_ack", session: initial.session, sequence: initial.sequence });
    f.player.tickCallbackPhaseJson = () => JSON.stringify({ token: { sequence: "1" } });
    f.player.commitCallbackPhaseJson = () => { committed += 1; };
    f.player.failCallbackPhaseJson = () => { failed += 1; };
    f.render.port2.postMessage({ type: "tick", timestamp: 16 });
    await began;
    endpoint.stop();
    resolvePhase("{\"token\":{\"sequence\":\"1\"},\"writes\":[]}");
    await turn();
    assert.equal(committed, 0);
    assert.equal(failed, 1);
    assert.equal(f.stats().returned, 1);
  } finally { f.close(); }
});

test("a callback failure latches the endpoint and never invokes the opaque callback again", async () => {
  let invocations = 0;
  const failure = new Error("opaque callback failed");
  const f = fixture("transferable", async () => {
    invocations += 1;
    throw failure;
  });
  let failed = 0;
  try {
    const ready = next(f.control.port2);
    const endpoint = await f.attach();
    await ready;
    const initial = await nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    f.render.port2.postMessage({ type: "execution_ack", session: initial.session, sequence: initial.sequence });
    f.player.tickCallbackPhaseJson = () => JSON.stringify({ token: { sequence: "2" } });
    f.player.failCallbackPhaseJson = () => { failed += 1; };

    f.render.port2.postMessage({ type: "tick", timestamp: 16 });
    await nextMatching(f.control.port2, (message) => message.type === "error" && /opaque callback failed/.test(message.message));
    f.render.port2.postMessage({ type: "tick", timestamp: 32 });
    await turn();
    assert.equal(invocations, 1);
    assert.equal(failed, 1);
    endpoint.stop();
  } finally { f.close(); }
});

test("a typed callback-advance failure is surfaced once and never retried", async () => {
  let ticks = 0;
  const f = fixture("transferable", async () => {
    throw new Error("callback should not run after advance failure");
  });
  let endpoint;
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach();
    await ready;
    f.player.tickCallbackPhaseJson = () => {
      ticks += 1;
      throw new Error("unsupported required callback target");
    };

    f.render.port2.postMessage({ type: "tick", timestamp: 16 });
    await nextMatching(
      f.control.port2,
      (message) => message.type === "error" && /unsupported required callback target/.test(message.message),
    );
    f.render.port2.postMessage({ type: "tick", timestamp: 32 });
    await turn();

    assert.equal(ticks, 1);
  } finally { endpoint?.stop(); f.close(); }
});

test("full transport queues controls until a writable event without recursive draining", async () => {
  const f = fixture();
  try {
    const ready = next(f.control.port2);
    const endpoint = await f.attach();
    await ready;
    const initial = await nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    f.player.drainDeltaJson = () => f.player.initialDeltaJson();
    const tick = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    f.render.port2.postMessage({ type: "tick", timestamp: 16 });
    const second = await tick;
    const paused = request(f.control.port2, "pause", 8);
    await turn();
    f.render.port2.postMessage({ type: "execution_ack", session: initial.session, sequence: initial.sequence });
    assert.equal((await paused).playing, false);
    f.render.port2.postMessage({ type: "execution_ack", session: second.session, sequence: second.sequence });
    endpoint.stop();
  } finally { f.close(); }
});

test("stalled native-event queue rejects overflow and preserves accepted command order", async () => {
  const f = fixture();
  try {
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    const endpoint = await f.attach();
    await ready;
    const first = await initial;
    f.player.drainDeltaJson = () => f.player.initialDeltaJson();
    const tick = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    f.render.port2.postMessage({ type: "tick", timestamp: 16 });
    await tick;
    f.player.drainDeltaJson = () => null;
    const accepted = [];
    f.control.port2.on("message", (message) => {
      if (message.type === "native_event") accepted.push(message.requestId);
    });
    const rejected = nextMatching(f.control.port2, (message) => message.type === "error");
    for (let id = 1; id <= MAX_PENDING_SEMANTIC_CONTROLS + 1; id += 1) {
      f.control.port2.postMessage({
        channel: "noon.engine", protocolVersion: 1, type: "native_event", requestId: id,
        source: { kind: "control_commit", name: `control-${id}` },
      });
    }
    const overflow = await rejected;
    assert.equal(overflow.requestId, MAX_PENDING_SEMANTIC_CONTROLS + 1);
    assert.match(overflow.message, /control queue is full/);
    assert.deepEqual(accepted, []);
    const drained = nextMatching(f.control.port2,
      (message) => message.requestId === MAX_PENDING_SEMANTIC_CONTROLS);
    f.render.port2.postMessage({ type: "execution_ack", session: first.session, sequence: first.sequence });
    await drained;
    assert.deepEqual(accepted, Array.from({ length: MAX_PENDING_SEMANTIC_CONTROLS }, (_, index) => index + 1));
    assert.deepEqual(
      f.stats().nativeInputs.map(({ value }) => value.source.name),
      Array.from({ length: MAX_PENDING_SEMANTIC_CONTROLS }, (_, index) => `control-${index + 1}`),
    );
    endpoint.stop();
  } finally { f.close(); }
});
