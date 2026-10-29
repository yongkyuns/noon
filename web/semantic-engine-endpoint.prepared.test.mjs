import assert from "node:assert/strict";
import test from "node:test";
import { MessageChannel, MessagePort } from "node:worker_threads";
import { attachSemanticEngine } from "./semantic-engine-endpoint.js";
import { decodeTransferableExecutionDelta, SharedExecutionDeltaReader } from "./execution-transport.js";

globalThis.MessagePort = MessagePort;

// Double only the Rust API boundary. Transport, ports, admission, lifecycle and
// publication ordering below are the production endpoint implementations.
function fixture(mode = "transferable", options = {}) {
  const control = new MessageChannel(), render = new MessageChannel();
  const calls = [], messages = [], deltas = [], replies = [], waiters = new Set();
  let sequence = 0, requestId = 0, endpoint, reader;
  let time = 0, frontier = 4, end, playing = !options.initiallyPaused, buffering = false;
  let observation = null, pendingDelta = false, tickDelta = false, tickFailure = null, payloadPadding;
  function changed() {
    for (const waiter of [...waiters]) {
      if (waiter.predicate()) { waiters.delete(waiter); waiter.resolve(); }
    }
  }
  const record = (name, ...values) => { calls.push([name, ...values]); changed(); };
  const count = name => calls.filter(call => call[0] === name).length;
  const when = predicate => predicate() ? Promise.resolve()
    : new Promise(resolve => waiters.add({ predicate, resolve }));
  const json = () => JSON.stringify({
    channel: "noon.execution.retained", protocol_version: 18, session: 7,
    sequence: sequence++, snapshot: sequence === 1, time, objects: [], padding: payloadPadding,
  });
  const player = {
    resourceBundleBytes() { record("resources"); return new Uint8Array([1]); },
    initialDeltaJson() { record("initial"); return json(); },
    drainDeltaTransportJson() {
      record("drain");
      if (!pendingDelta) return null;
      pendingDelta = false;
      const payload = json();
      const { channel, session, sequence, snapshot } = JSON.parse(payload);
      return `${JSON.stringify({ channel, session, sequence, snapshot })}\n${payload}`;
    },
    tickDeltaJson(wallTime) {
      record("tick", wallTime);
      if (tickFailure) throw tickFailure;
      return tickDelta ? json() : null;
    },
    seekDeltaJson(value) {
      if (!Number.isFinite(value) || value < 0 || (end === undefined ? value >= frontier : value > end)) {
        throw new Error("prepared target is unavailable");
      }
      record("seek", value); time = value;
      return json();
    },
    preparedTime: () => time,
    preparedFrontier: () => frontier,
    preparedEndTime: () => end,
    preparedPlaying: () => playing,
    preparedBuffering: () => buffering,
    preparedPlaybackTimeAt(wallTime) { record("observe-time", wallTime); return observation ?? time; },
    pausePreparedAt(wallTime) {
      record("pause-at", wallTime); time = observation ?? time; playing = false;
    },
    setPreparedPlaying(value) { record("playing", value); playing = value; },
    executionWake(wallTime) {
      record("wake", wallTime);
      return { cadence: playing && !buffering ? "animation_frame" : "idle", timerAfterMilliseconds: null };
    },
    setBrowserPointerViewJson(value) { record("view", JSON.parse(value)); },
    setPointerFillSelection(value) { record("selection", value); },
    notePointerPresentationJson(value) { record("presented", JSON.parse(value)); return true; },
    invalidatePointerPresentationJson(value) { record("invalidated", JSON.parse(value)); return false; },
    submitBrowserPointerInputJson(value) { record("pointer", JSON.parse(value)); return true; },
    scrollInspectionViewJson(value) { record("scroll", JSON.parse(value)); return true; },
    stopSourcePreparation() { record("stop-source"); },
    free() { record("free"); },
  };
  const receive = payload => { deltas.push(JSON.parse(payload)); changed(); return true; };
  render.port2.on("message", message => {
    messages.push(message);
    if (message.type === "execution_delta") receive(decodeTransferableExecutionDelta(message).json);
    if (message.type === "transport_setup") reader = new SharedExecutionDeltaReader(message.mailbox);
    if (message.type === "shared_delta") reader.drain(receive);
    changed();
  });
  control.port2.on("message", message => { replies.push(message); changed(); });
  return {
    player, calls, messages, deltas, replies, count, when,
    get endpoint() { return endpoint; },
    async attach(request = {}) {
      // No lease API is supplied: prepared startup must not depend on it.
      endpoint = await attachSemanticEngine({}, {
        controlPort: control.port1, renderPort: render.port1,
        transportMode: mode, session: 7, loopDurationSeconds: 4, ...options, ...request,
      }, () => record("cancel-source"), null, null, null, null, null,
      { player, wakeSource: () => record("source-wake") });
      await when(() => replies.some(value => value.type === "ready") && deltas.length === 1);
      return endpoint;
    },
    async request(type, fields = {}) {
      const id = ++requestId;
      control.port2.postMessage({ channel: "noon.engine", protocolVersion: 1, type, requestId: id, ...fields });
      await when(() => replies.some(value => value.requestId === id));
      return replies.find(value => value.requestId === id);
    },
    post(type, fields = {}) {
      control.port2.postMessage({ channel: "noon.engine", protocolVersion: 1, type, requestId: ++requestId, ...fields });
    },
    tick(timestamp = 123) { render.port2.postMessage({ type: "tick", timestamp }); },
    rendererMessage(message) { render.port2.postMessage(message); },
    acknowledge(delta = deltas[0]) {
      render.port2.postMessage({ type: "execution_ack", session: delta.session, sequence: delta.sequence });
      render.port2.postMessage({ type: "transport_writable" });
    },
    async present(delta = deltas[0], receipt = undefined) {
      const prior = count("wake");
      render.port2.postMessage({ type: "execution_presented", session: delta.session,
        sequence: delta.sequence, ...(receipt ? { pointerReceipt: receipt } : {}) });
      await when(() => count("wake") > prior);
    },
    observe(value) { observation = value; },
    publishOnTick(value = true) { tickDelta = value; },
    oversizePayload() { payloadPadding = "x".repeat(4096); },
    setTickFailure(error) { tickFailure = error; },
    makeDirty() { pendingDelta = true; },
    deliver(value, terminal = false) {
      frontier = value; if (terminal) end = value; buffering = false;
      endpoint.sourcePrefixDelivered();
    },
    close() {
      endpoint?.stop();
      control.port1.close(); control.port2.close(); render.port1.close(); render.port2.close();
    },
  };
}

