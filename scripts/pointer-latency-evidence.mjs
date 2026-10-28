// Offline diagnostic evidence only (#2298). This module never drives a browser,
// supplies a clock, retries an input, or decides performance acceptance.
import assert from "node:assert/strict";

const OUTCOMES = new Set(["accepted", "rejected", "cancelled", "error", "pending"]);
const RECEIPT_FIELDS = ["session", "sequence", "presentation", "view_revision"];
const PROFILES = ["browser", "backend", "renderHost", "buildProfile", "device", "workload"];
const isTime = value => typeof value === "number" && Number.isFinite(value) && value >= 0;
const present = value => value !== undefined && value !== null;
const key = (...parts) => JSON.stringify(parts);

function requireTime(value, label) {
  assert.ok(isTime(value), `${label} must be a finite nonnegative Window timestamp`);
}

function receiptKey(receipt, label) {
  assert.ok(receipt && RECEIPT_FIELDS.every(field =>
    Number.isSafeInteger(receipt[field]) && receipt[field] >= 0), `${label} must be a complete receipt`);
  assert.ok(receipt.presentation > 0, `${label}.presentation must be positive`);
  return key(...RECEIPT_FIELDS.map(field => receipt[field]));
}

export function distribution(values) {
  assert.ok(Array.isArray(values) && values.every(isTime), "latency values must be nonnegative finite numbers");
  const sorted = [...values].sort((a, b) => a - b);
  const n = sorted.length;
  return {
    count: n,
    medianMs: n ? (sorted[Math.floor((n - 1) / 2)] + sorted[Math.floor(n / 2)]) / 2 : null,
    // Nearest-rank p95; an empty cohort is unknown, never zero latency.
    p95Ms: n ? sorted[Math.ceil(0.95 * n) - 1] : null,
    maxMs: n ? sorted[n - 1] : null,
  };
}

function metadata(trace) {
  assert.equal(trace.schema, 1, "unsupported pointer timing evidence schema");
  assert.equal(trace.clock, "window-performance-now", "cross-realm/epoch clocks cannot be subtracted here");
  assert.ok(typeof trace.clockId === "string" && trace.clockId.length > 0, "one Window navigation clock ID required");
  assert.ok(trace.source && /^[0-9a-f]{40}$/.test(trace.source.revision) &&
    /^[0-9a-f]{40}$/.test(trace.source.tree) && /^[0-9a-f]{64}$/.test(trace.source.buildId),
    "exact source revision, tree and runtime build identity are required");
  for (const name of PROFILES) {
    assert.ok(typeof trace.profile?.[name] === "string" && trace.profile[name].length > 0,
      `profile.${name} must be declared`);
  }
  for (const name of ["width", "height", "deviceScaleFactor"]) {
    assert.ok(Number.isFinite(trace.profile.viewport?.[name]) && trace.profile.viewport[name] > 0,
      `profile.viewport.${name} must be positive`);
  }
  for (const name of ["fixtureSha256", "instrumentationSha256", "planId"]) {
    assert.ok(/^[0-9a-f]{64}$/.test(trace[name]), `${name} must be an exact SHA-256 pin`);
  }
  const window = trace.window;
  requireTime(window?.startMs, "window.startMs");
  requireTime(window?.endMs, "window.endMs");
  assert.ok(window.endMs > window.startMs, "observation window must be nonempty");
  assert.ok(typeof window.phase === "string" && window.phase.length > 0, "named observation phase required");
  assert.ok(["completed", "failed", "aborted"].includes(trace.runStatus), "run outcome must be preserved");
  assert.ok(Number.isSafeInteger(trace.dropped) && trace.dropped >= 0, "explicit dropped-record count required");
  assert.ok(Array.isArray(trace.observerErrors), "explicit observer-error array required");
  assert.ok(Array.isArray(trace.attempts) && Array.isArray(trace.presentations), "raw input and presentation arrays required");
}

