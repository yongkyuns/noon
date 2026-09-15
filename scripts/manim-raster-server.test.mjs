import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { serveRepository } from "./browser-test-server.mjs";

async function host(t, options) {
  const root = await mkdtemp(path.join(tmpdir(), "noon-raster-server-"));
  await writeFile(path.join(root, "scene.html"), "<!doctype html><title>scene</title>");
  await writeFile(path.join(root, "worker.js"), "self.ready = true;");
  await writeFile(path.join(root, "engine.wasm"), new Uint8Array([0, 97, 115, 109]));
  const server = await serveRepository(root, 0, options);
  t.after(async () => {
    await server.close();
    await rm(root, { recursive: true, force: true });
  });
  return { ...server, root };
}

test("isolated raster document, worker and WASM share COOP/COEP and MIME types", async (t) => {
  const server = await host(t, { crossOriginIsolated: true });
  assert.notEqual(new URL(server.baseUrl).port, "0");
  for (const [file, mime] of [["scene.html", "text/html"],
    ["worker.js", "text/javascript"], ["engine.wasm", "application/wasm"]]) {
    const response = await fetch(`${server.baseUrl}/${file}`);
    assert.equal(response.status, 200);
    assert.equal(response.headers.get("cross-origin-opener-policy"), "same-origin");
    assert.equal(response.headers.get("cross-origin-embedder-policy"), "require-corp");
    assert.equal(response.headers.get("cross-origin-resource-policy"), "same-origin");
    assert.equal(response.headers.get("content-type"), mime);
    assert.ok((await response.arrayBuffer()).byteLength > 0);
  }
});

test("isolation remains opt-in for existing non-isolated hosts", async (t) => {
  const server = await host(t);
  const response = await fetch(`${server.baseUrl}/scene.html`);
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("cross-origin-opener-policy"), null);
  assert.equal(response.headers.get("cross-origin-embedder-policy"), null);
  await response.arrayBuffer();
  const missing = await fetch(`${server.baseUrl}/missing.html`);
  assert.equal(missing.status, 404);
  await missing.arrayBuffer();
});

test("a bind collision rejects instead of capturing another task's server", async (t) => {
  const server = await host(t, { crossOriginIsolated: true });
  await assert.rejects(
    serveRepository(server.root, Number(new URL(server.baseUrl).port), { crossOriginIsolated: true }),
    { code: "EADDRINUSE" },
  );
});
