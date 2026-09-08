import assert from "node:assert/strict";
import test from "node:test";
import { AuthoringExecutionClient } from "./authoring-execution-client.js";
import { ExecutionWorkerClient } from "./execution-worker-client.js";

test("authoring execution exposes only shared session attachment", () => {
  for (const method of ["start", "startRetained", "startRetainedCanonical", "reconcileScene", "applyPatchBatch"]) {
    assert.equal(method in AuthoringExecutionClient.prototype, false, method);
  }
  assert.equal(typeof AuthoringExecutionClient.prototype.startSemanticExecution, "function");
  assert.equal(typeof AuthoringExecutionClient.prototype.reconcileSemanticExecution, "function");
});

test("explicit transport diagnostics cannot regain the deleted split retained surface", () => {
  for (const method of ["startRetained", "switchToRetained", "rebuildRetained"]) {
    assert.equal(method in ExecutionWorkerClient.prototype, false, method);
  }
});

test("execution client cannot restore unused migration transition APIs", () => {
  for (const method of ["reconcileScene", "switchToLegacy", "switchToRetainedCanonical", "rebuildRetainedCanonical"]) {
    assert.equal(method in ExecutionWorkerClient.prototype, false, method);
  }
});
