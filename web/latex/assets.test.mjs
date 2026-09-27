import assert from "node:assert/strict";
import test from "node:test";
import { gunzipSync, gzipSync } from "node:zlib";
import { readBounded, gunzipBounded, tarFiles, loadLatexAssets } from "./assets.js";
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

test("oversized streamed assets are cancelled before collection", async () => {
  let cancelled = false;
  const stream = new ReadableStream({
    start(controller) { controller.enqueue(new Uint8Array(10)); },
    cancel() { cancelled = true; },
  });
  await assert.rejects(readBounded(stream, 9), /exceeds/);
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
