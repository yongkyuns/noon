import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { captureAttemptedPage, captureChildProcessDiagnostics, captureNativeProcessLog, createBrowserFailureTrace, createNativeWebKitEvidence, parsePlaywrightWebKitProcesses } from "./playground-browser-failure-evidence.mjs";
import { collectWebKitCrashReports } from "./playground-browser-support.mjs";

async function nativeArtifactFixture(t) {
  const artifacts = await mkdtemp(path.join(tmpdir(), "noon-native-failure-"));
  t.after(() => rm(artifacts, { recursive: true, force: true }));
  return { artifacts, directories: [], prefix: "failed-case", pids: [222], startedAtMs: 1101, endedAtMs: 2501 };
}

test("missing IPS evidence retains native output for known PIDs and the actual query window", async t => {
  const options = await nativeArtifactFixture(t);
  const bytes = Buffer.from('{"processIdentifier":222,"eventMessage":"process exited"}\n');
  let args;
  const result = await captureNativeProcessLog({ ...options,
    pids: [222, 222, -1, "333 OR TRUE", NaN, 1.2],
    readNativeLog: async request => { args = request; return { stdout: bytes }; },
  });
  assert.deepEqual(result.pids, [222]);
  assert.equal(args[args.indexOf("--predicate") + 1], "processIdentifier == 222");
  assert.equal(args[args.indexOf("--start") + 1], "@1");
  assert.equal(args[args.indexOf("--end") + 1], "@3");
  assert.deepEqual([result.startedAtMs, result.endedAtMs], [1101, 2501]);
  assert.deepEqual([result.queriedStartAtMs, result.queriedEndAtMs], [1000, 3000]);
  assert.deepEqual(await readFile(path.join(options.artifacts, result.file)), bytes);
  assert.equal(result.truncated, false);
});

test("native log collection preserves existing PID-qualified IPS bytes", async t => {
  const options = await nativeArtifactFixture(t);
  const ips = pid => JSON.stringify({ bug_type: "309" }) + "\n" + JSON.stringify({
    pid, captureTime: new Date(1500).toISOString(), termination: { namespace: "SIGNAL", code: 11 },
  });
  const owned = ips(222);
  await writeFile(path.join(options.artifacts, "owned.ips"), owned);
  await writeFile(path.join(options.artifacts, "unrelated.ips"), ips(333));
  const evidence = createNativeWebKitEvidence({ enabled: true, ...options,
    executablePath: "/pw/webkit/bin", now: () => options.endedAtMs,
    readProcessTable: async () => "222 /pw/webkit/WebContent",
    collect: args => collectWebKitCrashReports({ ...args, directories: [options.artifacts] }),
    collectLog: args => captureNativeProcessLog({ ...args,
      readNativeLog: async () => { throw Object.assign(new Error("native logging denied"), {
        stdout: Buffer.from("native termination context\n"),
      }); },
    }),
  });
  await evidence.observe("ready");
  const result = await evidence.failure();
  assert.equal(result.reports.length, 1);
  assert.equal(result.reports[0].pid, 222);
  assert.deepEqual(result.reports[0].termination, { namespace: "SIGNAL", code: 11 });
  assert.equal(await readFile(path.join(options.artifacts, result.reports[0].file), "utf8"), owned);
  assert.equal(await readFile(path.join(options.artifacts, result.processLog.file), "utf8"), "native termination context\n");
  assert.match(result.processLog.error, /native logging denied/);
});

test("native log errors retain bounded partial output and never become crash evidence", async t => {
  const options = await nativeArtifactFixture(t);
  const error = Object.assign(new Error("native log output exceeded capacity"), {
    code: "ERR_CHILD_PROCESS_STDIO_MAXBUFFER", stdout: Buffer.alloc(150_000, 120), stderr: "log collector diagnostic",
  });
  const result = await captureNativeProcessLog({ ...options, readNativeLog: async () => { throw error; } });
  const saved = await readFile(path.join(options.artifacts, result.file));
  assert.equal(saved.length, 128 * 1024);
  assert.ok(saved.equals(error.stdout.subarray(0, saved.length)));
  assert.equal(result.truncated, true);
  assert.match(result.error, /exceeded capacity/);
  assert.equal(result.stderr, "log collector diagnostic");
});

test("unavailable native logging is explicit rather than evidence that no native event occurred", async t => {
  const options = await nativeArtifactFixture(t);
  const result = await captureNativeProcessLog({ ...options, readNativeLog: async () => {
    throw Object.assign(new Error("native logging denied"), { code: "EACCES" });
  } });
  assert.match(result.error, /denied/);
  assert.equal(result.bytes, 0);
  assert.equal(result.truncated, false);
  assert.deepEqual(await readFile(path.join(options.artifacts, result.file)), Buffer.alloc(0));
});

