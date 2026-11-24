import assert from "node:assert/strict";
import test from "node:test";
import { createWorkerTaskYield } from "./worker-task-yield.js";

test("worker yields resume on separate channel tasks in order", async () => {
  const messages = [];
  const channel = { port1: {}, port2: { postMessage: value => messages.push(value) } };
  const yieldTask = createWorkerTaskYield(channel);
  const resumed = [];
  const first = yieldTask().then(() => resumed.push(1));
  const second = yieldTask().then(() => resumed.push(2));
  await Promise.resolve();
  assert.deepEqual(resumed, [], "a microtask must not release a worker turn");
  assert.deepEqual(messages, [null, null]);
  channel.port1.onmessage();
  await first;
  assert.deepEqual(resumed, [1]);
  channel.port1.onmessage();
  await second;
  assert.deepEqual(resumed, [1, 2]);
});
