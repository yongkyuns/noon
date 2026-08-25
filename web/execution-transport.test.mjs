import assert from "node:assert/strict";
import { MessageChannel } from "node:worker_threads";
import test from "node:test";

import {
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  SharedExecutionDeltaReader,
  SharedExecutionDeltaWriter,
  TransferableExecutionDeltaReceiver,
  TransferableExecutionDeltaSender,
  createSharedExecutionMailbox,
  executionDeltaMetadata,
  selectExecutionTransportMode,
} from "./execution-transport.js";

function delta(sequence, { session = 1, snapshot = sequence === 0 } = {}) {
  return JSON.stringify({
    channel: "noon.execution",
    protocol_version: 1,
    session,
    sequence,
    snapshot,
    time: sequence / 60,
    objects: [],
  });
}

function turn() {
  return new Promise((resolve) => setImmediate(resolve));
}

test("transport mode selects SAB only for isolated contexts", () => {
  assert.equal(
    selectExecutionTransportMode({ crossOriginIsolated: false, SharedArrayBuffer }),
    EXECUTION_TRANSPORT_TRANSFERABLE,
  );
  assert.equal(
    selectExecutionTransportMode({ crossOriginIsolated: true, SharedArrayBuffer }),
    EXECUTION_TRANSPORT_SHARED,
  );
  assert.equal(
    selectExecutionTransportMode({ crossOriginIsolated: true, SharedArrayBuffer: undefined }),
    EXECUTION_TRANSPORT_TRANSFERABLE,
  );
});

test("shared two-slot mailbox retains ownership until consumer accepts", () => {
  const mailbox = createSharedExecutionMailbox(4096);
  const writer = new SharedExecutionDeltaWriter(mailbox);
  const reader = new SharedExecutionDeltaReader(mailbox);
  assert.equal(writer.send(delta(0)), true);
  assert.equal(writer.send(delta(1, { snapshot: false })), true);
  assert.equal(writer.canSend(), false);
  assert.equal(writer.send(delta(2, { snapshot: false })), false);
  assert.equal(writer.backpressureCount(), 1);

  let accepting = false;
  const received = [];
  const apply = (json) => {
    if (!accepting) {
      return false;
    }
    received.push(executionDeltaMetadata(json).sequence);
    return true;
  };

  assert.equal(reader.drain(apply), 0);
  assert.equal(writer.canSend(), false, "rejected reads must leave shared slots owned");
  accepting = true;
  assert.equal(reader.drain(apply), 2);
  assert.deepEqual(received, [0, 1]);
  assert.equal(writer.canSend(), true);

  assert.equal(writer.send(delta(2, { snapshot: false })), true);
  reader.drain(apply);
  assert.deepEqual(received, [0, 1, 2]);
});

test("transferable mailbox defers ack until consumer accepts", async () => {
  const { port1, port2 } = new MessageChannel();
  const writable = [];
  const received = [];
  let accepting = false;
  const sender = new TransferableExecutionDeltaSender(port1, {
    maxInFlight: 2,
    onWritable: () => writable.push(true),
  });
  const receiver = new TransferableExecutionDeltaReceiver(port2, (json) => {
    if (!accepting) {
      return false;
    }
    received.push(executionDeltaMetadata(json).sequence);
    return true;
  });

  assert.equal(sender.send(delta(0)), true);
  assert.equal(sender.send(delta(1, { snapshot: false })), true);
  assert.equal(sender.send(delta(2, { snapshot: false })), false);
  assert.equal(sender.backpressureCount(), 1);
  await turn();
  await turn();
  assert.equal(sender.inFlight(), 2);
  assert.equal(receiver.pendingCount(), 2);
  assert.deepEqual(received, []);

  accepting = true;
  assert.equal(receiver.drain(), 2);
  await turn();
  await turn();
  assert.deepEqual(received, [0, 1]);
  assert.equal(sender.inFlight(), 0);
  assert.ok(writable.length >= 1);

  assert.equal(sender.send(delta(2, { snapshot: false })), true);
  await turn();
  await turn();
  assert.deepEqual(received, [0, 1, 2]);
  port1.close();
  port2.close();
});

test("transferable envelope metadata must match encoded payload", async () => {
  const { port1, port2 } = new MessageChannel();
  const receiver = new TransferableExecutionDeltaReceiver(port2, () => true);
  const errors = [];
  process.once("uncaughtException", (error) => errors.push(error));
  const payload = new TextEncoder().encode(delta(0));
  const buffer = payload.buffer.slice(payload.byteOffset, payload.byteOffset + payload.byteLength);
  port1.postMessage(
    {
      type: "execution_delta",
      session: 1,
      sequence: 99,
      buffer,
    },
    [buffer],
  );
  await turn();
  await turn();
  assert.equal(receiver.pendingCount(), 0);
  assert.match(String(errors[0]), /metadata does not match/);
  port1.close();
  port2.close();
});

test("metadata rejects future protocols and unsafe sequence values", () => {
  const future = JSON.parse(delta(0));
  future.protocol_version = 2;
  assert.throws(
    () => executionDeltaMetadata(JSON.stringify(future)),
    /unsupported execution transport version 2/,
  );

  const unsafe = JSON.parse(delta(0));
  unsafe.sequence = Number.MAX_SAFE_INTEGER + 1;
  assert.throws(
    () => executionDeltaMetadata(JSON.stringify(unsafe)),
    /invalid sequence/,
  );
});
