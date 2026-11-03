import assert from "node:assert/strict";
import test from "node:test";
import { AuthoringWorkerGeneration } from "./authoring-worker-generation.js";

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

function client(interpreterGeneration, libraryTrust = "pristine", readiness = Promise.resolve()) {
  let terminated = false;
  return {
    interpreterGeneration,
    get libraryTrust() { return libraryTrust; },
    setTrust(value) { libraryTrust = value; },
    get terminated() { return terminated; },
    ready: () => readiness,
    terminate() { terminated = true; },
  };
}

function fixture() {
  let currentToken = null;
  let nextGeneration = 0;
  const clients = [];
  const owner = new AuthoringWorkerGeneration({
    createClient() {
      const created = client(++nextGeneration);
      clients.push(created);
      return created;
    },
    isCurrentRun: token => token === currentToken,
  });
  return {
    owner, clients,
    token(value) { currentToken = value; return value; },
  };
}

test("a pristine interpreter is reused across runs", async () => {
  const f = fixture();
  const first = await f.owner.clientForRun(f.token("run-1"));
  const second = await f.owner.clientForRun(f.token("run-2"));
  assert.equal(second, first);
  assert.equal(f.clients.length, 1);
  assert.equal(first.libraryTrust, "pristine");
  f.owner.dispose();
});

test("taint after an opaque failure still requires a pristine replacement", async () => {
  const f = fixture();
  const old = await f.owner.clientForRun(f.token("run-1"));
  old.setTrust("tainted"); // Worker-reported trust survives source failure/cancellation.
  const replacement = await f.owner.clientForRun(f.token("run-2"));
  assert.notEqual(replacement, old);
  assert.notEqual(replacement.interpreterGeneration, old.interpreterGeneration);
  assert.equal(replacement.libraryTrust, "pristine");
  assert.equal(old.terminated, true);
  assert.equal(f.owner.activeClient, replacement);
  f.owner.dispose();
});

test("candidate initialization failure preserves the old active owner", async () => {
  let currentToken = "run-1";
  const failure = new Error("worker bootstrap failed");
  const old = client(1);
  const broken = client(2, "unknown", Promise.reject(failure));
  const clients = [old, broken];
  const owner = new AuthoringWorkerGeneration({
    createClient: () => clients.shift(),
    isCurrentRun: token => token === currentToken,
  });
  assert.equal(await owner.clientForRun(currentToken), old);
  old.setTrust("tainted");
  currentToken = "run-2";
  await assert.rejects(owner.clientForRun(currentToken), /worker bootstrap failed/);
  assert.equal(owner.activeClient, old);
  assert.equal(old.terminated, false);
  assert.equal(broken.terminated, true);
  owner.dispose();
});

test("unknown candidate trust is not accepted as pristine", async () => {
  let currentToken = "run-1";
  const old = client(1);
  const unverified = client(2, "unknown");
  let created = 0;
  const owner = new AuthoringWorkerGeneration({
    createClient: () => (++created === 1 ? old : unverified),
    isCurrentRun: token => token === currentToken,
  });
  await owner.clientForRun(currentToken);
  old.setTrust("tainted");
  currentToken = "run-2";
  await assert.rejects(owner.clientForRun(currentToken), /did not confirm a pristine interpreter/);
  assert.equal(owner.activeClient, old);
  assert.equal(unverified.terminated, true);
  owner.dispose();
});

test("the newest run can adopt a shared candidate; stale callers cannot promote it", async () => {
  let currentToken = "old";
  const gate = deferred();
  const old = client(1, "pristine");
  const candidate = client(2, "pristine", gate.promise);
  let created = 0;
  const owner = new AuthoringWorkerGeneration({
    createClient: () => (++created === 1 ? old : candidate),
    isCurrentRun: token => token === currentToken,
  });
  assert.equal(await owner.clientForRun(currentToken), old);
  old.setTrust("tainted");
  const stale = owner.clientForRun("run-a");
  currentToken = "run-b";
  const latest = owner.clientForRun(currentToken);
  gate.resolve();
  assert.equal(await stale, null);
  assert.equal(await latest, candidate);
  assert.equal(owner.activeClient, candidate);
  assert.equal(old.terminated, true);
  assert.equal(owner.retire(old), false, "late retirement cannot clear a newer owner");
  owner.dispose();
});

test("a late stale call cannot discard the candidate awaited by the newest run", async () => {
  let currentToken = "initial";
  const gate = deferred();
  const old = client(1);
  const candidate = client(2, "pristine", gate.promise);
  let created = 0;
  const owner = new AuthoringWorkerGeneration({
    createClient: () => (++created === 1 ? old : candidate),
    isCurrentRun: token => token === currentToken,
  });
  await owner.clientForRun("initial");
  old.setTrust("tainted");
  currentToken = "run-a";
  const earlyA = owner.clientForRun("run-a");
  currentToken = "run-b";
  const latestB = owner.clientForRun("run-b");
  assert.equal(await owner.clientForRun("run-a"), null, "late stale call must not become latest owner");
  gate.resolve();
  assert.equal(await earlyA, null);
  assert.equal(await latestB, candidate);
  assert.equal(owner.activeClient, candidate);
  assert.equal(candidate.terminated, false);
  owner.dispose();
});

test("explicit observed policy may reuse tainted owner without restoring trust", async () => {
  const f = fixture();
  const old = await f.owner.clientForRun(f.token("run-1"));
  old.setTrust("tainted");
  assert.equal(await f.owner.clientForRun(f.token("run-2"), { requirePristine: false }), old);
  assert.equal(old.libraryTrust, "tainted");
  assert.equal(f.clients.length, 1);
  f.owner.dispose();
});

test("a terminated pristine owner is replaced instead of being reused", async () => {
  const f = fixture();
  const dead = await f.owner.clientForRun(f.token("run-1"));
  dead.terminate();
  const live = await f.owner.clientForRun(f.token("run-2"));
  assert.notEqual(live, dead);
  assert.equal(live.terminated, false);
  assert.notEqual(live.interpreterGeneration, dead.interpreterGeneration);
  assert.equal(f.owner.activeClient, live);
  f.owner.dispose();
});

test("dispose retires the active owner and its single pending candidate", async () => {
  let currentToken = "run-1";
  const old = client(1);
  const gate = deferred();
  const candidate = client(2, "pristine", gate.promise);
  let created = 0;
  const owner = new AuthoringWorkerGeneration({
    createClient: () => (++created === 1 ? old : candidate),
    isCurrentRun: token => token === currentToken,
  });
  assert.equal(await owner.clientForRun(currentToken), old);
  old.setTrust("tainted");
  const pending = owner.clientForRun(currentToken);
  await Promise.resolve();
  assert.equal(owner.retire(old), true);
  owner.dispose();
  gate.resolve();
  assert.equal(await pending, null);
  assert.equal(old.terminated, true);
  assert.equal(candidate.terminated, true);
});
