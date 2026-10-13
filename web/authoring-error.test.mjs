import assert from "node:assert/strict";
import test from "node:test";
import { attachAuthoringDiagnostic, authoringErrorFromMessage } from "./authoring-error.js";
import { PythonAuthoringClient, AUTHORING_CHANNEL, AUTHORING_PROTOCOL_VERSION } from "./authoring-client.js";

export function diagnostic(document = { filename: "scene.py", revision: "run-1" }) {
  return { version: 1, phase: "construct", document, truncated: false,
    exception: { type: "NameError", module: "builtins", message: "name 'missing' is not defined",
      location: { line: 1, column: 0, endLine: 1, endColumn: 7 }, frames: [],
      engine: null, notes: [], cause: null, causeRelation: null, children: [] } };
}
const envelope = (type, payload = {}) => ({ channel: AUTHORING_CHANNEL, protocolVersion: AUTHORING_PROTOCOL_VERSION, type, ...payload });
class Worker {
  listeners = new Map(); messages = []; terminated = false;
  addEventListener(name, listener) { this.listeners.set(name, listener); }
  postMessage(message) { this.messages.push(message); }
  terminate() { this.terminated = true; }
  emit(message) { this.listeners.get("message")({ data: message }); }
}

test("invocation-local failure attaches facts without changing the original error", () => {
  const error = new Error("raw PythonError");
  assert.equal(attachAuthoringDiagnostic(error, { get: () => JSON.stringify(diagnostic()) }), error);
  assert.deepEqual(error.authoringDiagnostic, diagnostic());
});
for (const value of [null, "{", JSON.stringify({ version: 50 })]) {
  test(`malformed capture preserves original failure: ${value}`, () => {
    const error = new Error("original");
    assert.equal(attachAuthoringDiagnostic(error, { get: () => value }), error);
    assert.equal(error.authoringDiagnostic, undefined);
  });
}
test("frozen failures and broken Python proxies keep original identity", () => {
  const frozen = Object.freeze(new Error("trap"));
  assert.equal(attachAuthoringDiagnostic(frozen, { get: () => JSON.stringify(diagnostic()) }), frozen);
  const error = new Error("original");
  assert.equal(attachAuthoringDiagnostic(error, { get() { throw new Error("broken proxy"); } }), error);
});
test("request identity must match before exposing a source link", () => {
  const message = { message: "original", diagnostic: { stack: "raw stack" }, authoringDiagnostic: diagnostic() };
  for (const request of [null, { source: "missing", sourceDocument: { filename: "other.py", revision: "run-1" } },
    { source: "missing", sourceDocument: { filename: "scene.py", revision: "run-2" } }]) {
    const error = authoringErrorFromMessage(message, request);
    assert.equal(error.authoringDiagnostic, undefined);
    assert.match(error.message, /original\nraw stack/);
  }
});
test("real client preserves structured error then accepts corrected source on same worker", async () => {
  const worker = new Worker();
  const client = new PythonAuthoringClient(worker);
  worker.emit(envelope("ready"));
  const run = client.run("missing");
  const rejected = assert.rejects(run, (error) => {
    assert.deepEqual(error.authoringDiagnostic, diagnostic(worker.messages[0].sourceDocument));
    assert.equal(error.authoringSource, "missing");
    assert.match(error.message, /raw stack/);
    return true;
  });
  await Promise.resolve();
  const request = worker.messages[0];
  worker.emit(envelope("error", { requestId: request.requestId, message: "NameError",
    diagnostic: { stack: "raw stack" }, authoringDiagnostic: diagnostic(request.sourceDocument) }));
  await rejected;
  assert.equal(client.terminated, false);
  const corrected = client.run("result = scene");
  await Promise.resolve();
  const next = worker.messages[1];
  assert.notEqual(next.sourceDocument.revision, request.sourceDocument.revision);
  worker.emit(envelope("result", { requestId: next.requestId, resultJson: JSON.stringify({
    kind: "semantic_scene", semantic_execution: { context_id: "semantic-1" }, duration: 0,
  }) }));
  assert.equal((await corrected).kind, "semantic_scene");
  assert.equal(client.diagnostics.pendingRequests, 0);
  worker.emit(envelope("error", { requestId: request.requestId, message: "late", authoringDiagnostic: diagnostic(request.sourceDocument) }));
  assert.equal(client.diagnostics.staleResponses, 1);
  client.terminate();
});
test("new clients never reuse diagnostic request identity", async () => {
  const documents = [];
  for (let i = 0; i < 2; i++) {
    const worker = new Worker(); const client = new PythonAuthoringClient(worker);
    worker.emit(envelope("ready"));
    const failure = assert.rejects(client.run("missing"));
    await Promise.resolve();
    documents.push(worker.messages[0].sourceDocument);
    worker.emit(envelope("error", { requestId: 0, message: "failure" }));
    await failure; client.terminate();
  }
  assert.notEqual(documents[0].revision, documents[1].revision);
});
