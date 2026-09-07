import assert from "node:assert/strict";
import test from "node:test";
import { resetRenderHostSelectionForTests, selectExecutionRenderHost } from "./render-host-selection.js";

function browserHosts(t, {
  worker = true,
  main = true,
  workerWebGpuContext = false,
  mainWebGpuContext = false,
  workerAdapter = undefined,
  mainAdapter = undefined,
  constructorError = false,
} = {}) {
  const saved = new Map();
  const counts = {
    transfers: 0,
    terminated: 0,
    released: 0,
    revoked: 0,
    appended: 0,
    removed: 0,
    mainContextOptions: null,
    workerOptions: null,
    workerSource: "",
    adapterRequests: [],
  };
  const gpu = (adapterAvailable, host) => adapterAvailable === undefined ? undefined : {
    async requestAdapter(options) {
      counts.adapterRequests.push([host, options]);
      return adapterAvailable ? {} : null;
    },
  };
  const surface = (host) => ({
    getContext(kind, options) {
      if (kind === "webgpu") {
        return (host === "worker" ? workerWebGpuContext : mainWebGpuContext) ? {} : null;
      }
      if (kind === "webgl2") {
        if (host === "main") counts.mainContextOptions = options;
        const available = host === "worker" ? worker : main;
        return available ? {
          getExtension() { return { loseContext() { counts.released += 1; } }; },
        } : null;
      }
      return null;
    },
  });
  const Canvas = class {
    transferControlToOffscreen() {
      counts.transfers += 1;
      return surface(counts.transfers === 1 ? "worker" : "main");
    }
    remove() { counts.removed += 1; }
  };
  for (const [name, value] of Object.entries({
    HTMLCanvasElement: Canvas,
    document: {
      body: { append() { counts.appended += 1; } },
      createElement() { return new Canvas(); },
    },
    Blob: class {
      constructor(parts) { counts.workerSource = parts.join(""); }
    },
    Worker: class {
      constructor(_url, options) {
        counts.workerOptions = options;
        if (constructorError) throw new Error("blob worker denied");
        const workerGlobal = {
          navigator: { gpu: gpu(workerAdapter, "worker") },
          postMessage: (data) => queueMicrotask(() => this.onmessage({ data })),
        };
        Function("self", counts.workerSource)(workerGlobal);
        this.workerGlobal = workerGlobal;
      }
      postMessage(data) { this.workerGlobal.onmessage({ data }); }
      terminate() { counts.terminated += 1; }
    },
    URL: class extends URL {
      static createObjectURL() { return "blob:probe"; }
      static revokeObjectURL() { counts.revoked += 1; }
    },
    __NOON_RENDER_HOST__: null,
    navigator: { gpu: gpu(mainAdapter, "main") },
    location: { href: "https://example.test/" },
  })) {
    saved.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
    Object.defineProperty(globalThis, name, { configurable: true, writable: true, value });
  }
  resetRenderHostSelectionForTests();
  t.after(() => {
    resetRenderHostSelectionForTests();
    for (const [name, descriptor] of saved) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else delete globalThis[name];
    }
  });
  return counts;
}

test("automatic selection prefers and caches the usable worker host", async (t) => {
  const counts = browserHosts(t);
  assert.equal(await selectExecutionRenderHost(), "worker");
  assert.equal(await selectExecutionRenderHost(), "worker");
  assert.equal(counts.transfers, 1);
  assert.equal(counts.terminated, 1);
  assert.equal(counts.revoked, 1);
  assert.equal(counts.appended, 1);
  assert.equal(counts.removed, 1);
  assert.deepEqual(counts.workerOptions, {
    type: "module",
    name: "noon-render-capability-probe",
  });
  assert.match(counts.workerSource, /getContext\("webgl2", \{ antialias: false \}\)/);
  assert.match(counts.workerSource, /requestAdapter/);
});

test("unusable worker surface falls back and releases the disposable main-thread context", async (t) => {
  const counts = browserHosts(t, { worker: false });
  assert.equal(await selectExecutionRenderHost(), "main-thread");
  assert.equal(counts.transfers, 2);
  assert.equal(counts.released, 1);
  assert.equal(counts.appended, 2);
  assert.equal(counts.removed, 2);
  assert.deepEqual(counts.mainContextOptions, { antialias: false });
});

test("worker construction failure still probes the main-thread host", async (t) => {
  const counts = browserHosts(t, { constructorError: true });
  assert.equal(await selectExecutionRenderHost(), "main-thread");
  assert.equal(counts.revoked, 1);
  assert.equal(counts.transfers, 1);
});

test("a WebGPU canvas context without an adapter falls back to exact WebGL capability", async (t) => {
  const counts = browserHosts(t, {
    worker: false,
    main: false,
    workerWebGpuContext: true,
    mainWebGpuContext: true,
    workerAdapter: false,
    mainAdapter: false,
  });
  await assert.rejects(selectExecutionRenderHost(), /either a worker or the main thread/);
  assert.deepEqual(counts.adapterRequests.map(([host]) => host), ["worker", "main"]);
  assert.deepEqual(counts.mainContextOptions, { antialias: false });
});

test("an available WebGPU adapter selects its host before claiming the context", async (t) => {
  const workerCounts = browserHosts(t, {
    worker: false,
    main: false,
    workerWebGpuContext: true,
    workerAdapter: true,
  });
  assert.equal(await selectExecutionRenderHost(), "worker");
  assert.deepEqual(workerCounts.adapterRequests, [["worker", {
    powerPreference: "high-performance",
    forceFallbackAdapter: false,
  }]]);

  resetRenderHostSelectionForTests();
  const mainCounts = browserHosts(t, {
    worker: false,
    main: false,
    mainWebGpuContext: true,
    mainAdapter: true,
  });
  assert.equal(await selectExecutionRenderHost(), "main-thread");
  assert.deepEqual(mainCounts.adapterRequests, [["main", {
    powerPreference: "high-performance",
    forceFallbackAdapter: false,
  }]]);
});

test("failed automatic selection can retry when surfaces become available", async (t) => {
  browserHosts(t, { worker: false, main: false });
  await assert.rejects(selectExecutionRenderHost(), /either a worker or the main thread/);
  globalThis.Worker = class {
    postMessage() { queueMicrotask(() => this.onmessage({ data: { ok: true } })); }
    terminate() {}
  };
  assert.equal(await selectExecutionRenderHost(), "worker");
});

test("forced hosts bypass probing and reject unknown host values", async (t) => {
  const counts = browserHosts(t);
  assert.equal(selectExecutionRenderHost({ force: "main-thread" }), "main-thread");
  assert.equal(counts.transfers, 0);
  assert.throws(() => selectExecutionRenderHost({ force: "unknown" }), /unsupported Noon render host/);
});
