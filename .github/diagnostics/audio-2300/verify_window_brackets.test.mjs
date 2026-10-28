import assert from "node:assert/strict";
import test from "node:test";
import { validateWindowBracketReport } from "./verify_window_brackets.mjs";

const sample = 1_793_152_307_172.3;
const receipt = 1_793_152_307_172.2998;
function report() {
  const anchor = {
    session: 7, time: 10, sampledEpochMs: sample,
    receivedEpochMs: receipt, sources: [8, 9],
    windowClockBracket: {
      schema: 1, requestId: 34, requestType: "scene_control",
      session: 7, lifecycle: 2,
      sentMs: 100, receivedMs: 106, appliedMs: 110,
      observableAgeLowerMs: 4, observableAgeUpperMs: 10,
      authoredTime: 10, sampledEpochMs: sample, receivedEpochMs: receipt,
      reconstructedEpochAgeMs: receipt - sample,
    },
  };
  return { status: "passed", browserErrors: [], consoleErrors: [], failedResponses: [],
    cases: Array.from({ length: 15 }, (_, i) => ({
      status: "passed", ...(i === 0 ? { running: { anchors: [anchor] } } :
        i === 1 ? { snapshot: { anchors: [structuredClone(anchor)] } } : {}),
    })) };
}

test("causally bracketed Start permits the historical backwards cross-realm epoch", () => {
  const result = validateWindowBracketReport(report());
  assert.equal(result.distinctStarts, 1);
  assert.equal(result.observations[0].observableLowerMs, 4);
  assert.equal(result.observations[0].observableUpperMs, 10);
  assert.equal(result.observations[0].reconstructedEpochAgeMs, -0.000244140625);
  assert.match(result.clockError, /unknown/);
});

test("same-Window request and response cannot reverse even by one ULP", () => {
  const altered = report();
  altered.cases[0].running.anchors[0].windowClockBracket.receivedMs = 99.99999999999999;
  assert.ok(altered.cases[0].running.anchors[0].windowClockBracket.receivedMs < 100);
  altered.cases[0].running.anchors[0].windowClockBracket.observableAgeLowerMs =
    110 - altered.cases[0].running.anchors[0].windowClockBracket.receivedMs;
  assert.throws(() => validateWindowBracketReport(altered));
});

test("the applied moment cannot precede matching response", () => {
  const altered = report();
  const b = altered.cases[0].running.anchors[0].windowClockBracket;
  b.appliedMs = 105;
  b.observableAgeLowerMs = -1;
  b.observableAgeUpperMs = 5;
  assert.throws(() => validateWindowBracketReport(altered));
});

test("unobserved native Start cannot count as qualified", () => {
  const altered = report();
  delete altered.cases[0].running.anchors[0].windowClockBracket;
  assert.throws(() => validateWindowBracketReport(altered), /correlated bracket|cannot change/i);
});

test("duplicate cumulative native Start snapshots cannot disagree", () => {
  const altered = report();
  altered.cases[1].snapshot.anchors[0].windowClockBracket.appliedMs = 111;
  assert.throws(() => validateWindowBracketReport(altered), /cannot change/);
});

test("reported native Start bytes must match the bracket observation exactly", () => {
  const altered = report();
  altered.cases[0].running.anchors[0].windowClockBracket.receivedEpochMs += 0.000244140625;
  assert.throws(() => validateWindowBracketReport(altered), /cannot change|strictEqual/);
});

test("missing or failed original cases never qualify", () => {
  const altered = report();
  altered.cases[14].status = "failed";
  assert.throws(() => validateWindowBracketReport(altered));
  altered.cases[14].status = "passed";
  altered.cases.pop();
  assert.throws(() => validateWindowBracketReport(altered));
});

test("original browser and console error guards remain required", () => {
  const altered = report();
  altered.consoleErrors.push({ text: "audio Start failed" });
  assert.throws(() => validateWindowBracketReport(altered));
});

test("a missing native Start trace is a failure, not an empty pass", () => {
  const altered = report();
  delete altered.cases[0].running;
  delete altered.cases[1].snapshot;
  assert.throws(() => validateWindowBracketReport(altered), /no native Start/);
});

