import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const runSceneStart = main.indexOf("async function runScene()");
const selectExampleStart = main.indexOf("async function selectExample(", runSceneStart);
assert.ok(runSceneStart >= 0 && selectExampleStart > runSceneStart, "runScene boundary must exist");
const runSceneBody = main.slice(runSceneStart, selectExampleStart);

const runtimeTaskStart = runSceneBody.indexOf("const runtimeTask = (async () => {");
const runtimeReady = runSceneBody.indexOf("await ensureRuntimeReady();", runtimeTaskStart);
const executionReady = runSceneBody.indexOf("await ensureExecutionReady();", runtimeReady);
const runtimeTaskEnd = runSceneBody.indexOf("})();", executionReady);
const authoringTaskStart = runSceneBody.indexOf("const authoringTask = client", runtimeTaskEnd);
const authoringRun = runSceneBody.indexOf(".run(source,", authoringTaskStart);
const joinBarrier = runSceneBody.indexOf("await Promise.allSettled([", authoringRun);

assert.ok(runtimeTaskStart >= 0, "Run must create an execution-startup task");
assert.ok(runtimeReady > runtimeTaskStart, "execution-startup task must start the deferred runtime");
assert.ok(
  executionReady > runtimeReady && executionReady < runtimeTaskEnd,
  "execution recovery must remain sequenced after runtime startup inside the execution task",
);
assert.ok(
  authoringTaskStart > runtimeTaskEnd && authoringRun > authoringTaskStart,
  "Run must start source authoring as a sibling task after execution startup has been kicked off",
);
assert.ok(
  joinBarrier > authoringRun,
  "Run must not wait for execution startup before submitting source authoring",
);

const joinBody = runSceneBody.slice(joinBarrier, runSceneBody.indexOf("]);", joinBarrier) + 3);
assert.match(joinBody, /runtimeTask,\s*authoringTask,/, "reconciliation barrier must join both startup tasks");
assert.doesNotMatch(
  runSceneBody.slice(runtimeTaskEnd, authoringTaskStart),
  /await\s+(?:runtimeTask|ensureRuntimeReady\(\)|ensureExecutionReady\(\))/,
  "source authoring must not be serialized behind execution readiness",
);
assert.ok(
  runSceneBody.indexOf("player.reconcileScene(", joinBarrier) > joinBarrier,
  "scene reconciliation must remain after the joined startup barrier",
);

console.log("✓ playground overlaps Python source authoring with cold execution startup");