function validateAttempt(row, index) {
  const label = `attempt[${index}]`;
  assert.ok(typeof row.endpoint === "string" && row.endpoint.length > 0, `${label}: endpoint generation required`);
  assert.ok(typeof row.attemptId === "string" && row.attemptId.length > 0, `${label}: attempt ID required`);
  assert.ok(row.kind === "press" || row.kind === "release", `${label}: only press/release cohorts are supported`);
  assert.ok(OUTCOMES.has(row.outcome), `${label}: unknown outcome`);
  requireTime(row.domMs, `${label}.domMs`);
  for (const field of ["postedMs", "replyMs", "controlReplyMs", "presentedMs"]) {
    if (present(row[field])) {
      requireTime(row[field], `${label}.${field}`);
      assert.ok(row[field] >= row.domMs, `${label}: ${field} precedes DOM collection`);
    }
  }
  if (present(row.postedMs)) {
    assert.ok(Number.isSafeInteger(row.requestId) && row.requestId >= 0, `${label}: post requires exact request ID`);
  } else {
    assert.ok(!present(row.requestId) && !present(row.replyMs), `${label}: reply/request without a post`);
    assert.ok(row.outcome !== "accepted" && row.outcome !== "rejected", `${label}: admission outcome without a post`);
  }
  if (present(row.replyMs)) assert.ok(row.replyMs >= row.postedMs, `${label}: reply precedes post`);
  if (row.outcome === "accepted" || row.outcome === "rejected") {
    assert.ok(present(row.replyMs), `${label}: admission outcome requires its matching reply`);
  }
  if (row.outcome === "pending") {
    assert.ok(!present(row.replyMs) && !present(row.controlReplyMs) && !present(row.presentedMs),
      `${label}: pending input cannot have a terminal observation`);
  }
  if (present(row.applied)) assert.equal(typeof row.applied, "boolean", `${label}: applied must be boolean or unknown`);
  if (present(row.controlReplyMs)) {
    assert.equal(row.outcome, "accepted", `${label}: control acknowledgement requires accepted input`);
    assert.ok(typeof row.applied === "boolean", `${label}: control result requires explicit applied status`);
    assert.ok(row.controlReplyMs >= row.replyMs, `${label}: control result precedes pointer reply`);
  }
  if (row.applied === true) assert.ok(present(row.controlReplyMs), `${label}: applied control lacks acknowledgement time`);
  if (present(row.presentedMs)) {
    assert.equal(row.applied, true, `${label}: presented control requires application evidence`);
    assert.ok(row.presentedMs >= row.postedMs, `${label}: presentation precedes request`);
    assert.equal(receiptKey(row.expectedReceipt, `${label}.expectedReceipt`),
      receiptKey(row.observedReceipt, `${label}.observedReceipt`), `${label}: presentation receipt does not match`);
    // Presentation can be acknowledged before the control reply reaches Window.
    // Do NOT require presentedMs >= replyMs or guess the next received frame.
  } else {
    assert.ok(!present(row.observedReceipt), `${label}: receipt without an observation time`);
  }
}

