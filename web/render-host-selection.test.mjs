import assert from "node:assert/strict";
import test from "node:test";
import { resetRenderHostSelectionForTests, selectExecutionRenderHost } from "./render-host-selection.js";

function browserHosts(t, { worker = true, main = true, constructorError = false } = {}) {
  const saved = new Map();
  const counts = { transfers: 0, terminated: 0, released: 0, revoked: 0 };
  for (const [name, value] of Object.entries({
    HTMLCanvasElement: class { transferControlToOffscreen() {} },
    document: { createElement() {
      return { transferControlToOffscreen() {
        counts.transfers += 1;
        return { getContext(kind) {
          return kind === "webgl2" && main ? {
            getExtension() { return { loseContext() { counts.released += 1; } }; },
          } : null;
        } };
      } };
    } },
    Worker: class {
      constructor() { if (constructorError) throw new Error("blob worker denied"); }
      postMessage() { queueMicrotask(() => this.onmessage({ data: { ok: worker } })); }
      terminate() { counts.terminated += 1; }
    },
    URL: class extends URL {
      static createObjectURL() { return "blob:probe"; }
      static revokeObjectURL() { counts.revoked += 1; }
    },
    __NOON_RENDER_HOST__: null,
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
});

test("unusable worker surface falls back and releases the disposable main-thread context", async (t) => {
  const counts = browserHosts(t, { worker: false });
  assert.equal(await selectExecutionRenderHost(), "main-thread");
  assert.equal(counts.transfers, 2);
  assert.equal(counts.released, 1);
});

test("worker construction failure still probes the main-thread host", async (t) => {
  const counts = browserHosts(t, { constructorError: true });
  assert.equal(await selectExecutionRenderHost(), "main-thread");
  assert.equal(counts.revoked, 1);
  assert.equal(counts.transfers, 1);
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
