// A measurement oracle for the selected nonuniform gallery scenes, not a renderer
// or a general definition of visual usefulness. Ignore CSS decoration and alpha-zero
// bytes; require scene ink distinguishable from the dominant canvas background.
export function sceneInkPixels({ data, width, height }, margin = 8) {
  const colors = new Map();
  for (let y = margin; y < height - margin; y++) for (let x = margin; x < width - margin; x++) {
    const i = (y * width + x) * 4;
    if (data[i + 3] === 0) continue;
    const color = (data[i] << 16) | (data[i + 1] << 8) | data[i + 2];
    colors.set(color, (colors.get(color) ?? 0) + 1);
  }
  let background = 0, frequency = 0;
  for (const [color, count] of colors) if (count > frequency) { background = color; frequency = count; }
  const rgb = [background >> 16, (background >> 8) & 255, background & 255];
  let ink = 0;
  for (const [color, count] of colors) {
    if (Math.max(Math.abs((color >> 16) - rgb[0]), Math.abs(((color >> 8) & 255) - rgb[1]),
      Math.abs((color & 255) - rgb[2])) >= 8) ink += count;
  }
  return ink;
}

export function distribution(values) {
  const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
  const percentile = fraction => sorted.length ? sorted[Math.max(0, Math.ceil(sorted.length * fraction) - 1)] : null;
  return { n: sorted.length, min: sorted[0] ?? null, median: sorted.length ? (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2 : null, p90: percentile(0.9),
    p95: percentile(0.95), max: sorted.at(-1) ?? null, percentileMethod: "midpoint median; nearest-rank tails" };
}