for (const mode of ["transferable", "shared"]) {
  test(`prepared ${mode} cold startup waits for actual presentation before clock use`, { timeout: 3000 }, async () => {
    const f = fixture(mode);
    try {
      await f.attach();
      assert.equal(f.replies.find(value => value.type === "ready").preparedPlayback, true);
      assert.equal(f.messages[0].type, "retained_resources");
      assert.equal(f.deltas[0].snapshot, true);
      assert.equal(f.count("wake"), 0);
      f.acknowledge();
      f.tick();
      f.endpoint.sourcePrefixDelivered();
      const cold = await f.request("state");
      assert.equal(cold.preparedPlayback, true);
      assert.equal(cold.preparedFrontier, 4);
      assert.equal(cold.durationSeconds, null);
      assert.equal(cold.time, 0);
      assert.equal(f.count("observe-time"), 0);
      assert.equal(f.count("tick"), 0);
      assert.equal(f.count("wake"), 0, "transport consumption and future delivery do not start a clock");
      assert.equal((await f.request("pause")).playing, false);
      assert.equal(f.count("pause-at"), 0, "cold pause cannot observe wall time");
      assert.equal((await f.request("resume")).playing, true);
      assert.equal(f.count("wake"), 0);
      await f.present();
      assert.equal(f.count("wake"), 1);
      f.tick(999_999_999);
      await f.when(() => f.count("tick") === 1);
      assert.notEqual(f.calls.find(call => call[0] === "tick")[1], 999_999_999,
        "foreign worker timestamps cannot become the engine clock");
      await f.request("state");
      assert.equal(f.count("observe-time"), 1);
    } finally { f.close(); }
  });

  test(`prepared ${mode} history delivery stays quiet and keeps historical pointer receipts`, { timeout: 3000 }, async () => {
    const f = fixture(mode, { initiallyPaused: true });
    try {
      await f.attach(); f.acknowledge();
      const receipt = { session: 7, sequence: 0, presentation: 1, viewRevision: 3 };
      await f.present(undefined, receipt);
      const press = { kind: "press", presentation: receipt, sequence: 1, x: 12, y: 13 };
      assert.equal((await f.request("browser_pointer_input", press)).pointerInputAccepted, true);
      const drains = f.count("drain"), snapshots = f.count("initial"), wakes = f.count("source-wake");
      f.deliver(8); f.deliver(8, true);
      const state = await f.request("state");
      assert.equal(state.preparedFrontier, 8);
      assert.equal(state.durationSeconds, 8);
      assert.equal(state.time, 0);
      assert.equal(f.count("drain"), drains);
      assert.equal(f.count("initial"), snapshots);
      assert.equal(f.deltas.length, 1);
      assert.equal(f.count("source-wake"), wakes + 2);
      assert.equal(f.count("presented"), 1);
      const release = { ...press, kind: "release", sequence: 2 };
      assert.equal((await f.request("browser_pointer_input", release)).pointerInputAccepted, true);
      assert.deepEqual(f.calls.filter(call => call[0] === "pointer").map(call => call[1]), [press, release]);
      assert.equal((await f.request("seek", { time: 8 })).time, 8);
    } finally { f.close(); }
  });
}

