import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../scripts/playground-prepared-lifecycle-qualification.mjs", import.meta.url), "utf8");
const start = source.indexOf("async function waitForObservation(");
const end = source.indexOf("async function pixels(", start);
assert.ok(start >= 0 && end > start);
const observe = vm.runInNewContext(`${source.slice(start, end)}\nwaitForObservation`, { setTimeout });

test("gallery observation polls the resolved boolean and forwards the prior GPU generation", async () => {
  const previous = { gpuGeneration: 1, presentedFrames: 2 };
  let calls = 0;
  const result = await observe({ async evaluate(_predicate, argument) {
    assert.equal(argument, previous);
    return ++calls === 2;
  } }, async () => {}, previous);
  assert.equal(result, true);
  assert.equal(calls, 2, "an unresolved promise is not a fresh presentation");
});

test("gallery observation preserves a rejected renderer query", async () => {
  const failure = new Error("receiver unavailable");
  await assert.rejects(observe({ async evaluate() { throw failure; } }, async () => {}),
    error => error === failure);
});
