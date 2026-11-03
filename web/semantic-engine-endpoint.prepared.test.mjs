import assert from "node:assert/strict";
import test from "node:test";
import { MessageChannel, MessagePort } from "node:worker_threads";
import { attachSemanticEngine } from "./semantic-engine-endpoint.js";
import { decodeTransferableExecutionDelta, SharedExecutionDeltaReader, MAX_SHARED_EXECUTION_DELTA_BYTES } from "./execution-transport.js";

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
  let loopDuration = null, loopCount = 0, noChangeProof = null;
  const seekDeltaJson = value => {
    if (!Number.isFinite(value) || value < 0 || (end === undefined ? value >= frontier : value > end)) {
      throw new Error("prepared target is unavailable");
    }
    record("seek", value); time = value;
    return json();
  };
  const player = {
    sectionsJson: () => "[]",
    preparedRetentionMetricsJson: () => '{"pending":{"batches":1}}',
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
    seekDeltaJson,
    preparedTime: () => time,
    takePreparedNoChangePresentationJson() {
      const proof = noChangeProof ?? { session: 7, sequence: sequence - 1, time };
      noChangeProof = null;
      return proof === null ? null : JSON.stringify(proof);
    },
    preparedFrontier: () => frontier,
    preparedEndTime: () => end,
    preparedPlaying: () => playing,
    preparedBuffering: () => buffering,
    preparedPlaybackStateJson: () => JSON.stringify({ time, frontier, end: end ?? null,
      seekRange: { start: 0, end: end ?? Math.max(0, frontier - Number.EPSILON * Math.max(1, frontier)) },
      playing, buffering, completed: end === 0 || (end != null && loopDuration === null && time >= end),
      advancing: playing && !buffering, loopDuration, loopCount, loopError: null }),
    setPreparedLoopDuration(value) { record("loop", value); loopDuration = value ?? null; },
    sourceExecutionReady: () => options.sourceExecutionRequired === true && buffering,
    preparedPlaybackTimeAt(wallTime) { record("observe-time", wallTime); return observation ?? time; },
    pausePreparedAt(wallTime) {
      record("pause-at", wallTime); time = observation ?? time; playing = false;
      noChangeProof = { session: 7, sequence: sequence - 1, time };
    },
    setPreparedPlaying(value) {
      record("playing", value); playing = value;
      noChangeProof = { session: 7, sequence: sequence - 1, time };
    },
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
    if (message.type === "shared_delta" && reader.drain(receive) > 0) {
      render.port2.postMessage({ type: "transport_writable" });
    }
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
      { player, wakeSource: () => record("source-wake"), generation: 19 }, options.onExecutionDiagnostic ?? null);
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
    noDeltaSeek(proof = {}) {
      player.seekDeltaJson = value => {
        record("seek", value); time = value;
        noChangeProof = { session: 7, sequence: sequence - 1, time, ...proof };
        return null;
      };
    },
    restoreDeltaSeek() { noChangeProof = null; player.seekDeltaJson = seekDeltaJson; },
    buffer() { buffering = true; },
    publishOnTick(value = true) { tickDelta = value; },
    oversizePayload(bytes = 4096) { payloadPadding = "x".repeat(bytes); },
    setTickFailure(error) { tickFailure = error; },
    makeDirty() { pendingDelta = true; },
    deliver(value, terminal = false) {
      frontier = value; if (terminal) end = value; buffering = false;
      endpoint.sourcePrefixDelivered();
    },
    wrap() { time = 0; loopCount++; },
    close() {
      endpoint?.stop();
      control.port1.close(); control.port2.close(); render.port1.close(); render.port2.close();
    },
  };
}

