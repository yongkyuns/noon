import assert from "node:assert/strict";
import test from "node:test";

import { LatestSourceRunner } from "./latest-source-runner.js";

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function createRunHarness() {
  let active = null;
  const starts = [];

  const run = () => {
    if (active !== null) return active.promise;
    const gate = deferred();
    active = gate;
    starts.push(gate);
    return gate.promise.finally(() => {
      if (active === gate) active = null;
    });
  };

  return {
    run,
    runInFlight: () => active !== null,
    starts,
  };
}

async function flushMicrotasks() {
  for (let index = 0; index < 6; index += 1) {
    await Promise.resolve();
  }
}

test("edits arriving during a live run collapse to one latest-source rerun", async () => {
  const harness = createRunHarness();
  const runner = new LatestSourceRunner({
    run: harness.run,
    runInFlight: harness.runInFlight,
    currentExampleId: () => "example-a",
    delayMs: 0,
  });

  const drain = runner.request("example-a", { immediate: true });
  assert.equal(harness.starts.length, 1);

  runner.request("example-a", { immediate: true });
  runner.request("example-a", { immediate: true });
  assert.equal(harness.starts.length, 1, "in-flight edits must not start parallel Python runs");

  harness.starts[0].resolve();
  await flushMicrotasks();
  assert.equal(
    harness.starts.length,
    2,
    "all edits received during the first run must collapse to one fresh rerun",
  );

  harness.starts[1].resolve();
  await drain;
  assert.deepEqual(runner.diagnostics, {
    requestedVersion: 3,
    completedVersion: 3,
    requestedExampleId: "example-a",
    pending: false,
    draining: false,
    disposed: false,
  });
});

test("an edit that joins an older explicit Run is rerun after that Run completes", async () => {
  const harness = createRunHarness();
  const runner = new LatestSourceRunner({
    run: harness.run,
    runInFlight: harness.runInFlight,
    currentExampleId: () => "example-a",
    delayMs: 0,
  });

  const explicitRun = harness.run();
  assert.equal(harness.starts.length, 1);

  const drain = runner.request("example-a", { immediate: true });
  assert.equal(
    harness.starts.length,
    1,
    "the runner must join the existing Run instead of starting a concurrent authoring request",
  );

  harness.starts[0].resolve();
  await explicitRun;
  await flushMicrotasks();
  assert.equal(
    harness.starts.length,
    2,
    "joining an older Run must not falsely mark the newer editor source as rendered",
  );

  harness.starts[1].resolve();
  await drain;
  assert.equal(runner.diagnostics.completedVersion, runner.diagnostics.requestedVersion);
});

test("queued edits from a previous example are dropped after selection changes", async () => {
  const harness = createRunHarness();
  let currentExampleId = "example-a";
  let timerCallback = null;
  const runner = new LatestSourceRunner({
    run: harness.run,
    runInFlight: harness.runInFlight,
    currentExampleId: () => currentExampleId,
    delayMs: 180,
    setTimer(callback) {
      timerCallback = callback;
      return 1;
    },
    clearTimer() {
      timerCallback = null;
    },
  });

  runner.request("example-a");
  assert.equal(typeof timerCallback, "function");
  currentExampleId = "example-b";
  await runner.flush();

  assert.equal(harness.starts.length, 0);
  assert.equal(runner.diagnostics.pending, false);
});

test("dispose cancels a pending debounce without creating execution work", () => {
  const harness = createRunHarness();
  let cleared = false;
  const runner = new LatestSourceRunner({
    run: harness.run,
    runInFlight: harness.runInFlight,
    currentExampleId: () => "example-a",
    setTimer() {
      return 7;
    },
    clearTimer(timer) {
      assert.equal(timer, 7);
      cleared = true;
    },
  });

  runner.request("example-a");
  runner.dispose();

  assert.equal(cleared, true);
  assert.equal(runner.diagnostics.disposed, true);
  assert.equal(harness.starts.length, 0);
});
