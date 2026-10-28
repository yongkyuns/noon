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
