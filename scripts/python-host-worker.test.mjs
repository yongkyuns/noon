import assert from "node:assert/strict";
import test from "node:test";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

test("no-JSPI interception preserves worker isolation without rewriting the real worker", async () => {
  for (const crossOriginIsolated of [false, true]) {
    let fulfilled, handler, preflight = false;
    const context = {
      async addInitScript(source) { assert.equal(typeof source, "function"); },
      async route(pattern, callback) {
        assert.equal(pattern, "**/python-worker-no-jspi-test.js");
        handler = callback;
      },
    };
    await disableAuthoringJspi(context, {
      crossOriginIsolated, beforeImport: () => { preflight = true; },
    });
    await handler({ async fulfill(value) { fulfilled = value; } });
    assert.equal(preflight, true);
    assert.equal(fulfilled.status, 200);
    assert.equal(fulfilled.contentType, "text/javascript");
    assert.deepEqual(fulfilled.headers, crossOriginIsolated ? {
      "Cross-Origin-Embedder-Policy": "require-corp",
      "Cross-Origin-Resource-Policy": "same-origin",
    } : {});
    assert.match(fulfilled.body, /delete WebAssembly.promising/);
    assert.match(fulfilled.body, /delete WebAssembly.Suspending/);
    assert.match(fulfilled.body, /await import\('\.\/python-worker\.js'\)/);
  }
});
