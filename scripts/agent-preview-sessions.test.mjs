import assert from "node:assert/strict";
import test from "node:test";

import { AgentPreviewSessionRegistry } from "./agent-preview-sessions.mjs";

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function fixture(t, overrides = {}, limits = {}) {
  const created = [];
  const registry = new AgentPreviewSessionRegistry({
    createSession: () => {
      const session = {
        state: "new",
        closed: false,
        openCalls: [],
        sampleCalls: [],
        closeCalls: [],
        snapshotValue: { state: "new" },
        async open(source, options) {
          this.openCalls.push([source, options]);
          this.state = "ready";
          this.snapshotValue = { state: "ready", frame: { publishedTime: 0 } };
          return this.snapshotValue;
        },
        async sample(time) {
          this.sampleCalls.push(time);
          this.snapshotValue = { state: "ready", frame: { publishedTime: time } };
          return this.snapshotValue;
        },
        get snapshot() { return this.snapshotValue; },
        async close(reason) {
          this.closeCalls.push(reason);
          this.closed = true;
          this.state = "closed";
          this.snapshotValue = { state: "closed" };
          await overrides.onClose?.(this, reason);
          return this.snapshotValue;
        },
        ...overrides.session,
      };
      created.push(session);
      return session;
    },
    ...limits,
  });
  t.after(async () => { await registry.dispose().catch(() => {}); });
  return { registry, created };
}

test("open publishes one opaque session only after first-frame readiness", async (t) => {
  const gate = deferred();
  const { registry, created } = fixture(t, { session: {
    async open(source, options) {
      this.openCalls.push([source, options]);
      await gate.promise;
      this.snapshotValue = { state: "ready", sourceState: "running", authoredDuration: null,
        frame: { requestedTime: 0, publishedTime: 0 } };
      return this.snapshotValue;
    },
  } });
  const scope = registry.openScope();
  const opening = registry.open(scope, "scene", { loopDurationSeconds: 4 });
  assert.deepEqual(registry.counts, { scopes: 1, sessions: 1 });
  gate.resolve();
  const opened = await opening;
  assert.match(opened.sessionId, /^[0-9a-f-]{36}$/);
  assert.equal(opened.snapshot.sourceState, "running");
  assert.equal(opened.snapshot.authoredDuration, null);
  assert.deepEqual(created[0].openCalls, [["scene", { loopDurationSeconds: 4 }]]);
});

test("known IDs are rejected across transport scopes and immediately after close starts", async (t) => {
  const gate = deferred();
  const { registry } = fixture(t, { onClose: () => gate.promise });
  const scopeA = registry.openScope();
  const scopeB = registry.openScope();
  const { sessionId } = await registry.open(scopeA, "scene");
  assert.throws(() => registry.inspect(scopeB, sessionId), /cross-scope|stale/);
  const closing = registry.close(scopeA, sessionId);
  assert.throws(() => registry.inspect(scopeA, sessionId), /cross-scope|stale/);
  assert.deepEqual(registry.counts, { scopes: 2, sessions: 0 });
  gate.resolve();
  assert.equal((await closing).state, "closed");
});

test("concurrent conflicting samples are rejected before duplicate engine work", async (t) => {
  const gate = deferred();
  const entered = deferred();
  const { registry, created } = fixture(t, { session: {
    async sample(time) {
      this.sampleCalls.push(time);
      entered.resolve();
      await gate.promise;
      this.snapshotValue = { state: "ready", frame: { publishedTime: time } };
      return this.snapshotValue;
    },
  } });
  const scope = registry.openScope();
  const { sessionId } = await registry.open(scope, "scene");
  const first = registry.sample(scope, sessionId, 1);
  await entered.promise;
  await assert.rejects(registry.sample(scope, sessionId, 2), /already in progress/);
  assert.deepEqual(created[0].sampleCalls, [1]);
  gate.resolve();
  assert.equal((await first).frame.publishedTime, 1);
  assert.equal((await registry.sample(scope, sessionId, 2)).frame.publishedTime, 2);
});

test("operation cancellation waits for asynchronous cleanup before rejecting", async (t) => {
  const sampleGate = deferred();
  const entered = deferred();
  const closeGate = deferred();
  const { registry, created } = fixture(t, { session: {
    async sample(time) {
      this.sampleCalls.push(time);
      entered.resolve();
      await sampleGate.promise;
      return { state: "ready", frame: { publishedTime: time } };
    },
    async close(reason) {
      this.closeCalls.push(reason);
      await closeGate.promise;
      return { state: "closed" };
    },
  } });
  const scope = registry.openScope();
  const { sessionId } = await registry.open(scope, "scene");
  const controller = new AbortController();
  const sampling = registry.sample(scope, sessionId, 1, { signal: controller.signal });
  await entered.promise;
  controller.abort("client canceled");
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(created[0].closeCalls, ["client canceled"]);
  assert.throws(() => registry.inspect(scope, sessionId), /stale|cross-scope/);
  let settled = false;
  sampling.finally(() => { settled = true; }).catch(() => {});
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(settled, false, "canceled operation must not settle before cleanup");
  closeGate.resolve();
  await assert.rejects(sampling, /client canceled/);
  sampleGate.resolve();
});

