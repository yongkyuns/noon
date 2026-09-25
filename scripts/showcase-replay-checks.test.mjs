import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import { replayOracle, assertOracleImage, assertReplaySample } from "./showcase-replay-checks.mjs";
import { captureSchedule } from "./showcase-capture-checks.mjs";

const hash = bytes => createHash("sha256").update(bytes).digest("hex");
function fixture() {
  const entry = { id: "example", duration: 2, thumbnail_time: 0.5,
    beats: [{ time: 1.5 }, { time: 2 }], still_intervals: [] };
  const expected = { buildIdentity: { schema: 1, sourceRevision: "a".repeat(40), buildId: "b".repeat(64) },
    browserVersion: "test-browser", backendRequested: "webgpu", sourceSha256: "c".repeat(64) };
  const bytes = Buffer.from("separately captured PNG bytes");
  const image = { width: 960, height: 540, data: Buffer.alloc(960 * 540 * 4, 127) };
  const { wanted, completionTime } = captureSchedule(entry);
  const samples = wanted.map(time => ({
    requestedTime: time, publishedTime: time === completionTime ? entry.duration : time,
    completionProbe: time === completionTime, error: null, presented: true,
    rendererBackend: "WebGPU", objectCount: 4, width: image.width, height: image.height,
    pngSha256: hash(bytes), pixelSha256: hash(image.data),
    filename: `${entry.id}-${String(time).replace(".", "_")}.png`,
    ...(time === completionTime ? { sourceCompleted: true, sourceState: "completed", authoredDuration: entry.duration } : {}),
  }));
  const report = { servedBuildIdentity: structuredClone(expected.buildIdentity),
    checkoutRevision: expected.buildIdentity.sourceRevision, browserVersion: expected.browserVersion,
    backendRequested: expected.backendRequested, samplingHz: 30, viewport: { width: 960, height: 540 },
    results: [{ id: entry.id, sourceSha256: expected.sourceSha256, outcome: "pass", pageErrors: [], samples }] };
  return { entry, expected, report, bytes, image };
}

test("every independent storyboard sample is retained and the completion bound is not relabeled", () => {
  const { entry, expected, report } = fixture();
  const before = structuredClone(report);
  const oracle = replayOracle(entry, report, expected);
  assert.deepEqual(oracle.map(sample => sample.replayTime), [0.5, 1.5, 2]);
  assert.ok(oracle.at(-1).requestedTime > oracle.at(-1).replayTime);
  assert.deepEqual(report, before);
});

test("a passing endpoint cannot replace missing, duplicate, reordered or failed intermediate evidence", () => {
  for (const mutate of [
    first => { first.samples = first.samples.slice(-1); },
    first => { first.samples.splice(1, 1); },
    first => { first.samples.push(first.samples[0]); },
    first => { first.samples.reverse(); },
    first => { first.outcome = "fail"; },
    first => { first.pageErrors.push("render error"); },
    first => { first.samples[0].presented = false; },
    first => { first.samples[0].error = "sample failed"; },
    first => { first.samples.at(-1).sourceCompleted = false; },
  ]) {
    const { entry, expected, report } = fixture();
    mutate(report.results[0]);
    assert.throws(() => replayOracle(entry, report, expected));
  }
});

test("source, runtime, checkout, browser, backend, cadence and viewport identities must match", () => {
  for (const mutate of [
    report => { report.results[0].sourceSha256 = "d".repeat(64); },
    report => { report.servedBuildIdentity.buildId = "d".repeat(64); },
    report => { report.checkoutRevision = "d".repeat(40); },
    report => { report.browserVersion = "another-browser"; },
    report => { report.backendRequested = "webgl"; },
    report => { report.results[0].samples[0].rendererBackend = "WebGL2"; },
    report => { report.samplingHz = 1; },
    report => { report.viewport.height = 541; },
    report => { report.results[0].samples[0].height = 541; },
    report => { report.results = []; },
    report => { report.results.push(structuredClone(report.results[0])); },
  ]) {
    const { entry, expected, report } = fixture();
    mutate(report);
    assert.throws(() => replayOracle(entry, report, expected));
  }
  const { entry, expected } = fixture();
  assert.throws(() => replayOracle(entry, undefined, expected));
});

