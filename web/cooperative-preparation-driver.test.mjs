import assert from "node:assert/strict";
import test from "node:test";
import { createCooperativePreparationDriver } from "./cooperative-preparation-driver.js";
function tasks() {
  const queue = new Map(); let next = 0;
  return { queue, setTimeout(fn) { const id = ++next; queue.set(id, fn); return id; },
    clearTimeout(id) { queue.delete(id); }, run() { const [id, fn] = queue.entries().next().value; queue.delete(id); fn(); } };
}
test("one preparation per task, one outstanding task, finished stays idle", () => {
  const t = tasks(); let calls = 0, wakes = 0;
  const driver = createCooperativePreparationDriver(() => ++calls < 3, { ...t, onProgress() { wakes++; } });
  assert.equal(calls, 0); assert.equal(t.queue.size, 1);
  t.run(); assert.equal(calls, 1); assert.equal(t.queue.size, 1);
  t.run(); assert.equal(calls, 2); assert.equal(t.queue.size, 1);
  t.run(); assert.equal(calls, 3); assert.equal(t.queue.size, 0);
  assert.equal(wakes, 3); assert.deepEqual(driver.stats(), { active: false, turns: 3, failed: false });
});
test("stop cancels queued work and is safe during delivery", () => {
  const t = tasks(); let calls = 0;
  const driver = createCooperativePreparationDriver(() => { calls++; driver.stop(); return true; }, t);
  t.run(); assert.equal(calls, 1); assert.equal(t.queue.size, 0);
  driver.stop(); assert.equal(t.queue.size, 0);
  const other = createCooperativePreparationDriver(() => { calls++; return true; }, t);
  other.stop(); assert.equal(calls, 1); assert.equal(t.queue.size, 0);
});
test("a failed callback is reported once and never retried", () => {
  const t = tasks(); let calls = 0, errors = 0;
  const driver = createCooperativePreparationDriver(() => { calls++; throw new Error("source failed"); }, { ...t, onError(error) { assert.match(error.message, /source failed/); errors++; } });
  t.run(); assert.equal(calls, 1); assert.equal(errors, 1); assert.equal(t.queue.size, 0);
  assert.equal(driver.stats().failed, true);
});
test("async callbacks cannot accidentally cause an unbounded preparation loop", () => {
  const t = tasks(); let errors = 0;
  const driver = createCooperativePreparationDriver(() => Promise.resolve(true), { ...t, onError(error) { assert.match(error.message, /synchronously/); errors++; } });
  t.run(); assert.equal(errors, 1); assert.equal(driver.stats().failed, true); assert.equal(t.queue.size, 0);
});
