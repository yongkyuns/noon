import assert from "node:assert/strict";
import test from "node:test";
import { comparePointerLatency, distribution, summarizePointerLatency } from "./pointer-latency-evidence.mjs";

const receipt = (sequence = 1, presentation = sequence + 1, view_revision = 1, session = 1) =>
  ({ session, sequence, presentation, view_revision });
const attempt = (overrides = {}) => ({
  clockId: "navigation-1", endpoint: "engine-generation-1", attemptId: "case-3/play/attempt-1/press", kind: "press",
  requestId: 1, domMs: 110, postedMs: 112, replyMs: 120, outcome: "accepted", ...overrides,
});
const trace = (overrides = {}) => ({
  schema: 1, clock: "window-performance-now", clockId: "navigation-1", pairId: "pair-1", planId: "d".repeat(64),
  fixtureSha256: "e".repeat(64), instrumentationSha256: "f".repeat(64),
  source: { revision: "a".repeat(40), tree: "b".repeat(40), buildId: "c".repeat(64) },
  profile: { browser: "Firefox/version-pinned", backend: "WebGL2", renderHost: "worker",
    buildProfile: "release/pinned-flags", device: "runner/device-pinned", workload: "finite-controls/pinned",
    viewport: { width: 1280, height: 900, deviceScaleFactor: 1 } },
  window: { startMs: 100, endMs: 400, phase: "declared-finite-controls" },
  runStatus: "completed", dropped: 0, observerErrors: [], attempts: [attempt()], presentations: [],
  ...overrides,
});

function appliedTrace(overrides = {}) {
  return trace({ attempts: [attempt({ kind: "release", applied: true, replyMs: 120, controlReplyMs: 125,
    presentedMs: 118, expectedReceipt: receipt(), observedReceipt: receipt(), ...overrides })],
    presentations: [{ clockId: "navigation-1", endpoint: "engine-generation-1", atMs: 118, receipt: receipt() }] });
}

test("empty cohorts are unknown, never zero latency or accepted", () => {
  const result = summarizePointerLatency(trace({ attempts: [] }));
  assert.deepEqual(result.all.domToReply, { count: 0, medianMs: null, p95Ms: null, maxMs: null });
  assert.equal(result.acceptance, "not-evaluated");
});

test("quantiles use an explicit even median and nearest-rank p95 without mutating input", () => {
  const values = [20, 1, ...Array.from({ length: 18 }, (_, i) => i + 2)];
  const original = [...values];
  assert.deepEqual(distribution(values), { count: 20, medianMs: 10.5, p95Ms: 19, maxMs: 20 });
  assert.deepEqual(values, original);
  for (const invalid of [[NaN], [Infinity], [-1]]) assert.throws(() => distribution(invalid));
});

test("rejected, cancelled, error, and pending attempts remain in denominators and raw records", () => {
  const evidence = trace({ runStatus: "failed", attempts: [
    attempt(), attempt({ attemptId: "reject", requestId: 2, outcome: "rejected", replyMs: 140 }),
    attempt({ attemptId: "cancel", requestId: null, postedMs: null, replyMs: null, outcome: "cancelled" }),
    attempt({ attemptId: "error", requestId: 3, outcome: "error", replyMs: 150 }),
    attempt({ attemptId: "pending", requestId: 4, outcome: "pending", replyMs: null }),
  ] });
  const result = summarizePointerLatency(evidence);
  assert.deepEqual(result.all.outcomes, { accepted: 1, rejected: 1, cancelled: 1, error: 1, pending: 1 });
  assert.equal(result.all.attempts, 5);
  assert.equal(result.all.domToReply.count, 3);
  assert.equal(result.all.replyNotObservedByEnd, 2);
  assert.equal(result.all.postNotObservedByEnd, 1);
  assert.equal(result.raw, evidence);
  assert.equal(result.runStatus, "failed");
});

test("fixed window excludes warmup and exact end starts but retains every raw attempt", () => {
  const result = summarizePointerLatency(trace({ attempts: [
    attempt({ attemptId: "warmup", requestId: 1, domMs: 90, postedMs: 95, replyMs: 99 }),
    attempt({ attemptId: "start", requestId: 2, domMs: 100 }),
    attempt({ attemptId: "end", requestId: 3, domMs: 400, postedMs: 400, replyMs: 400 }),
  ] }));
  assert.equal(result.all.attempts, 1);
  assert.equal(result.excludedBeforeWindow, 1);
  assert.equal(result.excludedAtOrAfterEnd, 1);
  assert.equal(result.raw.attempts.length, 3);
});

test("late replies are right-censored rather than scored or removed", () => {
  const result = summarizePointerLatency(trace({ attempts: [attempt({ replyMs: 401 })] }));
  assert.equal(result.all.attempts, 1);
  assert.equal(result.all.outcomes.accepted, 1);
  assert.equal(result.all.replyNotObservedByEnd, 1);
  assert.equal(result.all.domToReply.count, 0);
  assert.equal(result.raw.attempts[0].replyMs, 401);
});

