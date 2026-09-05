import assert from "node:assert/strict";
import test from "node:test";
import { MessageChannel, MessagePort } from "node:worker_threads";
import { attachSemanticEngine } from "./semantic-engine-endpoint.js";
import { decodeTransferableExecutionDelta } from "./execution-transport.js";

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
const request = (port, type, requestId, fields = {}) => {
  const result = next(port);
  port.postMessage({ channel: "noon.engine", protocolVersion: 1, type, requestId, ...fields });
  return result;
};
function fixture() {
  const control = new MessageChannel();
  const render = new MessageChannel();
  let time = 0, playing = true, sequence = 0, returned = 0, returnedPlayer = null, stopped = 0;
  const json = () => JSON.stringify({ channel: "noon.execution.retained", protocol_version: 4, session: 7, sequence: sequence++, snapshot: sequence === 1, time, objects: [] });
  const player = {
    initialDeltaJson: json,
    resourceBundleBytes: () => new Uint8Array([1]),
    tickDeltaJson: () => null,
    seekDeltaJson: (value) => { if (!Number.isFinite(value)) throw new Error("invalid time"); time = value; return json(); },
    setLoopDuration: () => {}, pause: () => { playing = false; }, resume: () => { playing = true; },
    time: () => time, isPlaying: () => playing,
  };
  const context = {
    createExecutionPlayer: () => player,
    returnExecutionPlayer: (value) => { returned += 1; returnedPlayer = value; },
  };
  return { control, render, player, stats: () => ({ returned, returnedPlayer, stopped }),
    attach: () => attachSemanticEngine(context, {
      controlPort: control.port1, renderPort: render.port1, session: 7,
      loopDurationSeconds: 2, transportMode: "transferable",
    }, () => { stopped += 1; }),
    close: () => { control.port1.close(); control.port2.close(); render.port1.close(); render.port2.close(); },
  };
}

test("semantic producer installs mixed resources before its retained snapshot and supports controls", async () => {
  const f = fixture();
  try {
    const ready = next(f.control.port2);
    const resources = nextMatching(f.render.port2, (message) => message.type === "retained_resources");
    const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
    const endpoint = f.attach();
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

test("initial snapshot failure returns the exact player for retry", () => {
  const f = fixture();
  f.player.initialDeltaJson = () => { throw new Error("snapshot failed"); };
  try {
    assert.throws(f.attach, /snapshot failed/);
    assert.equal(f.stats().returned, 1);
    assert.equal(f.stats().returnedPlayer, f.player);
    assert.equal(f.stats().stopped, 1);
  } finally { f.close(); }
});

test("player construction failure closes both transferred ports", () => {
  const control = new MessageChannel();
  const render = new MessageChannel();
  let closed = 0;
  for (const port of [control.port1, render.port1]) {
    const close = port.close.bind(port);
    port.close = () => { closed += 1; close(); };
  }
  let stopped = 0;
  assert.throws(() => attachSemanticEngine({
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
