// Test-only WebKit experiment; neither scene engine nor product worker changes.
import assert from "node:assert/strict";

export function repeatedProbeQueue(queue, repeats) {
  assert.ok(Number.isSafeInteger(repeats) && repeats >= 1 && repeats <= 24,
    "probe repeats must be in 1..24");
  assert.equal(queue.length, 2, "probe needs exactly one normal and one no-JSPI case");
  assert.equal(queue[0].entry.id, queue[1].entry.id, "probe modes must share source");
  assert.deepEqual(queue.map(x => x.noJspi), [false, true]);
  return Array.from({length: repeats}, (_, i) =>
    (i % 2 ? [...queue].reverse() : queue).map(spec => ({
      ...spec, probeTrial: i + 1,
    }))).flat();
}

const WORKER_SOURCE = "// Diagnostic-only worker, loaded before the unchanged production worker.\nconst observed = new Map();\nlet nextId = 0;\nconst register = instance => {\n  const exports = instance?.exports;\n  if (!exports) return;\n  for (const [name, value] of Object.entries(exports))\n    if (value instanceof WebAssembly.Memory && !observed.has(value))\n      observed.set(value, {id: ++nextId, name});\n};\nconst originalInstantiate = WebAssembly.instantiate;\nWebAssembly.instantiate = function(...args) {\n  return Promise.resolve(originalInstantiate.apply(this, args)).then(result => {\n    register(result?.instance ?? result); return result;\n  });\n};\nconst streaming = WebAssembly.instantiateStreaming;\nif (streaming) WebAssembly.instantiateStreaming = function(...args) {\n  return Promise.resolve(streaming.apply(this, args)).then(result => {\n    register(result?.instance ?? result); return result;\n  });\n};\nconst OriginalInstance = WebAssembly.Instance;\nWebAssembly.Instance = new Proxy(OriginalInstance, {\n  construct(target, args, newTarget) {\n    const instance = Reflect.construct(target, args, newTarget);\n    register(instance); return instance;\n  },\n});\nconst jspiInitiallyAvailable = \"Suspending\" in WebAssembly && \"promising\" in WebAssembly;\nif (__NO_JSPI__) {\n  delete WebAssembly.promising;\n  delete WebAssembly.Suspending;\n  if (\"promising\" in WebAssembly || \"Suspending\" in WebAssembly)\n    throw Error(\"no-JSPI precondition not satisfied\");\n}\nconst activeJspi = \"Suspending\" in WebAssembly && \"promising\" in WebAssembly;\nlet library = null, sequence = 0;\nimport(\"./pkg/noon_web.js\").then(module => { library = module; }).catch(() => {});\nfunction sample(phase) {\n  const memories = [];\n  for (const [memory, info] of observed) {\n    let bytes = null;\n    try { bytes = memory.buffer.byteLength; } catch {}\n    memories.push({...info, bytes, pages: bytes === null ? null : bytes / 65536});\n  }\n  let allocator = null;\n  try {\n    if (library?.noon_allocator_probe_snapshot)\n      allocator = JSON.parse(library.noon_allocator_probe_snapshot());\n  } catch (error) { allocator = {readError:String(error).slice(0,240)}; }\n  postMessage({\n    channel:\"noon.diagnostic.wasm-memory\", seq:++sequence,\n    phase, at:performance.now(), jspiInitiallyAvailable, activeJspi,\n    memories, allocator,\n  });\n}\nself.addEventListener(\"error\", () => sample(\"worker-error\"));\nself.addEventListener(\"unhandledrejection\", () => sample(\"worker-rejection\"));\nsetInterval(() => sample(\"sample\"), 200);\nsample(\"before-import\");\nawait import(\"./python-worker.js\");\nsample(\"after-import\");\n";

export function workerProbeSource(noJspi = false) {
  return WORKER_SOURCE.replace("__NO_JSPI__", noJspi ? "true" : "false");
}

export async function installWasmAllocationObserver(context, {noJspi = false} = {}) {
  const body = workerProbeSource(noJspi);
  await context.route("**/python-worker-alloc-probe.js", route => route.fulfill({
    status: 200, contentType: "text/javascript",
    headers: {
      "Cross-Origin-Embedder-Policy": "require-corp",
      "Cross-Origin-Resource-Policy": "same-origin",
    }, body,
  }));
  await context.addInitScript(() => {
    window.__noonWasmMemorySamples = [];
    window.Worker = new Proxy(window.Worker, {
      construct(target, args, newTarget) {
        const workerArgs = [...args];
        const original = new URL(workerArgs[0], location.href);
        if (original.pathname.endsWith("/python-worker.js"))
          workerArgs[0] = new URL("./python-worker-alloc-probe.js", original);
        const worker = Reflect.construct(target, workerArgs, newTarget);
        if (original.pathname.endsWith("/python-worker.js")) {
          worker.addEventListener("message", event => {
            if (event.data?.channel !== "noon.diagnostic.wasm-memory") return;
            window.__noonWasmMemorySamples.push(event.data);
            if (window.__noonWasmMemorySamples.length > 500)
              window.__noonWasmMemorySamples.shift();
            // Do not expose test telemetry to the production worker protocol.
            event.stopImmediatePropagation();
          }, {capture:true});
        }
        return worker;
      },
    });
  });
}
