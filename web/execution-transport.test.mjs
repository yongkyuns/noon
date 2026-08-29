import assert from "node:assert/strict";
import { MessageChannel } from "node:worker_threads";
import test from "node:test";

import {
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  RETAINED_EXECUTION_TRANSPORT_CHANNEL,
  SharedExecutionDeltaReader,
  SharedExecutionDeltaWriter,
  TransferableExecutionDeltaReceiver,
  TransferableExecutionDeltaSender,
  createSharedExecutionMailbox,
  decodeTransferableExecutionDelta,
  executionDeltaMetadata,
  selectExecutionTransportMode,
} from "./execution-transport.js";

function delta(sequence, { session = 1, snapshot = sequence === 0, channel = "noon.execution" } = {}) {
  return JSON.stringify({
    channel,
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

test("shared transport also carries retained execution framing", () => {
  const mailbox = createSharedExecutionMailbox(4096);
  const writer = new SharedExecutionDeltaWriter(mailbox);
  const reader = new SharedExecutionDeltaReader(mailbox);
  const retained = delta(0, { channel: RETAINED_EXECUTION_TRANSPORT_CHANNEL });
  assert.equal(writer.send(retained), true);
  const received = [];
  assert.equal(
    reader.drain((json) => {
      received.push(JSON.parse(json).channel);
      return true;
    }),
    1,
  );
  assert.deepEqual(received, [RETAINED_EXECUTION_TRANSPORT_CHANNEL]);
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

test("transferable envelope metadata must match encoded payload", () => {
  const payload = new TextEncoder().encode(delta(0));
  const validBuffer = payload.buffer.slice(
    payload.byteOffset,
    payload.byteOffset + payload.byteLength,
  );
  assert.deepEqual(
    decodeTransferableExecutionDelta({
      type: "execution_delta",
      session: 1,
      sequence: 0,
      buffer: validBuffer,
    }).metadata,
    { session: 1, sequence: 0, snapshot: true },
  );

  const retainedPayload = new TextEncoder().encode(
    delta(0, { channel: RETAINED_EXECUTION_TRANSPORT_CHANNEL }),
  );
  const retainedBuffer = retainedPayload.buffer.slice(
    retainedPayload.byteOffset,
    retainedPayload.byteOffset + retainedPayload.byteLength,
  );
  assert.deepEqual(
    decodeTransferableExecutionDelta({
      type: "execution_delta",
      session: 1,
      sequence: 0,
      buffer: retainedBuffer,
    }).metadata,
    { session: 1, sequence: 0, snapshot: true },
  );

  const mismatchPayload = new TextEncoder().encode(delta(0));
  const mismatchBuffer = mismatchPayload.buffer.slice(
    mismatchPayload.byteOffset,
    mismatchPayload.byteOffset + mismatchPayload.byteLength,
  );
  assert.throws(
    () =>
      decodeTransferableExecutionDelta({
        type: "execution_delta",
        session: 1,
        sequence: 99,
        buffer: mismatchBuffer,
      }),
    /metadata does not match/,
  );
});

test("metadata validates the current execution schema directly", () => {
  assert.equal(executionDeltaMetadata(delta(0)).sequence, 0);
  assert.equal(
    executionDeltaMetadata(delta(0, { channel: RETAINED_EXECUTION_TRANSPORT_CHANNEL })).sequence,
    0,
  );

  const unrelated = JSON.parse(delta(0));
  unrelated.channel = "noon.execution.unrelated";
  assert.throws(
    () => executionDeltaMetadata(JSON.stringify(unrelated)),
    /invalid channel/,
  );

  const unsafe = JSON.parse(delta(0));
  unsafe.sequence = Number.MAX_SAFE_INTEGER + 1;
  assert.throws(
    () => executionDeltaMetadata(JSON.stringify(unsafe)),
    /invalid sequence/,
  );
});
