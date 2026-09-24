import assert from "node:assert/strict";
import test from "node:test";

import {
  FakeCanvas, FakeMessageChannel, FakeWorker, FakeResizeObserver, FakeSemanticAuthoringClient,
} from "./test-support/execution-fakes.mjs";

globalThis.ResizeObserver = FakeResizeObserver;

globalThis.HTMLCanvasElement = FakeCanvas;
globalThis.MessageChannel = FakeMessageChannel;
globalThis.Worker = FakeWorker;
globalThis.window = { devicePixelRatio: 1 };

const {
  AuthoringExecutionClient,
} = await import("./authoring-execution-client.js");

function envelope(channel, type, payload = {}) {
  return { channel, protocolVersion: 1, type, ...payload };
}

function renderWorker() {
  return FakeWorker.instances.findLast((worker) => worker.name === "noon-render");
}

function request(worker, type) {
  const entry = worker.messages.findLast(({ message }) => message.type === type);
  assert.ok(entry, `missing ${type}`);
  return entry.message;
}

async function waitForRequest(worker, type) {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const entry = worker.messages.findLast(({ message }) => message.type === type);
    if (entry) return entry.message;
    await Promise.resolve();
  }
  assert.fail(`missing ${type}`);
}

function replyRender(worker, type, responseType, payload = {}) {
  const sent = request(worker, type);
  worker.emitMessage(
    envelope("noon.render", responseType, {
      requestId: sent.requestId,
      transportMode: "transferable",
      backend: "WebGL2",
      ...payload,
    }),
  );
}

async function prepare(client) {
  const prepared = client.prepare({ transportMode: "transferable" });
  const render = renderWorker();
  replyRender(render, "prepare", "prepared");
  await prepared;
  return render;
}

const { MAX_IN_FLIGHT_NATIVE_INPUTS } = await import("./execution-worker-client.js");
const inputTurn = () => new Promise(resolve => setImmediate(resolve));

async function startInputClient(t, canvas = new FakeCanvas(), callbacks = {}) {
  const authoring = new FakeSemanticAuthoringClient();
  const client = new AuthoringExecutionClient(canvas, callbacks);
  t.after(() => client.terminate());
  const render = await prepare(client);
  const started = client.startSemanticExecution({ contextId: "original" }, { authoringClient: authoring });
  await waitForRequest(render, "start_engine");
  replyRender(render, "start_engine", "engine_started");
  await started;
  authoring.autoRespond = false;
  return { client, authoring, render, engine: authoring.attachments.at(-1).controlPort.peer };
}

function pointerCanvas(t) {
  const target = new EventTarget();
  const win = new EventTarget();
  const previous = globalThis.window;
  win.devicePixelRatio = 1;
  globalThis.window = win;
  t.after(() => { globalThis.window = previous; });
  const canvas = new FakeCanvas();
  const listeners = new Map();
  const rect = { left: 10, top: 20, width: 640, height: 360 };
  canvas.getBoundingClientRect = () => ({ ...rect });
  canvas.addEventListener = (type, listener, options) => {
    listeners.set(type, listener);
    target.addEventListener(type, listener, options);
  };
  let buttons = 0;
  return { canvas, listeners, rect, win, emit(type, values = {}) {
    if (type === "pointerdown") buttons = 1;
    if (type === "pointerup" || type === "pointercancel") buttons = 0;
    const event = new Event(type);
    Object.assign(event, {
      clientX: 110, clientY: 220,
      button: type === "pointermove" ? -1 : 0, buttons,
      pointerId: 1, pointerType: "mouse", isPrimary: true, ...values,
    });
    target.dispatchEvent(event);
  } };
}

// Unwrap only for the pre-existing collector assertions; worker receipt tests
// below assert the full immutable wire envelope independently.
const pointerMessages = engine => engine.messages
  .filter(message => message.type === "browser_pointer_input")
  .map(({ input, ...envelope }) => ({ ...envelope, ...input }));

// Supply raw samples; browsers can copy one parent button label into several
// samples. Test the real collector/client route, not a second click recognizer.
function coalescedSamples(values) {
  return function () {
    return values.map(value => ({
      pointerId: this.pointerId, pointerType: this.pointerType, isPrimary: this.isPrimary,
      clientX: this.clientX, clientY: this.clientY, button: this.button, buttons: this.buttons,
      timeStamp: this.timeStamp, ...value,
    }));
  };
}

function acknowledgePointers(engine) {
  for (const message of pointerMessages(engine)) {
    engine.emitMessage(envelope("noon.engine", message.type, { requestId: message.requestId }));
  }
}

