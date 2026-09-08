import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const mainSource = await readFile(new URL("./main.js", import.meta.url), "utf8");
const runtimeStart = mainSource.indexOf("async function ensureRuntimeReady(");
const runtimeEnd = mainSource.indexOf("\nasync function ensureExecutionReady()", runtimeStart);
assert.ok(runtimeStart >= 0 && runtimeEnd > runtimeStart, "playground runtime startup boundary must exist");
const runtimeReady = mainSource.slice(runtimeStart, runtimeEnd);
assert.match(
  runtimeReady,
  /nextPlayer\.startSemanticExecution\(semanticExecution, \{\s*authoringClient: client,/u,
  "cold startup must attach the authoring client to the shared callback execution session",
);
assert.doesNotMatch(
  runtimeReady,
  /configureHostCallbacks|reconcileScene|startRetainedCanonical|sceneJson/u,
  "cold startup must not reconstruct callbacks through a legacy scene or a second reconciliation",
);
console.log("✓ cold-start callbacks attach through the shared semantic session");
