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
  prepareExecutionDeltaMetadataForSend,
  prepareExecutionDeltaTransportForSend,
  selectExecutionTransportMode,
} from "./execution-transport.js";

function delta(sequence, { session = 1, snapshot = sequence === 0, channel = RETAINED_EXECUTION_TRANSPORT_CHANNEL } = {}) {
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

test("transferable transport preserves a large delta after buffer ownership moves", async () => {
  const { port1, port2 } = new MessageChannel();
  const first = JSON.stringify({
    ...JSON.parse(delta(0)),
    objects: [{ geometry: "●".repeat(450_000) }],
  });
  assert.ok(new TextEncoder().encode(first).byteLength > 1024 * 1024);
  const received = [];
  const sender = new TransferableExecutionDeltaSender(port1);
  new TransferableExecutionDeltaReceiver(port2, (json) => { received.push(json); });
  assert.equal(sender.send(first), true);
  await turn();
  assert.deepEqual(received, [first]);
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
  assert.throws(
    () => executionDeltaMetadata(delta(0, { channel: "noon.execution" })),
    /invalid channel/,
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

test("delta send reuses only metadata validated for the identical JSON payload", async () => {
  const json = delta(0);
  const metadata = prepareExecutionDeltaMetadataForSend(json);
  const mailbox = createSharedExecutionMailbox(4096);
  const shared = new SharedExecutionDeltaWriter(mailbox);
  const sharedReader = new SharedExecutionDeltaReader(mailbox);
  const { port1, port2 } = new MessageChannel();
  const transferable = new TransferableExecutionDeltaSender(port1);
  const transferableMetadata = prepareExecutionDeltaMetadataForSend(json);
  const mismatchedMetadata = prepareExecutionDeltaMetadataForSend(delta(7));
  const malformedMetadata = prepareExecutionDeltaMetadataForSend(json);
  const originalParse = JSON.parse;
  let parseCount = 0;
  JSON.parse = function (...args) {
    parseCount += 1;
    return originalParse.apply(this, args);
  };
  try {
    assert.equal(shared.send(json, metadata), true);
    assert.equal(parseCount, 0, "the first send reuses the prepared exact-string metadata");
    assert.equal(shared.send(json, metadata), true);
    assert.equal(parseCount, 1, "a consumed handoff is reparsed when reused");
    assert.equal(transferable.send(json, transferableMetadata), true);
    assert.equal(parseCount, 1, "a fresh one-shot handoff avoids reparsing for transferable send");
    assert.equal(sharedReader.drain(() => true), 2, "accept the first two deltas to free shared slots");

    const parseCountBeforePublicSend = parseCount;
    assert.equal(shared.send(delta(1)), true);
    assert.equal(parseCount, parseCountBeforePublicSend + 1,
      "send without an internal validated handoff still validates");
    const parseCountBeforeMismatchedHandoff = parseCount;

    assert.equal(shared.send(delta(2), mismatchedMetadata), true);
    assert.equal(parseCount, parseCountBeforeMismatchedHandoff + 1,
      "metadata from a different JSON payload is reparsed");
    assert.equal(sharedReader.drain(() => true), 2, "accept deltas and free slots for the reuse regression");
    const parseCountBeforeOneShotReuse = parseCount;
    assert.equal(shared.send(json, malformedMetadata), true);
    assert.equal(parseCount, parseCountBeforeOneShotReuse,
      "the first send consumes a fresh exact-string handoff without parsing");
    assert.throws(() => shared.send("not JSON", malformedMetadata), /invalid JSON/);
    assert.equal(parseCount, parseCountBeforeOneShotReuse + 1,
      "a consumed handoff cannot bypass malformed-payload rejection when reused");
    assert.throws(() => shared.send(undefined), /execution delta must be a JSON string/);

    const received = [];
    assert.equal(sharedReader.drain((payload) => {
      received.push(executionDeltaMetadata(payload).sequence);
      return true;
    }), 1);
    assert.deepEqual(received, [0]);
  } finally {
    JSON.parse = originalParse;
    port1.close();
    port2.close();
  }
});


test("canonical worker header avoids a full producer parse while receivers validate the unchanged body", async () => {
  const header = { channel: RETAINED_EXECUTION_TRANSPORT_CHANNEL, session: 1,
    sequence: 0, snapshot: true, pointer_view: { revision: 8, width: 640, height: 360 } };
  const json = JSON.stringify({ ...header, time: 0,
    objects: Array.from({ length: 600 }, (_, id) => ({ id, geometry: "●\n".repeat(256) })) });
  const packet = `${JSON.stringify(header)}\n${json}`;
  const mailbox = createSharedExecutionMailbox(new TextEncoder().encode(json).length);
  const shared = new SharedExecutionDeltaWriter(mailbox);
  const reader = new SharedExecutionDeltaReader(mailbox);
  const { port1, port2 } = new MessageChannel();
  try {
    const sender = new TransferableExecutionDeltaSender(port1);
    const incoming = new Promise(resolve => port2.once("message", resolve));
    const parsedLengths = [];
    const originalParse = JSON.parse;
    let prepared;
    JSON.parse = function (text, ...args) {
      parsedLengths.push(text.length);
      return originalParse.call(this, text, ...args);
    };
    try {
      prepared = prepareExecutionDeltaTransportForSend(packet);
      assert.equal(prepared.json, json);
      assert.equal(shared.send(prepared.json, prepared.metadata), true);
      const second = prepareExecutionDeltaTransportForSend(packet);
      assert.equal(sender.send(second.json, second.metadata), true);
    } finally { JSON.parse = originalParse; }
    assert.deepEqual(parsedLengths, [JSON.stringify(header).length, JSON.stringify(header).length],
      "only the constant-size headers are parsed at the producer");
    assert.ok(Object.isFrozen(prepared.metadata) && Object.isFrozen(prepared.metadata.pointerView));
    let sharedBody;
    assert.equal(reader.drain((body, metadata) => {
      sharedBody = body;
      assert.deepEqual(metadata, executionDeltaMetadata(json));
    }), 1);
    assert.equal(sharedBody, json);
    const received = decodeTransferableExecutionDelta(await incoming);
    assert.equal(received.json, json);
    assert.deepEqual(received.metadata, prepared.metadata);
  } finally { port1.close(); port2.close(); }
});

test("worker carrier validates bounded metadata and never relaxes generic JSON or receiver validation", () => {
  const header = JSON.parse(delta(0));
  const packet = value => `${JSON.stringify(value)}\n${delta(0)}`;
  assert.throws(() => prepareExecutionDeltaTransportForSend(null), /header\/body string/);
  for (const bad of ["", delta(0), `${delta(0)}\n`, `${" ".repeat(513)}\n${delta(0)}`]) {
    assert.throws(() => prepareExecutionDeltaTransportForSend(bad), /bounded metadata header/);
  }
  for (const [field, value, error] of [
    ["channel", "other", /invalid channel/], ["session", -1, /invalid session/],
    ["sequence", Number.MAX_SAFE_INTEGER + 1, /invalid sequence/],
    ["snapshot", 1, /snapshot flag/], ["pointer_view", { revision: 0, width: 0, height: 1 }, /pointer view/],
  ]) assert.throws(() => prepareExecutionDeltaTransportForSend(packet({ ...header, [field]: value })), error);
  assert.throws(() => prepareExecutionDeltaMetadataForSend("not JSON"), /invalid JSON/);
  assert.throws(() => decodeTransferableExecutionDelta({ type: "execution_delta", session: 1,
    sequence: 99, buffer: new TextEncoder().encode(delta(0)).buffer }), /does not match/);
  const mailbox = createSharedExecutionMailbox(4096);
  const writer = new SharedExecutionDeltaWriter(mailbox);
  const reader = new SharedExecutionDeltaReader(mailbox);
  const malformed = prepareExecutionDeltaTransportForSend(`${JSON.stringify(header)}\nnot JSON`);
  assert.equal(writer.send(malformed.json, malformed.metadata), true);
  let applied = false;
  assert.throws(() => reader.drain(() => { applied = true; }), /invalid JSON/);
  assert.equal(applied, false, "an invalid body must not reach the consumer");
});
