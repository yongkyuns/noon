import test from "node:test";
import assert from "node:assert/strict";
import { MessageChannel, receiveMessageOnPort } from "node:worker_threads";
import {
  SharedExecutionDeltaReader,
  SharedExecutionDeltaWriter,
  createSharedExecutionMailbox,
  decodeTransferableExecutionDelta,
} from "../web/execution-transport.js";
import { createGlowWorkerTransport } from "./glow-worker-transport.mjs";

const resources = () => new Uint8Array([7, 11, 13]);
const snapshot = JSON.stringify({
  channel: "noon.execution.retained", session: 1, sequence: 0, snapshot: true,
});

function channel(t) {
  const ports = new MessageChannel();
  t.after(() => { ports.port1.close(); ports.port2.close(); });
  return ports;
}

// Model only the controller's bootstrap boundary, with actual MessagePorts,
// transferable buffers and shared readers. Pixel/rendering behavior stays in
// glow-worker-qualification.mjs, not in this transport regression.
function receiver(port) {
  let installed = null, reader = null;
  const messages = [], frames = [];
  const apply = json => {
    assert.ok(installed, "snapshot arrived before its resource bundle");
    frames.push(json);
  };
  return {
    messages, frames,
    get resources() { return installed; },
    drain() {
      let received;
      while ((received = receiveMessageOnPort(port)) !== undefined) {
        const message = received.message;
        messages.push(message.type);
        if (message.type === "retained_resources") installed = message.bytes;
        else if (message.type === "transport_setup") {
          reader = new SharedExecutionDeltaReader(message.mailbox);
          reader.drain(apply);
        } else if (message.type === "shared_delta") reader.drain(apply);
        else if (message.type === "execution_delta") {
          apply(decodeTransferableExecutionDelta(message).json);
        } else assert.fail(`unexpected message ${message.type}`);
      }
    },
  };
}

test("shared bootstrap installs resources before draining a pre-filled mailbox", t => {
  const { port1, port2 } = channel(t);
  const target = receiver(port2);
  const sender = createGlowWorkerTransport(port1, "shared", resources());
  // No yield, sleep, or shared_delta notification: setup itself exposes the frame.
  assert.equal(sender.send(snapshot), true);
  target.drain();
  assert.deepEqual(target.messages, ["retained_resources", "transport_setup"]);
  assert.deepEqual(target.resources, resources());
  assert.deepEqual(target.frames, [snapshot]);
  assert.equal(sender.canSend(), true);
  // A later wake cannot apply the already-consumed initial frame twice.
  port1.postMessage({ type: "shared_delta" });
  target.drain();
  assert.deepEqual(target.frames, [snapshot]);
});

test("shared bootstrap also supports a snapshot published after setup", t => {
  const { port1, port2 } = channel(t);
  const target = receiver(port2);
  const sender = createGlowWorkerTransport(port1, "shared", resources());
  target.drain();
  assert.deepEqual(target.frames, []);
  assert.equal(sender.send(snapshot), true);
  port1.postMessage({ type: "shared_delta" });
  target.drain();
  assert.deepEqual(target.frames, [snapshot]);
});

test("transferable bootstrap preserves resources-before-snapshot ordering", t => {
  const { port1, port2 } = channel(t);
  const target = receiver(port2);
  const sender = createGlowWorkerTransport(port1, "transferable", resources());
  assert.equal(sender.send(snapshot), true);
  target.drain();
  assert.deepEqual(target.messages, ["retained_resources", "execution_delta"]);
  assert.deepEqual(target.resources, resources());
  assert.deepEqual(target.frames, [snapshot]);
});

for (const mode of ["shared", "transferable"]) {
  test(`${mode} worker recreation cannot detach the reusable fixture resources`, t => {
    const fixture = resources();
    for (let iteration = 0; iteration < 2; iteration++) {
      const { port1, port2 } = channel(t);
      const target = receiver(port2);
      const sender = createGlowWorkerTransport(port1, mode, fixture);
      assert.equal(sender.send(snapshot), true);
      target.drain();
      assert.deepEqual(fixture, resources());
      assert.deepEqual(target.resources, resources());
      assert.deepEqual(target.frames, [snapshot]);
    }
  });
}

test("old setup-before-resources ordering fails even with FIFO port delivery", t => {
  const { port1, port2 } = channel(t);
  const mailbox = createSharedExecutionMailbox(1024);
  const sender = new SharedExecutionDeltaWriter(mailbox);
  port1.postMessage({ type: "transport_setup", mode: "shared", mailbox });
  const bytes = resources();
  port1.postMessage({ type: "retained_resources", bytes }, [bytes.buffer]);
  assert.equal(sender.send(snapshot), true);
  assert.throws(() => receiver(port2).drain(), /before its resource bundle/);
});

test("invalid bootstrap inputs do not publish a partial setup", t => {
  const { port1, port2 } = channel(t);
  assert.throws(() => createGlowWorkerTransport(port1, "unknown", resources()), /unsupported/);
  for (const mode of ["shared", "transferable"]) {
    assert.throws(() => createGlowWorkerTransport(port1, mode, []), /non-empty/);
  }
  assert.equal(receiveMessageOnPort(port2), undefined);
});
