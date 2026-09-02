import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(
  new URL("./authoring-render-worker.js", import.meta.url),
  "utf8",
);
const executionClientSource = readFileSync(
  new URL("./authoring-execution-client.js", import.meta.url),
  "utf8",
);

function functionSlice(name, nextName) {
  const start = source.indexOf(`function ${name}`);
  assert.notEqual(start, -1, `missing ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  assert.notEqual(end, -1, `missing ${nextName}`);
  return source.slice(start, end);
}

function executionMethodSlice(name, nextName) {
  const start = executionClientSource.indexOf(`async #${name}`);
  assert.notEqual(start, -1, `missing AuthoringExecutionClient.#${name}`);
  const end = executionClientSource.indexOf(`async #${nextName}`, start + 1);
  assert.notEqual(end, -1, `missing AuthoringExecutionClient.#${nextName}`);
  return executionClientSource.slice(start, end);
}

test("renderer transition keeps the active renderer until replacement bootstrap arrives", () => {
  const begin = functionSlice("beginRendererTransition", "resize");
  assert.match(begin, /transitionMode = nextMode;/);
  assert.doesNotMatch(begin, /disposeRenderer\(\)/);
  assert.doesNotMatch(begin, /mode = nextMode;/);
  assert.doesNotMatch(begin, /needsPresent = false;/);

  const consume = functionSlice("consumeDelta", "commitRendererTransition");
  assert.match(
    consume,
    /if \(transitionMode !== null\) \{\s*return commitRendererTransition\(json\);/s,
  );

  const commit = functionSlice("commitRendererTransition", "bootstrapRenderer");
  const disposeIndex = commit.indexOf("disposeRenderer();");
  const publishModeIndex = commit.indexOf("mode = nextMode;");
  const bootstrapIndex = commit.indexOf("bootstrapPromise = bootstrapRenderer(initial);");
  assert.ok(disposeIndex >= 0, "transition commit must retire the previous renderer");
  assert.ok(
    publishModeIndex > disposeIndex,
    "next mode must not publish before the previous renderer is retired",
  );
  assert.ok(
    bootstrapIndex > publishModeIndex,
    "replacement renderer bootstrap must start only after commit state is published",
  );
});

test("retained transition resources stage separately from the active renderer", () => {
  const resources = functionSlice("handleRetainedResources", "drainTransport");
  assert.match(resources, /if \(transitionMode !== null\)/);
  assert.match(resources, /transitionMode !== MODE_RETAINED/);
  assert.match(resources, /transitionResourceBytes = message\.bytes;/);

  const commit = functionSlice("commitRendererTransition", "bootstrapRenderer");
  assert.match(
    commit,
    /nextMode === MODE_RETAINED && transitionResourceBytes === null/,
  );
  assert.match(commit, /const nextResourceBytes = transitionResourceBytes;/);
  assert.match(commit, /resourceBytes = nextResourceBytes;/);
});

test("transition state is observable without changing the presented mode", () => {
  const metrics = functionSlice("currentMetrics", "disposeRenderer");
  assert.match(metrics, /mode,/);
  assert.match(metrics, /transitionMode,/);
});

test("authoring mode switches keep the same canvas-owning execution client", () => {
  const retained = executionMethodSlice(
    "switchRetainedCanonical",
    "rebuildRetainedCanonical",
  );
  const legacy = executionMethodSlice("switchLegacy", "runTransition");

  for (const [mode, method] of [
    ["retained", retained],
    ["legacy", legacy],
  ]) {
    assert.match(method, /const player = this\.#player;/);
    assert.doesNotMatch(
      method,
      /new ExecutionWorkerClient|cloneNode|replaceWith|replaceChild|transferControlToOffscreen/,
      `${mode} transition must reuse the existing canvas-owning execution client`,
    );
    assert.doesNotMatch(
      method,
      /this\.#canvas\s*=/,
      `${mode} transition must not replace the HTML canvas`,
    );
  }

  assert.match(retained, /player\.switchToRetainedCanonical\(/);
  assert.match(legacy, /player\.switchToLegacy\(/);
});
