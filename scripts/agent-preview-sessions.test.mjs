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
        close(reason) {
          this.closeCalls.push(reason);
          this.closed = true;
          this.state = "closed";
          this.snapshotValue = { state: "closed" };
          overrides.onClose?.(this, reason);
          return this.snapshotValue;
        },
        ...overrides.session,
      };
      created.push(session);
      return session;
    },
    ...limits,
  });
  t.after(() => registry.dispose());
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

test("known IDs are still rejected across transport scopes and after close", async (t) => {
  const { registry } = fixture(t);
  const scopeA = registry.openScope();
  const scopeB = registry.openScope();
  const { sessionId } = await registry.open(scopeA, "scene");
  assert.throws(() => registry.inspect(scopeB, sessionId), /cross-scope|stale/);
  registry.close(scopeA, sessionId);
  assert.throws(() => registry.inspect(scopeA, sessionId), /cross-scope|stale/);
  assert.deepEqual(registry.counts, { scopes: 2, sessions: 0 });
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

test("operation cancellation closes ownership and makes the session ID stale", async (t) => {
  const gate = deferred();
  const entered = deferred();
  const { registry, created } = fixture(t, { session: {
    async sample(time) {
      this.sampleCalls.push(time);
      entered.resolve();
      await gate.promise;
      return { state: "ready", frame: { publishedTime: time } };
    },
  } });
  const scope = registry.openScope();
  const { sessionId } = await registry.open(scope, "scene");
  const controller = new AbortController();
  const sampling = registry.sample(scope, sessionId, 1, { signal: controller.signal });
  await entered.promise;
  controller.abort("client canceled");
  await assert.rejects(sampling, /client canceled/);
  assert.deepEqual(created[0].closeCalls, ["client canceled"]);
  assert.throws(() => registry.inspect(scope, sessionId), /stale|cross-scope/);
  gate.resolve();
});

test("transport disconnect rejects a non-cooperative open immediately and cleans accounting", async (t) => {
  const gate = deferred();
  const entered = deferred();
  const { registry, created } = fixture(t, { session: {
    async open() {
      entered.resolve();
      await gate.promise;
      return { state: "ready" };
    },
  } });
  const scope = registry.openScope();
  const opening = registry.open(scope, "scene");
  await entered.promise;
  registry.closeScope(scope, "transport disconnected");
  await assert.rejects(opening, /transport disconnected/);
  assert.deepEqual(created[0].closeCalls, ["transport disconnected"]);
  assert.deepEqual(registry.counts, { scopes: 0, sessions: 0 });
  gate.resolve();
  assert.throws(() => registry.open(scope, "again"), /stale preview scope/);
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

test("failed open releases quota and a fresh open can recover", async (t) => {
  let fail = true;
  const { registry } = fixture(t, { session: {
    async open() {
      if (fail) { fail = false; throw new Error("source failed"); }
      this.snapshotValue = { state: "ready" };
      return this.snapshotValue;
    },
  } }, { maxSessions: 1, maxSessionsPerScope: 1 });
  const scope = registry.openScope();
  await assert.rejects(registry.open(scope, "bad"), /source failed/);
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

test("invalid factories, signals, and close reasons do not corrupt registry state", async (t) => {
  const invalid = new AgentPreviewSessionRegistry({ createSession: () => ({}) });
  t.after(() => invalid.dispose());
  const invalidScope = invalid.openScope();
  await assert.rejects(invalid.open(invalidScope, "scene"), /invalid session/);
  assert.deepEqual(invalid.counts, { scopes: 1, sessions: 0 });

  const { registry } = fixture(t);
  const scope = registry.openScope();
  await assert.rejects(registry.open(scope, "scene", { signal: {} }), /AbortSignal/);
  const { sessionId } = await registry.open(scope, "scene");
  assert.throws(() => registry.close(scope, sessionId, ""), /reason/);
  assert.equal(registry.inspect(scope, sessionId).state, "ready");
  assert.throws(() => registry.closeScope(scope, ""), /reason/);
  assert.equal(registry.inspect(scope, sessionId).state, "ready");
});

test("registry shutdown closes every session and rejects future operations", async (t) => {
  const { registry, created } = fixture(t);
  const scopeA = registry.openScope();
  const scopeB = registry.openScope();
  await registry.open(scopeA, "a");
  await registry.open(scopeB, "b");
  registry.dispose("server shutdown");
  assert.deepEqual(created.map((session) => session.closeCalls), [["server shutdown"], ["server shutdown"]]);
  assert.deepEqual(registry.counts, { scopes: 0, sessions: 0 });
  assert.throws(() => registry.openScope(), /disposed/);
  registry.dispose("server shutdown");
});