for (const mode of ["transferable", "shared"]) {
  test(`prepared ${mode} diagnostics report actual adoption and first Rust wrap once`, async () => {
    const events = [], f = fixture(mode, { onExecutionDiagnostic: event => events.push(event) });
    try {
      await f.attach();
      assert.deepEqual(events.map(event => event.stage), ["attached"]);
      assert.equal(events[0].capabilities.replay, true);
      assert.ok(events[0].capabilities.commands.includes("loop"));
      f.deliver(4, true);
      await f.request("state");
      assert.deepEqual(events.map(event => event.stage), ["attached", "terminal-adopted"]);
      await f.request("set_loop_duration", { loopDurationSeconds: 4 });
      f.wrap();
      await f.request("state"); await f.request("state");
      assert.deepEqual(events.map(event => event.stage), ["attached", "terminal-adopted", "disposition"]);
      assert.equal(events.at(-1).disposition, "looped");
      assert.equal(events.at(-1).loopCount, 1);
    } finally { f.close(); }
  });
  test(`prepared ${mode} source handoff waits for demand, quiesces and frees on retirement`, { timeout: 3000 }, async () => {
    const f = fixture(mode, { sourceExecutionRequired: true });
    try {
      await f.attach();
      f.acknowledge(); await f.present();
      f.endpoint.sourceExecutionRequired();
      assert.equal(f.replies.some(message => message.type === "source_handoff_required"), false);
      f.buffer();
      f.endpoint.sourceExecutionRequired();
      await f.when(() => f.replies.some(message => message.type === "source_handoff_required"));
      assert.equal(f.replies.find(message => message.type === "source_handoff_required").generation, 19);
      f.endpoint.sourceExecutionRequired();
      const ticks = f.count("tick");
      f.tick();
      await new Promise(resolve => setImmediate(resolve));
      assert.equal(f.count("tick"), ticks);
      assert.equal(f.count("free"), 0, "old picture remains owned until replacement retires its endpoint");
      f.post("stop");
      await f.when(() => f.count("free") === 1);
      assert.equal(f.count("free"), 1);
      assert.equal(f.count("stop-source"), 1);
      assert.equal(f.count("cancel-source"), 1);
    } finally { f.close(); }
  });
  test(`prepared ${mode} cold startup waits for actual presentation before clock use`, { timeout: 3000 }, async () => {
    const f = fixture(mode);
    try {
      await f.attach();
      assert.equal(f.replies.find(value => value.type === "ready").preparedPlayback, true);
      assert.deepEqual(f.replies.find(value => value.type === "ready").seekRange,
        { start: 0, end: 4 - Number.EPSILON * 4 });
      assert.equal(f.messages[0].type, "retained_resources");
      assert.equal(f.deltas[0].snapshot, true);
      assert.equal(f.count("wake"), 0);
      f.acknowledge();
      f.tick();
      f.endpoint.sourcePrefixDelivered();
      const cold = await f.request("state");
      assert.equal(cold.preparedPlayback, true);
      assert.equal(cold.preparedFrontier, 4);
      assert.deepEqual(cold.seekRange, { start: 0, end: 4 - Number.EPSILON * 4 });
      assert.equal(cold.durationSeconds, null);
      assert.equal(cold.time, 0);
      assert.equal(f.count("observe-time"), 0);
      assert.equal(f.count("tick"), 0);
      assert.equal(f.count("wake"), 0, "transport consumption and future delivery do not start a clock");
      const pausing = f.request("pause");
      await new Promise(resolve => setImmediate(resolve));
      assert.equal(f.replies.some(reply => reply.type === "pause"), false,
        "prepared pause waits for the cached initial picture's matching receipt");
      await f.present();
      const paused = await pausing;
      assert.equal(paused.playing, false);
      assert.ok(paused.commandTiming.presentationSettledEpochMs >= paused.commandTiming.acceptedEpochMs);
      assert.deepEqual(paused.presentationReceipt, {
        session: 7, sequence: f.deltas[0].sequence, time: 0, unchanged: true,
      });
      assert.equal(f.count("pause-at"), 0, "cold pause cannot observe wall time");
      const resumed = await f.request("resume");
      assert.equal(resumed.playing, true);
      assert.ok(resumed.commandTiming.presentationSettledEpochMs >= resumed.commandTiming.acceptedEpochMs);
      assert.deepEqual(resumed.presentationReceipt, paused.presentationReceipt,
        "resume reuses the already-presented cached frame without forcing a render");
      assert.ok(f.count("wake") >= 1, "the matching presentation admits subsequent source wakes");
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
      assert.deepEqual(state.seekRange, { start: 0, end: 8 });
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
      const seeking = f.request("seek", { time: 8 });
      await f.when(() => f.deltas.length === 2);
      f.rendererMessage({ type: "execution_presented", session: 7, sequence: 1 });
      assert.equal((await seeking).time, 8);
    } finally { f.close(); }
  });
}

