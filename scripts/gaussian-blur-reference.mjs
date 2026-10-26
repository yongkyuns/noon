// Independent, deliberately slow full 2-D convolution. No engine imports,
// separable kernel, GPU uniform or captured intermediate supplies this oracle.
import assert from "node:assert/strict";

export function referenceRgbaGaussian(bytes, width, height, sigma, opacity = 1) {
  assert.equal(bytes.length, width * height * 4);
  assert.ok(Number.isFinite(sigma) && sigma >= 0 && sigma <= 64);
  assert.ok(Number.isFinite(opacity) && opacity >= 0 && opacity <= 1);
  if (sigma === 0) return Array.from(bytes, byte => byte / 255 * opacity);
  const radius = Math.ceil(3 * sigma);
  const taps = [];
  let mass = 0;
  for (let dy = -radius; dy <= radius; dy++) for (let dx = -radius; dx <= radius; dx++) {
    const weight = Math.exp(-(dx * dx + dy * dy) / (2 * sigma * sigma));
    taps.push([dx, dy, weight]); mass += weight;
  }
  const result = new Array(bytes.length).fill(0);
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const input = (y * width + x) * 4;
    for (const [dx, dy, weight] of taps) {
      const xx = x + dx, yy = y + dy;
      if (xx < 0 || yy < 0 || xx >= width || yy >= height) continue;
      const output = (yy * width + xx) * 4;
      for (let channel = 0; channel < 4; channel++) {
        result[output + channel] += bytes[input + channel] / 255 * weight / mass * opacity;
      }
    }
  }
  return result;
}

// Direct full-2D point oracle for a maximum-radius large field, avoiding a
// quadratic all-pixels CPU run while still checking the entire finite support.
export function referenceRgbaGaussianAt(bytes, width, height, sigma, x, y, opacity = 1) {
  const radius = Math.ceil(3 * sigma), result = [0, 0, 0, 0];
  let mass = 0;
  for (let dy = -radius; dy <= radius; dy++) for (let dx = -radius; dx <= radius; dx++) {
    const weight = Math.exp(-(dx * dx + dy * dy) / (2 * sigma * sigma));
    mass += weight;
    const xx = x + dx, yy = y + dy;
    if (xx < 0 || yy < 0 || xx >= width || yy >= height) continue;
    for (let channel = 0; channel < 4; channel++) result[channel] += bytes[(yy * width + xx) * 4 + channel] / 255 * weight;
  }
  return result.map(value => value / mass * opacity);
}