test("coalesced out-and-back motion preserves every occurrence before release, not the parent summary", async t => {
  const dom = pointerCanvas(t);
  const { engine } = await startInputClient(t, dom.canvas);
  let layoutReads = 0;
  dom.canvas.getBoundingClientRect = () => { layoutReads += 1; return { ...dom.rect }; };
  dom.emit("pointerdown");
  layoutReads = 0;
  dom.emit("pointermove", {
    clientX: 111, // deliberately differs from the raw last sample
    getCoalescedEvents: coalescedSamples([
      { clientX: 150, clientY: 225, shiftKey: true },
      { clientX: 110, clientY: 220, ctrlKey: true },
    ]),
    getPredictedEvents() { assert.fail("predictions are not observed input"); },
  });
  assert.equal(layoutReads, 1, "one receipt-time viewport is shared by the entire packet");
  dom.emit("pointerup");
  await inputTurn();
  const messages = pointerMessages(engine);
  assert.deepEqual(messages.map(m => [m.kind, m.surface_x, m.surface_y]), [
    ["press", 100, 200], ["move", 140, 205], ["move", 100, 200], ["release", 100, 200],
  ]);
  assert.equal(messages[1].shift, true);
  assert.equal(messages[2].control, true);
  assert.equal(new Set(messages.map(m => m.source_id)).size, 1);
  assert.equal(new Set(messages.map(m => m.view_revision)).size, 1);
});

test("empty coalesced lists fall back once and down/up never expand an attached list", async t => {
  const dom = pointerCanvas(t);
  const { engine } = await startInputClient(t, dom.canvas);
  const forbidden = () => assert.fail("only pointermove can expand samples");
  dom.emit("pointerdown", { getCoalescedEvents: forbidden });
  dom.emit("pointermove", { getCoalescedEvents: () => [] });
  dom.emit("pointerup", { getCoalescedEvents: forbidden });
  await inputTurn();
  assert.deepEqual(pointerMessages(engine).map(m => m.kind), ["press", "move", "release"]);
});

test("coalesced packet snapshots are immutable and identical positions are not deduplicated", async t => {
  const dom = pointerCanvas(t);
  const { engine } = await startInputClient(t, dom.canvas);
  let samples;
  dom.emit("pointermove", { getCoalescedEvents() {
    samples = coalescedSamples([{ altKey: true }, { metaKey: true }]).call(this);
    return samples;
  } });
  samples[0].clientX = 999;
  samples[0].altKey = false;
  samples[1].pointerId = 99;
  await inputTurn();
  const messages = pointerMessages(engine);
  assert.equal(messages.length, 2);
  assert.deepEqual(messages.map(m => m.surface_x), [100, 100]);
  assert.equal(messages[0].alt, true);
  assert.equal(messages[1].meta, true);
  assert.equal(messages[1].pointer_id, 1);
});

test("coalesced chord labels do not duplicate button edges or erase sample-local positions", async t => {
  const dom = pointerCanvas(t);
  const { engine } = await startInputClient(t, dom.canvas);
  dom.emit("pointerdown");
  dom.emit("pointermove", { button: 2, buttons: 3,
    getCoalescedEvents: coalescedSamples([{ clientX: 115 }, { clientX: 120 }]),
  });
  dom.emit("pointermove", { button: 0, buttons: 2,
    getCoalescedEvents: coalescedSamples([{ clientX: 125 }, { clientX: 130 }]),
  });
  dom.emit("pointerup", { button: 2, buttons: 0 });
  await inputTurn();
  assert.deepEqual(pointerMessages(engine).map(m => [m.kind, m.button, m.surface_x]), [
    ["press", 0, 100], ["press", 2, 105], ["move", null, 110],
    ["release", 0, 115], ["move", null, 120], ["release", 2, 100],
  ]);
});

for (const [label, invalid] of [
  ["foreign identity", { pointerId: 2 }],
  ["foreign pointer type", { pointerType: "pen" }],
  ["nonprimary sample", { isPrimary: false }],
  ["nonfinite position", { clientX: Number.NaN }],
  ["nonfinite timestamp", { timeStamp: Infinity }],
  ["future timestamp", { timeStamp: Number.MAX_VALUE }],
  ["unsupported mask", { buttons: 64 }],
  ["ambiguous button transition", { buttons: 6 }],
  ["missing button edge", { buttons: 3, button: -1 }],
  ["inconsistent final mask", { buttons: 3, button: 2 }],
]) {
  test(`malformed coalesced ${label} faults before forwarding any packet prefix`, async t => {
    const dom = pointerCanvas(t);
    const errors = [];
    const { engine } = await startInputClient(t, dom.canvas, { onRecoverableError: e => errors.push(e) });
    dom.emit("pointerdown");
    await inputTurn();
    dom.emit("pointermove", { getCoalescedEvents: coalescedSamples([{}, invalid]) });
    dom.emit("pointerup");
    await inputTurn();
    assert.equal(errors.length, 1);
    assert.match(errors[0].message, /coalesced|DOM pointer/);
    assert.deepEqual(pointerMessages(engine).map(m => m.kind), ["press"]);
    acknowledgePointers(engine);
    await inputTurn();
    assert.deepEqual(pointerMessages(engine).map(m => m.kind), ["press", "cancel"]);
  });
}

