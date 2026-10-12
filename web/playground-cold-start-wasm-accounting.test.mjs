import assert from "node:assert/strict";
import test from "node:test";

import { installNoonWasmAccounting } from "./playground-cold-start-wasm-accounting.js";

const noonWasmBytes = wasmWithExports([
  "__wbindgen_start",
  "__wbindgen_malloc",
  "__wbindgen_free",
]);
const noonWasmWithMemoryBytes = wasmWithExports([
  "__wbindgen_start",
  "__wbindgen_malloc",
  "__wbindgen_free",
], { memory: true });
const unrelatedWasmBytes = wasmWithExports(["run"]);

test("counts only Noon wasm-bindgen instances and records exact byte inputs", async () => {
  const worker = testWorkerGlobal();
  const report = installNoonWasmAccounting(worker, { role: "authoring" });

  await worker.WebAssembly.instantiate(noonWasmBytes, {});
  await worker.WebAssembly.instantiate(unrelatedWasmBytes, {});

  assert.deepEqual(report.records, [{
    ordinal: 1,
    instantiatedBytes: noonWasmBytes.byteLength,
    byteLengthSource: "instantiate-input",
    identity: "wasm-bindgen exports __wbindgen_start, __wbindgen_malloc, __wbindgen_free",
  }]);
  assert.equal(report.role, "authoring");
});

test("reports per-instance linear-memory capacity and observes growth through the snapshot boundary", async () => {
  const worker = testWorkerGlobal();
  const report = installNoonWasmAccounting(worker, { role: "authoring" });
  const result = await worker.WebAssembly.instantiate(noonWasmWithMemoryBytes, {});

  const initial = report.snapshot()[0];
  assert.equal(initial.available, true);
  assert.equal(initial.initialBytes, 64 * 1024);
  assert.equal(initial.peakObservedBytes, 64 * 1024);
  assert.equal(initial.latestBytes, 64 * 1024);
  assert.equal(initial.samplingIntervalMs, 25);

  result.instance.exports.memory.grow(2);
  worker.runMemorySamples();
  const grown = report.snapshot()[0];
  assert.equal(grown.peakObservedBytes, 3 * 64 * 1024);
  assert.equal(grown.latestBytes, 3 * 64 * 1024);
  assert.ok(grown.sampleCount >= 3);
});

test("marks a Noon instance without an exported memory as unavailable instead of reporting zero", async () => {
  const worker = testWorkerGlobal();
  const report = installNoonWasmAccounting(worker, { role: "render" });

  await worker.WebAssembly.instantiate(noonWasmBytes, {});

  assert.deepEqual(report.snapshot(), [{
    ordinal: 1,
    available: false,
    reason: "Noon instance does not export its WebAssembly.Memory",
  }]);
});

test("counts a streamed Noon instance and only reports identity response content length", async () => {
  const worker = testWorkerGlobal();
  const report = installNoonWasmAccounting(worker, { role: "render" });
  const response = new Response(noonWasmBytes, {
    headers: { "content-type": "application/wasm", "content-length": String(noonWasmBytes.byteLength) },
  });

  await worker.WebAssembly.instantiateStreaming(response, {});

  assert.deepEqual(report.records, [{
    ordinal: 1,
    instantiatedBytes: noonWasmBytes.byteLength,
    byteLengthSource: "identity-response-content-length",
    identity: "wasm-bindgen exports __wbindgen_start, __wbindgen_malloc, __wbindgen_free",
  }]);
});

test("does not infer module bytes from compressed or unknown-length streams", async () => {
  const worker = testWorkerGlobal();
  const report = installNoonWasmAccounting(worker, { role: "render" });

  await worker.WebAssembly.instantiateStreaming(new Response(noonWasmBytes, {
    headers: {
      "content-type": "application/wasm",
      "content-length": String(noonWasmBytes.byteLength),
      "content-encoding": "gzip",
    },
  }), {});
  await worker.WebAssembly.instantiateStreaming(new Response(noonWasmBytes, {
    headers: { "content-type": "application/wasm" },
  }), {});

  assert.deepEqual(report.records.map(({ instantiatedBytes, byteLengthSource }) => ({
    instantiatedBytes,
    byteLengthSource,
  })), [
    { instantiatedBytes: null, byteLengthSource: null },
    { instantiatedBytes: null, byteLengthSource: null },
  ]);
});

test("counts precompiled Noon modules without inventing source byte lengths", async () => {
  const worker = testWorkerGlobal();
  const report = installNoonWasmAccounting(worker, { role: "authoring" });
  const module = await WebAssembly.compile(noonWasmBytes);

  await worker.WebAssembly.instantiate(module, {});

  assert.equal(report.records.length, 1);
  assert.equal(report.records[0].instantiatedBytes, null);
  assert.equal(report.records[0].byteLengthSource, null);
});

function testWorkerGlobal() {
  const memorySamplers = [];
  return {
    runMemorySamples: () => memorySamplers.forEach((sample) => sample()),
    setInterval: (sample) => memorySamplers.push(sample),
    WebAssembly: {
      Instance: WebAssembly.Instance,
      Module: WebAssembly.Module,
      Memory: WebAssembly.Memory,
      instantiate: WebAssembly.instantiate.bind(WebAssembly),
      instantiateStreaming: WebAssembly.instantiateStreaming.bind(WebAssembly),
    },
  };
}

function wasmWithExports(names, { memory = false } = {}) {
  const sections = [
    section(1, [1, 0x60, 0, 0]),
    section(3, [names.length, ...names.map(() => 0)]),
    ...(memory ? [section(5, [1, 0, 1])] : []),
    section(7, [names.length + Number(memory),
      ...(memory ? [...stringBytes("memory"), 2, 0] : []),
      ...names.flatMap((name, index) => [...stringBytes(name), 0, index])]),
    section(10, [names.length, ...names.flatMap(() => [2, 0, 0x0b])]),
  ];
  return Uint8Array.from([
    0, 0x61, 0x73, 0x6d, 1, 0, 0, 0,
    ...sections.flatMap(({ id, bytes }) => [id, ...leb128(bytes.length), ...bytes]),
  ]);
}

function section(id, bytes) {
  return { id, bytes };
}

function stringBytes(value) {
  const bytes = [...new TextEncoder().encode(value)];
  return [...leb128(bytes.length), ...bytes];
}

function leb128(value) {
  const bytes = [];
  do {
    let byte = value & 0x7f;
    value >>>= 7;
    if (value !== 0) byte |= 0x80;
    bytes.push(byte);
  } while (value !== 0);
  return bytes;
}