test("native logging has a fixed PID bound and rejects excessive windows before invoking the provider", async t => {
  const options = await nativeArtifactFixture(t);
  let calls = 0;
  const readNativeLog = async () => { calls++; return { stdout: Buffer.alloc(0) }; };
  const bounded = await captureNativeProcessLog({ ...options,
    pids: Array.from({ length: 40 }, (_, i) => i + 1), readNativeLog,
  });
  assert.equal(bounded.pids.length, 32);
  assert.equal(bounded.omittedPidCount, 8);
  const skipped = await captureNativeProcessLog({ ...options, endedAtMs: 200_000, readNativeLog });
  assert.match(skipped.reason, /180 seconds/);
  for (const update of [{ pids: [] }, { startedAtMs: NaN }, { endedAtMs: 0 }]) {
    const invalid = await captureNativeProcessLog({ ...options, ...update, readNativeLog });
    assert.ok(invalid.reason);
  }
  assert.equal(calls, 1);
});

test("file-server diagnostics retain bounded output and exact process failure", () => {
  const child = new EventEmitter();
  child.stdout = new EventEmitter(); child.stderr = new EventEmitter(); child.pid = 42;
  const trace = captureChildProcessDiagnostics(child, { maxOutputChars: 12 });
  child.stdout.emit("data", Buffer.from("old serving log"));
  child.stderr.emit("data", Buffer.from("old GET logs\n"));
  child.stderr.emit("data", Buffer.from("Broken pipe!"));
  child.emit("error", new Error("bind failed"));
  child.exitCode = 1;
  const first = trace.snapshot();
  assert.equal(first.pid, 42);
  assert.equal(first.exitCode, 1);
  assert.equal(first.stderrTail, "Broken pipe!");
  assert.equal(first.stdoutTail.length, 12);
  assert.equal(first.error, "Error: bind failed");
  child.stdout.emit("data", Buffer.from("next"));
  assert.equal(first.stdoutTail.length, 12);
  assert.notEqual(trace.snapshot().stdoutTail, first.stdoutTail);
});

test("file-server diagnostics observe a real exiting child without restarting it", async () => {
  const child = spawn(process.execPath, ["-e",
    "process.stdout.write('Serving test'); process.stderr.write('WASM GET aborted'); process.exitCode = 7"],
  { stdio: ["ignore", "pipe", "pipe"] });
  const trace = captureChildProcessDiagnostics(child);
  try {
    await new Promise(resolve => child.once("close", resolve));
    const observed = trace.snapshot();
    assert.equal(observed.exitCode, 7);
    assert.equal(observed.stdoutTail, "Serving test");
    assert.equal(observed.stderrTail, "WASM GET aborted");
    assert.equal(observed.error, null);
  } finally {
    trace.dispose();
  }
});

test("file-server diagnostic cleanup preserves unrelated listeners and tolerates absent streams", () => {
  const child = new EventEmitter(); child.stdout = new EventEmitter();
  const unrelated = () => {};
  child.stdout.on("data", unrelated);
  const trace = captureChildProcessDiagnostics(child);
  assert.equal(child.stdout.listenerCount("data"), 2);
  trace.dispose(); trace.dispose();
  assert.deepEqual(child.stdout.listeners("data"), [unrelated]);
  assert.deepEqual(trace.snapshot(), { pid: null, exitCode: null, signalCode: null,
    error: null, stdoutTail: "", stderrTail: "" });
  for (const maxOutputChars of [0, -1, 1.5, 64_001]) {
    assert.throws(() => captureChildProcessDiagnostics(child, { maxOutputChars }), RangeError);
  }
});

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

