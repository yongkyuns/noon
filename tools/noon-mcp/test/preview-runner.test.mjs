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
  return new Promise((resolve, reject) => {
    let buffered = Buffer.alloc(0);
    const cleanup = () => {
      stream.off("data", onData);
      stream.off("error", onError);
    };
    const onError = (error) => { cleanup(); reject(error); };
    const onData = (chunk) => {
      buffered = Buffer.concat([buffered, Buffer.from(chunk)]);
      const newline = buffered.indexOf(0x0a);
      if (newline < 0) return;
      cleanup();
      try { resolve(JSON.parse(buffered.subarray(0, newline).toString("utf8"))); }
      catch (error) { reject(error); }
    };
    stream.on("data", onData);
    stream.once("error", onError);
  });
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

test("cancellation rejects the request and poisons the control channel", async () => {
  const { client, toWorker, fromWorker } = fixture();
  const outbound = requestLine(toWorker);
  const controller = new AbortController();
  const pending = client.request({ op: "sample", time: 1 }, { signal: controller.signal });
  await outbound;
  controller.abort("client canceled");
  await assert.rejects(pending, /client canceled/);
  await assert.rejects(client.request({ op: "inspect" }), /client canceled/);
});

test("concurrent requests are rejected before duplicate worker work", async () => {
  const { client, toWorker } = fixture();
  const outbound = requestLine(toWorker);
  const first = client.request({ op: "inspect" });
  await outbound;
  await assert.rejects(client.request({ op: "sample", time: 1 }), /already in progress/);
  client.close("test complete");
  await assert.rejects(first, /test complete/);
});

test("request timeout poisons the control channel", async () => {
  const { client, toWorker } = fixture();
  const outbound = requestLine(toWorker);
  const pending = client.request({ op: "inspect" }, { timeoutMs: 1 });
  await outbound;
  await assert.rejects(pending, /timed out/);
  await assert.rejects(client.request({ op: "inspect" }), /timed out/);
});

test("oversized source request is rejected before writing to the worker", async () => {
  const { client } = fixture();
  const huge = "x".repeat(8 * 1024 * 1024);
  await assert.rejects(client.request({ op: "open", source: huge }), /protocol byte limit/);
  client.close();
});
