import assert from "node:assert/strict";
import test from "node:test";
import { createPreparedInterpreterDriver } from "./prepared-interpreter-driver.js";

const flush = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function fixture(overrides = {}) {
  const tasks = new Map(), events = [];
  let next = 0, needed = false;
  const driver = createPreparedInterpreterDriver({
    prepareSegment: async (context, signal) => { events.push(["prepare", context]); assert.equal(signal.aborted, false); },
    needsPreparation: () => needed,
    finishSource: async value => { events.push(["finish", value]); },
    setTimeout: callback => { tasks.set(++next, callback); return next; },
    clearTimeout: id => tasks.delete(id),
    ...overrides,
  });
  return {
    driver, tasks, events,
    demand(value) { needed = value; },
    async turn() {
      assert.equal(tasks.size, 1, "exactly one coalesced task");
      const [id, callback] = tasks.entries().next().value;
      tasks.delete(id); callback(); await flush();
    },
  };
}

test("completed segment sleeps with the source stack suspended until demand changes", async () => {
  const f = fixture();
  let statements = 0;
  const done = f.driver.run(async () => {
    statements++;
    const event = await f.driver.awaitSegment("first");
    assert.deepEqual(JSON.parse(event), { kind: "complete" });
    statements++;
    await f.driver.awaitSegment("second");
    statements++;
    return "source-result";
  });
  assert.equal(statements, 1);
  await f.turn();
  assert.equal(f.driver.state(), "waiting");
  assert.equal(statements, 1);
  assert.deepEqual(f.events, [["prepare", "first"]]);
  assert.equal(f.tasks.size, 0);
  for (let i = 0; i < 50; i++) f.driver.wake();
  assert.equal(f.tasks.size, 0, "satisfied demand cannot poll");
  f.demand(true);
  for (let i = 0; i < 50; i++) f.driver.wake();
  assert.equal(statements, 1, "input wake must not execute source inline");
  await f.turn();
  assert.equal(statements, 2);
  assert.deepEqual(f.events, [["prepare", "first"]], "waking must not prepare the same segment twice");
  await f.turn();
  assert.equal(await done, "source-result");
  assert.deepEqual(f.events, [["prepare", "first"], ["prepare", "second"], ["finish", "source-result"]]);
  assert.equal(statements, 3);
  assert.equal(f.driver.state(), "finished");
  f.driver.wake(); f.driver.cancel();
  assert.equal(f.tasks.size, 0);
  assert.equal(f.driver.state(), "finished");
});

test("even continuously demanded barriers yield one platform task each", async () => {
  const f = fixture(); f.demand(true);
  let statements = 0;
  const done = f.driver.run(async () => {
    for (let i = 0; i < 20; i++) { await f.driver.awaitSegment(i); statements++; }
  });
  for (let i = 0; i < 20; i++) {
    assert.equal(statements, i); await f.turn(); assert.equal(statements, i + 1);
  }
  await done;
  assert.equal(f.events.filter(([kind]) => kind === "prepare").length, 20);
});

test("demand is rechecked on task delivery, not captured by an earlier wake", async () => {
  const f = fixture(); const done = f.driver.run(() => f.driver.awaitSegment("one"));
  await f.turn(); f.demand(true); f.driver.wake(); f.demand(false);
  await f.turn(); assert.equal(f.driver.state(), "waiting"); assert.equal(f.tasks.size, 0);
  f.demand(true); f.driver.wake(); await f.turn(); await done;
  assert.equal(f.events.filter(([kind]) => kind === "prepare").length, 1);
});

test("cancellation removes queued work and cannot restart or finish the source", async () => {
  const f = fixture(); let after = false;
  const done = f.driver.run(async () => { await f.driver.awaitSegment("one"); after = true; });
  const rejected = assert.rejects(done, { name: "AbortError" });
  f.driver.cancel(); await rejected;
  assert.equal(f.tasks.size, 0); assert.equal(after, false); assert.deepEqual(f.events, []);
  f.driver.wake(); assert.equal(f.tasks.size, 0);
  await assert.rejects(f.driver.run(() => {}), /single-use/);
});

test("cancellation while capped preserves adopted output and runs interpreter finally once", async () => {
  const f = fixture(); let cleanup = 0;
  const done = f.driver.run(async () => {
    try { await f.driver.awaitSegment("history"); } finally { cleanup++; }
  });
  const rejected = assert.rejects(done, { name: "AbortError" });
  await f.turn(); f.driver.cancel(); await rejected;
  assert.deepEqual(f.events, [["prepare", "history"]]);
  assert.equal(cleanup, 1); assert.equal(f.driver.state(), "cancelled");
});

