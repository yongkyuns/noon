import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const mainSource = await readFile(new URL("./main.js", import.meta.url), "utf8");
const executionSource = await readFile(
  new URL("./authoring-execution-client.js", import.meta.url),
  "utf8",
);

const runtimeStart = mainSource.indexOf("async function ensureRuntimeReady(");
const runtimeEnd = mainSource.indexOf("\nasync function ensureExecutionReady()", runtimeStart);
assert.ok(runtimeStart >= 0 && runtimeEnd > runtimeStart, "playground runtime startup boundary must exist");
const runtimeReady = mainSource.slice(runtimeStart, runtimeEnd);

assert.match(
  runtimeReady,
  /nextPlayer\.start\(sceneJson, \{[\s\S]*callbacks,[\s\S]*authoringClient: client,/u,
  "legacy cold start must attach host callbacks through the initial startup",
);
assert.doesNotMatch(
  runtimeReady,
  /nextPlayer\.reconcileScene/u,
  "legacy cold start must not reconcile the just-started scene only to attach callbacks",
);

const startMethod = executionSource.indexOf("  async start(");
const retainedStart = executionSource.indexOf("\n  async startRetainedCanonical(", startMethod);
assert.ok(startMethod >= 0 && retainedStart > startMethod, "legacy authoring startup method must exist");
const legacyStart = executionSource.slice(startMethod, retainedStart);
assert.match(legacyStart, /callbacks = null,/u);
assert.match(legacyStart, /authoringClient = null,/u);
assert.match(
  legacyStart,
  /configureHostCallbacks\(callbacks, authoringClient\)/u,
  "legacy startup must reuse the shared host-callback configuration path",
);

console.log("✓ cold-start host callbacks attach without duplicate scene reconciliation");
