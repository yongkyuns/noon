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
  requestOptions = {},
) {
  const control = new MessageChannel();
  const render = new MessageChannel();
  let time = 0, playing = true, sequence = 0, returned = 0, returnedPlayer = null, stopped = 0;
  let created = 0, resumed = 0, completedSegments = 0, drained = 0, committedPhases = 0;
  let leased = false;
  let initialSnapshots = 0, resourceBundles = 0;
  const callbackReads = [];
  const nativeInputs = [];
  const continuationDriveTimes = [];
  const authoredSampleTimes = [];
  const executionWakeTimes = [];
  const json = () => JSON.stringify({ channel: "noon.execution.retained", protocol_version: 4, session: 7, sequence: sequence++, snapshot: sequence === 1, time, objects: [] });
  const player = {
    sealReplay: () => {},
    initialDeltaJson: () => { initialSnapshots += 1; return json(); },
    debugFrameJson: () => JSON.stringify({ time, source: "active" }),
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
    drainRendererObservationPublicationJson: () => {
      throw new Error("fixture did not configure a renderer observation publication");
    },
    requiredCallbackReadJson: (token, request) => {
      callbackReads.push({ token, request });
      return JSON.stringify({ kind: "scalar", value: 3 });
    },
    failCallbackPhaseJson: () => {},
    callbackTerminationJson: () => null,
    tickDeltaJson: () => null,
    executionWake: (wallTime) => {
      executionWakeTimes.push(wallTime);
      return {
        presentNow: false,
        cadence: playing ? "animation_frame" : "idle",
        timerAfterMilliseconds: undefined,
      };
    },
    seekDeltaJson: (value) => { if (!Number.isFinite(value)) throw new Error("invalid time"); time = value; return json(); },
    setNativeStateInputJson: (value) => { nativeInputs.push({ type: "state", value: JSON.parse(value) }); },
    emitNativeEventJson: (value) => { nativeInputs.push({ type: "event", value: JSON.parse(value) }); },
    submitBrowserPointerInputJson: (value) => {
      nativeInputs.push({ type: "pointer", value: JSON.parse(value) });
    },
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
    driveLiveSegmentToAuthoredTime: (value) => {
      authoredSampleTimes.push(value);
      if (!Number.isFinite(value) || value < time) throw new Error("invalid external sample");
      time = value;
      return { callbackPhaseJson: null, reachedEndpoint: false };
    },
    completeLiveSegment: () => { completedSegments += 1; },
    setLoopDuration: () => {}, pause: () => { playing = false; }, resume: () => { playing = true; },
    time: () => time, playbackTimeAt: () => time, isPlaying: () => playing,
  };
  const context = {
    createExecutionPlayer: () => { leased = true; created += 1; return player; },
    resumeExecutionPlayer: () => { leased = true; resumed += 1; return player; },
    returnExecutionPlayer: (value) => { leased = false; returned += 1; returnedPlayer = value; },
    liveHandoffDuration: () => leased ? undefined : Math.max(time, 1),
    liveDebugFrameJson: () => JSON.stringify({ time, source: "returned" }),
    drainReturnedPublicationJson: () => player.drainDeltaJson(),
  };
  return { control, render, player, context, stats: () => ({
    returned, returnedPlayer, stopped, nativeInputs, created, resumed, completedSegments,
    initialSnapshots, resourceBundles, continuationDriveTimes, executionWakeTimes,
    authoredSampleTimes,
    drained, committedPhases,
    callbackReads,
  }),
    attach: () => attachSemanticEngine(context, {
      controlPort: control.port1, renderPort: render.port1, session: 7,
      loopDurationSeconds: 2, transportMode, ...requestOptions,
    }, () => { stopped += 1; }, runRequiredCallbackPhase, continuation),
    close: () => { control.port1.close(); control.port2.close(); render.port1.close(); render.port2.close(); },
  };
}

async function prepareRendererObservationFixture(f, invocations) {
  const ready = next(f.control.port2);
  const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
  const endpoint = await f.attach();
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
  f.player.pause();
  const advanceForward = f.player.advanceForwardToCallbackPhaseJson;
  let phasePending = true;
  const phase = { token: { sequence: "1" }, time: 1, invocations };
  f.player.advanceForwardToCallbackPhaseJson = (time) => {
    advanceForward(time);
    if (!phasePending) return null;
    phasePending = false;
    return JSON.stringify(phase);
  };
  f.player.drainRendererObservationPublicationJson = (phaseJson, slot, generation) => {
    assert.deepEqual(JSON.parse(phaseJson), phase);
    assert.deepEqual({ slot, generation }, invocations[0].target);
    const delta = JSON.parse(f.player.initialDeltaJson());
    return JSON.stringify({
      delta,
      observation: {
        schema_version: 1,
        publication: { session: delta.session, sequence: delta.sequence },
        slot: { slot, generation },
        committed: {},
      },
    });
  };
  return {
    endpoint,
    begin(requestId) {
      const observationRequest = nextMatching(
        f.render.port2,
        (message) => message.type === "renderer_observation_request",
      );
      const publicationMessage = nextMatching(
        f.render.port2,
        (message) => message.type === "execution_delta" &&
          message.sequence !== initialDelta.sequence,
      );
      const advanced = nextMatching(
        f.control.port2,
        (message) => message.requestId === requestId,
      );
      f.control.port2.postMessage({
        channel: "noon.engine",
        protocolVersion: 1,
        type: "advance_to",
        requestId,
        time: 1,
        observeRenderer: true,
      });
      return { observationRequest, publicationMessage, advanced };
    },
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
    const changed = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_delta",
    );
    assert.equal((await request(f.control.port2, "seek", 3, { time: 0.5 })).time, 0.5);
    assert.equal(JSON.parse(decodeTransferableExecutionDelta(await changed).json).time, 0.5);
    assert.equal((await request(f.control.port2, "apply_patch", 4)).type, "error");
    endpoint.stop(); endpoint.stop();
    assert.equal(f.stats().returned, 1);
    assert.equal(f.stats().returnedPlayer, f.player);
    assert.equal(f.stats().stopped, 1);
  } finally { f.close(); }
});

test("initially paused semantic execution presents time zero without automatic advancement", async () => {
  const f = fixture("transferable", null, null, { initiallyPaused: true });
  let endpoint;
  try {
    let tickCalls = 0;
    f.player.tickCallbackPhaseJson = (timestamp) => {
      tickCalls += 1;
      if (f.player.isPlaying()) f.player.seekDeltaJson(timestamp / 1_000);
      return null;
    };
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    const initialWake = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_wake",
    );
    endpoint = await f.attach();
    await ready;
    const delta = await initial;
    assert.equal((await initialWake).cadence, "idle");
    assert.equal(JSON.parse(decodeTransferableExecutionDelta(delta).json).time, 0);
    assert.equal(f.player.isPlaying(), false);
    f.render.port2.postMessage({
      type: "execution_presented",
      session: delta.session,
      sequence: delta.sequence,
    });

    f.render.port2.postMessage({ type: "tick", timestamp: 500 });
    await turn();
    await turn();
    assert.equal(f.player.time(), 0, "renderer wakes must not advance an initially paused player");
    assert.equal(tickCalls, 1, "only the deliberately injected tick reaches the paused player");

    const advanced = await request(f.control.port2, "advance_to", 79, { time: 0.25 });
    assert.equal(advanced.time, 0.25);
    assert.equal(advanced.playing, false);

    const resumedWake = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_wake" && message.cadence === "animation_frame",
    );
    assert.equal((await request(f.control.port2, "resume", 80)).playing, true);
    assert.equal((await resumedWake).cadence, "animation_frame");
    const pausedWake = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_wake" && message.cadence === "idle",
    );
    assert.equal((await request(f.control.port2, "pause", 81)).playing, false);
    assert.equal((await pausedWake).cadence, "idle");
  } finally { endpoint?.stop(); f.close(); }
});

test("playing static execution obeys Rust idle cadence instead of polling isPlaying", async () => {
  const f = fixture();
  let endpoint;
  let tickCalls = 0;
  try {
    f.player.executionWake = () => ({
      presentNow: false,
      cadence: "idle",
      timerAfterMilliseconds: undefined,
    });
    f.player.tickCallbackPhaseJson = () => { tickCalls += 1; return null; };
    const ready = next(f.control.port2);
    const wake = nextMatching(f.render.port2, (message) => message.type === "execution_wake");
    endpoint = await f.attach();
    await ready;
    assert.equal(f.player.isPlaying(), true);
    assert.deepEqual(await wake, {
      type: "execution_wake",
      cadence: "idle",
      timerAfterMilliseconds: null,
    });
    await turn();
    assert.equal(tickCalls, 0, "a clean static player receives no synthetic engine tick");
  } finally { endpoint?.stop(); f.close(); }
});