test("an exact end reply is observed; equal coarsened times remain valid zeros", () => {
  assert.equal(summarizePointerLatency(trace({ attempts: [attempt({ replyMs: 400 })] }))
    .all.replyNotObservedByEnd, 0);
  const result = summarizePointerLatency(trace({ attempts: [attempt({ domMs: 110, postedMs: 110, replyMs: 110 })] }));
  assert.equal(result.all.domToReply.medianMs, 0);
});

test("request IDs can repeat across endpoint generations without cross-joining", () => {
  const result = summarizePointerLatency(trace({ attempts: [attempt(),
    attempt({ endpoint: "engine-generation-2", attemptId: "next-run/press", replyMs: 180 })] }));
  assert.equal(result.all.domToReply.count, 2);
  assert.equal(result.all.domToReply.maxMs, 70);
});

test("duplicate request or logical attempt identities are rejected, not deduplicated", () => {
  assert.throws(() => summarizePointerLatency(trace({ attempts: [attempt(), attempt({ attemptId: "duplicate-request" })] })), /duplicate request/);
  assert.throws(() => summarizePointerLatency(trace({ attempts: [attempt(), attempt({ endpoint: "other", requestId: 2 })] })), /duplicate logical/);
});

test("strict time-domain, source pins, profile, and window checks", () => {
  for (const clock of ["worker-performance-now", "epoch", undefined]) {
    assert.throws(() => summarizePointerLatency(trace({ clock })), /clock/);
  }
  assert.throws(() => summarizePointerLatency(trace({ source: {} })), /source/);
  assert.throws(() => summarizePointerLatency(trace({ profile: {} })), /profile/);
  assert.throws(() => summarizePointerLatency(trace({ window: { startMs: 100, endMs: 100, phase: "x" } })), /nonempty/);
  for (const pin of ["planId", "fixtureSha256", "instrumentationSha256"]) {
    assert.throws(() => summarizePointerLatency(trace({ [pin]: null })), /SHA-256/);
  }
});

test("clock reversal and missing timestamps cannot masquerade as fast input", () => {
  for (const overrides of [ { domMs: NaN }, { postedMs: 109 }, { replyMs: 111 },
    { postedMs: null }, { replyMs: null }, { requestId: null } ]) {
    assert.throws(() => summarizePointerLatency(trace({ attempts: [attempt(overrides)] })));
  }
});

test("pending outcomes with terminal data are rejected", () => {
  assert.throws(() => summarizePointerLatency(trace({ attempts: [attempt({ outcome: "pending" })] })), /pending/);
});

test("presented acknowledgement may precede pointer/control reply arrival", () => {
  const result = summarizePointerLatency(appliedTrace());
  assert.equal(result.release.domToMatchedPresentedAck.medianMs, 8);
  assert.equal(result.release.domToAppliedAck.medianMs, 15);
  assert.equal(result.release.domToReply.medianMs, 10);
});

test("application acknowledgements require explicit acceptance, result, and causally ordered time", () => {
  for (const overrides of [ { applied: true }, { applied: true, controlReplyMs: 119 },
    { applied: true, controlReplyMs: 125, outcome: "rejected" }, { controlReplyMs: 125 } ]) {
    assert.throws(() => summarizePointerLatency(trace({ attempts: [attempt(overrides)] })));
  }
});

test("missing presentation remains unknown rather than selecting the next frame", () => {
  const evidence = appliedTrace({ presentedMs: null, observedReceipt: null });
  const result = summarizePointerLatency(evidence);
  assert.equal(result.all.appliedPresentationNotObservedByEnd, 1);
  assert.equal(result.all.domToMatchedPresentedAck.count, 0);
});

test("all receipt identity fields participate in a presentation join", () => {
  for (const field of ["session", "sequence", "presentation", "view_revision"]) {
    const r = receipt(); r[field]++;
    assert.throws(() => summarizePointerLatency(appliedTrace({ observedReceipt: r })), /does not match/);
  }
});

test("presentation cannot be claimed without its exact raw event or complete receipt", () => {
  const missing = appliedTrace(); missing.presentations = [];
  assert.throws(() => summarizePointerLatency(missing), /raw receipt stream/);
  assert.throws(() => summarizePointerLatency(appliedTrace({ expectedReceipt: {} })), /complete receipt/);
  assert.throws(() => summarizePointerLatency(appliedTrace({ presentedMs: 111 })), /precedes request/);
});

test("pacing is grouped by endpoint/session/view, never across unrelated pictures", () => {
  const result = summarizePointerLatency(trace({ presentations: [
    { clockId: "navigation-1", endpoint: "a", atMs: 110, receipt: receipt(1, 2, 1, 1) },
    { clockId: "navigation-1", endpoint: "a", atMs: 126, receipt: receipt(2, 3, 1, 1) },
    { clockId: "navigation-1", endpoint: "a", atMs: 180, receipt: receipt(3, 4, 2, 1) },
    { clockId: "navigation-1", endpoint: "a", atMs: 195, receipt: receipt(1, 2, 2, 2) },
    { clockId: "navigation-1", endpoint: "b", atMs: 210, receipt: receipt(1, 2, 1, 1) },
  ] }));
  assert.deepEqual(result.presentationArrivalIntervals, { count: 1, medianMs: 16, p95Ms: 16, maxMs: 16 });
});

