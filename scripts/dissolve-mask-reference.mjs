// Specification-derived scalar seeded field. No engine state or shader helpers.
function lattice(x, y, seed) {
  let h = Math.imul(x, 0x9e3779b9) ^ Math.imul(y, 0x85ebca6b) ^ seed;
  h = Math.imul(h ^ (h >>> 16), 0x7feb352d);
  h = Math.imul(h ^ (h >>> 15), 0x846ca68b);
  return ((h ^ (h >>> 16)) >>> 8) / 16777216;
}
const mix = (a, b, t) => a * (1 - t) + b * t;
export function dissolveCoverage(x, y, { seed, softness, progress }) {
  if (progress <= 0) return 1;
  if (progress >= 1) return 0;
  const px = x * 16, py = y * 16, bx = Math.floor(px), by = Math.floor(py);
  const fx = px - bx, fy = py - by;
  const wx = fx * fx * (3 - 2 * fx), wy = fy * fy * (3 - 2 * fy);
  const noise = mix(mix(lattice(bx, by, seed), lattice(bx + 1, by, seed), wx),
    mix(lattice(bx, by + 1, seed), lattice(bx + 1, by + 1, seed), wx), wy);
  if (softness === 0) return noise >= progress ? 1 : 0;
  const t = Math.max(0, Math.min(1, (progress - noise) / softness + 0.5));
  return 1 - t * t * (3 - 2 * t);
}
