import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

// Keep the production lifecycle/anchor implementation; replace only the real
// WASM import so a deterministic device boundary can observe its calls.
const source = await readFile(new URL("./worker-audio-device.js", import.meta.url), "utf8");
function fixture({ loading = null } = {}) {
  const outputs = [];
  class Output {
    constructor(bytes) {
      this.bounded = bytes[0] === 1;
      this.context = new EventTarget();
      this.calls = [];
      this.enabled = false;
      this.failure = null;
      outputs.push(this);
    }
    requiresHorizon() { return this.bounded; }
    ready() { return this.enabled; }
    audioContext() { return this.context; }
    unlock() { this.calls.push(["unlock"]); this.enabled = true; return Promise.resolve(); }
    setVolume(value) { this.calls.push(["volume", value]); }
    setMuted(value) { this.calls.push(["muted", value]); }
    start(...values) { this.calls.push(["start", ...values]); }
    startBounded(...values) {
      if (this.failure) throw this.failure;
      this.calls.push(["bounded", ...values]);
    }
    pause() { this.calls.push(["pause"]); }
    stop() { this.calls.push(["stop"]); }
    free() { this.calls.push(["free"]); }
  }
  const module = { default: async () => {}, WasmWorkerAudioOutput: Output };
  const Device = new Function("importModule",
    `${source.replace('import("./pkg/noon_web.js")', "importModule()")
      .replace("export class WorkerAudioDevice", "class WorkerAudioDevice")}
return WorkerAudioDevice;`)(() => loading ?? Promise.resolve(module));
  let interrupted = 0;
  return { device: new Device(() => interrupted++), outputs, module, interrupts: () => interrupted };
}

function anchor(audioSequence, overrides = {}) {
  return {
    session: 7, time: 0.5, playing: true,
    sampledEpochMs: performance.timeOrigin + performance.now() - 1,
    availableUntil: 1, audioSequence, ...overrides,
  };
}

test("prepared install stays silent and only ordered bounded anchors can start output", async () => {
  const f = fixture();
  await f.device.install(new Uint8Array([1]));
  const output = f.outputs[0];
  assert.deepEqual(output.calls.map(call => call[0]), ["volume", "muted"]);
  assert.throws(() => f.device.applyAnchor(anchor(1), 7), /Play with sound/);
  await f.device.unlock();
  assert.equal(f.device.applyAnchor(anchor(1), 7), true);
  const started = output.calls.find(call => call[0] === "bounded");
  assert.equal(started[1], 7);
  assert.equal(started[2], 0.5);
  assert.equal(started[5], 1);
  assert.ok(started[4] >= started[3]);
  assert.equal(f.device.applyAnchor(anchor(3, { playing: false }), 7), true);
  const paused = output.calls.length;
  assert.equal(f.device.applyAnchor(anchor(2), 7), false);
  assert.equal(f.device.applyAnchor(anchor(3), 7), false);
  assert.equal(output.calls.length, paused, "late anchors cannot revive paused output");

  assert.throws(() => f.device.applyAnchor(anchor(4, { availableUntil: NaN }), 7), /invalid prepared/);
  assert.equal(f.device.applyAnchor(anchor(4, { time: 0.25, availableUntil: 3 }), 7), true);
  output.failure = new Error("device rejected anchor");
  assert.throws(() => f.device.applyAnchor(anchor(5), 7), /device rejected/);
  output.failure = null;
  assert.equal(f.device.applyAnchor(anchor(5), 7), true, "failed output does not consume its sequence");
  assert.throws(() => f.device.applyAnchor(anchor(6, { availableUntil: undefined }), 7), /history horizon/);
  assert.throws(() => f.device.applyAnchor(anchor(6, { session: 8 }), 7), /different playback session/);
  assert.equal(f.device.applyAnchor(anchor(6, { time: 3, availableUntil: 3, playing: false }), 7), true);
  assert.equal(output.calls.filter(call => call[0] === "start").length, 0);
  f.device.dispose();
});

test("replacement disposes old output and resets sequence for the new session", async () => {
  const f = fixture();
  await f.device.install(new Uint8Array([1]));
  await f.device.unlock();
  f.device.applyAnchor(anchor(40), 7);
  const old = f.outputs[0];
  await f.device.install(new Uint8Array([1]));
  assert.deepEqual(old.calls.slice(-2), [["stop"], ["free"]]);
  const current = f.outputs[1];
  assert.equal(current.calls.some(call => call[0] === "bounded"), false);
  await f.device.unlock();
  assert.equal(f.device.applyAnchor(anchor(1, { session: 8 }), 8), true);
  old.context.dispatchEvent(new Event("statechange"));
  assert.equal(f.interrupts(), 0, "retired device no longer owns state callbacks");
  current.enabled = false;
  current.context.dispatchEvent(new Event("statechange"));
  assert.equal(f.interrupts(), 1);
  assert.equal(current.calls.at(-1)[0], "stop");
  f.device.dispose();
});

test("finite output keeps its existing unbounded anchor behavior", async () => {
  const f = fixture();
  await f.device.install(new Uint8Array([0]));
  await f.device.unlock();
  const finite = { session: 7, time: 0.5, playing: true, sampledEpochMs: performance.timeOrigin };
  assert.equal(f.device.applyAnchor(finite, 7), true);
  assert.equal(f.outputs[0].calls.at(-1)[0], "start");
  assert.equal(f.device.applyAnchor({ ...finite, playing: false }, 7), true);
  assert.equal(f.outputs[0].calls.at(-1)[0], "pause");
  f.device.dispose();
});

test("cancellation during module preparation cannot install a retired output", async () => {
  let resolve;
  const loading = new Promise(done => { resolve = done; });
  const f = fixture({ loading });
  const installing = f.device.install(new Uint8Array([1]));
  f.device.dispose();
  resolve(f.module);
  await assert.rejects(installing, /preparation was retired/);
  assert.equal(f.outputs.length, 0);
  assert.equal(f.device.available, false);
});
