// Independent specification oracle: enumerate and clip every stripe interval.
// This deliberately does not use the production constant-work period split,
// shader code, Rust sampling, packed uniforms, or generated reference pixels.
export function bandAverage(center, footprint, duty) {
  if (![center, footprint, duty].every(Number.isFinite) || footprint < 0 || duty < 0 || duty > 1) {
    throw new RangeError("invalid periodic band sample");
  }
  if (duty === 0 || duty === 1 || footprint >= 256) return duty;
  const q = ((center % 1) + 1) % 1;
  if (footprint === 0) return ((q + duty / 2) % 1) < duty ? 1 : 0;
  const lo = q - footprint / 2, hi = q + footprint / 2;
  let covered = 0;
  for (let line = Math.floor(lo) - 1; line <= Math.ceil(hi) + 1; line++) {
    covered += Math.max(0, Math.min(hi, line + duty / 2) - Math.max(lo, line - duty / 2));
  }
  return Math.max(0, Math.min(1, covered / footprint));
}

export function filteredHatch(local, options, footprint) {
  const { spacing, width, angle, phase, cross } = options;
  const c = Math.cos(angle), s = Math.sin(angle), duty = width / spacing;
  const periodic = phase.map(value => ((value % 1) + 1) % 1);
  const a = bandAverage((-s * local[0] + c * local[1]) / spacing + periodic[0], footprint[0], duty);
  if (!cross) return a;
  const b = bandAverage((c * local[0] + s * local[1]) / spacing + periodic[1], footprint[1], duty);
  return a + b - a * b;
}

export function hatchFootprint(options, pose, pixelsPerUnit) {
  const c = Math.cos(pose.angle), s = Math.sin(pose.angle);
  const dx = [c / pose.scale[0] / pixelsPerUnit, -s / pose.scale[1] / pixelsPerUnit];
  const dy = [s / pose.scale[0] / pixelsPerUnit, c / pose.scale[1] / pixelsPerUnit];
  const p = Math.cos(options.angle), q = Math.sin(options.angle);
  return [[-q, p], [p, q]].map(normal => (
    Math.abs(normal[0] * dx[0] + normal[1] * dx[1]) +
    Math.abs(normal[0] * dy[0] + normal[1] * dy[1])
  ) / options.spacing);
}

export function localFromPixel(x, y, width, height, pose) {
  const pixelsPerUnit = height / 8;
  const delta = [(x + 0.5 - width / 2) / pixelsPerUnit - pose.translation[0],
    (height / 2 - y - 0.5) / pixelsPerUnit - pose.translation[1]];
  const c = Math.cos(pose.angle), s = Math.sin(pose.angle);
  return [(c * delta[0] + s * delta[1]) / pose.scale[0],
    (-s * delta[0] + c * delta[1]) / pose.scale[1]];
}
