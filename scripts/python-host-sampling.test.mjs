import assert from "node:assert/strict";
import test from "node:test";
import { sampleOrSourceFailure } from "./python-host-sampling.mjs";

test("source success never replaces the final renderer receipt", async () => {
  let finishSample;
  const sample = new Promise(resolve => { finishSample = resolve; });
  let settled = false;
  const pending = sampleOrSourceFailure(sample, Promise.resolve({ ok: true }));
  pending.then(() => { settled = true; });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(settled, false);
  finishSample({ time: 1, sourceCompleted: true });
  assert.deepEqual(await pending, { sample: { time: 1, sourceCompleted: true } });
});

test("source cancellation does not wait for a receipt that cannot arrive", async () => {
  const failure = { ok: false, message: "CancelledError" };
  assert.deepEqual(await sampleOrSourceFailure(new Promise(() => {}), Promise.resolve(failure)),
    { terminal: failure });
});

test("failed sample on a successful source is not hidden", async () => {
  const failure = new Error("render request failed");
  await assert.rejects(sampleOrSourceFailure(Promise.reject(failure), Promise.resolve({ ok: true })),
    error => error === failure);
});

test("concurrent source and sample rejection retain the source failure", async () => {
  const failure = { ok: false, message: "NoonCallbackError" };
  const result = await sampleOrSourceFailure(Promise.reject(new Error("retired endpoint")),
    Promise.resolve(failure));
  assert.deepEqual(result.terminal, failure);
});

test("initial callback failure waits for the Python source to unwind", async () => {
  let finishSource;
  const terminal = new Promise(resolve => { finishSource = resolve; });
  let settled = false;
  const attached = Promise.reject(new Error("callback prevented initial attachment"));
  const pending = sampleOrSourceFailure(attached, terminal);
  pending.then(() => { settled = true; });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(settled, false, "attachment failure bypassed Python teardown");
  const failure = { ok: false, message: "NoonCallbackError: intentional callback abort" };
  finishSource(failure);
  assert.deepEqual((await pending).terminal, failure);
});
