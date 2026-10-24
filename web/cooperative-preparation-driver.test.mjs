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
  const driver = createCooperativePreparationDriver(() => ++calls < 3 ? "continue" : "finished", { ...t, onProgress() { wakes++; } });
  assert.equal(calls, 0); assert.equal(t.queue.size, 1);
  t.run(); assert.equal(calls, 1); assert.equal(t.queue.size, 1);
  t.run(); assert.equal(calls, 2); assert.equal(t.queue.size, 1);
  t.run(); assert.equal(calls, 3); assert.equal(t.queue.size, 0);
  assert.equal(wakes, 3); assert.deepEqual(driver.stats(), { active: false, waiting: false, turns: 3, failed: false });
});
test("stop cancels queued work and is safe during delivery", () => {
  const t = tasks(); let calls = 0;
  const driver = createCooperativePreparationDriver(() => { calls++; driver.stop(); return "continue"; }, t);
  t.run(); assert.equal(calls, 1); assert.equal(t.queue.size, 0);
  driver.stop(); assert.equal(t.queue.size, 0);
  const other = createCooperativePreparationDriver(() => { calls++; return "continue"; }, t);
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

test("satisfied demand waits without polling or presentation feedback; wakes coalesce", () => {
  const t = tasks(); let calls = 0, wakes = 0, more = false;
  const driver = createCooperativePreparationDriver(() => { calls++; return more ? "finished" : "waiting"; }, { ...t, onProgress() { wakes++; driver.wake(); } });
  t.run();
  assert.deepEqual(driver.stats(), { active: true, waiting: true, turns: 1, failed: false });
  assert.equal(t.queue.size, 0); assert.equal(wakes, 0);
  for (let i = 0; i < 20; i++) driver.wake();
  assert.equal(t.queue.size, 1); assert.equal(calls, 1);
  t.run(); assert.equal(t.queue.size, 0); assert.equal(wakes, 0);
  more = true; driver.wake(); t.run();
  assert.equal(calls, 3); assert.equal(wakes, 1); assert.equal(t.queue.size, 0);
  driver.wake(); assert.equal(t.queue.size, 0, "completed source cannot revive");
});

test("waiting source can be cancelled and stale queued work is inert", () => {
  const t = tasks(); let calls = 0;
  const driver = createCooperativePreparationDriver(() => { calls++; return "waiting"; }, t);
  t.run(); driver.wake(); const stale = [...t.queue.values()][0];
  driver.stop(); stale(); driver.wake();
  assert.equal(calls, 1); assert.equal(t.queue.size, 0);
  assert.deepEqual(driver.stats(), { active: false, waiting: false, turns: 1, failed: false });
});

test("reentrant demand while delivering progress creates only one next task", () => {
  const t = tasks(); let calls = 0;
  const driver = createCooperativePreparationDriver(() => ++calls < 3 ? "continue" : "finished", { ...t, onProgress() { driver.wake(); driver.wake(); } });
  t.run(); assert.equal(t.queue.size, 1);
  t.run(); assert.equal(t.queue.size, 1);
  t.run(); assert.equal(t.queue.size, 0);
  assert.equal(calls, 3);
});

test("failed preparation cannot be woken back into execution", () => {
  const t = tasks(); let calls = 0, errors = 0;
  const driver = createCooperativePreparationDriver(() => { calls++; throw new Error("failed"); }, { ...t, onError() { errors++; driver.wake(); } });
  t.run(); driver.wake();
  assert.equal(calls, 1); assert.equal(errors, 1); assert.equal(t.queue.size, 0);
});