test("prepared backpressure preserves accepted control order and never extracts blocked output", { timeout: 3000 }, async () => {
  const f = fixture();
  try {
    await f.attach();
    await f.present(); // Intentionally withhold transport acknowledgements.
    assert.equal((await f.request("seek", { time: 1 })).time, 1);
    await f.when(() => f.deltas.length === 2);
    const wakes = f.count("source-wake");
    const blocked = f.request("seek", { time: 2 });
    const observed = await f.request("state");
    assert.equal(observed.time, 1);
    assert.equal(f.count("seek"), 1);
    assert.equal(f.count("source-wake"), wakes);
    f.acknowledge(f.deltas[0]);
    assert.equal((await blocked).time, 2);
    await f.when(() => f.deltas.length === 3);
    assert.deepEqual(f.deltas.map(value => value.sequence), [0, 1, 2]);
    assert.deepEqual(f.calls.filter(call => call[0] === "seek").map(call => call[1]), [1, 2]);
    f.publishOnTick(); f.tick();
    await f.request("state");
    assert.equal(f.count("tick"), 0);
    f.acknowledge(f.deltas[1]);
    await f.when(() => f.deltas.length === 4);
    assert.equal(f.count("tick"), 1);
    assert.deepEqual(f.deltas.map(value => value.sequence), [0, 1, 2, 3]);
  } finally { f.close(); }
});

test("cold prepared seek starts its clock only after the newly selected frame presents", { timeout: 3000 }, async () => {
  const f = fixture();
  try {
    await f.attach();
    assert.equal((await f.request("seek", { time: 1 })).time, 1);
    await f.when(() => f.deltas.length === 2);
    f.acknowledge(f.deltas[0]);
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 0,
      pointerReceipt: { session: 7, sequence: 0, presentation: 1 } });
    await f.when(() => f.count("presented") === 1);
    assert.equal((await f.request("state")).time, 1);
    assert.equal(f.count("wake"), 0);
    assert.equal(f.count("observe-time"), 0);
    f.tick();
    await f.request("state");
    assert.equal(f.count("tick"), 0);
    await f.present(f.deltas[1]);
    assert.equal(f.count("wake"), 1);
    await f.request("state");
    assert.equal(f.count("observe-time"), 1);
  } finally { f.close(); }
});

test("an oversized prepared shared publication retires the stream after extraction", { timeout: 3000 }, async () => {
  const f = fixture("shared", { sharedSlotCapacity: 512 });
  try {
    await f.attach(); await f.present();
    f.oversizePayload();
    const failed = await f.request("seek", { time: 1 });
    assert.equal(failed.type, "error");
    assert.match(failed.message, /capacity|capacity|bytes|size|slot/i);
    assert.equal(f.count("seek"), 1);
    assert.equal(f.count("cancel-source"), 1);
    assert.equal(f.count("free"), 1);
    assert.equal(f.deltas.length, 1);
    assert.throws(() => f.endpoint.sourcePrefixDelivered(), /stopped/);
    f.post("seek", { time: 2 });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(f.count("seek"), 1, "a missing transport sequence cannot be retried as another control");
  } finally { f.close(); }
});

test("prepared pause commits Rust's static-wait sample and unsupported controls leave it unchanged", { timeout: 3000 }, async () => {
  const f = fixture();
  try {
    await f.attach(); f.acknowledge(); await f.present();
    f.observe(0.75);
    assert.equal((await f.request("state")).time, 0.75);
    const paused = await f.request("pause");
    assert.equal(paused.time, 0.75);
    assert.equal(paused.playing, false);
    assert.equal(f.count("pause-at"), 1);
    const mutations = f.calls.length;
    for (const type of ["set_loop_duration", "advance_to", "sample_to_authored_time",
      "native_state_input", "native_event", "undo_translation_drag", "debug_frame"]) {
      const result = await f.request(type, { time: 2, loopDurationSeconds: 8 });
      assert.equal(result.type, "error"); assert.match(result.message, /prepared playback does not support/);
    }
    assert.equal(f.calls.length, mutations);
    const rejected = await f.request("seek", { time: 4 });
    assert.equal(rejected.type, "error"); assert.match(rejected.message, /unavailable/);
    assert.equal((await f.request("resume")).playing, true);
  } finally { f.close(); }
});

test("prepared stop cancels source before freeing its reader and rejects late adoption", { timeout: 3000 }, async () => {
  const f = fixture();
  try {
    await f.attach();
    f.endpoint.stop(); f.endpoint.stop();
    assert.deepEqual(f.calls.slice(-3).map(call => call[0]), ["cancel-source", "stop-source", "free"]);
    assert.equal(f.count("cancel-source"), 1); assert.equal(f.count("free"), 1);
    assert.throws(() => f.endpoint.sourcePrefixDelivered(), /stopped/);
    assert.equal(f.count("wake"), 0);
  } finally { f.close(); }
});

