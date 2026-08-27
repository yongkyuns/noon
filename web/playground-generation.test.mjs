import assert from "node:assert/strict";
import test from "node:test";

import { PlaygroundGeneration } from "./playground-generation.js";

test("newest selection request wins when source loads complete out of order", () => {
  const generations = new PlaygroundGeneration();
  const first = generations.beginSelectionRequest("first");
  const second = generations.beginSelectionRequest("second");

  assert.equal(generations.commitSelection(first), null);
  const committed = generations.commitSelection(second);
  assert.ok(committed);
  assert.equal(committed.exampleId, "second");
  assert.equal(generations.isSelectionCurrent(committed), true);
});

test("loading a newer selection preserves the current run until that selection commits", () => {
  const generations = new PlaygroundGeneration();
  generations.commitSelection(generations.beginSelectionRequest("first"));
  const currentRun = generations.beginRun("first");

  const loadingSecond = generations.beginSelectionRequest("second");
  assert.equal(
    generations.isRunCurrent(currentRun, "first"),
    true,
    "a slow or failed source load must not cancel the current valid scene",
  );

  const committedSecond = generations.commitSelection(loadingSecond);
  assert.ok(committedSecond);
  assert.equal(
    generations.isRunCurrent(currentRun, "first"),
    false,
    "the loaded selection invalidates prior authoring at commit time",
  );
});

test("committing a new selection invalidates authoring from the previous selection", () => {
  const generations = new PlaygroundGeneration();
  const firstSelection = generations.commitSelection(
    generations.beginSelectionRequest("first"),
  );
  const firstRun = generations.beginRun("first");
  assert.equal(generations.isRunCurrent(firstRun, "first"), true);

  const secondSelection = generations.commitSelection(
    generations.beginSelectionRequest("second"),
  );
  assert.ok(secondSelection);
  assert.equal(generations.isRunCurrent(firstRun, "first"), false);
});

test("a newer run supersedes an older run for the same example", () => {
  const generations = new PlaygroundGeneration();
  generations.commitSelection(generations.beginSelectionRequest("scene"));
  const first = generations.beginRun("scene");
  const second = generations.beginRun("scene");

  assert.equal(generations.isRunCurrent(first, "scene"), false);
  assert.equal(generations.isRunCurrent(second, "scene"), true);
});

test("stale-result diagnostics are monotonic and retain the last trace", () => {
  const generations = new PlaygroundGeneration();
  generations.commitSelection(generations.beginSelectionRequest("scene"));
  const stale = generations.beginRun("scene");
  generations.beginRun("scene");

  const diagnostics = generations.recordStale(stale, "after-authoring");
  assert.equal(diagnostics.staleDrops, 1);
  assert.deepEqual(diagnostics.lastStale, {
    kind: "run",
    exampleId: "scene",
    selectionGeneration: 1,
    runGeneration: 2,
    requestGeneration: null,
    stage: "after-authoring",
  });
});

test("seeded rapid selection/run churn never revives a superseded token", () => {
  const generations = new PlaygroundGeneration();
  let seed = 0x6e6f6f6e;
  const random = () => {
    seed ^= seed << 13;
    seed ^= seed >>> 17;
    seed ^= seed << 5;
    return seed >>> 0;
  };

  let activeExample = "scene-0";
  generations.commitSelection(generations.beginSelectionRequest(activeExample));
  let latestRun = generations.beginRun(activeExample);
  const staleRuns = [];

  for (let index = 0; index < 500; index += 1) {
    const operation = random() % 3;
    if (operation === 0) {
      const request = generations.beginSelectionRequest(`scene-${index + 1}`);
      const committed = generations.commitSelection(request);
      assert.ok(committed);
      staleRuns.push(latestRun);
      activeExample = committed.exampleId;
      latestRun = generations.beginRun(activeExample);
    } else {
      staleRuns.push(latestRun);
      latestRun = generations.beginRun(activeExample);
    }

    assert.equal(generations.isRunCurrent(latestRun, activeExample), true);
    const sample = staleRuns[random() % staleRuns.length];
    if (sample) {
      assert.equal(generations.isRunCurrent(sample, activeExample), false);
    }
  }
});
