import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const executionClient = await readFile(
  new URL("./execution-worker-client.js", import.meta.url),
  "utf8",
);
const authoringExecutionClient = await readFile(
  new URL("./authoring-execution-client.js", import.meta.url),
  "utf8",
);
const main = await readFile(new URL("./main.js", import.meta.url), "utf8");
const pagesWorkflow = await readFile(
  new URL("../.github/workflows/pages.yml", import.meta.url),
  "utf8",
);

const hostErrorBranch = executionClient.match(
  /if \(message\.type === "host_callback_error"\) \{([\s\S]*?)\n\s*\}/,
)?.[1];

assert.ok(hostErrorBranch, "execution client must handle host callback errors explicitly");
assert.match(
  hostErrorBranch,
  /#notifyRecoverableError\(/,
  "host callback errors must use the recoverable scene-error boundary",
);
assert.doesNotMatch(
  hostErrorBranch,
  /#notifyError\(/,
  "host callback errors must never enter the fatal worker-error boundary",
);
assert.match(
  executionClient,
  /constructor\(canvas, \{ onError = null, onRecoverableError = null \} = \{\}\)/,
  "execution client must expose distinct fatal and recoverable error callbacks",
);
assert.match(
  authoringExecutionClient,
  /constructor\(canvas, \{ onError = null, onRecoverableError = null \} = \{\}\)/,
  "authoring execution must preserve the recoverable error callback boundary",
);
assert.equal(
  (authoringExecutionClient.match(/onRecoverableError: this\.#onRecoverableError/g) ?? []).length,
  1,
  "authoring execution must configure recoverable errors once on its persistent execution owner",
);
assert.doesNotMatch(
  authoringExecutionClient,
  /RetainedExecutionWorkerClient/,
  "authoring mode transitions must not construct a second retained execution client",
);
assert.match(
  authoringExecutionClient,
  /player\.switchToRetained\(/,
  "legacy to retained authoring must switch the persistent execution owner in place",
);
assert.match(
  authoringExecutionClient,
  /player\.rebuildRetained\(/,
  "retained authoring edits must rebuild on the persistent execution owner",
);
assert.match(
  authoringExecutionClient,
  /player\.switchToLegacy\(/,
  "retained to legacy authoring must switch the persistent execution owner in place",
);

assert.match(
  main,
  /let canvas = document\.querySelector\("#scene"\);/,
  "playground must be able to adopt a replacement DOM canvas after prepared-owner rollback",
);
const createRuntimeStart = main.indexOf("function createRuntimeClient()");
const preparationStart = main.indexOf("function ensureRuntimePreparation()", createRuntimeStart);
const runtimeReadyStart = main.indexOf("async function ensureRuntimeReady(", preparationStart);
assert.ok(
  createRuntimeStart >= 0 && preparationStart > createRuntimeStart && runtimeReadyStart > preparationStart,
  "playground must separate client construction, mode-free preparation, and authored startup",
);
const createRuntimeBody = main.slice(createRuntimeStart, preparationStart);
const preparationBody = main.slice(preparationStart, runtimeReadyStart);
const playgroundConstruction = createRuntimeBody.match(
  /candidate = new AuthoringExecutionClient\(canvas, \{([\s\S]*?)\n\s*\}\);/,
)?.[1];
assert.ok(playgroundConstruction, "deferred runtime must configure its authoring execution client once");
const fatalHandler = playgroundConstruction.match(
  /onError\(error\) \{([\s\S]*?)\n\s*\}/,
)?.[1];
assert.match(
  fatalHandler ?? "",
  /if \(player !== candidate\) return;/,
  "fatal callbacks from an unpublished preparation must not mark a nonexistent player for restart",
);
assert.match(
  fatalHandler ?? "",
  /playerNeedsRestart = true;/,
  "fatal callbacks from the published player must still request worker restart",
);
assert.match(
  playgroundConstruction,
  /onRecoverableError\(error\) \{\s*showRecoverableSceneError\(error\);\s*\}/,
  "recoverable execution errors must be presented as scene errors",
);
const recoverableHandler = playgroundConstruction.match(
  /onRecoverableError\(error\) \{([\s\S]*?)\n\s*\}/,
)?.[1];
assert.doesNotMatch(
  recoverableHandler ?? "",
  /playerNeedsRestart\s*=\s*true/,
  "recoverable scene errors must not request worker restart",
);
assert.match(
  preparationBody,
  /if \(runtimePreparation !== null\) return runtimePreparation;/,
  "a prepared unpublished owner must survive Python/stale retries instead of spawning duplicate owners",
);
assert.match(
  preparationBody,
  /adoptRuntimeCanvas\(candidate\);[\s\S]*runtimePreparation = null;/,
  "failed render preparation must adopt the fresh replacement canvas before allowing retry",
);
assert.doesNotMatch(
  preparationBody,
  /showError\(error\)/,
  "an unpublished preparation failure must not race Python authoring to publish a fatal UI state",
);
assert.match(
  main,
  /function showRecoverableSceneError\(error\) \{\s*console\.warn\([\s\S]*?patchStatus\.dataset\.state = "error";/,
  "recoverable callback errors must be visible without becoming fatal console errors",
);
assert.match(
  pagesWorkflow,
  /concurrency:\s*\n\s*group: pages\s*\n\s*cancel-in-progress: true/,
  "superseded playground deployments must be cancelled",
);

console.log(
  "✓ overlapped playground preparation preserves unpublished ownership, canvas recovery, and recoverable scene-error boundaries",
);
