import assert from "node:assert/strict";
import test from "node:test";
import { FIRST_FLIGHT_SAMPLES, assertVisibleFirstFlight } from "./showcase-triangle-light-motion.mjs";

const aHash = char => char.repeat(64);
function report() {
  return { results: [{ id: "showcase-triangle-light", outcome: "pass", pageErrors: [], samples:
    FIRST_FLIGHT_SAMPLES.map((time, index) => ({ requestedTime: time, publishedTime: time,
      presented: true, error: null, objectCount: 360, pixelSha256: aHash("abcdef"[index]) })) }] };
}

test("requires three different actual frames inside one bounce", () => {
  const result = assertVisibleFirstFlight(report());
  assert.deepEqual(result.times, [1.1, 1.5, 1.9]);
  assert.equal(result.hashes.length, 3);
});

test("rejects instantaneous full-line reveal despite three successful captures", () => {
  const all = report(); all.results[0].samples[1].pixelSha256 = all.results[0].samples[0].pixelSha256;
  assert.throws(() => assertVisibleFirstFlight(all), /did not visibly progress/);
});

test("cannot relabel bounce endpoints as continuous movement", () => {
  const all = report(); all.results[0].samples[2].objectCount++;
  assert.throws(() => assertVisibleFirstFlight(all), /not within a single/);
});

test("missing, stale or malformed actual samples remain fatal", () => {
  for (const change of [
    all => { all.results[0].samples.pop(); },
    all => { all.results[0].samples[1].publishedTime = 1.6; },
    all => { all.results[0].samples[1].pixelSha256 = "bad"; },
    all => { all.results[0].samples[1].presented = false; },
    all => { all.results[0].pageErrors = ["error"]; },
  ]) {
    const all = report(); change(all);
    assert.throws(() => assertVisibleFirstFlight(all));
  }
});
