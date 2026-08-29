import assert from "node:assert/strict";
import test from "node:test";

import {
  decodeTransferableExecutionVisibility,
  encodeTransferableExecutionVisibility,
  executionFrameKey,
  executionVisibilityMetadata,
} from "./execution-visibility-transport.js";

function visibility(overrides = {}) {
  return JSON.stringify({
    channel: "noon.execution.visibility",
    protocol_version: 1,
    time: 0.5,
    layout_generation: 7,
    total_live: 2,
    slots: [
      { slot: 4, generation: 1 },
      { slot: 9, generation: 3 },
    ],
    stats: { candidates: 2, results: 2, full_scan_fallbacks: 0 },
    ...overrides,
  });
}

test("visibility metadata validates the retained-query transport envelope", () => {
  assert.deepEqual(executionVisibilityMetadata(visibility()), {
    time: 0.5,
    layoutGeneration: 7,
    totalLive: 2,
  });

  assert.throws(
    () => executionVisibilityMetadata(visibility({ channel: "noon.execution" })),
    /invalid channel/,
  );
  assert.throws(
    () => executionVisibilityMetadata(visibility({ protocol_version: 2 })),
    /unsupported execution visibility version 2/,
  );
  assert.throws(
    () => executionVisibilityMetadata(visibility({ layout_generation: -1 })),
    /invalid layout generation/,
  );
});

test("transferable visibility keeps the candidate set bound to one execution frame", () => {
  const encoded = encodeTransferableExecutionVisibility(
    { session: 12, sequence: 41 },
    visibility(),
  );
  assert.equal(encoded.transfer.length, 1);
  assert.equal(encoded.transfer[0], encoded.message.buffer);

  const decoded = decodeTransferableExecutionVisibility(encoded.message);
  assert.equal(decoded.json, visibility());
  assert.deepEqual(decoded.frame, { session: 12, sequence: 41 });
  assert.deepEqual(decoded.visibility, {
    time: 0.5,
    layoutGeneration: 7,
    totalLive: 2,
  });
  assert.equal(executionFrameKey(decoded.frame), "12:41");
});

test("frame identity rejects malformed or unsafe pairing metadata", () => {
  assert.throws(
    () => encodeTransferableExecutionVisibility({ session: -1, sequence: 0 }, visibility()),
    /invalid session/,
  );
  assert.throws(
    () => encodeTransferableExecutionVisibility({ session: 1, sequence: 1.5 }, visibility()),
    /invalid sequence/,
  );
  assert.throws(
    () => executionFrameKey({ session: 1, sequence: Number.MAX_SAFE_INTEGER + 1 }),
    /invalid sequence/,
  );
});

test("decoder rejects non-transferable visibility payloads before pairing", () => {
  assert.throws(
    () =>
      decodeTransferableExecutionVisibility({
        type: "execution_visibility",
        session: 1,
        sequence: 0,
        buffer: "not-a-buffer",
      }),
    /must be an ArrayBuffer/,
  );
});
