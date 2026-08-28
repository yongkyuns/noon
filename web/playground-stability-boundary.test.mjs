import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const executionClient = await readFile(
  new URL("./execution-worker-client.js", import.meta.url),
  "utf8",
);
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
  pagesWorkflow,
  /concurrency:\s*\n\s*group: pages\s*\n\s*cancel-in-progress: true/,
  "superseded playground deployments must be cancelled",
);

console.log("✓ playground keeps recoverable scene errors and deployments inside stable boundaries");
