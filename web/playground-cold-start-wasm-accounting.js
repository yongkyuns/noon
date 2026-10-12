// Installed only by the cold-start browser probe after it rewrites worker entry
// modules. Production workers never import this file.
const ACCOUNTING_KEY = "__noonColdStartWasmAccounting";
const MEMORY_SAMPLE_INTERVAL_MS = 25;

export function installNoonWasmAccounting(workerGlobal, { role = "unknown" } = {}) {
  const wasm = workerGlobal.WebAssembly;
  if (!wasm || typeof wasm.instantiate !== "function") {
    throw new TypeError("worker WebAssembly API is unavailable");
  }
  if (workerGlobal[ACCOUNTING_KEY] !== undefined) {
    throw new Error("Noon WASM accounting is already installed in this worker");
  }

  const records = [];
  const memoryStates = [];
  const accounting = Object.freeze({
    schemaVersion: 2,
    role,
    records,
    snapshot: () => memoryStates.map(snapshotMemoryState),
  });
  Object.defineProperty(workerGlobal, ACCOUNTING_KEY, {
    configurable: false,
    enumerable: false,
    value: accounting,
  });

  const originalInstantiate = wasm.instantiate;
  wasm.instantiate = async function (source, imports) {
    const result = await Reflect.apply(originalInstantiate, this, [source, imports]);
    recordNoonInstance(result, byteLengthOf(source), "instantiate-input");
    return result;
  };

  if (typeof wasm.instantiateStreaming === "function") {
    const originalInstantiateStreaming = wasm.instantiateStreaming;
    wasm.instantiateStreaming = async function (source, imports) {
      const inputBytes = responseWasmByteLength(source);
      const result = await Reflect.apply(originalInstantiateStreaming, this, [source, imports]);
      recordNoonInstance(result, inputBytes, "identity-response-content-length");
      return result;
    };
  }

  return accounting;

  function recordNoonInstance(result, inputBytes, byteLengthSource) {
    const instance = result instanceof wasm.Instance ? result : result?.instance;
    const exports = instance?.exports;
    if (!isNoonWasmBindgenExports(exports)) return;
    records.push(Object.freeze({
      ordinal: records.length + 1,
      instantiatedBytes: Number.isSafeInteger(inputBytes) ? inputBytes : null,
      byteLengthSource: Number.isSafeInteger(inputBytes) ? byteLengthSource : null,
      identity: "wasm-bindgen exports __wbindgen_start, __wbindgen_malloc, __wbindgen_free",
    }));
    const memory = exports.memory;
    if (!(memory instanceof wasm.Memory)) {
      memoryStates.push({
        ordinal: records.length,
        available: false,
        reason: "Noon instance does not export its WebAssembly.Memory",
      });
      return;
    }
    const state = {
      ordinal: records.length,
      available: true,
      memory,
      initialBytes: null,
      peakObservedBytes: 0,
      latestBytes: 0,
      sampleCount: 0,
      firstSampleAtMs: null,
      peakObservedAtMs: null,
      lastSampleAtMs: null,
      samplingError: null,
    };
    memoryStates.push(state);
    sampleMemoryState(state);
    state.firstSampleAtMs = state.lastSampleAtMs;
    workerGlobal.setInterval(() => sampleMemoryState(state), MEMORY_SAMPLE_INTERVAL_MS);
  }
}

function sampleMemoryState(state) {
  if (!state.available) return;
  try {
    const bytes = state.memory.buffer.byteLength;
    const atMs = globalThis.performance?.now?.() ?? null;
    if (state.initialBytes === null) state.initialBytes = bytes;
    state.latestBytes = bytes;
    state.sampleCount += 1;
    state.lastSampleAtMs = atMs;
    if (bytes > state.peakObservedBytes || state.peakObservedAtMs === null) {
      state.peakObservedAtMs = atMs;
    }
    state.peakObservedBytes = Math.max(state.peakObservedBytes, bytes);
  } catch (error) {
    state.samplingError = String(error);
  }
}

function snapshotMemoryState(state) {
  sampleMemoryState(state);
  return state.available ? {
    ordinal: state.ordinal,
    available: true,
    initialBytes: state.initialBytes,
    peakObservedBytes: state.peakObservedBytes,
    latestBytes: state.latestBytes,
    sampleCount: state.sampleCount,
    samplingIntervalMs: MEMORY_SAMPLE_INTERVAL_MS,
    firstSampleAtMs: state.firstSampleAtMs,
    peakObservedAtMs: state.peakObservedAtMs,
    lastSampleAtMs: state.lastSampleAtMs,
    samplingError: state.samplingError,
  } : { ordinal: state.ordinal, available: false, reason: state.reason };
}

function isNoonWasmBindgenExports(exports) {
  return exports !== null && typeof exports === "object" &&
    typeof exports.__wbindgen_start === "function" &&
    typeof exports.__wbindgen_malloc === "function" &&
    typeof exports.__wbindgen_free === "function";
}

function byteLengthOf(source) {
  if (source instanceof ArrayBuffer ||
      (typeof SharedArrayBuffer === "function" && source instanceof SharedArrayBuffer)) {
    return source.byteLength;
  }
  if (ArrayBuffer.isView(source)) return source.byteLength;
  return null;
}

function responseWasmByteLength(source) {
  const headers = source?.headers;
  if (!headers || typeof headers.get !== "function") return null;
  const encoding = headers.get("content-encoding");
  if (encoding !== null && encoding !== "" && encoding.toLowerCase() !== "identity") return null;
  const header = headers.get("content-length");
  if (header === null || !/^\d+$/.test(header)) return null;
  const length = Number(header);
  return Number.isSafeInteger(length) ? length : null;
}

if (typeof self !== "undefined" && self?.WebAssembly) {
  installNoonWasmAccounting(self, {
    role: /python-worker(?:\.source)?\.js/.test(self.location?.href ?? "") ? "authoring" :
      /execution-render-worker\.js/.test(self.location?.href ?? "") ? "render" : "unknown",
  });
}