test("renderer timestamps admit generic ticks without becoming the playback clock", async () => {
  const f = fixture();
  let endpoint;
  const driven = [];
  try {
    f.player.tickCallbackPhaseJson = (wallTime) => { driven.push(wallTime); return null; };
    const ready = next(f.control.port2);
    endpoint = await f.attach();
    await ready;
    const foreignRendererTime = 9_000_000_000;
    f.render.port2.postMessage({ type: "tick", timestamp: foreignRendererTime });
    await turn();
    await turn();
    assert.equal(driven.length, 1);
    assert.notEqual(driven[0], foreignRendererTime);
    const wakeTimes = f.stats().executionWakeTimes;
    assert.ok(wakeTimes.length >= 2);
    assert.ok(Math.abs(wakeTimes.at(-1) - driven[0]) < 1_000);
  } finally { endpoint?.stop(); f.close(); }
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

test("callback sparse reads are pinned to the pending phase and never publish it", async () => {
  let resolveCallback;
  let readPhase = null;
  const phase = {
    token: { runtime: 3, publication: { scene: 1, execution: 2, frame: 3 }, sequence: 4 },
    invocations: [{ callback_id: 9 }],
  };
  const f = fixture("transferable", () => new Promise((resolve) => { resolveCallback = resolve; }), {
    generation: 41,
    onComplete: () => {},
    onError: (_generation, error) => { throw error; },
    onCallbackReadAvailable: (read) => { readPhase = read; },
  });
  let endpoint;
  try {
    f.player.initialCallbackPhaseJson = () => JSON.stringify(phase);
    const attaching = f.attach().then((value) => { endpoint = value; return value; });
    await turn();
    assert.equal(typeof readPhase, "function", "read service is available before the initial callback runs");
    const token = JSON.stringify(phase.token);
    const result = readPhase(token, {
      request_id: 11,
      kind: "scalar_signal",
      node: { slot: 8, generation: 2 },
    });
    assert.equal(result, JSON.stringify({ kind: "scalar", value: 3 }));
    assert.deepEqual(f.stats().callbackReads, [{
      token,
      request: JSON.stringify({ kind: "scalar_signal", node: { slot: 8, generation: 2 } }),
    }]);
    assert.equal(f.stats().committedPhases, 0);
    assert.equal(f.stats().drained, 0, "a sparse read cannot drain a renderer delta");
    readPhase(token, { request_id: 12, kind: "family", node: { slot: 9, generation: 2 } });
    assert.equal(f.stats().callbackReads[1].request,
      JSON.stringify({ kind: "family", node: { slot: 9, generation: 2 } }));
    assert.equal(f.stats().committedPhases, 0, "a bulk family read cannot publish the phase");


    assert.throws(
      () => readPhase(JSON.stringify({ ...phase.token, sequence: 5 }), {
        request_id: 12, kind: "scalar_signal", node: { slot: 8, generation: 2 },
      }),
      /token is stale/,
    );
    resolveCallback(JSON.stringify({ token: phase.token, writes: [] }));
    await attaching;
    assert.equal(f.stats().committedPhases, 1);
    assert.throws(
      () => readPhase(token, {
        request_id: 13, kind: "object", node: { slot: 8, generation: 2 },
      }),
      /no pending live phase/,
    );
    endpoint.stop();
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

test("external sample pacing ignores render ticks and resolves after exact presentation", async () => {
  const f = fixture("transferable", null, {
    generation: 31, onComplete: () => {}, onError: (_generation, error) => { throw error; },
  }, { pacing: "external_samples" });
  let endpoint;
  try {
    f.player.drainDeltaJson = () => f.player.seekDeltaJson(f.player.time());
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    endpoint = await f.attach();
    await ready;
    const initialDelta = await initial;
    f.render.port2.postMessage({
      type: "execution_ack", session: initialDelta.session, sequence: initialDelta.sequence,
    });
    f.render.port2.postMessage({
      type: "execution_presented", session: initialDelta.session, sequence: initialDelta.sequence,
    });

    f.render.port2.postMessage({ type: "tick", timestamp: 100 });
    await turn();
    assert.deepEqual(f.stats().continuationDriveTimes, []);
    assert.deepEqual(f.stats().authoredSampleTimes, []);

    const publication = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_delta" && message.sequence !== initialDelta.sequence,
    );
    const sampled = request(f.control.port2, "sample_to_authored_time", 61, { time: 0.5 });
    const delta = await publication;
    let settled = false;
    sampled.then(() => { settled = true; });
    f.render.port2.postMessage({
      type: "execution_ack", session: delta.session, sequence: delta.sequence,
    });
    await turn();
    assert.equal(settled, false);
    f.render.port2.postMessage({
      type: "execution_presented", session: delta.session, sequence: delta.sequence,
    });
    assert.equal((await sampled).time, 0.5);
    assert.deepEqual(f.stats().authoredSampleTimes, [0.5]);
    const beforeDebug = f.stats();
    const debug = await request(f.control.port2, "debug_frame", 63);
    assert.deepEqual(debug.debugFrame, { time: 0.5, source: "active" });
    assert.deepEqual(f.stats(), beforeDebug, "diagnostics must not seek, publish, or replace execution");

    const backward = await request(
      f.control.port2, "sample_to_authored_time", 62, { time: 0.25 },
    );
    assert.equal(backward.type, "error");
    assert.match(backward.message, /must be monotonic/);
    assert.deepEqual(f.stats().authoredSampleTimes, [0.5]);
  } finally { endpoint?.stop(); f.close(); }
});

test("one external sample crosses continuation segments with the returned player", async () => {
  let endpoint;
  let stage = 0;
  const f = fixture("transferable", null, {
    generation: 32,
    onComplete: () => {
      stage += 1;
      setImmediate(() => endpoint.startContinuation(32));
    },
    onError: (_generation, error) => { throw error; },
  }, { pacing: "external_samples" });
  const acknowledge = (publication) => {
    f.render.port2.postMessage({
      type: "execution_ack", session: publication.session, sequence: publication.sequence,
    });
    f.render.port2.postMessage({
      type: "execution_presented", session: publication.session, sequence: publication.sequence,
    });
  };
  const delta = () => nextMatching(f.render.port2, (message) => message.type === "execution_delta");
  try {
    f.player.driveLiveSegmentToAuthoredTime = (target) => {
      f.stats().authoredSampleTimes.push(target);
      if (stage === 0) {
        f.player.seekDeltaJson(1.0);
        return { callbackPhaseJson: null, reachedEndpoint: true };
      }
      f.player.seekDeltaJson(target);
      return { callbackPhaseJson: null, reachedEndpoint: false };
    };
    f.player.drainDeltaJson = () => f.player.seekDeltaJson(f.player.time());
    const ready = next(f.control.port2);
    const initial = delta();
    endpoint = await f.attach();
    await ready;
    acknowledge(await initial);

    const sampled = request(f.control.port2, "sample_to_authored_time", 63, { time: 1.5 });
    acknowledge(await delta()); // first segment endpoint
    acknowledge(await delta()); // first segment completion
    acknowledge(await delta()); // next segment authored/resume publication
    acknowledge(await delta()); // requested frame in the next segment
    assert.equal((await sampled).time, 1.5);
    assert.deepEqual(f.stats().authoredSampleTimes, [1.5, 1.5]);
    assert.equal(f.stats().created, 1);
    assert.equal(f.stats().resumed, 1);
    assert.equal(f.stats().returned, 1);
    assert.equal(f.stats().resourceBundles, 1);
    assert.equal(f.stats().completedSegments, 1);
  } finally { endpoint?.stop(); f.close(); }
});

for (const [time, stopAtSourceCompletion] of [[2, false], [3, true], [3, false]]) {
  test(`external sampling completion: time=${time}, stop=${stopAtSourceCompletion}`, { timeout: 2_000 }, async () => {
    let endpoint;
    let stage = 0;
    const f = fixture("transferable", null, {
      generation: 33,
      onError: () => {},
      onComplete: () => {
        stage += 1;
        if (stage === 1) endpoint.startContinuation(33);
        else void endpoint.publishContinuationResult(33);
      },
    }, { pacing: "external_samples" });
    f.render.port2.on("message", (message) => {
      if (message.type !== "execution_delta") return;
      for (const type of ["execution_ack", "execution_presented"]) {
        f.render.port2.postMessage({ type, session: message.session, sequence: message.sequence });
      }
    });
    f.player.driveLiveSegmentToAuthoredTime = () => {
      f.player.seekDeltaJson(stage + 1);
      return { callbackPhaseJson: null, reachedEndpoint: true };
    };
    // No visual change at the continuation boundary is valid and is not EOF.
    f.player.drainDeltaJson = () => null;
    try {
      const ready = next(f.control.port2);
      endpoint = await f.attach();
      await ready;
      const result = await request(f.control.port2, "sample_to_authored_time", 64, { time, stopAtSourceCompletion });
      if (time > 2 && !stopAtSourceCompletion) {
        assert.equal(result.type, "error");
        assert.match(result.message, /before external sample/);
        return;
      }
      assert.equal(result.sourceCompleted, true);
      assert.equal(result.type, "sample_to_authored_time");
      assert.equal(result.time, 2);
      assert.equal(result.playing, false);
      assert.equal(f.stats().completedSegments, 2);
      assert.equal(f.stats().resumed, 1);
      const beforeDebug = f.stats();
      const debug = await request(f.control.port2, "debug_frame", 65);
      assert.deepEqual(debug.debugFrame, { time: 2, source: "returned" });
      assert.deepEqual(f.stats(), beforeDebug, "completed-source diagnostics use the returned owner");
    } finally { endpoint?.stop(); f.close(); }
  });
}

test("continuation sends coherent intermediate publications before its endpoint", async () => {
  const f = fixture("transferable", null, {
    generation: 25, onComplete: () => {}, onError: (_generation, error) => { throw error; },
  });
  let endpoint;
  try {
    f.player.driveLiveSegmentFromWallTime = () => ({ callbackPhaseJson: null, reachedEndpoint: false });
    f.player.drainDeltaJson = () => f.player.seekDeltaJson(0.5);
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    endpoint = await f.attach();
    await ready;
    const initialDelta = await initial;
    f.render.port2.postMessage({ type: "execution_ack", session: initialDelta.session, sequence: initialDelta.sequence });
    const intermediate = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    f.render.port2.postMessage({ type: "tick", timestamp: 1 });
    const publication = await intermediate;
    assert.equal(JSON.parse(decodeTransferableExecutionDelta(publication).json).time, 0.5);
    assert.equal(publication.sequence, initialDelta.sequence + 1);
    assert.equal(f.stats().completedSegments, 0);
    assert.equal(f.stats().returned, 0, "an intermediate frame retains the source continuation lease");
  } finally { endpoint?.stop(); f.close(); }
});

test("initial continuation attachment drives its first wake through presentation and completion", async () => {
  const completions = [];
  const f = fixture("transferable", null, {
    generation: 26,
    onComplete: (generation) => completions.push(generation),
    onError: (_generation, error) => { throw error; },
  });
  let endpoint;
  const acknowledge = (publication) => {
    f.render.port2.postMessage({
      type: "execution_ack", session: publication.session, sequence: publication.sequence,
    });
    f.render.port2.postMessage({
      type: "execution_presented", session: publication.session, sequence: publication.sequence,
    });
  };
  try {
    // The initial attachment must use the same renderer wake path as a resumed
    // segment.  It has no later `startContinuation` call to kick progress.
    f.player.drainDeltaJson = () => f.player.seekDeltaJson(f.player.time());
    const ready = next(f.control.port2);
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    const initialWake = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_wake",
    );
    endpoint = await f.attach();
    await ready;
    const initialDelta = await initial;
    const wake = await initialWake;
    assert.equal(wake.cadence, "animation_frame");
    acknowledge(initialDelta);

    const endpointPublication = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_delta" && message.sequence !== initialDelta.sequence,
    );
    f.render.port2.postMessage({ type: "tick", timestamp: 1 });
    const atEndpoint = await endpointPublication;
    assert.equal(f.stats().continuationDriveTimes.length, 1);
    assert.ok(Number.isFinite(f.stats().continuationDriveTimes[0]));
    assert.equal(f.stats().completedSegments, 0);
    assert.deepEqual(completions, []);

    const completionPublication = nextMatching(
      f.render.port2,
      (message) => message.type === "execution_delta" && message.sequence !== atEndpoint.sequence,
    );
    acknowledge(atEndpoint);
    const completed = await completionPublication;
    assert.equal(f.stats().completedSegments, 1);
    assert.deepEqual(completions, []);
    acknowledge(completed);
    await turn();
    await turn();

    assert.deepEqual(completions, [26]);
    assert.equal(f.stats().returned, 1);
    assert.equal(f.stats().returnedPlayer, f.player);
  } finally { endpoint?.stop(); f.close(); }
});

test("continuation presents admitted native input before completing and returning its lease", async () => {
  const completions = [];
  const f = fixture("transferable", null, {
    generation: 23,
    onComplete: (generation) => completions.push(generation),
    onError: (_generation, error) => { throw error; },
  });
  let endpoint;
  const delta = () => nextMatching(f.render.port2, (message) => message.type === "execution_delta");
  const acknowledge = (publication) => {
    f.render.port2.postMessage({ type: "execution_ack", session: publication.session, sequence: publication.sequence });
    f.render.port2.postMessage({ type: "execution_presented", session: publication.session, sequence: publication.sequence });
  };
  try {
    // Every effective input changes the coherent frame in this fixture.
    f.player.drainDeltaJson = () => f.player.seekDeltaJson(f.player.time());
    const ready = next(f.control.port2);
    const initial = delta();
    endpoint = await f.attach();
    await ready;
    acknowledge(await initial);
    const endpointDelta = delta();
    f.render.port2.postMessage({ type: "tick", timestamp: 1 });
    const atEndpoint = await endpointDelta;
    const stateReply = request(f.control.port2, "native_state_input", 51, { source: 2, value: 0.75 });
    await turn();
    await turn();
    assert.equal(f.stats().nativeInputs.length, 0, "input waits for the coherent endpoint");
    assert.equal(f.stats().completedSegments, 0);
    const stateDelta = delta();
    acknowledge(atEndpoint);
    const statePublication = await stateDelta;
    assert.equal((await stateReply).type, "native_state_input");
    assert.equal(f.stats().completedSegments, 0, "completion waits for accepted input presentation");

    const completionDelta = delta();
    acknowledge(statePublication);
    const completed = await completionDelta;
    assert.equal(f.stats().completedSegments, 1);
    assert.equal(f.stats().returned, 0);
    const eventReply = request(f.control.port2, "native_event", 52, { source: 3 });
    await turn();
    await turn();
    const eventDelta = delta();
    acknowledge(completed);
    const eventPublication = await eventDelta;
    assert.equal((await eventReply).type, "native_event");
    assert.equal(f.stats().returned, 0, "input admitted during completion still owns the lease");
    assert.deepEqual(f.stats().nativeInputs, [
      { type: "state", value: { source: 2, value: 0.75 } },
      { type: "event", value: { source: 3 } },
    ]);
    assert.equal(f.player.time(), 1, "input does not advance authored time");
    acknowledge(eventPublication);
    await turn();
    await turn();
    assert.deepEqual(completions, [23]);
    assert.equal(f.stats().returned, 1);
    assert.equal(f.stats().returnedPlayer, f.player);
  } finally { endpoint?.stop(); f.close(); }
});

test("continuation reanchors Rust wake after callback completion but preserves phase retry time", async () => {
  const f = fixture("transferable", async (phase) => {
    await turn();
    return JSON.stringify({ token: phase.token, writes: [] });
  }, { generation: 24, onComplete: () => {}, onError: (_generation, error) => { throw error; } });
  let endpoint;
  const drives = [];
  const anchors = [];
  try {
    f.player.driveLiveSegmentFromWallTime = (wallTime) => {
      drives.push(wallTime);
      return {
        callbackPhaseJson: drives.length === 1 ? JSON.stringify({ token: { sequence: 1 } }) : null,
        reachedEndpoint: false,
      };
    };
    f.player.reanchorLiveSegmentWake = (wallTime) => {
      anchors.push(wallTime);
      return { cadence: "animation_frame", timerAfterMilliseconds: undefined };
    };
    const ready = next(f.control.port2);
    endpoint = await f.attach();
    await ready;
    const resumedWake = nextMatching(f.render.port2, (message) => message.type === "execution_wake");
    f.render.port2.postMessage({ type: "tick", timestamp: 1 });
    await resumedWake;
    await turn();
    await turn();
    assert.equal(drives.length, 2);
    assert.equal(drives[0], drives[1]);
    assert.equal(anchors.length, 1);
    assert.ok(anchors[0] >= drives[1]);
    f.render.port2.postMessage({ type: "tick", timestamp: 2 });
    await turn();
    await turn();
    assert.equal(drives.length, 3);
    assert.equal(anchors.length, 1, "callback-free drive keeps its original wake anchor");
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
    const initialWakeObservations = f.stats().executionWakeTimes.length;

    const state = await request(f.control.port2, "native_state_input", 20, {
      source: { kind: "control", name: "opacity" },
      value: { kind: "scalar", value: 0.75 },
    });
    assert.equal(state.type, "native_state_input");
    assert.equal(f.stats().executionWakeTimes.length, initialWakeObservations + 1);
    const event = await request(f.control.port2, "native_event", 21, {
      source: { kind: "pointer_down", button: 0 },
    });
    assert.equal(event.type, "native_event");
    assert.equal(f.stats().executionWakeTimes.length, initialWakeObservations + 2);
    const pointerInput = {
      kind: "move",
      surface_x: 10,
      surface_y: 20,
      viewport_width: 800,
      viewport_height: 400,
      button: null,
      view_revision: 3,
      shift: false,
      control: false,
      alt: false,
      meta: false,
    };
    const pointer = await request(
      f.control.port2,
      "browser_pointer_input",
      22,
      pointerInput,
    );
    assert.equal(pointer.type, "browser_pointer_input");
    assert.equal(f.stats().executionWakeTimes.length, initialWakeObservations + 3);
    assert.deepEqual(f.stats().nativeInputs, [
      {
        type: "state",
        value: {
          source: { kind: "control", name: "opacity" },
          value: { kind: "scalar", value: 0.75 },
        },
      },
      { type: "event", value: { source: { kind: "pointer_down", button: 0 } } },
      { type: "pointer", value: pointerInput },
    ]);

    f.player.setNativeStateInputJson = () => { throw new Error("native value rejected"); };
    const rejected = await request(f.control.port2, "native_state_input", 23, {
      source: { kind: "control", name: "opacity" },
      value: { kind: "bool", value: true },
    });
    assert.equal(rejected.type, "error");
    assert.match(rejected.message, /native value rejected/);
    assert.equal(f.stats().nativeInputs.length, 3);
    assert.equal(
      f.stats().executionWakeTimes.length,
      initialWakeObservations + 3,
      "failed input does not publish or replace the current Rust wake",
    );
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

test("callback renderer observation waits for the exact presented publication and matching result", async () => {
  const f = fixture("transferable", async (phase) =>
    JSON.stringify({ token: phase.token, writes: [] }));
  let endpoint;
  try {
    const prepared = await prepareRendererObservationFixture(f, [
        { target: { slot: 4, generation: 2 } },
        { target: { slot: 9, generation: 1 } },
    ]);
    endpoint = prepared.endpoint;
    const { observationRequest: requestMessage, publicationMessage, advanced } =
      prepared.begin(37);
    const observationRequest = await requestMessage;
    const publication = await publicationMessage;
    assert.equal(observationRequest.session, publication.session);
    assert.equal(observationRequest.sequence, publication.sequence);

    let settled = false;
    advanced.then(() => { settled = true; });
    f.render.port2.postMessage({
      type: "execution_presented",
      session: publication.session,
      sequence: publication.sequence,
    });
    await turn();
    assert.equal(settled, false, "presentation alone cannot synthesize renderer evidence");

    const invalid = nextMatching(
      f.control.port2,
      (message) => message.type === "error" && message.requestId === null,
    );
    f.render.port2.postMessage({
      type: "renderer_observation",
      session: publication.session + 1,
      sequence: publication.sequence,
      json: JSON.stringify({
        outcome: "presented",
        publication: { session: publication.session + 1, sequence: publication.sequence },
      }),
    });
    assert.match((await invalid).message, /invalid publication observation/);
    await turn();
    assert.equal(settled, false, "a foreign renderer observation cannot release the control");

    const rendererObservation = {
      outcome: "resource_unavailable",
      publication: { session: publication.session, sequence: publication.sequence },
      resource: "text_upload_ranges",
    };
    f.render.port2.postMessage({
      type: "renderer_observation",
      session: publication.session,
      sequence: publication.sequence,
      json: JSON.stringify(rendererObservation),
    });
    const result = await advanced;
    assert.deepEqual(result.rendererObservation, rendererObservation);
  } finally { endpoint?.stop(); f.close(); }
});

test("malformed renderer observation rejects its control and clears the pending request", async () => {
  const f = fixture("transferable", async (phase) =>
    JSON.stringify({ token: phase.token, writes: [] }));
  let endpoint;
  try {
    const prepared = await prepareRendererObservationFixture(f, [
      { target: { slot: 4, generation: 2 } },
    ]);
    endpoint = prepared.endpoint;
    const { observationRequest, publicationMessage, advanced } = prepared.begin(39);
    const requestMessage = await observationRequest;
    const publication = await publicationMessage;
    f.render.port2.postMessage({
      type: "execution_presented",
      session: publication.session,
      sequence: publication.sequence,
    });

    const malformedDiagnostic = nextMatching(
      f.control.port2,
      (message) => message.type === "error" && message.requestId === null,
    );
    f.render.port2.postMessage({
      type: "renderer_observation",
      session: publication.session,
      sequence: publication.sequence,
      json: JSON.stringify({ outcome: "presented" }),
    });
    assert.match((await malformedDiagnostic).message, /does not match its publication/);
    const rejected = await advanced;
    assert.equal(rejected.type, "error");
    assert.match(rejected.message, /does not match its publication/);

    const noLongerPending = nextMatching(
      f.control.port2,
      (message) => message.type === "error" && message.requestId === null,
    );
    f.render.port2.postMessage({
      type: "renderer_observation",
      session: publication.session,
      sequence: publication.sequence,
      json: JSON.stringify({
        outcome: "presented",
        publication: {
          session: requestMessage.session,
          sequence: requestMessage.sequence,
        },
      }),
    });
    assert.match((await noLongerPending).message, /invalid publication observation/);
  } finally { endpoint?.stop(); f.close(); }
});

test("an unchanged callback observation fails explicitly without waiting for renderer evidence", async () => {
  const f = fixture("transferable", async (phase) =>
    JSON.stringify({ token: phase.token, writes: [] }));
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
    f.player.pause();
    let phasePending = true;
    f.player.advanceForwardToCallbackPhaseJson = (time) => {
      if (!phasePending) return null;
      phasePending = false;
      return JSON.stringify({
        token: { sequence: "1" },
        time,
        invocations: [{ target: { slot: 4, generation: 2 } }],
      });
    };
    f.player.drainRendererObservationPublicationJson = () => {
      throw new Error("callback commit produced no retained renderer publication");
    };
    let observationRequests = 0;
    f.render.port2.on("message", (message) => {
      if (message.type === "renderer_observation_request") observationRequests += 1;
    });

    const result = await request(f.control.port2, "advance_to", 38, {
      time: 1,
      observeRenderer: true,
    });
    assert.equal(result.type, "error");
    assert.match(result.message, /no retained renderer publication/);
    assert.equal(observationRequests, 0);
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
  const f = fixture("transferable", () => phase, null, { initiallyPaused: true });
  let committed = 0;
  let callbackObservedPaused = false;
  try {
    f.player.initialCallbackPhaseJson = () => {
      callbackObservedPaused = !f.player.isPlaying();
      return JSON.stringify({ token: { sequence: "0" } });
    };
    f.player.commitCallbackPhaseJson = (batch) => {
      assert.equal(batch, "{\"token\":{\"sequence\":\"0\"},\"writes\":[]}");
      committed += 1;
    };
    const resources = nextMatching(f.render.port2, (message) => message.type === "retained_resources");
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    const wake = nextMatching(f.render.port2, (message) => message.type === "execution_wake");
    const attached = f.attach();
    await resources;
    let early = false;
    let wakeEarly = false;
    initial.then(() => { early = true; });
    wake.then(() => { wakeEarly = true; });
    await turn();
    assert.equal(early, false);
    assert.equal(wakeEarly, false, "the renderer stays unscheduled behind the initial barrier");
    assert.equal(committed, 0);
    assert.equal(callbackObservedPaused, true);
    resolvePhase("{\"token\":{\"sequence\":\"0\"},\"writes\":[]}");
    const endpoint = await attached;
    assert.equal(committed, 1);
    const delta = await initial;
    assert.equal(JSON.parse(decodeTransferableExecutionDelta(delta).json).snapshot, true);
    assert.equal((await wake).cadence, "idle");
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


for (const reachedEndpoint of [false, true]) {
  test(`renderer failure terminates ${reachedEndpoint ? "endpoint" : "intermediate"} continuation once`, async () => {
    const failures = [];
    let completed = 0;
    let drives = 0;
    const f = fixture("transferable", null, {
      generation: 26,
      onComplete: () => { completed += 1; },
      onError: (generation, error) => { failures.push({ generation, error }); },
    });
    let endpoint;
    try {
      f.player.driveLiveSegmentFromWallTime = () => {
        drives += 1;
        return { callbackPhaseJson: null, reachedEndpoint };
      };
      f.player.drainDeltaJson = () => f.player.seekDeltaJson(0.5);
      const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
      endpoint = await f.attach();
      const initialDelta = await initial;
      f.render.port2.postMessage({ type: "execution_ack", session: initialDelta.session, sequence: initialDelta.sequence });
      const publication = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
      f.render.port2.postMessage({ type: "tick", timestamp: 1 });
      await publication;
      const error = nextMatching(f.control.port2, (message) => message.type === "error");
      f.render.port2.postMessage({ type: "render_error", message: "upload failed" });
      await error;
      await turn();
      f.render.port2.postMessage({ type: "tick", timestamp: 2 });
      await turn();
      assert.equal(failures.length, 1);
      assert.equal(failures[0].generation, 26);
      assert.match(failures[0].error.message, /upload failed/);
      assert.equal(drives, 1);
      assert.equal(completed, 0);
      assert.equal(f.stats().completedSegments, 0);
      assert.equal(f.stats().returned, 0);
      assert.equal(f.stats().stopped, 1);
    } finally { endpoint?.stop(); f.close(); }
  });
}


test("source result waits for final edits without another segment or callback drive", async () => {
  let finishSegment;
  const finished = new Promise((resolve) => { finishSegment = resolve; });
  const f = fixture("transferable", null, {
    generation: 27, onComplete: finishSegment, onError: (_generation, error) => { throw error; },
  });
  let endpoint;
  try {
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    endpoint = await f.attach();
    const initialDelta = await initial;
    f.render.port2.postMessage({ type: "execution_ack", session: initialDelta.session, sequence: initialDelta.sequence });
    f.render.port2.postMessage({ type: "execution_presented", session: initialDelta.session, sequence: initialDelta.sequence });
    f.render.port2.postMessage({ type: "tick", timestamp: 1 });
    await finished;
    assert.equal(f.stats().returned, 1);
    f.player.drainDeltaJson = () => f.player.seekDeltaJson(1);
    const changed = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    let resultReady = false;
    const result = endpoint.publishContinuationResult(27).then(() => { resultReady = true; });
    const finalDelta = await changed;
    f.render.port2.postMessage({ type: "execution_ack", session: finalDelta.session, sequence: finalDelta.sequence });
    await turn();
    assert.equal(resultReady, false);
    assert.equal(finalDelta.sequence, initialDelta.sequence + 1);
    assert.equal(JSON.parse(decodeTransferableExecutionDelta(finalDelta).json).time, 1);
    f.render.port2.postMessage({ type: "execution_presented", session: finalDelta.session, sequence: finalDelta.sequence });
    await result;
    assert.equal(f.stats().completedSegments, 1);
    assert.equal(f.stats().continuationDriveTimes.length, 1);
    assert.equal(f.stats().returned, 1);
    assert.equal(f.stats().resumed, 0);
    assert.equal(f.stats().initialSnapshots, 1);
  } finally { endpoint?.stop(); f.close(); }
});


test("live state reports elapsed time without presenting a segment horizon as the total duration", async () => {
  const f = fixture("transferable", null, { generation: 81, onComplete() {}, onError() {} });
  let endpoint;
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach();
    await ready;
    assert.equal(f.context.liveHandoffDuration(), undefined, "a leased context must not be queried for its player duration");
    const before = await request(f.control.port2, "state", 501);
    assert.equal(before.durationSeconds, null);
    f.player.seekDeltaJson(0.4);
    const after = await request(f.control.port2, "state", 502);
    assert.equal(after.time, 0.4);
    assert.equal(after.durationSeconds, null);
    assert.deepEqual(f.stats().continuationDriveTimes, [], "observing progress must not schedule or drive animation");
  } finally { endpoint?.stop(); f.close(); }
});


for (const pacing of ["realtime", "external_samples"]) {
  test(`returned ${pacing} state cannot jump to the next unplayed segment endpoint`, async () => {
    let completed;
    const returned = new Promise((resolve) => { completed = resolve; });
    const f = fixture("transferable", null, {
      generation: 91,
      onComplete: completed,
      onError: (_generation, error) => { throw error; },
    }, { pacing });
    let endpoint;
    try {
      // Presentation acknowledgement is independent of playback-state reads.
      f.render.port2.on("message", (message) => {
        if (message.type !== "execution_delta") return;
        f.render.port2.postMessage({ type: "execution_ack", session: message.session, sequence: message.sequence });
        f.render.port2.postMessage({ type: "execution_presented", session: message.session, sequence: message.sequence });
      });
      const ready = next(f.control.port2);
      endpoint = await f.attach();
      await ready;
      let sampled;
      if (pacing === "external_samples") {
        f.player.driveLiveSegmentToAuthoredTime = () => {
          f.player.seekDeltaJson(1);
          return { callbackPhaseJson: null, reachedEndpoint: true };
        };
        sampled = nextMatching(f.control.port2, (message) => message.requestId === 700);
        f.control.port2.postMessage({ channel: "noon.engine", protocolVersion: 1,
          type: "sample_to_authored_time", requestId: 700, time: 1 });
      } else {
        f.render.port2.postMessage({ type: "tick", timestamp: 1 });
      }
      await returned;
      const reads = f.stats().continuationDriveTimes.length;
      // Python has authored its next await but has not transferred the player.
      // Querying its horizon here used to publish 100 as the elapsed time.
      f.context.liveHandoffDuration = () => 100;
      const state = await request(f.control.port2, "state", 701);
      assert.equal(state.time, 1);
      assert.equal(state.playing, false);
      assert.equal(state.durationSeconds, null);
      assert.equal(f.stats().continuationDriveTimes.length, reads);
      if (sampled) {
        f.context.liveHandoffDuration = () => 1;
        await endpoint.publishContinuationResult(91);
        const result = await sampled;
        assert.equal(result.time, 1);
        assert.equal(result.sourceCompleted, true);
      }
    } finally { endpoint?.stop(); f.close(); }
  });
}

// Input must observe the active Rust wake owner, not the independently paused
// ordinary playback clock. Cover animation and pure-wait leases on every lane.
const continuationWakeInputCases = [
  ["native_state_input", {
    source: { kind: "control", name: "opacity" },
    value: { kind: "scalar", value: 0.75 },
  }],
  ["native_event", { source: { kind: "wheel" } }],
  ["browser_pointer_input", {
    kind: "move", surface_x: 200, surface_y: 100,
    viewport_width: 800, viewport_height: 400, view_revision: 0,
  }],
];

for (const [inputType, fields] of continuationWakeInputCases) {
  for (const cadence of ["animation_frame", "timer"]) {
    test(`native input preserves active continuation wake: ${inputType}/${cadence}`, { timeout: 5000 }, async () => {
      let completed;
      let failed;
      const completion = new Promise((resolve, reject) => { completed = resolve; failed = reject; });
      const f = fixture("transferable", null, {
        generation: 71,
        onComplete: completed,
        onError: (_generation, error) => failed(error),
      });
      let endpoint;
      let ordinaryReads = 0;
      let segmentReads = 0;
      let delay = 250;
      try {
        // A real live drive pauses the ordinary clock while its segment remains
        // active. A generic executionWake would therefore report idle here.
        f.player.pause();
        f.player.executionWake = () => {
          ordinaryReads += 1;
          return { cadence: "idle", timerAfterMilliseconds: undefined };
        };
        f.player.liveSegmentWake = () => {
          segmentReads += 1;
          return { cadence, timerAfterMilliseconds: cadence === "timer" ? delay : undefined };
        };
        const ready = next(f.control.port2);
        const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
        const initialWake = nextMatching(f.render.port2, (message) => message.type === "execution_wake");
        endpoint = await f.attach();
        await ready;
        assert.equal((await initialWake).cadence, cadence);
        const publication = await initial;
        f.render.port2.postMessage({ type: "execution_ack", session: publication.session, sequence: publication.sequence });
        f.render.port2.postMessage({ type: "execution_presented", session: publication.session, sequence: publication.sequence });
        await turn();

        const inputWake = nextMatching(f.render.port2, (message) => message.type === "execution_wake");
        const reply = await request(f.control.port2, inputType, 170, fields);
        assert.equal(reply.type, inputType, reply.message);
        const wake = await inputWake;
        assert.equal(wake.cadence, cadence, "input must not replace an active segment wake with idle");
        assert.equal(wake.timerAfterMilliseconds, cadence === "timer" ? delay : null);
        assert.equal(ordinaryReads, 0, "input cannot observe the ordinary playback clock during a lease");
        assert.equal(segmentReads, 2);
        assert.equal(f.stats().nativeInputs.length, 1);
        assert.equal(f.player.time(), 0, "input never advances authored time");
        assert.equal(f.stats().completedSegments, 0);
        assert.equal(f.stats().returned, 0);

        // The next due platform wake still reaches shared segment completion,
        // instead of requiring another input or an unrelated timer to unstick it.
        delay = 0;
        f.render.port2.postMessage({ type: "tick", timestamp: 1 });
        assert.equal(await completion, 71);
        assert.equal(f.stats().completedSegments, 1);
        assert.equal(f.stats().returned, 1);
        assert.equal(f.player.time(), 1);
      } finally { endpoint?.stop(); f.close(); }
    });
  }
}

test("external sample input never arms a realtime continuation wake", { timeout: 5000 }, async () => {
  const f = fixture("transferable", null, {
    generation: 72, onComplete: () => {}, onError: () => {},
  }, { pacing: "external_samples" });
  let endpoint;
  let wakeReads = 0;
  const wakes = [];
  try {
    f.player.executionWake = f.player.liveSegmentWake = () => {
      wakeReads += 1;
      return { cadence: "animation_frame" };
    };
    f.render.port2.on("message", (message) => {
      if (message.type === "execution_wake") wakes.push(message.cadence);
    });
    const ready = next(f.control.port2);
    endpoint = await f.attach();
    await ready;
    let requestId = 180;
    for (const [inputType, fields] of continuationWakeInputCases) {
      const reply = await request(f.control.port2, inputType, requestId++, fields);
      assert.equal(reply.type, inputType, reply.message);
    }
    await turn();
    assert.deepEqual(wakes, ["idle"]);
    assert.equal(wakeReads, 0);
    assert.equal(f.player.time(), 0);
    assert.equal(f.stats().completedSegments, 0);
  } finally { endpoint?.stop(); f.close(); }
});

test("rejected continuation pointer input preserves its existing wake", { timeout: 5000 }, async () => {
  const f = fixture("transferable", null, {
    generation: 73, onComplete: () => {}, onError: () => {},
  });
  let endpoint;
  let ordinaryReads = 0;
  let segmentReads = 0;
  const wakes = [];
  try {
    f.player.executionWake = () => { ordinaryReads += 1; return { cadence: "idle" }; };
    f.player.liveSegmentWake = () => {
      segmentReads += 1;
      return { cadence: "timer", timerAfterMilliseconds: 250 };
    };
    f.player.submitBrowserPointerInputJson = () => { throw new Error("pointer rejected"); };
    f.render.port2.on("message", (message) => {
      if (message.type === "execution_wake") wakes.push(message.cadence);
    });
    const ready = next(f.control.port2);
    endpoint = await f.attach();
    await ready;
    const reply = await request(f.control.port2, "browser_pointer_input", 190, continuationWakeInputCases[2][1]);
    assert.equal(reply.type, "error");
    assert.match(reply.message, /pointer rejected/);
    await turn();
    assert.deepEqual(wakes, ["timer"]);
    assert.equal(segmentReads, 1);
    assert.equal(ordinaryReads, 0);
    assert.equal(f.stats().nativeInputs.length, 0);
    assert.equal(f.player.time(), 0);
  } finally { endpoint?.stop(); f.close(); }
});


test("unavailable replay preserves final presentation and rejects transport commands", async () => {
  const f = fixture();
  f.player.sealReplay = () => { throw new Error("RetentionLimit"); };
  let endpoint;
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach(); await ready;
    const observed = await request(f.control.port2, "state", 901);
    assert.equal(observed.replaySupported, false);
    assert.equal(observed.playing, false);
    assert.match(observed.replayUnavailable, /RetentionLimit/);
    for (const command of ["seek", "resume", "restart_playback"]) {
      const reply = await request(f.control.port2, command, 902, { time: 0 });
      assert.equal(reply.type, "error");
      assert.match(reply.message, /Replay unavailable/);
    }
  } finally { endpoint?.stop(); f.close(); }
});

test("source continuation never seals a still-growing execution plan", async () => {
  const f = fixture("transferable", null, { generation: 901, onComplete() {}, onError() {} });
  f.player.sealReplay = () => { throw new Error("must not seal source-owned playback"); };
  let endpoint;
  try { const ready = next(f.control.port2); endpoint = await f.attach(); await ready;
    assert.equal((await request(f.control.port2, "state", 903)).durationSeconds, null);
  } finally { endpoint?.stop(); f.close(); }
});

for (const reason of ["Incomplete", "UnsupportedDomain", "UnrecordedInput", "RetentionLimit"]) {
  test(`replay rejection (${reason}) preserves paused forward observation without enabling rewind`, async () => {
    const f = fixture();
    f.player.sealReplay = () => { throw new Error(reason); };
    let seeks = 0;
    f.player.seekDeltaJson = () => { seeks += 1; throw new Error("forward controls must not seek"); };
    let ticks = 0;
    f.player.tickCallbackPhaseJson = () => { ticks += 1; return null; };
    f.render.port2.on("message", (message) => {
      if (message.type !== "execution_delta") return;
      f.render.port2.postMessage({ type: "execution_ack", session: message.session, sequence: message.sequence });
      f.render.port2.postMessage({ type: "execution_presented", session: message.session, sequence: message.sequence });
    });
    let endpoint;
    try {
      const ready = next(f.control.port2);
      endpoint = await f.attach(); await ready;
      const paused = await request(f.control.port2, "pause", 910);
      assert.equal(paused.type, "pause", paused.message);
      assert.equal(paused.playing, false);
      const first = await request(f.control.port2, "advance_to", 911, { time: 0.4 });
      assert.equal(first.type, "advance_to", first.message);
      assert.equal(first.time, 0.4);
      assert.equal(first.replaySupported, false);
      assert.equal(first.replayUnavailable, reason);
      // Forward validity is still decided by the player, not a second JS clock.
      for (const time of [0.2, -1, NaN, Infinity]) {
        const rejected = await request(f.control.port2, "advance_to", 912, { time });
        assert.equal(rejected.type, "error");
        assert.match(rejected.message, /invalid forward time/);
        assert.equal(f.player.time(), 0.4);
      }
      const second = await request(f.control.port2, "advance_to", 913, { time: 0.8 });
      assert.equal(second.time, 0.8);
      assert.equal(second.playing, false);
      for (const command of ["seek", "resume", "restart_playback", "set_loop_duration"]) {
        const rejected = await request(f.control.port2, command, 914, { time: 0, loopDurationSeconds: 4 });
        assert.equal(rejected.type, "error");
        assert.match(rejected.message, /Replay unavailable/);
      }
      f.render.port2.postMessage({ type: "tick", timestamp: 1000 });
      await turn(); await turn();
      assert.equal(f.player.time(), 0.8);
      assert.equal(ticks, 0, "denied replay must not restart through a renderer wake");
      assert.equal(seeks, 0);
      assert.equal(f.stats().created, 1);
    } finally { endpoint?.stop(); f.close(); }
  });
}

test("non-replayable callback observation still waits for matching renderer evidence", { timeout: 3000 }, async () => {
  let callbacks = 0;
  const f = fixture("transferable", async (phase) => {
    callbacks += 1;
    return JSON.stringify({ token: phase.token, writes: [] });
  });
  f.player.sealReplay = () => { throw new Error("Incomplete"); };
  let endpoint;
  try {
    const prepared = await prepareRendererObservationFixture(f, [
      { target: { slot: 4, generation: 2 } },
    ]);
    endpoint = prepared.endpoint;
    const { observationRequest, publicationMessage, advanced } = prepared.begin(920);
    // Surface an admission failure immediately instead of hanging on a missing publication.
    const [observation, publication] = await Promise.race([
      Promise.all([observationRequest, publicationMessage]),
      advanced.then((reply) => {
        assert.notEqual(reply.type, "error", reply.message);
        throw new Error("forward observation completed before its renderer evidence");
      }),
    ]);
    assert.equal(callbacks, 1);
    assert.equal(f.stats().committedPhases, 1);
    assert.equal(observation.session, publication.session);
    assert.equal(observation.sequence, publication.sequence);
    let settled = false;
    advanced.then(() => { settled = true; });
    f.render.port2.postMessage({ type: "execution_ack", session: publication.session, sequence: publication.sequence });
    f.render.port2.postMessage({ type: "execution_presented", session: publication.session, sequence: publication.sequence });
    await turn(); await turn();
    assert.equal(settled, false, "presentation alone cannot replace callback renderer evidence");
    const evidence = {
      outcome: "presented",
      publication: { session: publication.session, sequence: publication.sequence },
    };
    f.render.port2.postMessage({ type: "renderer_observation", ...evidence.publication, json: JSON.stringify(evidence) });
    const result = await advanced;
    assert.equal(result.type, "advance_to");
    assert.equal(result.time, 1);
    assert.equal(result.playing, false);
    assert.equal(result.replaySupported, false);
    assert.deepEqual(result.rendererObservation, evidence);
    assert.equal(callbacks, 1, "observation must not re-execute the callback");
  } finally { endpoint?.stop(); f.close(); }
});

test("real-time state observes the Rust wait clock without driving or publishing", async () => {
  const f = fixture();
  let endpoint;
  const observations = [];
  f.player.playbackTimeAt = now => { observations.push(now); return 0.75; };
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach(); await ready;
    const before = f.stats();
    const observed = await request(f.control.port2, "state", 900);
    assert.equal(observed.time, 0.75);
    assert.equal(f.player.time(), 0);
    assert.equal(observations.length, 1);
    assert.ok(Number.isFinite(observations[0]));
    assert.equal(f.stats().drained, before.drained);
    assert.deepEqual(f.stats().continuationDriveTimes, []);
    assert.deepEqual(f.stats().authoredSampleTimes, []);
  } finally { endpoint?.stop(); f.close(); }
});

test("pausing in a wait commits the observed time before freezing the replay clock", async () => {
  const f = fixture(); let endpoint;
  f.player.playbackTimeAt = () => f.player.isPlaying() ? 0.75 : f.player.time();
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach(); await ready;
    const paused = await request(f.control.port2, "pause", 901);
    assert.equal(paused.time, 0.75);
    assert.equal(paused.playing, false);
    assert.equal(f.player.time(), 0.75);
    assert.equal((await request(f.control.port2, "state", 902)).time, 0.75);
  } finally { endpoint?.stop(); f.close(); }
});


test("external-sample state never projects wall time", async () => {
  const f = fixture("transferable", null, { generation: 93, onComplete() {}, onError() {} }, { pacing: "external_samples" });
  let endpoint;
  f.player.playbackTimeAt = () => { throw new Error("unexpected wall-time projection"); };
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach(); await ready;
    f.player.seekDeltaJson(0.25);
    assert.equal((await request(f.control.port2, "state", 903)).time, 0.25);
    assert.deepEqual(f.stats().authoredSampleTimes, []);
  } finally { endpoint?.stop(); f.close(); }
});

test("seek acknowledgements retain the exact evaluated time", async () => {
  const f = fixture(); let endpoint;
  f.player.playbackTimeAt = () => { throw new Error("unexpected wall-time projection"); };
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach(); await ready;
    assert.equal((await request(f.control.port2, "seek", 904, { time: 0.25 })).time, 0.25);
  } finally { endpoint?.stop(); f.close(); }
});


test("callback-stalled pointer controls keep bounded order and occurrence-local motion evidence", async () => {
  let release;
  let entered;
  const callbackStarted = new Promise(resolve => { entered = resolve; });
  const barrier = new Promise(resolve => { release = resolve; });
  const f = fixture("transferable", async phase => {
    entered();
    await barrier;
    return JSON.stringify({ token: phase.token, writes: [] });
  });
  let endpoint;
  try {
    const initial = nextMatching(f.render.port2, message => message.type === "execution_delta");
    endpoint = await f.attach();
    const first = await initial;
    f.render.port2.postMessage({ type: "execution_ack", session: first.session, sequence: first.sequence });
    f.player.tickCallbackPhaseJson = () => JSON.stringify({ token: { sequence: 1 }, time: 0 });
    f.render.port2.postMessage({ type: "tick", timestamp: 1 });
    await callbackStarted;
    const inputs = Array.from({ length: MAX_PENDING_SEMANTIC_CONTROLS }, (_, i) => ({
      kind: i === 0 ? "press" : i === MAX_PENDING_SEMANTIC_CONTROLS - 1 ? "release" : "move",
      surface_x: i === 1 ? 700 : 20,
      surface_y: 40,
      viewport_width: 800, viewport_height: 400,
      button: i === 0 || i === MAX_PENDING_SEMANTIC_CONTROLS - 1 ? 0 : null,
      view_revision: 3,
      shift: i === 0, control: false, alt: false, meta: false,
    }));
    const replies = [];
    f.control.port2.on("message", message => {
      if (message.type === "browser_pointer_input") replies.push(message.requestId);
    });
    const overflow = nextMatching(f.control.port2, message => message.type === "error");
    for (let i = 0; i <= inputs.length; i += 1) {
      f.control.port2.postMessage({
        channel: "noon.engine", protocolVersion: 1, type: "browser_pointer_input",
        requestId: i, ...inputs[i % inputs.length],
      });
    }
    const rejection = await overflow;
    assert.equal(rejection.requestId, inputs.length);
    assert.match(rejection.message, /control queue is full/);
    assert.deepEqual(f.stats().nativeInputs, [], "input must not bypass the required callback barrier");
    assert.deepEqual(replies, []);
    const drained = nextMatching(f.control.port2, message => message.requestId === inputs.length - 1);
    release();
    await drained;
    assert.equal(f.stats().committedPhases, 1);
    assert.deepEqual(replies, inputs.map((_, i) => i));
    assert.deepEqual(f.stats().nativeInputs, inputs.map(value => ({ type: "pointer", value })));
    assert.equal(f.player.time(), 0, "delivery does not advance authored time");
  } finally { release(); endpoint?.stop(); f.close(); }
});

for (const transportMode of ["transferable", "shared"]) {
  test(`selection presentation uses ordinary ${transportMode} ordering while paused`, { timeout: 5000 }, async () => {
    const f = fixture(transportMode, null, null, { initiallyPaused: true });
    let endpoint;
    let reader;
    let sequence = 1;
    let pending = null;
    const deltas = [];
    const configurations = [];
    // This test doubles the Rust boundary, not picking or renderer semantics.
    const overlay = {
      geometry: { kind: "circle", radius: 1 },
      transform: { translation: { x: 0, y: 0 }, scale: { x: -2, y: 0.5 }, rotation: 0.7 },
    };
    f.player.setPointerFillSelection = value => { configurations.push(value); };
    f.player.drainDeltaJson = () => {
      if (pending === null) return null;
      const delta = pending;
      pending = null;
      return JSON.stringify(delta);
    };
    const receive = json => {
      const delta = JSON.parse(json);
      deltas.push(delta);
      f.render.port2.postMessage({ type: "execution_ack", session: 7, sequence: delta.sequence });
      f.render.port2.postMessage({ type: "transport_writable" });
      return true;
    };
    f.render.port2.on("message", message => {
      if (message.type === "transport_setup") {
        reader = new SharedExecutionDeltaReader(message.mailbox);
        reader.drain(receive);
      } else if (message.type === "shared_delta") reader.drain(receive);
      else if (message.type === "execution_delta") receive(decodeTransferableExecutionDelta(message).json);
    });
    const waitForDeltas = async count => {
      for (let n = 0; n < 50 && deltas.length < count; n += 1) await turn();
      assert.equal(deltas.length, count);
    };
    try {
      const ready = next(f.control.port2);
      endpoint = await f.attach();
      await ready;
      await waitForDeltas(1);
      assert.equal((await request(f.control.port2, "pointer_fill_selection", 1, { maxMovement: 4 })).type, "pointer_fill_selection");
      assert.deepEqual(configurations, [4]);
      assert.equal(deltas.length, 1, "configuration without an image change emits nothing");
      // Supply exact output from the mocked shared session after admission. JS
      // must forward this verbatim; it must not invent IDs, rows, or a new clock.
      pending = { channel: "noon.execution.retained", protocol_version: 6,
        session: 7, sequence: sequence++, snapshot: false, time: 0,
        objects: [], selection_overlay: overlay };
      const selected = await request(f.control.port2, "browser_pointer_input", 2, { kind: "release" });
      assert.equal(selected.type, "browser_pointer_input");
      await waitForDeltas(2);
      assert.deepEqual(deltas[1].selection_overlay, overlay);
      assert.deepEqual(deltas[1].objects, []);
      assert.equal(selected.time, 0);
      assert.equal(selected.playing, false);
      pending = { channel: "noon.execution.retained", protocol_version: 6,
        session: 7, sequence: sequence++, snapshot: false, time: 0, objects: [] };
      const cleared = await request(f.control.port2, "pointer_fill_selection", 3, { maxMovement: null });
      assert.equal(cleared.type, "pointer_fill_selection");
      await waitForDeltas(3);
      assert.deepEqual(configurations, [4, null]);
      assert.deepEqual(deltas.map(d => d.sequence), [0, 1, 2]);
      assert.equal(deltas[2].selection_overlay, undefined);
      assert.deepEqual(deltas.map(d => d.time), [0, 0, 0]);
      assert.equal(cleared.playing, false);
      assert.equal(f.stats().continuationDriveTimes.length, 0);
    } finally { endpoint?.stop(); f.close(); }
  });
}

test("selection configuration rejection does not drain or replace the current wake", { timeout: 5000 }, async () => {
  const f = fixture("transferable", null, null, { initiallyPaused: true });
  let endpoint;
  let calls = 0;
  f.player.setPointerFillSelection = () => { calls += 1; throw new Error("required callback barrier"); };
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach();
    await ready;
    const before = f.stats();
    for (const [index, maxMovement] of [undefined, -1, Infinity, NaN, "4", {}, true].entries()) {
      const response = await request(f.control.port2, "pointer_fill_selection", index + 10, { maxMovement });
      assert.equal(response.type, "error");
      assert.match(response.message, /selection tolerance/);
    }
    assert.equal(calls, 0, "malformed transport never reaches Rust");
    const response = await request(f.control.port2, "pointer_fill_selection", 20, { maxMovement: 4 });
    assert.equal(response.type, "error");
    assert.match(response.message, /required callback barrier/);
    assert.equal(calls, 1);
    assert.equal(f.stats().drained, before.drained);
    assert.equal(f.stats().executionWakeTimes.length, before.executionWakeTimes.length);
    assert.equal(f.player.time(), 0);
  } finally { endpoint?.stop(); f.close(); }
});

for (const cancelFails of [false, true]) {
  test(`worker receipt invalidations retain the acknowledged frame behind backpressure${cancelFails ? " and stop on cancellation failure" : ""}`, async () => {
    const f = fixture();
    let endpoint;
    const calls = [];
    try {
      f.player.notePointerPresentationJson = (json) => {
        calls.push(["presented", JSON.parse(json).presentation]);
        return true;
      };
      f.player.invalidatePointerPresentationJson = (json) => {
        calls.push(["cancel", JSON.parse(json).presentation]);
        if (cancelFails) throw new Error("guarded cancellation failed");
        return true;
      };
      const ready = next(f.control.port2);
      const initial = nextMatching(f.render.port2, m => m.type === "execution_delta");
      endpoint = await f.attach();
      await ready;
      const first = await initial;
      const receiptA = { session: first.session, sequence: first.sequence, presentation: 1, view_revision: 1 };
      const acknowledged = nextMatching(f.control.port2, m => m.type === "pointer_presented");
      f.render.port2.postMessage({ type: "execution_presented", session: first.session, sequence: first.sequence, pointerReceipt: receiptA });
      await acknowledged;
      const secondDelta = nextMatching(f.render.port2, m => m.type === "execution_delta");
      await request(f.control.port2, "seek", 401, { time: 0.5 });
      const second = await secondDelta;
      // Both consumed acknowledgements are withheld, so lifecycle cleanup must
      // stay ordered behind the existing transport boundary rather than recurse.
      const receiptB = { ...receiptA, sequence: second.sequence, presentation: 2 };
      const invalidatedB = nextMatching(f.control.port2, m =>
        m.type === "pointer_presentation_invalidated" && m.receipt.presentation === 2);
      f.render.port2.postMessage({ type: "pointer_presentation_invalidated", receipt: receiptA });
      f.render.port2.postMessage({ type: "execution_presented", session: second.session, sequence: second.sequence, pointerReceipt: receiptB });
      f.render.port2.postMessage({ type: "pointer_presentation_invalidated", receipt: receiptB });
      await invalidatedB;
      assert.deepEqual(calls, [["presented", 1]], "repaint acknowledgement waits for old contact cleanup");
      const failure = cancelFails
        ? nextMatching(f.control.port2, m => m.type === "error" && /guarded cancellation failed/.test(m.message))
        : null;
      f.render.port2.postMessage({ type: "execution_ack", session: first.session, sequence: first.sequence });
      if (failure) await failure;
      else {
        // A later render-port message is an ordered fence after writable delivery.
        const receiptC = { ...receiptB, presentation: 3 };
        const recovered = nextMatching(f.control.port2, m =>
          m.type === "pointer_presented" && m.receipt.presentation === 3);
        f.render.port2.postMessage({ type: "execution_presented", session: second.session, sequence: second.sequence, pointerReceipt: receiptC });
        await recovered;
      }
      assert.deepEqual(calls, cancelFails
        ? [["presented", 1], ["cancel", 1]]
        : [["presented", 1], ["cancel", 1], ["presented", 3]],
      "cancel frame A, never acknowledge already-invalidated frame B");
      assert.equal(f.stats().stopped, cancelFails ? 1 : 0);
    } finally { endpoint?.stop(); f.close(); }
  });
}

for (const [width, height] of [[0, 0], [0, 400], [800, 0]]) {
  test(`unavailable pointer view ${width}x${height} cannot block registration or reveal`, async () => {
    const f = fixture();
    let endpoint, watchdog;
    try {
      f.player.setBrowserPointerViewJson = () => {};
      f.player.drainDeltaJson = () => f.player.seekDeltaJson(f.player.time());
      const ready = next(f.control.port2);
      const initial = nextMatching(f.render.port2, message => message.type === "execution_delta");
      endpoint = await f.attach(); await ready;
      const first = await initial;
      f.render.port2.postMessage({ type: "execution_ack", session: first.session, sequence: first.sequence });
      f.render.port2.postMessage({ type: "execution_presented", session: first.session, sequence: first.sequence });
      const hidden = nextMatching(f.render.port2, message => message.type === "execution_delta");
      const registered = nextMatching(f.control.port2, message => message.requestId === 501);
      f.control.port2.postMessage({ channel: "noon.engine", protocolVersion: 1,
        type: "browser_pointer_view", requestId: 501, view: { revision: 1, width, height } });
      const hiddenDelta = await hidden;
      // Consumption can succeed while a zero-sized surface cannot present.
      f.render.port2.postMessage({ type: "execution_ack", session: hiddenDelta.session, sequence: hiddenDelta.sequence });
      const reply = await Promise.race([registered, new Promise((_, reject) => {
        watchdog = setTimeout(() => reject(new Error("unavailable view registration waited for impossible presentation")), 1000);
      })]);
      clearTimeout(watchdog);
      assert.equal(reply.type, "browser_pointer_view");
      const visible = nextMatching(f.render.port2, message => message.type === "execution_delta");
      const revealed = nextMatching(f.control.port2, message => message.requestId === 502);
      f.control.port2.postMessage({ channel: "noon.engine", protocolVersion: 1,
        type: "browser_pointer_view", requestId: 502, view: { revision: 2, width: 800, height: 400 } });
      const visibleDelta = await visible;
      f.render.port2.postMessage({ type: "execution_ack", session: visibleDelta.session, sequence: visibleDelta.sequence });
      f.render.port2.postMessage({ type: "execution_presented", session: visibleDelta.session, sequence: visibleDelta.sequence });
      assert.equal((await revealed).type, "browser_pointer_view");
      assert.equal(f.player.time(), 0);
    } finally { clearTimeout(watchdog); endpoint?.stop(); f.close(); }
  });
}
