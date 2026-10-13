import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { PythonAuthoringClient, AUTHORING_CHANNEL, AUTHORING_PROTOCOL_VERSION } from "./authoring-client.js";
import { authoringDiagnosticPayload } from "./authoring-diagnostic-runtime.js";

const python = spawnSync("python3", ["-c", `
import json
from _noon_diagnostics import capture_exception
try:
    exec(compile("missing\\n", "noon-request-0.py", "exec"), {})
except BaseException as error:
    print(json.dumps(capture_exception(error, source="missing\\n", filename="noon-request-0.py",
                                      revision="request-0", phase="construct")))
`], { encoding: "utf8", timeout: 15000, env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1",
  PYTHONPATH: fileURLToPath(new URL("./python", import.meta.url)) } });
assert.equal(python.status, 0, python.stderr);
const fixture = JSON.parse(python.stdout);

class FakeWorker {
  handlers = new Map();
  requests = [];
  stopped = 0;
  addEventListener(name, handler) { this.handlers.set(name, handler); }
  postMessage(message) { this.requests.push(message); }
  terminate() { this.stopped++; }
  send(payload) {
    this.handlers.get("message")({ data: {
      channel: AUTHORING_CHANNEL, protocolVersion: AUTHORING_PROTOCOL_VERSION, ...payload,
    } });
  }
}
function make() {
  const worker = new FakeWorker();
  const client = new PythonAuthoringClient(worker);
  worker.send({ type: "ready" });
  return { worker, client };
}
async function nextRun(client, worker) {
  const result = client.run("pass");
  // Attach a rejection handler before simulating delivery.
  const outcome = result.then(value => ({ value }), error => ({ error }));
  await Promise.resolve();
  return { outcome, requestId: worker.requests.at(-1).requestId };
}
function fail(worker, requestId, snapshot = fixture) {
  worker.send({ type: "error", requestId, message: "Python original message",
    diagnostic: { stack: "ORIGINAL PYTHON TRACEBACK" }, authoringDiagnostic: snapshot });
}
function succeed(worker, requestId) {
  worker.send({ type: "result", requestId, resultJson: JSON.stringify({
    kind: "semantic_scene", duration: 0,
    semantic_execution: { context_id: "context-1" },
  }) });
}

test("real client preserves Python facts plus the old raw traceback channel", async () => {
  const { worker, client } = make();
  const { outcome, requestId } = await nextRun(client, worker);
  const transport = authoringDiagnosticPayload({ authoringDiagnostic: fixture });
  worker.send({ type: "error", requestId, message: "Python original message",
    diagnostic: { stack: "ORIGINAL PYTHON TRACEBACK" }, ...transport });
  const { error } = await outcome;
  assert.deepEqual(error.authoringDiagnostic, fixture);
  assert.match(error.message, /ORIGINAL PYTHON TRACEBACK/);
  assert.equal(client.terminated, false);
  client.terminate();
});

test("source failure followed by a successful run keeps the same healthy client", async () => {
  const { worker, client } = make();
  const first = await nextRun(client, worker);
  fail(worker, first.requestId);
  assert.ok((await first.outcome).error.authoringDiagnostic);
  const second = await nextRun(client, worker);
  succeed(worker, second.requestId);
  assert.equal((await second.outcome).value.kind, "semantic_scene");
  assert.equal(client.terminated, false);
  assert.equal(client.diagnostics.pendingRequests, 0);
  assert.equal(worker.stopped, 0);
  client.terminate();
});

test("a malformed diagnostic remains a source failure, not a fatal protocol error", async () => {
  const { worker, client } = make();
  const first = await nextRun(client, worker);
  fail(worker, first.requestId, { version: 9 });
  const { error } = await first.outcome;
  assert.equal(error.authoringDiagnostic, undefined);
  assert.match(error.message, /ORIGINAL PYTHON TRACEBACK/);
  assert.equal(client.terminated, false);
  const second = await nextRun(client, worker);
  succeed(worker, second.requestId);
  assert.ok((await second.outcome).value);
  client.terminate();
});

test("late error for a settled request does not fail the newer run", async () => {
  const { worker, client } = make();
  const first = await nextRun(client, worker);
  succeed(worker, first.requestId);
  await first.outcome;
  const second = await nextRun(client, worker);
  fail(worker, first.requestId);
  assert.equal(client.diagnostics.staleResponses, 1);
  assert.equal(client.terminated, false);
  succeed(worker, second.requestId);
  assert.ok((await second.outcome).value);
  client.terminate();
});

test("a fatal worker error remains fatal even when it includes source facts", async () => {
  const { worker, client } = make();
  const first = await nextRun(client, worker);
  fail(worker, null);
  assert.ok((await first.outcome).error);
  assert.equal(client.terminated, true);
  assert.equal(worker.stopped, 1);
});

test("missing structured metadata preserves the existing error behavior", async () => {
  const { worker, client } = make();
  const first = await nextRun(client, worker);
  worker.send({ type: "error", requestId: first.requestId, message: "raw failure" });
  const { error } = await first.outcome;
  assert.equal(error.message, "raw failure");
  assert.equal(error.authoringDiagnostic, undefined);
  assert.equal(client.terminated, false);
  client.terminate();
});
