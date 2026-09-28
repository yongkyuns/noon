import assert from "node:assert/strict";
import test from "node:test";
import { gunzipSync, gzipSync } from "node:zlib";
import { readBounded, gunzipBounded, tarFiles, loadLatexAssets, verifiedFetch } from "./assets.js";
import { sparseFormatSnapshot } from "./engine-io.js";

function archive(name, data = new Uint8Array([1, 2])) {
  const bytes = new Uint8Array(2048);
  const encoder = new TextEncoder();
  bytes.set(encoder.encode(name));
  bytes.set(encoder.encode(data.length.toString(8).padStart(11, "0")), 124);
  bytes.fill(32, 148, 156);
  bytes[156] = 48;
  const checksum = bytes.subarray(0, 512).reduce((sum, byte) => sum + byte, 0);
  bytes.set(encoder.encode(checksum.toString(8).padStart(6, "0") + "\0 "), 148);
  bytes.set(data, 512);
  return bytes;
}

test("bounded decompression refuses expansion beyond the admission limit", async () => {
  const gzip = gzipSync(new Uint8Array(8192));
  await assert.rejects(gunzipBounded(gzip, 1024), /exceeds/);
  assert.deepEqual(await gunzipBounded(gzip, 8192), new Uint8Array(gunzipSync(gzip)));
});

test("successful single-use asset reads finish without releasing an exhausted reader", async () => {
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(Uint8Array.of(1, 2));
      controller.enqueue(Uint8Array.of(3));
      controller.close();
    },
  });
  const getReader = stream.getReader.bind(stream);
  stream.getReader = () => {
    const reader = getReader();
    reader.releaseLock = () => { throw new Error("exhausted-reader cleanup must not run"); };
    return reader;
  };
  assert.deepEqual(await readBounded(stream, 3), Uint8Array.of(1, 2, 3));
});

test("oversized streamed assets are cancelled before collection", async () => {
  let cancelled = false;
  const stream = new ReadableStream({
    start(controller) { controller.enqueue(new Uint8Array(10)); },
    cancel() { cancelled = true; },
  });
  await assert.rejects(readBounded(stream, 9), /exceeds/);
  assert.equal(cancelled, true);
});

async function sha256(bytes) {
  const hash = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return Array.from(hash, byte => byte.toString(16).padStart(2, "0")).join("");
}

test("verified fetch completes before its network deadline", async () => {
  const bytes = Uint8Array.of(1, 2, 3);
  let signal;
  assert.deepEqual(
    await verifiedFetch(
      "https://example.test/asset",
      await sha256(bytes),
      16,
      async (_url, options) => {
        signal = options.signal;
        return new Response(bytes);
      },
      10,
    ),
    bytes,
  );
  await new Promise(resolve => setTimeout(resolve, 20));
  assert.equal(signal.aborted, false);
});

test("verified fetch aborts a stalled request at its network deadline", async () => {
  let aborted = false;
  await assert.rejects(
    verifiedFetch(
      "https://example.test/stalled-request",
      "ignored",
      16,
      (_url, { signal }) => new Promise((_, reject) => {
        signal.addEventListener("abort", () => {
          aborted = true;
          reject(new DOMException("Fetch is aborted", "AbortError"));
        }, { once: true });
      }),
      10,
    ),
    /timed out/,
  );
  assert.equal(aborted, true);
});

test("verified fetch cancels a stalled response body at its network deadline", async () => {
  let cancelled = false;
  const stream = new ReadableStream({
    cancel(reason) {
      cancelled = /timed out/.test(String(reason));
    },
  });
  await assert.rejects(
    verifiedFetch(
      "https://example.test/stalled-body",
      "ignored",
      16,
      async () => new Response(stream),
      10,
    ),
    /timed out/,
  );
  assert.equal(cancelled, true);
});

test("tar admission rejects traversal, corruption and truncation", () => {
  assert.throws(() => tarFiles(archive("../escape")), /path/);
  const corrupt = archive("font.ttf"); corrupt[0]++;
  assert.throws(() => tarFiles(corrupt), /checksum/);
  assert.throws(() => tarFiles(archive("font.ttf").subarray(0, 513)), /Truncated/);
  const input = archive("font.ttf");
  const extracted = tarFiles(input).get("font.ttf");
  input[512] = 99;
  assert.deepEqual(extracted, Uint8Array.of(1, 2));
});

test("unverified remote data cannot become executable compiler input", async () => {
  await assert.rejects(loadLatexAssets(async () => new Response(Uint8Array.of(1, 2))), /integrity/);
});

test("sparse reset restores nonzero pages and clears prior compile memory", () => {
  const original = new Uint8Array(65536);
  original[42] = 19;
  const snapshot = sparseFormatSnapshot(original);
  assert.equal(snapshot.byteLength, 16384);
  original[42] = 0;
  const memory = new WebAssembly.Memory({ initial: 1 });
  new Uint8Array(memory.buffer).fill(71);
  snapshot.reset(memory);
  const restored = new Uint8Array(memory.buffer);
  assert.equal(restored[42], 19);
  assert.equal(restored[60000], 0);
});