test("transport disconnect rejects open ownership immediately but awaits cleanup completion", async (t) => {
  const openGate = deferred();
  const entered = deferred();
  const closeGate = deferred();
  const { registry, created } = fixture(t, { session: {
    async open() {
      entered.resolve();
      await openGate.promise;
      return { state: "ready" };
    },
    async close(reason) {
      this.closeCalls.push(reason);
      await closeGate.promise;
      return { state: "closed" };
    },
  } });
  const scope = registry.openScope();
  const opening = registry.open(scope, "scene");
  await entered.promise;
  const closingScope = registry.closeScope(scope, "transport disconnected");
  assert.deepEqual(registry.counts, { scopes: 0, sessions: 0 });
  assert.throws(() => registry.open(scope, "again"), /stale preview scope/);
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(created[0].closeCalls, ["transport disconnected"]);
  let scopeSettled = false;
  closingScope.finally(() => { scopeSettled = true; }).catch(() => {});
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(scopeSettled, false);
  closeGate.resolve();
  assert.equal((await closingScope)[0].state, "closed");
  await assert.rejects(opening, /transport disconnected/);
  openGate.resolve();
});

test("pre-aborted requests never create a session", async (t) => {
  const { registry, created } = fixture(t);
  const scope = registry.openScope();
  const controller = new AbortController();
  controller.abort(new Error("already canceled"));
  await assert.rejects(registry.open(scope, "scene", { signal: controller.signal }), /already canceled/);
  assert.equal(created.length, 0);
  assert.deepEqual(registry.counts, { scopes: 1, sessions: 0 });
});

test("failed open awaits cleanup, releases quota, and a fresh open can recover", async (t) => {
  let fail = true;
  const { registry, created } = fixture(t, { session: {
    async open() {
      if (fail) { fail = false; throw new Error("source failed"); }
      this.snapshotValue = { state: "ready" };
      return this.snapshotValue;
    },
  } }, { maxSessions: 1, maxSessionsPerScope: 1 });
  const scope = registry.openScope();
  await assert.rejects(registry.open(scope, "bad"), /source failed/);
  assert.deepEqual(created[0].closeCalls, ["source failed"]);
  assert.deepEqual(registry.counts, { scopes: 1, sessions: 0 });
  assert.match((await registry.open(scope, "good")).sessionId, /^[0-9a-f-]{36}$/);
});

test("scope and session quotas reject atomically without evicting live ownership", async (t) => {
  const { registry, created } = fixture(t, {}, { maxScopes: 2, maxSessions: 2, maxSessionsPerScope: 1 });
  const scopeA = registry.openScope();
  const scopeB = registry.openScope();
  assert.throws(() => registry.openScope(), /scope limit/);
  const a = await registry.open(scopeA, "a");
  const b = await registry.open(scopeB, "b");
  await assert.rejects(registry.open(scopeA, "extra"), /session limit/);
  assert.equal(created.length, 2);
  assert.equal(registry.inspect(scopeA, a.sessionId).state, "ready");
  assert.equal(registry.inspect(scopeB, b.sessionId).state, "ready");
});

test("cleanup failures are surfaced after atomic ownership release", async (t) => {
  const { registry } = fixture(t, { session: {
    async close() { throw new Error("container cleanup failed"); },
  } });
  const scope = registry.openScope();
  const { sessionId } = await registry.open(scope, "scene");
  await assert.rejects(registry.close(scope, sessionId), /container cleanup failed/);
  assert.deepEqual(registry.counts, { scopes: 1, sessions: 0 });
  assert.throws(() => registry.inspect(scope, sessionId), /stale|cross-scope/);
});

test("invalid factories, signals, and close reasons do not corrupt registry state", async (t) => {
  const invalid = new AgentPreviewSessionRegistry({ createSession: () => ({}) });
  t.after(async () => { await invalid.dispose().catch(() => {}); });
  const invalidScope = invalid.openScope();
  await assert.rejects(invalid.open(invalidScope, "scene"), /invalid session/);
  assert.deepEqual(invalid.counts, { scopes: 1, sessions: 0 });

  const { registry } = fixture(t);
  const scope = registry.openScope();
  await assert.rejects(registry.open(scope, "scene", { signal: {} }), /AbortSignal/);
  const { sessionId } = await registry.open(scope, "scene");
  await assert.rejects(registry.close(scope, sessionId, ""), /reason/);
  assert.equal(registry.inspect(scope, sessionId).state, "ready");
  await assert.rejects(registry.closeScope(scope, ""), /reason/);
  assert.equal(registry.inspect(scope, sessionId).state, "ready");
});

test("registry shutdown awaits every cleanup and rejects future operations immediately", async (t) => {
  const closeGate = deferred();
  const { registry, created } = fixture(t, { onClose: () => closeGate.promise });
  const scopeA = registry.openScope();
  const scopeB = registry.openScope();
  await registry.open(scopeA, "a");
  await registry.open(scopeB, "b");
  const disposing = registry.dispose("server shutdown");
  assert.deepEqual(registry.counts, { scopes: 0, sessions: 0 });
  assert.throws(() => registry.openScope(), /disposed/);
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(created.map((session) => session.closeCalls), [["server shutdown"], ["server shutdown"]]);
  let settled = false;
  disposing.finally(() => { settled = true; }).catch(() => {});
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(settled, false);
  closeGate.resolve();
  assert.equal((await disposing).length, 2);
  assert.equal(await registry.dispose("server shutdown"), await disposing);
});
