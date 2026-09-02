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
  /player\.switchToRetainedCanonical\(/,
  "legacy to retained authoring must switch the persistent execution owner in place using canonical SceneSpec",
);
assert.match(
  authoringExecutionClient,
  /player\.rebuildRetainedCanonical\(/,
  "retained authoring edits must rebuild on the persistent execution owner using canonical SceneSpec",
);
assert.match(
  authoringExecutionClient,
  /player\.switchToLegacy\(/,
  "retained to legacy authoring must switch the persistent execution owner in place",
);

const runtimeReadyStart = main.indexOf("async function ensureRuntimeReady(");
const runtimeReadyEnd = main.indexOf("async function ensureExecutionReady()", runtimeReadyStart);
assert.ok(
  runtimeReadyStart >= 0 && runtimeReadyEnd > runtimeReadyStart,
  "playground must keep an explicit on-demand runtime boundary",
);
const runtimeReadyBody = main.slice(runtimeReadyStart, runtimeReadyEnd);
const playgroundConstruction = runtimeReadyBody.match(
  /const nextPlayer = new AuthoringExecutionClient\(canvas, \{([\s\S]*?)\n\s*\}\);/,
)?.[1];
assert.ok(playgroundConstruction, "deferred runtime must configure its authoring execution client");
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
  main,
  /function showRecoverableSceneError\(error\) \{\s*console\.warn\([\s\S]*?patchStatus\.dataset\.state = "error";/,
  "recoverable callback errors must be visible without becoming fatal console errors",
);
assert.match(
  pagesWorkflow,
  /concurrency:\s*\n\s*group: pages[\s\S]*?cancel-in-progress: false/,
  "playground deployments must queue instead of starving an in-flight production release",
);
assert.match(
  pagesWorkflow,
  /name: Stamp deployed revision[\s\S]*?GITHUB_SHA[\s\S]*?web\/build-info\.json/,
  "the Pages artifact must carry the exact source revision",
);
assert.match(
  pagesWorkflow,
  /name: Verify deployed revision[\s\S]*?EXPECTED_SHA: \$\{\{ github\.sha \}\}[\s\S]*?build-info\.json/,
  "the deployment must verify the public Pages site serves the expected revision",
);

console.log("✓ deferred playground runtime keeps recoverable scene errors outside fatal recovery");