test("prepared backpressure preserves accepted control order and never extracts blocked output", { timeout: 3000 }, async () => {
  const f = fixture();
  try {
    await f.attach();
    await f.present(); // Intentionally withhold transport acknowledgements.
    const first = f.request("seek", { time: 1 });
    await f.when(() => f.deltas.length === 2);
    const wakes = f.count("source-wake");
    const blocked = f.request("seek", { time: 2 });
    assert.equal(f.count("seek"), 1);
    assert.equal(f.count("source-wake"), wakes);
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 1 });
    assert.equal((await first).time, 1);
    f.acknowledge(f.deltas[0]);
    await f.when(() => f.deltas.length === 3);
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 2 });
    assert.equal((await blocked).time, 2);
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
    const seeking = f.request("seek", { time: 1 });
    await f.when(() => f.deltas.length === 2);
    f.acknowledge(f.deltas[0]);
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 0,
      pointerReceipt: { session: 7, sequence: 0, presentation: 1 } });
    await f.when(() => f.count("presented") === 1);
    assert.equal(f.replies.some(reply => reply.requestId === 1), false,
      "the seek stays pending until its own publication is displayed");
    assert.equal(f.player.preparedTime(), 1);
    assert.equal(f.count("wake"), 0);
    assert.equal(f.count("observe-time"), 0);
    f.tick();
    assert.equal(f.count("tick"), 0);
    await f.present(f.deltas[1]);
    assert.equal((await seeking).time, 1);
    assert.equal(f.count("wake"), 1);
    await f.request("state");
    assert.equal(f.count("observe-time"), 1);
  } finally { f.close(); }
});

test("prepared seek returns a display receipt only for its matching publication", { timeout: 3000 }, async () => {
  const f = fixture();
  try {
    await f.attach();
    const seeking = f.request("seek", { time: 1 });
    await f.when(() => f.deltas.length === 2);
    assert.equal(f.replies.some(reply => reply.type === "seek"), false);
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 1 });
    const result = await seeking;
    assert.equal(result.time, 1);
    assert.deepEqual(result.presentationReceipt, { session: 7, sequence: 1, time: 1, unchanged: false });
  } finally { f.close(); }
});

test("no-delta seek joins the latest publication and a stale receipt cannot settle it", { timeout: 3000 }, async () => {
  const f = fixture();
  try {
    await f.attach();
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 0 });
    await f.when(() => f.count("wake") > 0);

    f.publishOnTick();
    await f.request("resume");
    f.tick();
    await f.when(() => f.deltas.length === 2);
    f.acknowledge(f.deltas[0]); // Free transport capacity without presenting the newer frame.
    f.noDeltaSeek({ time: 0.25 });
    const invalidProof = await f.request("seek", { time: 0.5 });
    assert.equal(invalidProof.type, "error");
    assert.match(invalidProof.message, /no-change seek.*Rust presentation proof/);

    f.noDeltaSeek();
    const seekingUnchanged = f.request("seek", { time: 0.5 });
    await new Promise(resolve => setImmediate(resolve));
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 0 });
    assert.equal(f.replies.some(reply => reply.type === "seek"), false,
      "an older displayed frame cannot settle a no-delta seek while a newer publication is pending");
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 1 });
    const result = await seekingUnchanged;
    assert.equal(result.time, 0.5);
    assert.deepEqual(result.presentationReceipt, { session: 7, sequence: 1, time: 0.5, unchanged: true });

    f.restoreDeltaSeek();
    const seeking = f.request("seek", { time: 1 });
    await f.when(() => f.deltas.length === 3);
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 1 });
    assert.equal(f.replies.some(reply => reply.requestId === 4), false);
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 2 });
    const next = await seeking;
    assert.deepEqual(next.presentationReceipt, { session: 7, sequence: 2, time: 1, unchanged: false });
  } finally { f.close(); }
});

