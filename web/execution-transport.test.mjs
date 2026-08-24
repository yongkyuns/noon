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

test("shared two-slot mailbox applies deltas in sequence and reports backpressure", () => {
  const mailbox = createSharedExecutionMailbox(4096);
  const writer = new SharedExecutionDeltaWriter(mailbox);
  const reader = new SharedExecutionDeltaReader(mailbox);
  assert.equal(writer.send(delta(0)), true);
  assert.equal(writer.send(delta(1, { snapshot: false })), true);
  assert.equal(writer.send(delta(2, { snapshot: false })), false);
  assert.equal(writer.backpressureCount(), 1);

  const received = [];
  assert.equal(
    reader.drain((json) => received.push(executionDeltaMetadata(json).sequence)),
    2,
  );
  assert.deepEqual(received, [0, 1]);

  // A writer that was backpressured must recover with a snapshot. The Rust
  // receiver accepts a snapshot across a sequence gap and rejects partial gaps.
  assert.equal(writer.send(delta(3, { snapshot: true })), true);
  reader.drain((json) => received.push(executionDeltaMetadata(json).sequence));
  assert.deepEqual(received, [0, 1, 3]);
});

test("transferable mailbox bounds in-flight buffers and resumes after ack", async () => {
  const { port1, port2 } = new MessageChannel();
  const writable = [];
  const received = [];
  const sender = new TransferableExecutionDeltaSender(port1, {
    maxInFlight: 2,
    onWritable: () => writable.push(true),
  });
  new TransferableExecutionDeltaReceiver(port2, (json) => {
    received.push(executionDeltaMetadata(json).sequence);
  });

  assert.equal(sender.send(delta(0)), true);
  assert.equal(sender.send(delta(1, { snapshot: false })), true);
  assert.equal(sender.send(delta(2, { snapshot: false })), false);
  assert.equal(sender.backpressureCount(), 1);
  await turn();
  await turn();
  assert.deepEqual(received, [0, 1]);
  assert.equal(sender.inFlight(), 0);
  assert.ok(writable.length >= 1);

  assert.equal(sender.send(delta(3, { snapshot: true })), true);
  await turn();
  assert.deepEqual(received, [0, 1, 3]);
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
