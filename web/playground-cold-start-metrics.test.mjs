import assert from "node:assert/strict";
import { test } from "node:test";
import {
  classifyWorkerUrl,
  coldStartMilestones,
  summarizeWorkers,
} from "./playground-cold-start-metrics.js";

test("coldStartMilestones reports phase-local durations", () => {
  assert.deepEqual(
    coldStartMilestones({
      navigationStart: 100,
      pageReady: 160,
      runRequested: 200,
      runtimeStarted: 470,
      firstMetrics: 610,
    }),
    {
      pageReadyMs: 60,
      runToRuntimeMs: 270,
      runToFirstMetricsMs: 410,
      runtimeToFirstMetricsMs: 140,
    },
  );
});

test("coldStartMilestones rejects invalid or non-monotonic timestamps", () => {
  assert.throws(
    () =>
      coldStartMilestones({
        navigationStart: 0,
        pageReady: 1,
        runRequested: 2,
        runtimeStarted: 1,
        firstMetrics: 3,
      }),
    /precedes/,
  );
  assert.throws(
    () =>
      coldStartMilestones({
        navigationStart: 0,
        pageReady: 1,
        runRequested: 2,
        runtimeStarted: 3,
        firstMetrics: Number.NaN,
      }),
    /finite non-negative/,
  );
});

test("worker classification covers current authoring, engine, and render entry points", () => {
  assert.equal(classifyWorkerUrl("http://localhost/web/python-worker.js"), "authoring");
  assert.equal(classifyWorkerUrl("http://localhost/web/execution-engine-worker.js"), "engine");
  assert.equal(classifyWorkerUrl("http://localhost/web/retained-execution-engine-worker.js"), "engine");
  assert.equal(classifyWorkerUrl("http://localhost/web/execution-render-worker.js"), "render");
  assert.equal(classifyWorkerUrl("http://localhost/web/retained-execution-render-worker.js"), "render");
  assert.equal(classifyWorkerUrl("http://localhost/web/authoring-render-worker.js"), "render");
  assert.equal(classifyWorkerUrl("http://localhost/web/editor-worker.js"), "other");
});

test("summarizeWorkers preserves event order and counts worker roles", () => {
  const summary = summarizeWorkers([
    { url: "python-worker.js", atMs: 5 },
    { url: "execution-engine-worker.js", atMs: 20 },
    { url: "execution-render-worker.js", atMs: 21 },
    { url: "editor-worker.js", atMs: 30 },
  ]);
  assert.equal(summary.total, 4);
  assert.deepEqual(summary.byRole, { authoring: 1, engine: 1, render: 1, other: 1 });
  assert.deepEqual(
    summary.workers.map(({ role }) => role),
    ["authoring", "engine", "render", "other"],
  );
});
