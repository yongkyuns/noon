import test from "node:test";
import assert from "node:assert/strict";
import { sceneInkPixels, distribution, summarizeLoading } from "./playground-prepared-loading-observation.mjs";

function canvas() {
  const data = new Uint8Array(40 * 40 * 4);
  for (let i = 0; i < data.length; i += 4) data.set([9, 12, 19, 255], i);
  return { data, width: 40, height: 40 };
}

test("blank canvas, CSS-like borders and transparent RGB do not prove useful scene ink", () => {
  const image = canvas();
  assert.equal(sceneInkPixels(image), 0);
  for (let y = 0; y < 40; y++) for (let x = 0; x < 40; x++) {
    if (x < 8 || y < 8 || x >= 32 || y >= 32) image.data.set([255, 255, 255, 255], (y * 40 + x) * 4);
  }
  image.data.set([255, 0, 255, 0], (20 * 40 + 20) * 4);
  assert.equal(sceneInkPixels(image), 0);
});

test("visible interior content qualifies while imperceptible byte noise does not", () => {
  const image = canvas();
  image.data.set([10, 12, 19, 255], (20 * 40 + 20) * 4);
  assert.equal(sceneInkPixels(image), 0);
  for (let y = 12; y < 25; y++) for (let x = 12; x < 25; x++) {
    image.data.set([60, 150, 255, 255], (y * 40 + x) * 4);
  }
  assert.equal(sceneInkPixels(image), 169);
});

test("timing summaries retain sample count and nearest-rank tails", () => {
  assert.deepEqual(distribution([9, 1, 5, 3]), {
    n: 4, min: 1, median: 4, p90: 9, p95: 9, max: 9, percentileMethod: "midpoint median; nearest-rank tails",
  });
  assert.equal(distribution([]).median, null);
});

test("failed runs retain counts without contributing initial pictures to successful timings", () => {
  const common = { example: "scene", phase: "fresh-process", warmup: false, trial: 0,
    sourceToUsefulObservationUpperMs: 20, sourceToFirstAnyPresentedMs: 10,
    memory: { peakSampledRssBytes: 100 } };
  const summaries = summarizeLoading([
    { ...common, arm: "live", status: "observed" },
    { ...common, arm: "prepared", status: "failed" },
    { ...common, arm: "prepared", status: "observed", warmup: true },
  ], ["scene"], 1);
  assert.equal(summaries[0].live.usefulUpperMs.n, 1);
  assert.equal(summaries[0].prepared.expected, 1);
  assert.equal(summaries[0].prepared.requested, 1);
  assert.equal(summaries[0].prepared.observed, 0);
  for (const metric of ["usefulUpperMs", "firstAnyMs", "peakSampledRssBytes"]) {
    assert.equal(summaries[0].prepared[metric].n, 0);
  }
  assert.equal(summaries[0].pairedPreparedMinusLiveMs.n, 0);
  assert.equal(summaries[1].prepared.expected, 1);
  assert.equal(summaries[1].prepared.requested, 0);
});
