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
assert.ok(
  (authoringExecutionClient.match(/onRecoverableError: this\.#onRecoverableError/g) ?? []).length >= 2,
  "legacy authoring execution must forward recoverable errors on initial start and rebuild",
);

const playgroundConstruction = main.match(
  /player = new AuthoringExecutionClient\(canvas, \{([\s\S]*?)\n\s*\}\);/,
)?.[1];
assert.ok(playgroundConstruction, "playground must configure its authoring execution client");
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
  /concurrency:\s*\n\s*group: pages\s*\n\s*cancel-in-progress: true/,
  "superseded playground deployments must be cancelled",
);

console.log("✓ playground keeps recoverable scene errors visible and outside fatal recovery");
