// This gate reads images captured by Noon at three timestamps INSIDE the
// first reflected segment, not across separate wall contacts. It prevents
// an instantaneous ray reveal from passing solely because later scenes differ.
// These screenshots are from the actual renderer, never generated here.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const FIRST_FLIGHT_SAMPLES = [1.1, 1.5, 1.9];

export function assertVisibleFirstFlight(report) {
  assert.deepEqual(report.results?.map(row => row.id), ["showcase-triangle-light"]);
  const result = report.results[0];
  assert.equal(result.outcome, "pass", "actual Noon capture did not pass");
  assert.deepEqual(result.pageErrors, [], "browser page errors prevent motion review");
  const samples = FIRST_FLIGHT_SAMPLES.map(time => {
    const matching = result.samples.filter(sample => sample.requestedTime === time);
    assert.equal(matching.length, 1, `missing/duplicate first-flight sample at ${time}s`);
    const sample = matching[0];
    assert.equal(sample.presented, true, `no presented frame at ${time}s`);
    assert.equal(sample.error, null, `render error at ${time}s`);
    assert.equal(sample.publishedTime, time, `incorrect presentation time at ${time}s`);
    assert.ok(/^[a-f0-9]{64}$/.test(sample.pixelSha256), `missing decoded-pixel identity at ${time}s`);
    return sample;
  });
  assert.equal(new Set(samples.map(sample => sample.objectCount)).size, 1,
    "first-flight samples are not within a single uninterrupted bounce");
  assert.equal(new Set(samples.map(sample => sample.pixelSha256)).size, samples.length,
    "the light did not visibly progress between interior timestamps of the first bounce");
  return { times: FIRST_FLIGHT_SAMPLES, objectCount: samples[0].objectCount,
    hashes: samples.map(sample => sample.pixelSha256) };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const dir = process.argv[2] ?? path.join(root, "browser-smoke-artifacts/triangle-light/webgl");
  const report = JSON.parse(await readFile(path.join(dir, "report.json"), "utf8"));
  const result = assertVisibleFirstFlight(report);
  console.log(`PASS first flight: ${result.times.join(", ")} s, ${result.hashes.length} distinct actual pixel frames`);
}