test("network and worker failures remain bounded and detach only owned listeners", () => {
  const trace = createBrowserFailureTrace({ maxEvents: 3, now: () => 42 });
  const browser = new EventEmitter(), context = new EventEmitter(), page = new EventEmitter();
  const worker = new EventEmitter();
  worker.url = () => "https://example.com/python-worker.js";
  const unrelated = () => {};
  context.on("requestfailed", unrelated);
  worker.on("close", unrelated);
  const detach = trace.attach(browser, context, page);
  page.emit("worker", worker);
  context.emit("response", { status: () => 200, url: () => "ok" });
  assert.equal(trace.snapshot().events.length, 1, "successful responses add no recurring trace work");
  context.emit("requestfailed", { url: () => "x".repeat(900), failure: () => ({ errorText: "e".repeat(900) }) });
  context.emit("response", { status: () => 503, url: () => "https://example.com/runtime.wasm" });
  worker.emit("close");
  assert.deepEqual(worker.listeners("close"), [unrelated], "closed workers are not retained until case cleanup");
  const events = trace.snapshot().events;
  assert.deepEqual(events.map(event => event.kind), ["request-failed", "http-error", "worker-close"]);
  assert.equal(events[0].url.length, 500);
  assert.equal(events[0].error.length, 300);
  assert.equal(events[1].status, 503);
  assert.equal(events[2].url, worker.url());
  detach(); detach();
  assert.deepEqual(context.listeners("requestfailed"), [unrelated]);
  assert.deepEqual(worker.listeners("close"), [unrelated]);
  assert.equal(page.listenerCount("worker"), 0);
  context.emit("requestfailed"); worker.emit("close");
  assert.deepEqual(trace.snapshot().events, events);
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
      : "", collect: async args => { requested = args; return Object.freeze({ reports: [], errors: [] }); },
    collectLog: async args => ({ pids: args.pids }),
  });
  await evidence.observe("before-showcase");
  assert.deepEqual(await evidence.failure(), { reports: [], errors: [], processLog: { pids: [222] } });
  assert.deepEqual(requested.pids, [222]);
  assert.equal(requested.prefix, "matrix-failure");
  assert.deepEqual([requested.startedAtMs, requested.endedAtMs], [1000, 2000]);
  assert.deepEqual(evidence.snapshots().map(x => x.stage), ["before-showcase", "after-failure"]);
});

test("native gallery evidence retains a later content PID and isolates case artifacts", async () => {
  const requests = [];
  for (const prefix of ["noon-ordinary-matrix-failure", "noon-ordinary-table-failure"]) {
    const tables = [
      "101 /pw/webkit-x/Playwright",
      "101 /pw/webkit-x/Playwright\n222 /pw/webkit-x/WebKit.WebContent\n333 /other/WebKit.WebContent",
      "101 /pw/webkit-x/Playwright",
    ];
    const evidence = createNativeWebKitEvidence({ enabled: true,
      executablePath: "/pw/webkit-x/pw_run.sh", artifacts: "/diagnostics", prefix,
      readProcessTable: async () => tables.shift(),
      collect: async args => { requests.push(args); return { reports: [], errors: [] }; },
      collectLog: async args => ({ pids: args.pids }),
    });
    await evidence.observe("gallery-navigation");
    await evidence.observe("gallery-shell-ready");
    await evidence.failure();
    assert.deepEqual(requests.at(-1).pids, [101, 222]);
    assert.deepEqual(evidence.snapshots().map(snapshot => snapshot.processes.map(p => p.pid)),
      [[101], [101, 222], [101]]);
  }
  assert.deepEqual(requests.map(request => request.prefix),
    ["noon-ordinary-matrix-failure", "noon-ordinary-table-failure"]);
});

test("native reporting failures never replace original test failure", async () => {
  const evidence = createNativeWebKitEvidence({ enabled: true, executablePath: "/pw/w/bin", artifacts: "/x",
    readProcessTable: async () => { throw new Error("ps denied"); },
    collect: async () => { throw new Error("IPS unavailable"); },
    collectLog: async () => { throw new Error("native logging denied"); },
  });
  const result = await evidence.failure();
  assert.match(evidence.snapshots()[0].error, /ps denied/);
  assert.match(result.errors[0].message, /IPS unavailable/);
  assert.match(result.processLog.error, /native logging denied/);
});

test("non-WebKit platforms do not sample native processes", async () => {
  const evidence = createNativeWebKitEvidence({ enabled: false, readProcessTable: async () => {
    assert.fail("should not inspect process table");
  }, collectLog: async () => { assert.fail("should not collect native logs"); } });
  await evidence.observe("start");
  assert.equal(await evidence.failure(), null);
  assert.deepEqual(evidence.snapshots(), []);
});

test("case cleanup detaches only owned lifecycle listeners and remains idempotent", () => {
  const browser = new EventEmitter(), context = new EventEmitter(), page = new EventEmitter();
  const independent = () => {};
  browser.on("disconnected", independent);
  const trace = createBrowserFailureTrace();
  const detach = trace.attach(browser, context, page);
  trace.setPhase("gallery-authored-execution");
  page.emit("close");
  const failure = trace.snapshot();
  assert.equal(failure.events.at(-1).kind, "page-close");
  detach(); detach();
  assert.deepEqual(browser.listeners("disconnected"), [independent]);
  assert.equal(context.listenerCount("close"), 0);
  assert.equal(page.listenerCount("close"), 0);
  assert.equal(page.listenerCount("crash"), 0);
  assert.equal(page.listenerCount("framenavigated"), 0);
  browser.emit("disconnected");
  assert.deepEqual(trace.snapshot(), failure);
});