test("prepared bootstrap failure cancels source and releases the adopted reader", { timeout: 3000 }, async () => {
  const f = fixture();
  try {
    f.player.initialDeltaJson = () => { throw new Error("initial prepared publication failed"); };
    await assert.rejects(f.attach(), /initial prepared publication failed/);
    assert.deepEqual(f.calls.slice(-3).map(call => call[0]), ["cancel-source", "stop-source", "free"]);
    assert.equal(f.count("wake"), 0);
  } finally { f.close(); }
});

for (const [label, request, message] of [
  ["session", { session: 0x1_0000_0000 }, /session must fit u32/],
  ["transport", { transportMode: "unsupported" }, /unsupported semantic execution transport/],
]) {
  test(`prepared invalid ${label} setup releases its adopted reader after cancelling source`, { timeout: 3000 }, async () => {
    const f = fixture();
    try {
      await assert.rejects(f.attach(request), message);
      assert.deepEqual(f.calls.map(call => call[0]), ["cancel-source", "stop-source", "free"]);
      assert.equal(f.count("cancel-source"), 1);
      assert.equal(f.count("free"), 1);
      assert.equal(f.count("resources"), 0);
      assert.equal(f.deltas.length, 0);
    } finally { f.close(); }
  });
}

test("prepared progression error retires the source once instead of retrying a tick", { timeout: 3000 }, async () => {
  const f = fixture();
  try {
    await f.attach(); f.acknowledge(); await f.present();
    f.setTickFailure(new Error("typed prepared advance failed"));
    f.tick();
    await f.when(() => f.replies.some(value => value.type === "error"));
    assert.match(f.replies.find(value => value.type === "error").message, /typed prepared advance failed/);
    assert.equal(f.count("tick"), 1);
    assert.deepEqual(f.calls.slice(-3).map(call => call[0]), ["cancel-source", "stop-source", "free"]);
    assert.throws(() => f.endpoint.sourcePrefixDelivered(), /stopped/);
  } finally { f.close(); }
});

test("prepared render failure cancels a suspended source and retires its endpoint once", { timeout: 3000 }, async () => {
  const f = fixture(undefined, { initiallyPaused: true });
  try {
    await f.attach();
    f.rendererMessage({ type: "render_error", message: "prepared render device lost" });
    await f.when(() => f.replies.some(value => value.type === "error"));
    assert.match(f.replies.find(value => value.type === "error").message, /device lost/);
    assert.deepEqual(f.calls.slice(-3).map(call => call[0]), ["cancel-source", "stop-source", "free"]);
    assert.equal(f.count("cancel-source"), 1);
    assert.throws(() => f.endpoint.sourcePrefixDelivered(), /stopped/);
  } finally { f.close(); }
});

for (const stage of ["bootstrap", "tick", "control", "state", "notification"]) {
  test(`prepared ${stage} WASM trap retires the host before source cancellation without Rust cleanup`, { timeout: 3000 }, async () => {
    const f = fixture(), trap = new WebAssembly.RuntimeError(`trapped ${stage}`);
    const previous = globalThis.noonReportEngineTrap;
    globalThis.noonReportEngineTrap = error => { f.calls.push(["fatal", error]); };
    try {
      if (stage === "bootstrap") {
        f.player.initialDeltaJson = () => { throw trap; };
        await assert.rejects(f.attach(), error => error === trap);
      } else {
        await f.attach(); f.acknowledge(); await f.present();
        if (stage === "tick") { f.setTickFailure(trap); f.tick(); }
        if (stage === "control") {
          f.player.seekDeltaJson = () => { throw trap; };
          f.post("seek", { time: 1 });
        }
        if (stage === "state") {
          f.player.preparedPlaybackTimeAt = () => { throw trap; };
          f.post("state");
        }
        if (stage === "notification") {
          f.player.executionWake = () => { throw trap; };
          assert.throws(() => f.endpoint.sourcePrefixDelivered(), error => error === trap);
        }
        await f.when(() => f.count("cancel-source") === 1);
      }
      assert.deepEqual(f.calls.slice(-2), [["fatal", trap], ["cancel-source"]]);
      assert.equal(f.count("fatal"), 1);
      assert.equal(f.count("stop-source"), 0);
      assert.equal(f.count("free"), 0, "a trapped Rust borrow is released by host teardown only");
      f.endpoint?.stop();
      assert.equal(f.count("cancel-source"), 1);
    } finally {
      if (previous === undefined) delete globalThis.noonReportEngineTrap;
      else globalThis.noonReportEngineTrap = previous;
      f.close();
    }
  });
}
