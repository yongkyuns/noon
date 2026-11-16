import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import { FrameArtifactStore } from "../../../scripts/agent-preview-artifacts.mjs";
import {
  buildFeedbackComparison, buildFrameFeedback, checkFeedbackSource, readFrameFeedback,
} from "../src/frame-feedback.mjs";

// Generated tiny PNG fixture, not a capture or educational quality result.
const PNG = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGNgAAIAAAUAAXpeqz8AAAAASUVORK5CYII=", "base64");
const SOURCE = "a".repeat(64);
const REVISED = "b".repeat(64);
const BUILD = { engineRevision: "c".repeat(40), wasmSha256: "d".repeat(64), workerSha256: null, buildId: "fixture-build" };
const note = () => ({
  interval: { start: 1, end: 1.5 },
  region: { x: 0.1, y: 0.2, width: 0.4, height: 0.5 },
  observation: "The two contributions disappear when their sum reaches zero.",
  desiredChange: "Keep both contributions visible while changing their sum.",
  preserve: "The inputs remain distinct even when they cancel.",
});

function retained({ source = SOURCE, sessionId = "fixture-session", time = 1.3, build = BUILD } = {}) {
  const store = new FrameArtifactStore();
  const scope = store.openScope({ sessionId, sourceSha256: source, build });
  const descriptor = scope.putFrame({
    png: PNG, requestedTime: time - 0.05, publishedTime: time,
    backend: "fixture", sceneRevision: null, frameRevision: 7,
  });
  const captured = scope.getFrame(descriptor.id);
  store.dispose();
  return captured;
}

test("feedback copies exact observed evidence and remains portable after scope closure", () => {
  const captured = retained();
  const input = note();
  const record = buildFrameFeedback(captured, input);
  const { retentionMs: _retentionMs, ...expected } = captured.descriptor;
  assert.equal(record.kind, "noon-frame-feedback");
  assert.equal(record.status, "pending_review");
  assert.deepEqual(record.frame, expected);
  assert.equal(record.frame.sha256, createHash("sha256").update(PNG).digest("hex"));
  assert.deepEqual(record.feedback, input);
  assert.notEqual(record.feedback.interval, input.interval);
  assert.notEqual(record.frame.provenance, captured.descriptor.provenance);
  input.interval.start = 0;
  input.region.width = 0.1;
  assert.equal(record.feedback.interval.start, 1);
  assert.equal(record.feedback.region.width, 0.4);
  for (const value of [record, record.feedback, record.feedback.interval, record.feedback.region,
    record.frame, record.frame.provenance, record.frame.provenance.build]) assert.ok(Object.isFrozen(value));
  assert.throws(() => { record.frame.provenance.sessionId = "replacement"; }, TypeError);
  assert.deepEqual(readFrameFeedback(JSON.parse(JSON.stringify(record)), PNG), record);
});

test("unknown provenance remains explicit and optional reviewer detail remains absent", () => {
  const build = { engineRevision: null, wasmSha256: null, workerSha256: null, buildId: null };
  const { region: _region, preserve: _preserve, ...input } = note();
  const record = buildFrameFeedback(retained({ build }), input);
  assert.deepEqual(record.frame.provenance.build, build);
  assert.equal(record.frame.provenance.sceneRevision, null);
  assert.equal(record.feedback.region, null);
  assert.equal(record.feedback.preserve, null);
});

test("annotation intervals contain the actual published time, with bounded finite coordinates", () => {
  const captured = retained();
  assert.doesNotThrow(() => buildFrameFeedback(captured, { ...note(), interval: { start: 1.3, end: 1.3 } }));
  for (const interval of [
    { start: 1, end: 1.25 }, { start: 2, end: 3 }, { start: 1.5, end: 1 },
    { start: -1, end: 2 }, { start: 0, end: 601 }, { start: 0, end: Infinity },
  ]) assert.throws(() => buildFrameFeedback(captured, { ...note(), interval }));
  assert.doesNotThrow(() => buildFrameFeedback(captured, { ...note(), region: { x: 0, y: 0, width: 1, height: 1 } }));
  for (const region of [
    { x: -0.1, y: 0, width: 0.2, height: 0.2 }, { x: 0, y: 0, width: 0, height: 1 },
    { x: 0.8, y: 0, width: 0.3, height: 0.2 }, { x: 0, y: 0.8, width: 0.2, height: 0.3 },
    { x: NaN, y: 0, width: 0.2, height: 0.2 },
  ]) assert.throws(() => buildFrameFeedback(captured, { ...note(), region }));
});

