import assert from "node:assert/strict";
import test from "node:test";

import { installNoonWasmAccounting } from "./playground-cold-start-wasm-accounting.js";

const noonWasmBytes = wasmWithExports([
  "__wbindgen_start",
  "__wbindgen_malloc",
  "__wbindgen_free",
]);
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
  return {
    WebAssembly: {
      Instance: WebAssembly.Instance,
      Module: WebAssembly.Module,
      instantiate: WebAssembly.instantiate.bind(WebAssembly),
      instantiateStreaming: WebAssembly.instantiateStreaming.bind(WebAssembly),
    },
  };
}

function wasmWithExports(names) {
  const sections = [
    section(1, [1, 0x60, 0, 0]),
    section(3, [names.length, ...names.map(() => 0)]),
    section(7, [names.length, ...names.flatMap((name, index) => [
      ...stringBytes(name), 0, index,
    ])]),
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