test("publication sequence gaps stay visible without dropping their intervals", () => {
  const result = summarizePointerLatency(trace({ presentations: [
    { clockId: "navigation-1", endpoint: "a", atMs: 110, receipt: receipt(1, 2) },
    { clockId: "navigation-1", endpoint: "a", atMs: 145, receipt: receipt(4, 5) },
  ] }));
  assert.equal(result.presentationArrivalIntervals.count, 1);
  assert.equal(result.presentationArrivalIntervals.maxMs, 35);
  assert.equal(result.presentationSequenceGaps.length, 1);
});

test("duplicate or reordered receipt streams fail rather than silently sorting", () => {
  for (const rows of [
    [{ atMs: 110, receipt: receipt() }, { atMs: 120, receipt: receipt() }],
    [{ atMs: 120, receipt: receipt() }, { atMs: 110, receipt: receipt(2) }],
    [{ atMs: 110, receipt: receipt(2) }, { atMs: 120, receipt: receipt(1) }],
  ]) assert.throws(() => summarizePointerLatency(trace({ presentations: rows.map(r => ({ clockId: "navigation-1", endpoint: "a", ...r })) })));
});

test("observer loss and errors stay visible in individual reports and forbid pairing", () => {
  for (const fault of [{ dropped: 1 }, { observerErrors: ["overflow"] }]) {
    const bad = trace(fault);
    assert.equal(summarizePointerLatency(bad).recordIntegrity.complete, false);
    assert.throws(() => comparePointerLatency(trace(), bad), /observer errors/);
  }
});

test("comparison retains adverse results, failed runs, and unmatched logical actions", () => {
  const before = trace({ runStatus: "aborted" });
  const after = trace({ attempts: [attempt({ replyMs: 200 }), attempt({ attemptId: "additional", requestId: 2 })] });
  const result = comparePointerLatency(before, after);
  assert.equal(result.baseline.runStatus, "aborted");
  assert.equal(result.candidate.all.domToReply.maxMs, 90);
  assert.equal(result.candidateOnly.length, 1);
  assert.equal(result.baselineOnly.length, 0);
  assert.equal(result.acceptance, "not-evaluated");
});

test("pairing rejects profile/device/phase/window/fixture/instrumentation/plan drift", () => {
  const before = trace();
  const variations = [
    trace({ pairId: "different" }),
    trace({ profile: { ...before.profile, device: "other" } }),
    trace({ profile: { ...before.profile, viewport: { ...before.profile.viewport, deviceScaleFactor: 2 } } }),
    trace({ window: { ...before.window, phase: "other" } }),
    trace({ window: { ...before.window, endMs: 401 } }),
    ...["fixtureSha256", "instrumentationSha256", "planId"].map(pin => trace({ [pin]: "0".repeat(64) })),
  ];
  for (const after of variations) assert.throws(() => comparePointerLatency(before, after));
});

test("summarization leaves frozen raw evidence untouched", () => {
  const evidence = appliedTrace();
  const freeze = value => { if (value && typeof value === "object") { Object.values(value).forEach(freeze); Object.freeze(value); } };
  freeze(evidence);
  const before = JSON.stringify(evidence);
  summarizePointerLatency(evidence);
  assert.equal(JSON.stringify(evidence), before);
});


test("mixed Window/navigation identities are rejected even for equal numeric timestamps", () => {
  assert.throws(() => summarizePointerLatency(trace({ clockId: null })), /clock ID/);
  assert.throws(() => summarizePointerLatency(trace({ attempts: [attempt({ clockId: "navigation-2" })] })), /another Window/);
  assert.throws(() => summarizePointerLatency(trace({ presentations: [
    { clockId: "navigation-2", endpoint: "a", atMs: 120, receipt: receipt() },
  ] })), /another Window/);
});

test("a control retaining an already visible picture preserves its raw time without a zero-latency sample", () => {
  const prior = { atMs: 101, receipt: receipt() };
  const evidence = trace({ attempts: [attempt({ kind: "release", applied: true, controlReplyMs: 125,
    expectedReceipt: receipt(), preexistingPresentation: prior })],
    presentations: [{ clockId: "navigation-1", endpoint: "engine-generation-1", ...prior }] });
  const summary = summarizePointerLatency(evidence);
  assert.equal(summary.all.appliedAlreadyVisible, 1);
  assert.equal(summary.all.domToMatchedPresentedAck.count, 0);
  assert.equal(summary.all.appliedPresentationNotObservedByEnd, 0);
  assert.throws(() => summarizePointerLatency({ ...evidence, presentations: [] }), /raw receipt stream/);
  for (const invalid of [
    { ...prior, atMs: 112 },
    { ...prior, receipt: receipt(1, 2, 3) },
  ]) assert.throws(() => summarizePointerLatency({ ...evidence, attempts: [
    { ...evidence.attempts[0], preexistingPresentation: invalid },
  ] }));
});
