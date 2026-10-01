// Installed only by the cold-start browser probe after it rewrites worker entry
// modules. Production workers never import this file.
const ACCOUNTING_KEY = "__noonColdStartWasmAccounting";

export function installNoonWasmAccounting(workerGlobal, { role = "unknown" } = {}) {
  const wasm = workerGlobal.WebAssembly;
  if (!wasm || typeof wasm.instantiate !== "function") {
    throw new TypeError("worker WebAssembly API is unavailable");
  }
  if (workerGlobal[ACCOUNTING_KEY] !== undefined) {
    throw new Error("Noon WASM accounting is already installed in this worker");
  }

  const records = [];
  const accounting = Object.freeze({ schemaVersion: 1, role, records });
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
  }
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
