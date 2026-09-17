import assert from "node:assert/strict";

// Acceptance limits, not a baseline fitted to current Noon output. Compare only
// tip-sized regions; a large black frame must never dilute an arrowhead defect.
export const TIP_LIMITS = Object.freeze({
  relativeInkError: 0.15,
  normalizedL1: 0.22,
  minimumPartialCoverageRatio: 0.40,
});

export function compareTipRoi(expected, actual, roi) {
  assert.equal(actual.width, expected.width, "raster width");
  assert.equal(actual.height, expected.height, "raster height");
  for (const image of [expected, actual]) {
    assert.equal(image.data.length, image.width * image.height * 4, "RGBA byte count");
  }
  const { x, y, width, height } = roi;
  assert.ok([x, y, width, height].every(Number.isSafeInteger), "integer ROI");
  assert.ok(x >= 0 && y >= 0 && width > 0 && height > 0
    && x + width <= expected.width && y + height <= expected.height, "bounded ROI");
  let referenceInk = 0;
  let actualInk = 0;
  let absoluteError = 0;
  let referencePartial = 0;
  let actualPartial = 0;
  for (let row = y; row < y + height; row += 1) {
    for (let col = x; col < x + width; col += 1) {
      const offset = (row * expected.width + col) * 4;
      assert.equal(expected.data[offset + 3], 255, "opaque reference background");
      assert.equal(actual.data[offset + 3], 255, "opaque actual background");
      const luminance = (image) => (image.data[offset] + image.data[offset + 1]
        + image.data[offset + 2]) / (3 * 255);
      const a = luminance(expected);
      const b = luminance(actual);
      referenceInk += a;
      actualInk += b;
      absoluteError += Math.abs(a - b);
      if (a > 0.04 && a < 0.96) referencePartial += 1;
      if (b > 0.04 && b < 0.96) actualPartial += 1;
    }
  }
  // A missing/empty oracle is a harness failure, never a successful comparison.
  assert.ok(referenceInk >= 1, "reference tip ROI contains visible ink");
  assert.ok(referencePartial >= 2, "reference tip ROI exercises edge antialiasing");
  return {
    referenceInk, actualInk, referencePartial, actualPartial,
    relativeInkError: Math.abs(actualInk - referenceInk) / referenceInk,
    normalizedL1: absoluteError / referenceInk,
    partialCoverageRatio: actualPartial / referencePartial,
  };
}

export function enforceTipMetrics(metrics, label) {
  for (const value of Object.values(metrics)) assert.ok(Number.isFinite(value), `${label}: finite metric`);
  assert.ok(metrics.relativeInkError <= TIP_LIMITS.relativeInkError,
    `${label}: tip ink/area error ${metrics.relativeInkError}`);
  assert.ok(metrics.normalizedL1 <= TIP_LIMITS.normalizedL1,
    `${label}: tip-local pixel error ${metrics.normalizedL1}`);
  assert.ok(metrics.partialCoverageRatio >= TIP_LIMITS.minimumPartialCoverageRatio,
    `${label}: antialiased edge coverage lost (${metrics.partialCoverageRatio})`);
}
