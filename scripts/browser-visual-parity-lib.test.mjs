import assert from "node:assert/strict";
import test from "node:test";

import {
  compareForegroundCoverage,
  foregroundMask,
} from "./browser-visual-parity-lib.mjs";

function image(width, height, foreground, colors = {}) {
  const background = colors.background ?? [20, 20, 20, 255];
  const ink = colors.ink ?? [220, 80, 60, 255];
  const data = new Uint8Array(width * height * 4);
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    data.set(background, pixel * 4);
  }
  for (const [x, y] of foreground) {
    data.set(ink, (y * width + x) * 4);
  }
  return { width, height, data };
}

function rectangle(minX, minY, maxX, maxY) {
  const pixels = [];
  for (let y = minY; y <= maxY; y += 1) {
    for (let x = minX; x <= maxX; x += 1) {
      pixels.push([x, y]);
    }
  }
  return pixels;
}

test("identical foreground coverage passes exactly", () => {
  const pixels = rectangle(2, 2, 5, 5);
  const result = compareForegroundCoverage(image(8, 8, pixels), image(8, 8, pixels));

  assert.equal(result.pass, true);
  assert.equal(result.unmatchedPixels, 0);
  assert.equal(result.mismatchFraction, 0);
  assert.equal(result.boundsDelta, 0);
});

test("one-pixel raster edge movement is absorbed by the neighborhood tolerance", () => {
  const left = image(12, 8, rectangle(2, 2, 5, 5));
  const right = image(12, 8, rectangle(3, 2, 6, 5));
  const result = compareForegroundCoverage(left, right, {
    neighborRadius: 1,
    maxMismatchFraction: 0,
    maxBoundsDelta: 1,
  });

  assert.equal(result.pass, true);
  assert.equal(result.unmatchedPixels, 0);
  assert.equal(result.boundsDelta, 1);
});

test("color-transfer differences do not masquerade as geometry differences", () => {
  const pixels = rectangle(2, 2, 6, 6);
  const left = image(10, 10, pixels, {
    background: [18, 18, 18, 255],
    ink: [235, 80, 65, 255],
  });
  const right = image(10, 10, pixels, {
    background: [24, 24, 24, 255],
    ink: [190, 104, 92, 255],
  });
  const result = compareForegroundCoverage(left, right);

  assert.equal(result.pass, true);
  assert.equal(result.mismatchFraction, 0);
});

test("explicit background preserves foreground that reaches the top-left canvas edge", () => {
  const background = [20, 20, 20, 255];
  const clipped = image(5, 4, [
    [0, 0],
    [1, 0],
    [0, 1],
  ], { background });

  const inferred = foregroundMask(clipped);
  assert.equal(
    inferred.count,
    17,
    "corner inference treats the clipped foreground color as the background",
  );

  const explicit = foregroundMask(clipped, 32, background);
  assert.equal(explicit.count, 3);
  assert.deepEqual([...explicit.mask.slice(0, 7)], [1, 1, 0, 0, 0, 1, 0]);

  const result = compareForegroundCoverage(clipped, clipped, {
    background,
    maxMismatchFraction: 0,
    maxBoundsDelta: 0,
  });
  assert.equal(result.pass, true);
  assert.equal(result.leftForegroundPixels, 3);
  assert.deepEqual(result.leftBounds, { minX: 0, minY: 0, maxX: 1, maxY: 1 });
});

test("explicit backgrounds reject malformed RGBA contracts", () => {
  const sample = image(4, 4, [[1, 1]]);
  assert.throws(
    () => compareForegroundCoverage(sample, sample, { background: [0, 0, 0] }),
    /RGBA array with four byte values/,
  );
  assert.throws(
    () => compareForegroundCoverage(sample, sample, { background: [0, 0, 0, 300] }),
    /integers within \[0, 255\]/,
  );
});

test("missing visible geometry fails the parity gate", () => {
  const shared = rectangle(1, 1, 4, 4);
  const missingBar = rectangle(7, 1, 8, 6);
  const left = image(10, 8, [...shared, ...missingBar]);
  const right = image(10, 8, shared);
  const result = compareForegroundCoverage(left, right, {
    neighborRadius: 1,
    maxMismatchFraction: 0.02,
    maxBoundsDelta: 2,
  });

  assert.equal(result.pass, false);
  assert.ok(result.mismatchFraction > 0.02);
  assert.ok(result.boundsDelta > 2);
});

test("dimension mismatch is rejected before comparison", () => {
  assert.throws(
    () => compareForegroundCoverage(image(4, 4, [[1, 1]]), image(5, 4, [[1, 1]])),
    /dimensions differ/,
  );
});