test("a receipt outside the bounded metadata window cannot replace a validated display", { timeout: 3000 }, async () => {
  const f = fixture();
  try {
    await f.attach();
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 0 });
    await f.when(() => f.count("wake") > 0);
    f.publishOnTick();
    await f.request("resume");
    for (let count = 2; count <= 4; count += 1) {
      f.tick();
      await f.when(() => f.deltas.length === count);
      f.acknowledge(f.deltas[count - 2]);
    }

    f.noDeltaSeek();
    const seeking = f.request("seek", { time: 0.5 });
    await f.when(() => f.count("seek") > 0);
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 1 });
    assert.equal(f.replies.some(reply => reply.type === "error"), false);
    assert.equal(f.replies.some(reply => reply.type === "seek"), false,
      "an unretained older receipt cannot settle the seek or advance displayed metadata");
    f.rendererMessage({ type: "execution_presented", session: 7, sequence: 3 });
    const result = await seeking;
    assert.deepEqual(result.presentationReceipt, { session: 7, sequence: 3, time: 0.5, unchanged: true });
  } finally { f.close(); }
});

test("exceeding the prepared shared packet byte cap retires the stream after extraction", { timeout: 3000 }, async () => {
  const f = fixture("shared", { sharedSlotCapacity: 512 });
  try {
    await f.attach(); await f.present();
    f.oversizePayload(MAX_SHARED_EXECUTION_DELTA_BYTES + 1);
    const failed = await f.request("seek", { time: 1 });
    assert.equal(failed.type, "error");
    assert.match(failed.message, /packet byte limit/);
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
    const calls = f.calls.length;
    assert.deepEqual((await f.request("metrics")).metrics.preparedRetention, { pending: { batches: 1 } });
    assert.equal(f.calls.length, calls, "metrics cannot drain or advance the reader");
    assert.equal((await f.request("state")).time, 0.75);
    const paused = await f.request("pause");
    assert.equal(paused.time, 0.75);
    assert.equal(paused.playing, false);
    assert.equal(f.count("pause-at"), 1);
    const loop = await f.request("set_loop_duration", { loopDurationSeconds: 8 });
    assert.equal(loop.loopDuration, 8);
    assert.equal(loop.playing, false, "loop configuration preserves Pause");
    const mutations = f.calls.length;
    for (const type of ["advance_to", "sample_to_authored_time",
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

test("prepared render failure freezes its reader until explicit retirement", { timeout: 3000 }, async () => {
  const f = fixture(undefined, { initiallyPaused: true });
  try {
    await f.attach();
    f.rendererMessage({ type: "render_error", message: "prepared render device lost" });
    await f.when(() => f.replies.some(value => value.type === "error"));
    assert.match(f.replies.find(value => value.type === "error").message, /device lost/);
    assert.equal(f.count("free"), 0);
    assert.equal(f.count("stop-source"), 0);
    f.endpoint.sourcePrefixDelivered();
    const rejected = await f.request("seek", { time: 0.5 });
    assert.equal(rejected.type, "error");
    assert.equal(f.count("seek"), 0);
    f.endpoint.stop();
    assert.equal(f.count("free"), 1);
    assert.equal(f.count("cancel-source"), 1);
  } finally { f.close(); }
});

test("prepared prefix audio extends only an already playing Rust anchor", async () => {
  const f = fixture(undefined, { initiallyPaused: true });
  Object.assign(f.player, {
    hasAudio: () => true,
    isPlaying: () => f.player.preparedPlaying(),
    time: () => f.player.preparedTime(),
    finiteAudioDuration: () => null,
    audioResourceBundleBytes: () => new Uint8Array([1]),
    audioAnchorJsonAt: (wallTime, sampledEpochMs) => JSON.stringify({
        session: 7, time: f.player.preparedPlaybackTimeAt(wallTime),
        playing: f.player.preparedPlaying(), sampledEpochMs,
        authorizedUntil: f.player.preparedFrontier(),
      }),
    tickCallbackPhaseJson: () => null,
  });
  try {
    await f.attach();
    const ready = f.replies.find(message => message.type === "ready");
    assert.equal(ready.hasAudio, true);
    assert.equal(ready.durationSeconds, null, "an open reader frontier is not a finite audio endpoint");
    assert.deepEqual(ready.audioResources, new Uint8Array([1]));
    f.deliver(5);
    assert.equal(f.replies.some(message => message.type === "audio_state"), false,
      "buffer growth cannot start a paused device");
    const resumed = f.request("resume");
    await f.present();
    const result = await resumed;
    assert.ok(result.audioAnchor, JSON.stringify(result));
    assert.equal(result.audioAnchor.playing, true);
    assert.equal(result.audioAnchor.authorizedUntil, 5);
    f.deliver(6);
    await f.when(() => f.replies.some(message => message.type === "audio_state"));
    const extension = f.replies.find(message => message.type === "audio_state");
    assert.equal(extension.audioAnchor.playing, true);
    assert.equal(extension.audioAnchor.authorizedUntil, 6);
    assert.equal(extension.audioAnchor.time, f.player.preparedTime());
    f.endpoint.sourcePrefixDelivered();
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(f.replies.filter(message => message.type === "audio_state").length, 1,
      "repeated delivery without new Rust authorization does not re-anchor the device");
  } finally { f.close(); }
});

test("prepared render failure commits observed static-wait time without losing playback intent", { timeout: 3000 }, async () => {
  const f = fixture();
  try {
    await f.attach(); f.acknowledge(); await f.present();
    f.observe(0.75);
    assert.equal((await f.request("state")).time, 0.75);
    assert.equal(f.player.preparedTime(), 0, "observation alone does not evaluate a frame");
    const sourceWakes = f.count("source-wake"), deltas = f.deltas.length;
    f.rendererMessage({ type: "render_error", message: "device lost during static wait" });
    await f.when(() => f.replies.some(value => value.type === "error"));
    assert.equal(f.player.preparedTime(), 0.75, "receiver reset must not lose previously observed elapsed time");
    assert.equal(f.player.preparedPlaying(), true);
    f.observe(10);
    assert.equal((await f.request("state")).time, 0.75, "failed receiver time stays frozen during downtime");
    assert.equal(f.count("source-wake"), sourceWakes);
    assert.equal(f.deltas.length, deltas);
    assert.equal(f.count("free"), 0);
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


test("prepared shared chunks do not admit the selected clock before the complete frame presents", { timeout: 3000 }, async () => {
  const f = fixture("shared", { sharedSlotCapacity: 512 });
  try {
    await f.attach();
    const wakes = f.count("wake");
    f.oversizePayload();
    const seeking = f.request("seek", { time: 1 });
    await f.when(() => f.deltas.length === 2);
    assert.equal(f.deltas[1].padding.length, 4096);
    assert.equal(f.count("seek"), 1);
    assert.equal(f.count("cancel-source"), 0);
    assert.equal(f.count("free"), 0);
    assert.equal(f.count("wake"), wakes, "chunk credit is not a displayed-frame receipt");
    await f.present(f.deltas[1]);
    assert.equal((await seeking).type, "seek");
    assert.ok(f.count("wake") > wakes);
  } finally { f.close(); }
});
