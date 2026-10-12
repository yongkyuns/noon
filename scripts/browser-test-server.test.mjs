import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { get } from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { serveRepository } from "./browser-test-server.mjs";

async function fixture(t) {
  const root = await mkdtemp(path.join(tmpdir(), "noon-file-server-"));
  const bytes = Buffer.alloc(8 * 1024 * 1024, 0x6e);
  await writeFile(path.join(root, "engine.wasm"), bytes);
  await writeFile(path.join(root, "identity.json"), '{"schema":1}');
  const server = await serveRepository(root, 0);
  t.after(async () => {
    await server.close();
    await rm(root, { recursive: true, force: true });
  });
  return { root, bytes, server };
}

function headers(url) {
  return new Promise((resolve, reject) => {
    get(url, resolve).on("error", reject);
  });
}

test("concurrent paused runtime downloads preserve bytes and survive peer cancellation", { timeout: 10_000 }, async t => {
  const { bytes, server } = await fixture(t);
  const expected = createHash("sha256").update(bytes).digest("hex");
  const download = async () => {
    const response = await headers(`${server.baseUrl}/engine.wasm`);
    assert.equal(response.statusCode, 200);
    assert.equal(response.headers["content-type"], "application/wasm");
    response.pause();
    const result = new Promise((resolve, reject) => {
      const hash = createHash("sha256");
      let length = 0;
      response.on("data", chunk => { length += chunk.length; hash.update(chunk); });
      response.on("error", reject);
      response.on("end", () => resolve({ length, digest: hash.digest("hex") }));
    });
    setTimeout(() => response.resume(), 20);
    return result;
  };
  const downloads = [download(), download(), download(), download()];
  const canceled = await headers(`${server.baseUrl}/engine.wasm`);
  canceled.on("error", () => {});
  canceled.destroy();
  assert.deepEqual(await Promise.all(downloads), Array(4).fill({ length: bytes.length, digest: expected }));
  assert.deepEqual(await (await fetch(`${server.baseUrl}/identity.json`)).json(), { schema: 1 });
});

test("bind collisions fail before admission and shutdown closes paused downloads", { timeout: 10_000 }, async t => {
  const { root, server } = await fixture(t);
  const port = Number(new URL(server.baseUrl).port);
  await assert.rejects(serveRepository(root, port), { code: "EADDRINUSE" });
  const response = await headers(`${server.baseUrl}/engine.wasm`);
  response.on("error", () => {});
  response.pause();
  await server.close();
  response.destroy();
});
