// #1874 diagnostic only. Sample the actual authoring worker after scored trials.
// No Python hooks, heap collection, source rewriting, runtime edits or GC flags.
import assert from "node:assert/strict";

const METHODS = new Set(["Runtime.getHeapUsage", "Runtime.getIsolateId", "Profiler.enable",
  "Profiler.setSamplingInterval", "Profiler.start", "Profiler.stop", "Profiler.disable"]);

// CDP's nested session transport keeps the inspector independent of Playwright's
// own worker attachment. Match both browser context and the worker's exact URL.
export function selectAuthoringTarget(targets, browserContextId, workerUrl) {
  assert.ok(typeof browserContextId === "string" && browserContextId.length > 0,
    "missing owning browser context");
  const matches = targets.filter(target => target.type === "worker" &&
    target.browserContextId === browserContextId && target.url === workerUrl);
  assert.equal(matches.length, 1, "expected exactly one authoring worker in its owning context");
  assert.ok(typeof matches[0].targetId === "string" && matches[0].targetId.length > 0);
  return matches[0];
}

export function workerInspectorSession(cdp, sessionId, timeoutMs = 30000) {
  assert.ok(typeof sessionId === "string" && sessionId.length > 0, "missing inspector session");
  assert.ok(Number.isFinite(timeoutMs) && timeoutMs > 0, "invalid inspector timeout");
  let nextId = 0, closed = false;
  const pending = new Map();
  function fail(error) {
    for (const { reject, timer } of pending.values()) { clearTimeout(timer); reject(error); }
    pending.clear();
  }
  const receive = event => {
    if (event.sessionId !== sessionId || closed) return;
    let message;
    try { message = JSON.parse(event.message); }
    catch { fail(new Error("malformed authoring worker inspector response")); return; }
    if (message === null || typeof message !== "object" || Array.isArray(message)) {
      fail(new Error("malformed authoring worker inspector response")); return;
    }
    if (!Object.hasOwn(message, "id")) return; // unsolicited protocol events
    const request = pending.get(message.id);
    if (!request) return; // timed-out or unrelated command; never relabel a reply
    pending.delete(message.id); clearTimeout(request.timer);
    if (message.error) request.reject(new Error(`worker inspector: ${message.error.message}`));
    else request.resolve(message.result);
  };
  const detached = event => {
    if (event.sessionId !== sessionId) return;
    closed = true; fail(new Error("authoring worker inspector detached"));
  };
  cdp.on("Target.receivedMessageFromTarget", receive);
  cdp.on("Target.detachedFromTarget", detached);
  return {
    send(method, params = {}) {
      assert.ok(METHODS.has(method), "worker diagnostic permits only sampling and usage observations");
      assert.ok(!closed, "worker inspector is closed");
      const id = ++nextId;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(id); reject(new Error(`worker inspector timeout: ${method}`));
        }, timeoutMs);
        pending.set(id, { resolve, reject, timer });
        // Catch synchronous adapter failures as well as a rejected send promise.
        Promise.resolve().then(() => cdp.send("Target.sendMessageToTarget", {
          sessionId, message: JSON.stringify({ id, method, params }),
        })).catch(error => {
          if (!pending.delete(id)) return;
          clearTimeout(timer); reject(error);
        });
      });
    },
    async close() {
      cdp.off("Target.receivedMessageFromTarget", receive);
      cdp.off("Target.detachedFromTarget", detached);
      fail(new Error("authoring worker inspector closed"));
      if (closed) return;
      closed = true;
      await cdp.send("Target.detachFromTarget", { sessionId });
    },
  };
}

export async function openAuthoringInspector(browser, page) {
  const workers = page.workers().filter(worker =>
    new URL(worker.url()).pathname.endsWith("/python-worker.js"));
  assert.equal(workers.length, 1, "missing or ambiguous production authoring worker");
  const pageSession = await page.context().newCDPSession(page);
  let browserContextId;
  try { ({ targetInfo: { browserContextId } } = await pageSession.send("Target.getTargetInfo")); }
  finally { await pageSession.detach(); }
  const cdp = await browser.newBrowserCDPSession();
  try {
    const { targetInfos } = await cdp.send("Target.getTargets");
    const target = selectAuthoringTarget(targetInfos, browserContextId, workers[0].url());
    const { sessionId } = await cdp.send("Target.attachToTarget", { targetId: target.targetId, flatten: false });
    const session = workerInspectorSession(cdp, sessionId);
    return { target, send: (...args) => session.send(...args),
      async close() { try { await session.close(); } finally { await cdp.detach(); } } };
  } catch (error) { await cdp.detach(); throw error; }
}

export function validateWorkerProfile(profile) {
  assert.ok(Number.isFinite(profile?.startTime) && Number.isFinite(profile.endTime) &&
    profile.endTime > profile.startTime, "invalid worker profile interval");
  assert.ok(Array.isArray(profile.nodes) && profile.nodes.length > 0, "missing worker call tree");
  const ids = new Set(profile.nodes.map(node => node.id));
  assert.equal(ids.size, profile.nodes.length, "duplicate worker call-tree node");
  assert.ok(Array.isArray(profile.samples) && profile.samples.length > 0 &&
    profile.samples.every(id => ids.has(id)), "missing or unknown worker samples");
  assert.ok(Array.isArray(profile.timeDeltas) && profile.timeDeltas.length === profile.samples.length &&
    profile.timeDeltas.every(value => Number.isFinite(value) && value >= 0), "invalid worker sample durations");
  return profile;
}

export async function collectWorkerProfile({ open, run, record }) {
  const evidence = { schema: 1, diagnosticOnly: true, scope: "whole_source_worker_cpu",
    samplingIntervalUs: 1000, complete: false, cleanupErrors: [] };
  let session, enabled = false, started = false, failure;
  try {
    session = await open(); evidence.target = session.target;
    evidence.isolate = await session.send("Runtime.getIsolateId");
    evidence.heapBefore = await session.send("Runtime.getHeapUsage");
    await session.send("Profiler.enable"); enabled = true;
    await session.send("Profiler.setSamplingInterval", { interval: evidence.samplingIntervalUs });
    await record(evidence);
    await session.send("Profiler.start"); started = true;
    evidence.observation = await run();
  } catch (error) { failure = error; evidence.error = String(error); }
  finally {
    // Retain available samples even when the measured source failed. A cleanup
    // failure cannot mask that original exception or silently indicate success.
    for (const [needed, operation] of [
      [started, async () => { const { profile } = await session.send("Profiler.stop");
        evidence.profile = profile; validateWorkerProfile(profile); }],
      [enabled, () => session.send("Profiler.disable")],
      [!!session, async () => { evidence.heapAfter = await session.send("Runtime.getHeapUsage"); }],
      [!!session, () => session.close()],
    ]) {
      if (!needed) continue;
      try { await operation(); }
      catch (error) { evidence.cleanupErrors.push(String(error)); failure ??= error; }
    }
  }
  evidence.complete = !failure;
  try { await record(evidence); } catch (error) { throw failure ?? error; }
  if (failure) throw failure;
  return evidence;
}
