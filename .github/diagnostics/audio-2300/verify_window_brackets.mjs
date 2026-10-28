// Diagnostic-only verifier for the ORIGINAL 15-case Python audio report.
// It makes no assertions about physical-time precision or symmetry.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

export function validateWindowBracketReport(report) {
  assert.equal(report.status, "passed", "the unchanged original fixture must pass");
  assert.equal(report.cases?.length, 15, "all 15 unchanged fixture cases are required");
  assert.ok(report.cases.every(c => c.status === "passed"));
  assert.deepEqual(report.browserErrors, []);
  assert.deepEqual(report.consoleErrors, []);
  assert.deepEqual(report.failedResponses, []);

  // The original fixture stores cumulative native-Start snapshots under
  // heterogeneous case fields; count each actual Start only once.
  const found = new Map();
  const visited = new WeakSet();
  const walk = (value) => {
    if (value === null || typeof value !== "object" || visited.has(value)) return;
    visited.add(value);
    if (Array.isArray(value.anchors)) {
      for (const anchor of value.anchors) {
        if (!Array.isArray(anchor.sources) || anchor.sources.length === 0) continue;
        const key = JSON.stringify([anchor.session, anchor.time, anchor.sampledEpochMs,
          anchor.receivedEpochMs, anchor.sources]);
        if (found.has(key)) {
          assert.deepEqual(anchor.windowClockBracket, found.get(key).windowClockBracket,
            "one native Start cannot change its receipt evidence");
        } else found.set(key, anchor);
      }
    }
    for (const entry of Object.values(value)) walk(entry);
  };
  for (const entry of report.cases) walk(entry);
  assert.ok(found.size >= 1, "no native Start observations reached the report");
  const observations = [];
  for (const anchor of found.values()) {
    const b = anchor.windowClockBracket;
    assert.ok(b && b.schema === 1, "every actual native Start needs a correlated bracket");
    for (const field of ["sentMs", "receivedMs", "appliedMs", "sampledEpochMs",
      "receivedEpochMs", "observableAgeLowerMs", "observableAgeUpperMs", "authoredTime"]) {
      assert.ok(Number.isFinite(b[field]), "invalid bracket field " + field);
    }
    assert.ok(Number.isSafeInteger(b.requestId) && b.requestId >= 0);
    assert.ok(Number.isSafeInteger(b.session) && b.session >= 0);
    assert.ok(Number.isSafeInteger(b.lifecycle) && b.lifecycle >= 0);
    assert.ok(["resume", "seek", "seek_section", "restart_playback",
      "scene_control", "browser_pointer_input"].includes(b.requestType));
    assert.ok(b.sentMs <= b.receivedMs && b.receivedMs <= b.appliedMs,
      "same-Window readings must retain their causal order");
    assert.equal(b.session, anchor.session);
    assert.equal(b.authoredTime, anchor.time);
    assert.equal(b.sampledEpochMs, anchor.sampledEpochMs);
    assert.equal(b.receivedEpochMs, anchor.receivedEpochMs);
    assert.equal(b.observableAgeLowerMs, b.appliedMs - b.receivedMs);
    assert.equal(b.observableAgeUpperMs, b.appliedMs - b.sentMs);
    assert.ok(b.observableAgeLowerMs >= 0 && b.observableAgeUpperMs >= b.observableAgeLowerMs);
    // This is intentionally JUST the original numeric epoch comparison; it
    // remains separate from the interval and is never used for admission.
    assert.equal(b.reconstructedEpochAgeMs, b.receivedEpochMs - b.sampledEpochMs);
    observations.push({ session: b.session, requestId: b.requestId, type: b.requestType,
      observableLowerMs: b.observableAgeLowerMs,
      observableUpperMs: b.observableAgeUpperMs,
      reconstructedEpochAgeMs: b.reconstructedEpochAgeMs });
  }
  return { status: "passed", caseCount: 15, distinctStarts: found.size,
    observations, clockError: "unknown; observed Window interval is not a physical accuracy certificate" };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const filename = process.argv[2];
  if (!filename) throw new Error("usage: node verify_window_brackets.mjs report.json");
  const report = JSON.parse(readFileSync(filename, "utf8"));
  console.log(JSON.stringify(validateWindowBracketReport(report), null, 2));
}
