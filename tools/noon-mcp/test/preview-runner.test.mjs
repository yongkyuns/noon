import assert from "node:assert/strict";
import { PassThrough } from "node:stream";
import test from "node:test";

import { JsonLineRpcClient } from "../src/preview-runner.mjs";

function fixture() {
  const toWorker = new PassThrough();
  const fromWorker = new PassThrough();
  const client = new JsonLineRpcClient({ stdin: toWorker, stdout: fromWorker });
  return { client, toWorker, fromWorker };
}

async function requestLine(stream) {
  const chunks = [];
  for await (const chunk of stream) {
    chunks.push(chunk);
    const buffer = Buffer.concat(chunks);
    const newline = buffer.indexOf(0x0a);
    if (newline >= 0) return JSON.parse(buffer.subarray(0, newline).toString("utf8"));
  }
  throw new Error("request stream ended");
}

test("JSON-line client correlates one bounded request and response", async () => {
  const { client, toWorker, fromWorker } = fixture();
  const outbound = requestLine(toWorker);
  const pending = client.request({ op: "inspect" });
  const request = await outbound;
  assert.equal(request.op, "inspect");
  fromWorker.write(`${JSON.stringify({ id: request.id, ok: true, snapshot: { state: "ready" } })}\n`);
  assert.equal((await pending).snapshot.state, "ready");
  client.close();
});

test("unknown response IDs fail outstanding work instead of being ignored", async () => {
  const { client, toWorker, fromWorker } = fixture();
  const outbound = requestLine(toWorker);
  const pending = client.request({ op: "inspect" });
  const request = await outbound;
  fromWorker.write(`${JSON.stringify({ id: request.id + 1, ok: true })}\n`);
  await assert.rejects(pending, /unknown request id/);
  await assert.rejects(client.request({ op: "inspect" }), /unknown request id/);
});

test("cancellation rejects the request and does not accept its late response", async () => {
  const { client, toWorker, fromWorker } = fixture();
  const outbound = requestLine(toWorker);
  const controller = new AbortController();
  const pending = client.request({ op: "sample", time: 1 }, { signal: controller.signal });
  const request = await outbound;
  controller.abort("client canceled");
  await assert.rejects(pending, /client canceled/);
  fromWorker.write(`${JSON.stringify({ id: request.id, ok: true })}\n`);
  await assert.rejects(client.request({ op: "inspect" }), /unknown request id/);
});

test("oversized source request is rejected before writing to the worker", async () => {
  const { client } = fixture();
  const huge = "x".repeat(8 * 1024 * 1024);
  await assert.rejects(client.request({ op: "open", source: huge }), /protocol byte limit/);
  client.close();
});