test("coalesced timestamps preserve supplied order and reject descending history", async t => {
  const dom = pointerCanvas(t);
  const errors = [];
  const { engine } = await startInputClient(t, dom.canvas, { onRecoverableError: e => errors.push(e) });
  dom.emit("pointermove", { getCoalescedEvents: coalescedSamples([{ timeStamp: 0 }, { timeStamp: 0 }]) });
  await inputTurn();
  assert.equal(pointerMessages(engine).length, 2, "equal timestamp resolution does not merge occurrences");
  dom.emit("pointermove", { getCoalescedEvents: coalescedSamples([{ timeStamp: 1 }, { timeStamp: 0 }]) });
  await inputTurn();
  assert.equal(errors.length, 1);
  assert.match(errors[0].message, /timestamp/);
  assert.equal(pointerMessages(engine).length, 2);
});

for (const [label, getCoalescedEvents] of [
  ["unavailable data", () => null],
  ["sparse data", () => new Array(1)],
  ["throwing provider", () => { throw new Error("coalesced provider failed"); }],
]) {
  test(`coalesced ${label} is not silently replaced by the parent`, async t => {
    const dom = pointerCanvas(t);
    const errors = [];
    const { engine } = await startInputClient(t, dom.canvas, { onRecoverableError: e => errors.push(e) });
    dom.emit("pointermove", { getCoalescedEvents });
    await inputTurn();
    assert.equal(errors.length, 1);
    assert.equal(pointerMessages(engine).length, 0);
  });
}

test("foreign and nonprimary parents never inspect or inject their coalesced history", async t => {
  const dom = pointerCanvas(t);
  const { engine } = await startInputClient(t, dom.canvas);
  const forbidden = () => assert.fail("ignored contact must not inspect coalesced history");
  dom.emit("pointerdown");
  dom.emit("pointermove", { pointerId: 2, getCoalescedEvents: forbidden });
  dom.emit("pointermove", { isPrimary: false, getCoalescedEvents: forbidden });
  dom.emit("pointerup");
  await inputTurn();
  assert.deepEqual(pointerMessages(engine).map(m => m.kind), ["press", "release"]);
});

test("oversized coalesced packets fail before walking samples or posting a truncated prefix", async t => {
  const dom = pointerCanvas(t);
  const errors = [];
  const { engine } = await startInputClient(t, dom.canvas, { onRecoverableError: e => errors.push(e) });
  const oversized = new Array(MAX_IN_FLIGHT_NATIVE_INPUTS + 1);
  Object.defineProperty(oversized, 0, { get() { assert.fail("size must be checked before visiting samples"); } });
  dom.emit("pointermove", { getCoalescedEvents: () => oversized });
  dom.emit("pointerdown");
  await inputTurn();
  assert.equal(errors.length, 1);
  assert.match(errors[0].message, /coalesced.*capacity/);
  assert.equal(pointerMessages(engine).length, 0);
});

test("coalesced packets use the existing aggregate producer bound and ordered overflow cleanup", async t => {
  const dom = pointerCanvas(t);
  const errors = [];
  const { engine } = await startInputClient(t, dom.canvas, { onRecoverableError: e => errors.push(e) });
  dom.emit("pointerdown");
  dom.emit("pointermove", { getCoalescedEvents: coalescedSamples(
    Array.from({ length: MAX_IN_FLIGHT_NATIVE_INPUTS }, (_, i) => ({ clientX: 110 + i })),
  ) });
  await inputTurn();
  assert.equal(pointerMessages(engine).length, MAX_IN_FLIGHT_NATIVE_INPUTS);
  assert.equal(errors.length, 1);
  assert.match(errors[0].message, /native input.*full/i);
  dom.emit("pointerup");
  acknowledgePointers(engine);
  await inputTurn();
  const messages = pointerMessages(engine);
  assert.equal(messages.length, MAX_IN_FLIGHT_NATIVE_INPUTS + 1);
  assert.equal(messages.at(-1).kind, "cancel");
  assert.equal(messages.some(m => m.kind === "release"), false);
});