test("review text is UTF-8 bounded and caller provenance, resolution and targeting claims are rejected", () => {
  const captured = retained();
  assert.doesNotThrow(() => buildFrameFeedback(captured, { ...note(), observation: "é".repeat(2000) }));
  for (const observation of ["", "  ", "text\0hidden", "é".repeat(2001)]) {
    assert.throws(() => buildFrameFeedback(captured, { ...note(), observation }));
  }
  for (const extra of [
    { sourceSha256: REVISED }, { status: "resolved" }, { semanticTarget: "object-1" }, { provenance: {} },
  ]) assert.throws(() => buildFrameFeedback(captured, { ...note(), ...extra }));
});

test("image changes and forged dimensions fail before accepting persisted feedback", () => {
  const captured = retained();
  const record = buildFrameFeedback(captured, note());
  const changed = Buffer.from(PNG);
  changed[45] ^= 1;
  assert.throws(() => buildFrameFeedback({ ...captured, png: changed }, note()), /does not match/);
  assert.throws(() => readFrameFeedback(record, changed), /does not match/);
  assert.throws(() => buildFrameFeedback({
    ...captured, descriptor: { ...captured.descriptor, width: 2 },
  }, note()), /does not match/);
  assert.throws(() => readFrameFeedback({ ...record, frame: { ...record.frame, height: 2 } }, PNG), /does not match/);
  assert.throws(() => readFrameFeedback({ ...record, status: "resolved" }, PNG));
  assert.throws(() => readFrameFeedback({ ...record, renderSucceeded: true }, PNG));
});

test("source checks explicitly flag stale feedback without rewriting original association", () => {
  const record = buildFrameFeedback(retained(), note());
  const original = JSON.stringify(record);
  assert.deepEqual(checkFeedbackSource(record, SOURCE), {
    status: "pending_review", sourceStatus: "current", observedSourceSha256: SOURCE, currentSourceSha256: SOURCE,
  });
  assert.deepEqual(checkFeedbackSource(record, REVISED), {
    status: "pending_review", sourceStatus: "stale", observedSourceSha256: SOURCE, currentSourceSha256: REVISED,
  });
  assert.equal(JSON.stringify(record), original);
  assert.throws(() => checkFeedbackSource(record, "not-a-source-hash"));
});

test("comparison preserves independent before/after identities and leaves interval association unverified", () => {
  const record = buildFrameFeedback(retained(), note());
  const candidate = retained({ source: REVISED, sessionId: "revised-session", time: 3 });
  const result = buildFeedbackComparison(record, candidate, REVISED);
  assert.equal(result.status, "pending_review");
  assert.equal(result.intervalAssociation, "unverified");
  assert.equal(result.sourceAssociation.sourceStatus, "stale");
  assert.deepEqual(result.before, record);
  assert.equal(result.before.frame.provenance.sourceSha256, SOURCE);
  assert.equal(result.before.frame.provenance.sessionId, "fixture-session");
  assert.equal(result.after.provenance.sourceSha256, REVISED);
  assert.equal(result.after.provenance.sessionId, "revised-session");
  assert.equal(result.after.provenance.publishedTime, 3);
  assert.deepEqual(result.before.feedback.interval, { start: 1, end: 1.5 });
  assert.ok(Object.isFrozen(result.after.provenance));
  assert.throws(() => buildFeedbackComparison(record, candidate, SOURCE), /does not match.*current source/);
});