export function summarizePointerLatency(trace) {
  metadata(trace);
  const { startMs, endMs } = trace.window;
  const allKeys = new Set(), requestKeys = new Set();
  trace.attempts.forEach((row, index) => {
    validateAttempt(row, index);
    assert.equal(row.clockId, trace.clockId, "input timestamps belong to another Window/navigation");
    const identity = row.attemptId;
    assert.ok(!allKeys.has(identity), "duplicate logical attempt identity");
    allKeys.add(identity);
    if (present(row.requestId)) {
      const request = key(row.endpoint, row.requestId);
      assert.ok(!requestKeys.has(request), "duplicate request within one endpoint generation");
      requestKeys.add(request);
    }
  });
  const selected = trace.attempts.filter(row => row.domMs >= startMs && row.domMs < endMs);
  const usable = time => present(time) && time <= endMs;
  const metricsFor = rows => ({
    attempts: rows.length,
    outcomes: Object.fromEntries([...OUTCOMES].map(outcome => [outcome, rows.filter(row => row.outcome === outcome).length])),
    appliedControls: rows.filter(row => row.applied === true).length,
    notAppliedControls: rows.filter(row => row.applied === false).length,
    postNotObservedByEnd: rows.filter(row => !usable(row.postedMs)).length,
    replyNotObservedByEnd: rows.filter(row => !usable(row.replyMs)).length,
    appliedPresentationNotObservedByEnd: rows.filter(row => row.applied === true && !usable(row.presentedMs)).length,
    // These are conditional distributions with explicit denominators above.
    // They must never be presented as all-input performance acceptance.
    domToPost: distribution(rows.filter(row => usable(row.postedMs)).map(row => row.postedMs - row.domMs)),
    domToReply: distribution(rows.filter(row => usable(row.replyMs)).map(row => row.replyMs - row.domMs)),
    postToReply: distribution(rows.filter(row => usable(row.replyMs)).map(row => row.replyMs - row.postedMs)),
    domToAppliedAck: distribution(rows.filter(row => row.applied === true && usable(row.controlReplyMs))
      .map(row => row.controlReplyMs - row.domMs)),
    domToMatchedPresentedAck: distribution(rows.filter(row => row.applied === true && usable(row.presentedMs))
      .map(row => row.presentedMs - row.domMs)),
  });
  const groups = new Map(), seenReceipts = new Set();
  for (const [i, row] of trace.presentations.entries()) {
    assert.ok(typeof row.endpoint === "string" && row.endpoint.length > 0, `presentation[${i}]: endpoint required`);
    requireTime(row.atMs, `presentation[${i}].atMs`);
    assert.equal(row.clockId, trace.clockId, "presentation belongs to another Window/navigation");
    const receipt = receiptKey(row.receipt, `presentation[${i}].receipt`);
    const identity = key(row.endpoint, receipt);
    assert.ok(!seenReceipts.has(identity), "duplicate presentation observation; do not silently deduplicate");
    seenReceipts.add(identity);
    if (row.atMs < startMs || row.atMs > endMs) continue;
    const group = key(row.endpoint, row.receipt.session, row.receipt.view_revision);
    const rows = groups.get(group) ?? [];
    rows.push(row);
    groups.set(group, rows);
  }
  for (const row of trace.attempts.filter(row => present(row.presentedMs))) {
    assert.ok(trace.presentations.some(observation => observation.endpoint === row.endpoint &&
      observation.atMs === row.presentedMs && receiptKey(observation.receipt, "presentation") ===
      receiptKey(row.observedReceipt, "matched receipt")),
      "matched presentation must exist in the raw receipt stream");
  }
  const intervals = [], sequenceGaps = [];
  for (const [group, rows] of groups) {
    // Preserve arrival order; sorting could hide a damaged observer timeline.
    for (let i = 1; i < rows.length; i++) {
      const a = rows[i - 1], b = rows[i];
      assert.ok(b.atMs >= a.atMs && b.receipt.presentation > a.receipt.presentation,
        "presentation timestamps/identities regressed within a generation and view");
      intervals.push(b.atMs - a.atMs);
      if (b.receipt.sequence !== a.receipt.sequence + 1) sequenceGaps.push({ group, before: a.receipt, after: b.receipt });
    }
  }
  return {
    scope: "offline-window-clock-diagnostic-not-performance-acceptance",
    source: trace.source, profile: trace.profile, window: trace.window, runStatus: trace.runStatus,
    recordIntegrity: { dropped: trace.dropped, observerErrors: trace.observerErrors,
      complete: trace.dropped === 0 && trace.observerErrors.length === 0 },
    excludedBeforeWindow: trace.attempts.filter(row => row.domMs < startMs).length,
    excludedAtOrAfterEnd: trace.attempts.filter(row => row.domMs >= endMs).length,
    all: metricsFor(selected), press: metricsFor(selected.filter(row => row.kind === "press")),
    release: metricsFor(selected.filter(row => row.kind === "release")),
    presentationArrivalIntervals: distribution(intervals), presentationSequenceGaps: sequenceGaps,
    // Retain raw records, including warmup, failed, unmatched and late outcomes.
    raw: trace,
    acceptance: "not-evaluated",
  };
}

export function comparePointerLatency(baseline, candidate) {
  const before = summarizePointerLatency(baseline), after = summarizePointerLatency(candidate);
  assert.deepEqual(candidate.profile, baseline.profile, "paired runs require identical declared profiles and devices");
  assert.equal(candidate.window.phase, baseline.window.phase, "paired observation phases must match");
  assert.equal(candidate.window.endMs - candidate.window.startMs,
    baseline.window.endMs - baseline.window.startMs, "paired observation durations must match");
  assert.ok(before.recordIntegrity.complete && after.recordIntegrity.complete,
    "dropped records or observer errors cannot produce a paired summary");
  assert.ok(baseline.pairId && baseline.pairId === candidate.pairId, "explicit matching pair ID required");
  for (const pin of ["fixtureSha256", "instrumentationSha256", "planId"]) {
    assert.equal(candidate[pin], baseline[pin], `paired ${pin} differs`);
  }
  const ids = trace => trace.attempts.filter(row => row.domMs >= trace.window.startMs && row.domMs < trace.window.endMs)
    .map(row => key(row.attemptId, row.kind));
  const baseIds = ids(baseline), candidateIds = ids(candidate);
  // Rejections/aborts may leave unmatched logical attempts; retain rather than
  // intersecting away the difficult part of the cohort.
  return {
    scope: "paired-descriptive-evidence-not-performance-acceptance", pairId: baseline.pairId,
    baseline: before, candidate: after,
    baselineOnly: baseIds.filter(id => !candidateIds.includes(id)),
    candidateOnly: candidateIds.filter(id => !baseIds.includes(id)),
    acceptance: "not-evaluated",
  };
}