test("a coalesced resize packet cannot resume an old held gesture in the new viewport", async t => {
  const dom = pointerCanvas(t);
  const { engine } = await startInputClient(t, dom.canvas);
  dom.emit("pointerdown");
  dom.rect.width = 800;
  dom.emit("pointermove", { getCoalescedEvents: coalescedSamples([{}, {}]) });
  dom.emit("pointerup");
  await inputTurn();
  assert.deepEqual(pointerMessages(engine).map(m => m.kind), ["press", "cancel"]);
});

test("an exactly capacity-sized coalesced packet forwards all samples without a second queue", async t => {
  const dom = pointerCanvas(t);
  const errors = [];
  const { engine } = await startInputClient(t, dom.canvas, { onRecoverableError: e => errors.push(e) });
  dom.emit("pointermove", { getCoalescedEvents: coalescedSamples(
    Array.from({ length: MAX_IN_FLIGHT_NATIVE_INPUTS }, (_, i) => ({ clientX: 110 + i })),
  ) });
  await inputTurn();
  const messages = pointerMessages(engine);
  assert.equal(messages.length, MAX_IN_FLIGHT_NATIVE_INPUTS);
  assert.deepEqual(messages.map(m => m.surface_x), Array.from({ length: MAX_IN_FLIGHT_NATIVE_INPUTS }, (_, i) => 100 + i));
  assert.equal(errors.length, 0);
});

test("a coalesced array cannot override bounded ordered indexing with a custom iterator", async t => {
  const dom = pointerCanvas(t);
  const { engine } = await startInputClient(t, dom.canvas);
  dom.emit("pointermove", { getCoalescedEvents() {
    const samples = coalescedSamples([{ clientX: 111 }, { clientX: 112 }]).call(this);
    samples[Symbol.iterator] = () => assert.fail("iterate only the bounded platform list indices");
    return samples;
  } });
  await inputTurn();
  assert.deepEqual(pointerMessages(engine).map(m => m.surface_x), [101, 102]);
});

test("retired coalesced callbacks neither inspect history nor deliver to a replacement session", async t => {
  const dom = pointerCanvas(t);
  const { client, render } = await startInputClient(t, dom.canvas);
  const listener = dom.listeners.get("pointermove");
  const replacement = new FakeSemanticAuthoringClient();
  const switching = client.reconcileSemanticExecution({ contextId: "replacement" }, { authoringClient: replacement });
  await waitForRequest(render, "rebuild_engine");
  replyRender(render, "rebuild_engine", "engine_rebuilt");
  await switching;
  listener({ pointerId: 1, pointerType: "mouse", isPrimary: true,
    getCoalescedEvents() { assert.fail("retired collector must not inspect samples"); },
  });
  await inputTurn();
  assert.equal(pointerMessages(replacement.attachments.at(-1).controlPort.peer).length, 0);
});

test("held coalesced re-entry after viewport cancellation is ignored until a new press", async t => {
  const dom = pointerCanvas(t);
  const errors = [];
  const { engine } = await startInputClient(t, dom.canvas, { onRecoverableError: e => errors.push(e) });
  dom.emit("pointerdown");
  dom.emit("pointerleave");
  dom.emit("pointermove", { buttons: 1, getCoalescedEvents() {
    assert.fail("a retired held contact must not inspect or resume coalesced history");
  } });
  dom.emit("pointerup");
  await inputTurn();
  assert.deepEqual(errors, []);
  assert.deepEqual(pointerMessages(engine).map(m => m.kind), ["press", "cancel"]);
  const retired = pointerMessages(engine)[0].source_id;
  dom.emit("pointerdown"); dom.emit("pointerup");
  await inputTurn();
  const messages = pointerMessages(engine);
  assert.deepEqual(messages.map(m => m.kind), ["press", "cancel", "press", "release"]);
  assert.ok(messages[2].source_id > retired);
  assert.equal(messages[2].source_id, messages[3].source_id);
});

test("an unbound held coalesced pointer keeps ordinary parent validation without inventing a press", async t => {
  const dom = pointerCanvas(t);
  const errors = [];
  const { engine } = await startInputClient(t, dom.canvas, { onRecoverableError: e => errors.push(e) });
  dom.emit("pointermove", { buttons: 1, getCoalescedEvents() {
    assert.fail("an outside gesture has no admitted press");
  } });
  await inputTurn();
  assert.deepEqual(errors, []);
  assert.equal(pointerMessages(engine).length, 0);
  // Ignoring an outside gesture must not bypass the existing parent preflight.
  dom.emit("pointermove", { buttons: 1, clientX: NaN, getCoalescedEvents() {
    assert.fail("invalid parent must fail before sample inspection");
  } });
  await inputTurn();
  assert.equal(errors.length, 1);
  assert.match(errors[0].message, /coordinates must be finite/);
  assert.equal(pointerMessages(engine).length, 0);
});
