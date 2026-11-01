import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

// Import the unchanged production adapter beside an isolated fake WASM module.
// Only the platform boundary is replaced; no source rewriting or timing sleeps.
const wasmFixture = `
export const state = {
  initializations: 0,
  initialize: async () => {},
  constructorError: null,
  startError: null,
  outputs: [],
};
export default async function initialize() {
  state.initializations += 1;
  await state.initialize();
}
export class WasmWorkerAudioOutput {
  constructor(bytes) {
    if (state.constructorError) throw state.constructorError;
    this.bytes = Array.from(bytes);
    this.context = new EventTarget();
    this.starts = [];
    this.stops = 0;
    this.frees = 0;
    this.pauses = 0;
    state.outputs.push(this);
  }
  setVolume(value) { this.volume = value; }
  setMuted(value) { this.muted = value; }
  audioContext() { return this.context; }
  ready() { return true; }
  unlock() { return Promise.resolve(); }
  start(...args) {
    this.starts.push(args);
    if (state.startError) throw state.startError;
  }
  pause() { this.pauses += 1; }
  stop() { this.stops += 1; }
  free() { this.frees += 1; }
}
`;

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "noon-worker-audio-"));
  const devices = [];
  t.after(async () => {
    try {
      for (const device of devices) device.dispose();
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
  await mkdir(join(root, "pkg"));
  await writeFile(join(root, "package.json"), '{"type":"module"}\n');
  await copyFile(new URL("./worker-audio-device.js", import.meta.url),
    join(root, "worker-audio-device.js"));
  await writeFile(join(root, "pkg/noon_web.js"), wasmFixture);
  const { state } = await import(pathToFileURL(join(root, "pkg/noon_web.js")).href);
  const { WorkerAudioDevice } = await import(pathToFileURL(join(root, "worker-audio-device.js")).href);
  return {
    state,
    device() {
      const device = new WorkerAudioDevice(() => assert.fail("unexpected audio interruption"));
      devices.push(device);
      return device;
    },
  };
}

const bundle = new Uint8Array([1, 2, 3]);

test("a failed WASM initialization does not poison the next explicit install", async t => {
  const { state, device: createDevice } = await fixture(t);
  const device = createDevice();
  const failure = new Error("WASM initialization failed");
  state.initialize = async () => { throw failure; };
  await assert.rejects(device.install(bundle), error => error === failure);
  assert.equal(state.initializations, 1);
  assert.equal(device.available, false);
  assert.equal(state.outputs.length, 0);

  // Recovery does not retry automatically or replay Play. Only a new install
  // may initialize again, and preparation must still remain silent.
  state.initialize = async () => {};
  assert.equal(state.initializations, 1);
  await device.install(bundle);
  assert.equal(state.initializations, 2);
  assert.equal(device.available, true);
  assert.deepEqual(state.outputs[0].starts, []);
});

test("concurrent callers share both the failed attempt and its explicit retry", async t => {
  const { state, device: createDevice } = await fixture(t);
  const first = createDevice();
  const second = createDevice();
  const entered = deferred();
  const initialization = deferred();
  state.initialize = () => { entered.resolve(); return initialization.promise; };
  const results = Promise.allSettled([first.install(bundle), second.install(bundle)]);
  await entered.promise;
  assert.equal(state.initializations, 1);
  const failure = new Error("shared initialization failed");
  initialization.reject(failure);
  for (const result of await results) {
    assert.equal(result.status, "rejected");
    assert.equal(result.reason, failure);
  }
  assert.equal(state.outputs.length, 0);

  state.initialize = async () => {};
  await Promise.all([first.install(bundle), second.install(bundle)]);
  assert.equal(state.initializations, 2);
  assert.equal(state.outputs.length, 2);
  assert.equal(first.available, true);
  assert.equal(second.available, true);
});

test("successful initialization is reused across devices and replacement installs", async t => {
  const { state, device: createDevice } = await fixture(t);
  const first = createDevice();
  first.setVolume(0.4);
  first.setMuted(true);
  await first.install(bundle);
  const original = state.outputs[0];
  await first.install(new Uint8Array([4]));
  await createDevice().install(bundle);
  assert.equal(state.initializations, 1);
  assert.equal(original.stops, 1);
  assert.equal(original.frees, 1);
  assert.equal(state.outputs[1].volume, 0.4);
  assert.equal(state.outputs[1].muted, true);
  assert.deepEqual(state.outputs[1].bytes, [4]);
  assert.ok(state.outputs.every(output => output.starts.length === 0));
});

test("disposing during initialization cannot publish a retired output", async t => {
  const { state, device: createDevice } = await fixture(t);
  const device = createDevice();
  const entered = deferred();
  const initialization = deferred();
  state.initialize = () => { entered.resolve(); return initialization.promise; };
  const retired = assert.rejects(device.install(bundle), /preparation was retired/);
  await entered.promise;
  device.dispose();
  initialization.resolve();
  await retired;
  assert.equal(device.available, false);
  assert.equal(state.outputs.length, 0);
  await device.install(bundle);
  assert.equal(state.initializations, 1);
  assert.equal(state.outputs.length, 1);
});

test("only the latest replacement install publishes after shared initialization", async t => {
  const { state, device: createDevice } = await fixture(t);
  const device = createDevice();
  const entered = deferred();
  const initialization = deferred();
  state.initialize = () => { entered.resolve(); return initialization.promise; };
  const retired = assert.rejects(device.install(bundle), /preparation was retired/);
  await entered.promise;
  const replacement = device.install(new Uint8Array([4, 5]));
  device.setVolume(0.25);
  device.setMuted(true);
  initialization.resolve();
  await Promise.all([retired, replacement]);
  assert.equal(state.initializations, 1);
  assert.equal(state.outputs.length, 1);
  assert.deepEqual(state.outputs[0].bytes, [4, 5]);
  assert.equal(state.outputs[0].volume, 0.25);
  assert.equal(state.outputs[0].muted, true);
});

test("a retired failed initialization leaves a later install recoverable", async t => {
  const { state, device: createDevice } = await fixture(t);
  const device = createDevice();
  const entered = deferred();
  const initialization = deferred();
  const failure = new Error("retired initialization failed");
  state.initialize = () => { entered.resolve(); return initialization.promise; };
  const failed = assert.rejects(device.install(bundle), error => error === failure);
  await entered.promise;
  device.dispose();
  initialization.reject(failure);
  await failed;
  assert.equal(state.outputs.length, 0);
  state.initialize = async () => {};
  await device.install(bundle);
  assert.equal(state.initializations, 2);
  assert.equal(state.outputs.length, 1);
});

test("output construction failure does not discard successful module initialization", async t => {
  const { state, device: createDevice } = await fixture(t);
  const device = createDevice();
  const failure = new Error("invalid Rust resource bundle");
  state.constructorError = failure;
  await assert.rejects(device.install(bundle), error => error === failure);
  assert.equal(device.available, false);
  state.constructorError = null;
  await device.install(bundle);
  assert.equal(state.initializations, 1);
  assert.equal(state.outputs.length, 1);
});

test("absent and invalid resources never initialize the audio module", async t => {
  const { state, device: createDevice } = await fixture(t);
  const device = createDevice();
  await device.install(undefined);
  for (const invalid of [null, [], new Uint8Array(), "bytes"]) {
    await assert.rejects(device.install(invalid), /nonempty byte bundle/);
  }
  assert.equal(device.available, false);
  assert.equal(state.initializations, 0);
  assert.equal(state.outputs.length, 0);
});

test("recovery preserves the Window bracket arguments and original Rust rejection", async t => {
  const { state, device: createDevice } = await fixture(t);
  const device = createDevice();
  state.initialize = async () => { throw new Error("initialization failed"); };
  await assert.rejects(device.install(bundle), /initialization failed/);
  state.initialize = async () => {};
  await device.install(bundle);
  const rejection = new Error("stale or invalid worker audio anchor");
  state.startError = rejection;
  const anchor = { session: 7, playing: true, time: 2.1, sampledEpochMs: 1_792_956_040_812 };
  t.mock.method(performance, "now", () => 110);
  assert.throws(() => device.applyAnchor(anchor, 7, { session: 7, sentMs: 100, receivedMs: 106 }),
    error => error === rejection);
  assert.deepEqual(state.outputs[0].starts[0], [7, 2.1, 100, 106, 110]);
  assert.equal(state.outputs[0].starts.length, 1);
  assert.equal(state.initializations, 2);
});

test("playing anchors require a matching bracket and never compare cross-realm epochs", async t => {
  const { state, device: createDevice } = await fixture(t);
  const device = createDevice();
  await device.install(bundle);
  t.mock.method(performance, "now", () => 110);
  const sampled = 1_793_152_307_172.3;
  const bracket = { session: 7, sentMs: 100, receivedMs: 106 };
  const anchor = { session: 7, playing: true, time: 2.1, sampledEpochMs: sampled };
  assert.throws(() => device.applyAnchor(anchor, 7), /correlated Window bracket/);
  assert.throws(() => device.applyAnchor(anchor, 7, { ...bracket, session: 8 }), /correlated Window bracket/);
  assert.throws(() => device.applyAnchor({ ...anchor, session: 8 }, 7, bracket), /different playback session/);
  assert.deepEqual(state.outputs[0].starts, []);
  // The recorded 1-ULP epoch reversal is diagnostic data, not admission or a
  // rewritten timestamp. The native call contains the original Window readings.
  t.mock.getter(performance, "timeOrigin", () => sampled - 110 - 0.000244140625);
  device.applyAnchor(anchor, 7, bracket);
  assert.deepEqual(state.outputs[0].starts, [[7, 2.1, 100, 106, 110]]);
  device.applyAnchor({ session: 7, playing: false }, 7);
  assert.equal(state.outputs[0].pauses, 1);
});
