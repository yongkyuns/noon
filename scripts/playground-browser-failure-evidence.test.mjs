import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { captureAttemptedPage, createBrowserFailureTrace, createNativeWebKitEvidence, parsePlaywrightWebKitProcesses } from "./playground-browser-failure-evidence.mjs";

test("records browser death independently from page evaluation and with exact phase", () => {
  let time = 2000;
  const trace = createBrowserFailureTrace({ now: () => ++time });
  const browser = new EventEmitter();
  const context = new EventEmitter();
  const page = new EventEmitter();
  const main = { url: () => "http://localhost/showcase" };
  page.mainFrame = () => main;
  trace.attach(browser, context, page);
  trace.setPhase("showcase-loading", { exampleId: "showcase-dynamic-scene" });
  page.emit("framenavigated", { url: () => "http://localhost/iframe" });
  page.emit("framenavigated", main);
  page.emit("crash");
  page.emit("close");
  context.emit("close");
  browser.emit("disconnected");
  assert.deepEqual(trace.snapshot().events.map(x => x.kind), [
    "phase", "main-frame-navigation", "page-crash", "page-close", "context-close", "browser-disconnected",
  ]);
  assert.ok(trace.snapshot().events.every(x => x.phase === "showcase-loading"));
  assert.ok(trace.snapshot().events.every(x => Number.isSafeInteger(x.atMs)));
});

test("events are bounded, and snapshots cannot mutate the retained trace", () => {
  const trace = createBrowserFailureTrace({ maxEvents: 3, now: () => 42 });
  for (let i = 0; i < 30; i++) trace.record("event", { i });
  const snapshot = trace.snapshot();
  assert.deepEqual(snapshot.events.map(x => x.i), [27, 28, 29]);
  snapshot.events[0].i = -1;
  assert.equal(trace.snapshot().events[0].i, 27);
});

test("invalid trace configuration and phase are rejected", () => {
  for (const maxEvents of [-1, 0, 129, 1.5]) assert.throws(() => createBrowserFailureTrace({ maxEvents }));
  const trace = createBrowserFailureTrace();
  assert.throws(() => trace.setPhase(""), TypeError);
});

test("captures attempted scene state rather than reporting last successful source", async () => {
  const data = { patchState: "running", selectedExampleId: "showcase-dynamic-scene", runDisabled: true };
  const result = await captureAttemptedPage({ evaluate: async () => data });
  assert.deepEqual(result, { state: "captured", observed: data });
});

test("captures bounded playback commands without querying an unavailable engine", async () => {
  const controls = {
    dataset: { playing: "true", busy: "false", elapsedSeconds: "11.17" },
    querySelector: () => ({ getAttribute: () => "Pause animation", disabled: false, textContent: "Pause" }),
  };
  const result = await captureAttemptedPage({
    evaluate: async callback => runInNewContext(`(${callback.toString()})()`, {
      location: { href: "http://localhost/current-scene" },
      document: { querySelector: selector => selector === ".playback-controls" ? controls : null },
      window: {
        __matrixPlaybackTrace: { events: Array.from({ length: 40 }, (_, id) => ({ id, type: "pause" })) },
        __noonExampleGallery: { executionMetrics: () => { throw new Error("engine unavailable"); } },
      },
    }),
  });
  assert.equal(result.state, "captured");
  assert.equal(result.observed.playbackControls.data.playing, "true");
  assert.equal(result.observed.playbackControls.toggle.ariaLabel, "Pause animation");
  assert.deepEqual(Array.from(result.observed.playbackTrace, event => event.id),
    Array.from({ length: 32 }, (_, index) => index + 8));
});

test("closed page stays unavailable even if an earlier scene was successful", async () => {
  const result = await captureAttemptedPage({ evaluate: async () => { throw new Error("page closed"); }, isClosed: () => true });
  assert.equal(result.state, "unavailable");
  assert.equal(result.pageClosed, true);
  assert.match(result.reason, /page closed/);
  assert.equal(Object.hasOwn(result, "observed"), false);
});

test("absent page has no fabricated state", async () => {
  assert.deepEqual(await captureAttemptedPage(null), { state: "unavailable", reason: "page not created" });
});

test("native PID selection is bounded to the exact installed WebKit directory", () => {
  const sample = [
    "101 /opt/pw/webkit-123/pw_run.sh",
    "102 /opt/pw/webkit-123/WebKit.Networking",
    "103 /opt/pw/webkit-1234/WebKit.WebContent",
    "104 /usr/libexec/WebKit.WebContent",
    "not-a-pid /opt/pw/webkit-123/WebKit.GPU",
    "0 /opt/pw/webkit-123/WebKit.GPU",
    "102 /opt/pw/webkit-123/WebKit.Networking",
    "105 /opt/pw/webkit-123/WebKit.WebContent",
  ].join("\n");
  assert.deepEqual(parsePlaywrightWebKitProcesses(sample, "/opt/pw/webkit-123/pw_run.sh").map(p => p.pid), [101, 102, 105]);
});

test("rejects incomplete or relative installation identities", () => {
  assert.throws(() => parsePlaywrightWebKitProcesses("", "pw_run.sh"), TypeError);
  assert.throws(() => parsePlaywrightWebKitProcesses(null, "/opt/pw/bin"), TypeError);
});


test("native evidence observes only installed PIDs before a process disappears", async () => {
  let calls = 0;
  let requested = null;
  const evidence = createNativeWebKitEvidence({ enabled: true, executablePath: "/pw/webkit-x/pw_run.sh",
    artifacts: "/diagnostics", startedAtMs: 1000, now: () => 2000,
    readProcessTable: async () => (++calls === 1)
      ? "222 /pw/webkit-x/WebKit.WebContent\n333 /other/WebKit.GPU"
      : "", collect: async args => { requested = args; return { reports: [], errors: [] }; },
  });
  await evidence.observe("before-showcase");
  assert.deepEqual(await evidence.failure(), { reports: [], errors: [] });
  assert.deepEqual(requested.pids, [222]);
  assert.deepEqual([requested.startedAtMs, requested.endedAtMs], [1000, 2000]);
  assert.deepEqual(evidence.snapshots().map(x => x.stage), ["before-showcase", "after-failure"]);
});

test("native reporting failures never replace original test failure", async () => {
  const evidence = createNativeWebKitEvidence({ enabled: true, executablePath: "/pw/w/bin", artifacts: "/x",
    readProcessTable: async () => { throw new Error("ps denied"); },
    collect: async () => { throw new Error("IPS unavailable"); },
  });
  const result = await evidence.failure();
  assert.match(evidence.snapshots()[0].error, /ps denied/);
  assert.match(result.errors[0].message, /IPS unavailable/);
});

test("non-WebKit platforms do not sample native processes", async () => {
  const evidence = createNativeWebKitEvidence({ enabled: false, readProcessTable: async () => {
    assert.fail("should not inspect process table");
  } });
  await evidence.observe("start");
  assert.equal(await evidence.failure(), null);
  assert.deepEqual(evidence.snapshots(), []);
});