test("the unchanged native Start wrapper receives its bracket after the wrapper returns", async t => {
  const { copyFile, mkdir, mkdtemp, rm, writeFile } = await import("node:fs/promises");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const { pathToFileURL } = await import("node:url");
  const dir = await mkdtemp(join(tmpdir(), "noon-audio-window-bracket-"));
  const previousQualification = globalThis.audioQualification;
  t.after(async () => {
    globalThis.audioQualification = previousQualification;
    await rm(dir, { recursive: true, force: true });
  });
  await mkdir(join(dir, "pkg"));
  await writeFile(join(dir, "package.json"), '{"type":"module"}\n');
  await copyFile(new URL("../../../web/worker-audio-device.js", import.meta.url),
    join(dir, "worker-audio-device.js"));
  await writeFile(join(dir, "pkg/noon_web.js"), [
    "export const state = { starts: [] };",
    "export default async function init() {}",
    "export class WasmWorkerAudioOutput {",
    "  constructor() { this.context = new EventTarget(); }",
    "  setVolume() {} setMuted() {}",
    "  audioContext() { return this.context; }",
    "  ready() { return true; }",
    "  unlock() { return Promise.resolve(); }",
    "  start(...args) { state.starts.push(args); }",
    "  pause() {} stop() {} free() {}",
    "}",
  ].join("\n"));
  const fixture = await import(pathToFileURL(join(dir, "pkg/noon_web.js")).href);
  const { WorkerAudioDevice } = await import(pathToFileURL(join(dir, "worker-audio-device.js")).href);
  const qualification = { anchors: [] };
  globalThis.audioQualification = qualification;
  const original = fixture.WasmWorkerAudioOutput.prototype.start;
  fixture.WasmWorkerAudioOutput.prototype.start = function (...args) {
    const result = original.call(this, ...args);
    qualification.anchors.push({
      session: args[0], time: args[1], sampledEpochMs: args[2],
      receivedEpochMs: args[3], sources: [fixture.state.starts.length],
    });
    return result;
  };
  const device = new WorkerAudioDevice(() => assert.fail("unexpected interruption"));
  t.after(() => device.dispose());
  await device.install(new Uint8Array([1, 2]));
  const anchor = { session: 7, time: 10, playing: true, sampledEpochMs: 1_793_152_307_172.3 };
  const tick = performance.now();
  const first = { session: 7, lifecycle: 3, requestId: 41, type: "resume",
    sentMs: tick - 8, receivedMs: tick - 2 };
  const second = { ...first, requestId: 42 };
  device.applyAnchor(anchor, 7, first);
  device.applyAnchor(anchor, 7, second);
  assert.equal(qualification.anchors.length, 2, "the unchanged fixture wraps both native Starts");
  assert.equal(qualification.anchors[0].windowClockBracket, undefined,
    "the fixture only appends after the native call returns");
  await Promise.resolve();
  assert.equal(qualification.anchors[0].windowClockBracket.requestId, 41);
  assert.equal(qualification.anchors[1].windowClockBracket.requestId, 42);
  for (let i = 0; i < 2; i++) {
    const saved = qualification.anchors[i], bracket = saved.windowClockBracket;
    assert.equal(bracket.session, 7);
    assert.equal(bracket.sampledEpochMs, anchor.sampledEpochMs);
    assert.equal(bracket.receivedEpochMs, saved.receivedEpochMs);
    assert.equal(bracket.reconstructedEpochAgeMs, saved.receivedEpochMs - anchor.sampledEpochMs);
    assert.equal(bracket.observableAgeLowerMs, bracket.appliedMs - bracket.receivedMs);
    assert.equal(bracket.observableAgeUpperMs, bracket.appliedMs - bracket.sentMs);
    assert.equal(fixture.state.starts[i].length, 4, "original Rust Start ABI unchanged");
  }
  assert.equal(qualification.anchors[0].windowClockBracket.type, undefined);
  assert.equal(qualification.anchors[0].windowClockBracket.requestType, "resume");
});