test("stale active samples and unbound file paths or hashes cannot become an oracle", () => {
  for (const mutate of [
    sample => { sample.publishedTime = 0; },
    sample => { sample.publishedTime = 0.6; },
    sample => { sample.publishedTime = "0.5"; },
    sample => { sample.filename = "../replay.png"; },
    sample => { sample.filename = "another-scene-0_5.png"; },
    sample => { sample.pngSha256 = "missing"; },
    sample => { sample.pixelSha256 = undefined; },
    sample => { sample.completionProbe = true; },
  ]) {
    const { entry, expected, report } = fixture();
    mutate(report.results[0].samples[0]);
    assert.throws(() => replayOracle(entry, report, expected));
  }
});

test("declared quiet intervals preserve the actual first-pass publication time", () => {
  const { entry, expected, report } = fixture();
  entry.still_intervals = [[0.25, 0.75]];
  report.results[0].samples[0].publishedTime = 0.25;
  const [sample] = replayOracle(entry, report, expected);
  assert.equal(sample.publishedTime, 0.25);
  assert.equal(sample.requestedTime, 0.5);
  assert.equal(sample.replayTime, 0.5);
});

test("the retained PNG bytes and every decoded RGBA channel are hash-bound", () => {
  const { entry, expected, report, bytes, image } = fixture();
  const [sample] = replayOracle(entry, report, expected);
  assertOracleImage(sample, bytes, image);
  assert.throws(() => assertOracleImage(sample, Buffer.from("different PNG"), image));
  for (const channel of [0, 1, 2, 3]) {
    const changed = { ...image, data: Buffer.from(image.data) };
    changed.data[channel] ^= 1;
    assert.throws(() => assertOracleImage(sample, bytes, changed));
  }
  assert.throws(() => assertOracleImage(sample, bytes, { ...image, width: 961 }));
  assert.throws(() => assertOracleImage(sample, bytes, { ...image, data: image.data.subarray(4) }));
});

test("replay requires the exact checkpoint and backend, not stale pixels at a matching endpoint", () => {
  const { entry, expected, report } = fixture();
  const [sample] = replayOracle(entry, report, expected);
  const metrics = { time: 0.5, backend: "WebGPU", objectCount: 4 };
  assertReplaySample(entry, sample, 0.5, metrics);
  assert.throws(() => assertReplaySample(entry, sample, 2, { ...metrics, time: 2 }));
  for (const time of [undefined, null, "0.5", NaN, Infinity, 0.49, 0.51, 0.50000001]) {
    assert.throws(() => assertReplaySample(entry, sample, 0.5, { ...metrics, time }));
  }
  assert.throws(() => assertReplaySample(entry, sample, 0.5, { ...metrics, backend: "WebGL2" }));
});

test("dense-scene evidence and replay cannot silently reduce the workload", () => {
  const { entry, expected, report } = fixture();
  entry.performance = true;
  entry.duration = 4;
  const end = entry.duration + 1e-9;
  Object.assign(report.results[0].samples.at(-1), {
    requestedTime: end, publishedTime: 4, authoredDuration: 4,
    filename: `example-${String(end).replace(".", "_")}.png`,
  });
  entry.beats.at(-1).time = 4;
  assert.throws(() => replayOracle(entry, report, expected), /workload/);
  report.results[0].samples.at(-1).objectCount = 600;
  const sample = replayOracle(entry, report, expected).at(-1);
  assert.throws(() => assertReplaySample(entry, sample, 4, { time: 4, backend: "WebGPU", objectCount: 599 }), /workload/);
  assertReplaySample(entry, sample, 4, { time: 4, backend: "WebGPU", objectCount: 600 });
});