test("cancellation retires an in-flight asynchronous bootstrap before it can resume source", async () => {
  const held = deferred(); let published = 0, signal;
  const f = fixture({ prepareSegment: async (_, token) => {
    signal = token; await held.promise; if (!token.aborted) published++;
  } });
  const done = f.driver.run(() => f.driver.awaitSegment("initial"));
  const rejected = assert.rejects(done, { name: "AbortError" });
  await f.turn(); f.driver.cancel(); await rejected;
  assert.equal(signal.aborted, true);
  held.resolve(); await flush(); f.driver.wake();
  assert.equal(published, 0); assert.equal(f.tasks.size, 0); assert.deepEqual(f.events, []);
});

test("preparation failure is propagated unchanged and never retried", async () => {
  const error = new Error("retention rejected"); let calls = 0;
  const f = fixture({ prepareSegment: () => { calls++; throw error; } });
  const done = f.driver.run(() => f.driver.awaitSegment("one"));
  const rejected = assert.rejects(done, e => e === error);
  await f.turn(); await rejected;
  for (let i = 0; i < 10; i++) f.driver.wake();
  assert.equal(calls, 1); assert.equal(f.tasks.size, 0); assert.equal(f.driver.state(), "failed");
});

test("catching a failed barrier in user source cannot turn the owner successful", async () => {
  const error = new Error("prepare failed");
  const f = fixture({ prepareSegment: () => { throw error; } });
  const done = f.driver.run(async () => { try { await f.driver.awaitSegment("one"); } catch {} });
  const rejected = assert.rejects(done, e => e === error);
  await f.turn(); await rejected; assert.deepEqual(f.events, []);
});

test("source exceptions do not publish a terminal batch or retry source", async () => {
  const f = fixture(); const error = new Error("source failed"); let calls = 0;
  await assert.rejects(f.driver.run(() => { calls++; throw error; }), e => e === error);
  f.driver.wake(); assert.equal(calls, 1); assert.deepEqual(f.events, []);
});

test("completion errors and late completion after cancellation retain terminal failure", async () => {
  const error = new Error("terminal rejected");
  const f = fixture({ finishSource: () => { throw error; } });
  await assert.rejects(f.driver.run(() => 7), e => e === error);
  assert.equal(f.driver.state(), "failed");
  const held = deferred(); let signal;
  const g = fixture({ finishSource: (_, token) => { signal = token; return held.promise; } });
  const done = g.driver.run(() => 1), reason = new Error("retired");
  const rejected = assert.rejects(done, e => e === reason);
  await flush(); g.driver.cancel(reason); assert.equal(signal.aborted, true);
  held.resolve(); await rejected; assert.equal(g.driver.state(), "cancelled");
});

test("duplicate awaits fail both pending calls instead of orphaning a suspended stack", async () => {
  const f = fixture();
  const done = f.driver.run(async () => {
    const outcomes = await Promise.allSettled([f.driver.awaitSegment(1), f.driver.awaitSegment(2)]);
    assert.deepEqual(outcomes.map(x => x.status), ["rejected", "rejected"]);
  });
  await assert.rejects(done, /already has a suspended/);
  assert.equal(f.tasks.size, 0); assert.deepEqual(f.events, []);
});

test("unawaited source output is rejected rather than being sealed", async () => {
  const f = fixture(); let stray;
  await assert.rejects(f.driver.run(() => {
    stray = f.driver.awaitSegment("one"); stray.catch(() => {});
  }), /without awaiting/);
  await assert.rejects(stray, /without awaiting/);
  assert.equal(f.tasks.size, 0); assert.deepEqual(f.events, []);
});

test("asynchronous or invalid demand is not silently treated as permission to execute", async () => {
  for (const value of [undefined, "yes", 1, Promise.resolve(true)]) {
    const f = fixture({ needsPreparation: () => value });
    const done = f.driver.run(() => f.driver.awaitSegment("one"));
    const rejected = assert.rejects(done, /synchronously return a boolean/);
    await f.turn(); await rejected; assert.equal(f.tasks.size, 0);
  }
});

test("demand errors during an explicit wake fail the waiting invocation", async () => {
  const error = new Error("renderer retired"); let broken = false;
  const f = fixture({ needsPreparation: () => { if (broken) throw error; return false; } });
  const done = f.driver.run(() => f.driver.awaitSegment("one"));
  const rejected = assert.rejects(done, e => e === error);
  await f.turn(); broken = true; f.driver.wake(); await rejected;
  assert.equal(f.tasks.size, 0);
});

test("reentrant cancellation inside demand cannot overwrite retired state", async () => {
  const reason = new Error("retired by demand observer");
  let driver;
  const f = fixture({ needsPreparation: () => { driver.cancel(reason); return true; } });
  driver = f.driver;
  const done = driver.run(() => driver.awaitSegment(1));
  const rejected = assert.rejects(done, e => e === reason);
  await f.turn(); await rejected;
  assert.equal(driver.state(), "cancelled"); assert.equal(f.tasks.size, 0);
});
