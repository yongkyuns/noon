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
function fixture(transportMode = "transferable", runRequiredCallbackPhase = null) {
  const control = new MessageChannel();
  const render = new MessageChannel();
  let time = 0, playing = true, sequence = 0, returned = 0, returnedPlayer = null, stopped = 0;
  const nativeInputs = [];
  const json = () => JSON.stringify({ channel: "noon.execution.retained", protocol_version: 4, session: 7, sequence: sequence++, snapshot: sequence === 1, time, objects: [] });
  const player = {
    initialDeltaJson: json,
    initialCallbackPhaseJson: () => null,
    resourceBundleBytes: () => new Uint8Array([1]),
    tickCallbackPhaseJson: () => null,
    advanceForwardToCallbackPhaseJson: (value) => {
      if (!Number.isFinite(value) || value < time) throw new Error("invalid forward time");
      time = value;
      return null;
    },
    drainDeltaJson: () => null,
    commitCallbackPhaseJson: () => {},
    failCallbackPhaseJson: () => {},
    callbackTerminationJson: () => null,
    tickDeltaJson: () => null,
    seekDeltaJson: (value) => { if (!Number.isFinite(value)) throw new Error("invalid time"); time = value; return json(); },
    setNativeStateInputJson: (value) => { nativeInputs.push({ type: "state", value: JSON.parse(value) }); },
    emitNativeEventJson: (value) => { nativeInputs.push({ type: "event", value: JSON.parse(value) }); },
    setLoopDuration: () => {}, pause: () => { playing = false; }, resume: () => { playing = true; },
    time: () => time, isPlaying: () => playing,
  };
  const context = {
    createExecutionPlayer: () => player,
    returnExecutionPlayer: (value) => { returned += 1; returnedPlayer = value; },
  };
  return { control, render, player, stats: () => ({ returned, returnedPlayer, stopped, nativeInputs }),
    attach: () => attachSemanticEngine(context, {
      controlPort: control.port1, renderPort: render.port1, session: 7,
      loopDurationSeconds: 2, transportMode,
    }, () => { stopped += 1; }, runRequiredCallbackPhase),
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
    f.player.advanceForwardToCallbackPhaseJson = (time) => {
      assert.equal(time, 1.0);
      advanceForward(time);
      return JSON.stringify({ token: { sequence: "1" }, time });
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
