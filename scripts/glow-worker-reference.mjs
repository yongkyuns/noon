// Independent full 2D Gaussian and encoded-premultiplied painter reference.
// Input RGB and alpha masks are ordinary, NO-EFFECT extended-canvas renders.
// No production glow coefficients, intermediate masks, or output pixels enter it.
export function referenceGlow(images, sample, { width, height, padding }) {
  const [back, source, alpha, mask, front, frontAlpha] = images;
  const w = width + 2 * padding, h = height + 2 * padding;
  if (images.length !== 6 || images.some(im => im.width !== w || im.height !== h)) {
    throw new Error("wrong full-canvas reference dimensions");
  }
  const radius = Math.ceil(3 * sample.sigma);
  if (!(sample.sigma > 0) || radius + 2 > padding) throw new Error("reference padding is insufficient");
  const kernel = [], blurred = new Float64Array(w * h);
  let normalization = 0;
  for (let dy = -radius; dy <= radius; dy++) for (let dx = -radius; dx <= radius; dx++) {
    const value = Math.exp(-(dx * dx + dy * dy) / (2 * sample.sigma * sample.sigma));
    kernel.push([dx, dy, value]); normalization += value;
  }
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const a = mask.data[(y * w + x) * 4] / 255;
    if (a === 0) continue;
    for (const [dx, dy, weight] of kernel) {
      const xx = x + dx, yy = y + dy;
      if (xx >= 0 && yy >= 0 && xx < w && yy < h) blurred[yy * w + xx] += a * weight / normalization;
    }
  }
  const data = Buffer.alloc(width * height * 4), removed = Buffer.alloc(data.length);
  const ordinary = Buffer.alloc(data.length);
  const byte = x => Math.round(Math.min(255, Math.max(0, x)));
  let haloSignal = 0;
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const p = (y + padding) * w + x + padding, q = (y * width + x) * 4, i = p * 4;
    const a = alpha.data[i] / 255, fa = frontAlpha.data[i] / 255;
    const halo = Math.min(1, sample.intensity * sample.tint[3] * blurred[p]);
    const ea = byte(255 * sample.opacity * (a + (1 - a) * halo)) / 255;
    for (let c = 0; c < 3; c++) {
      const effect = byte(sample.opacity * (source.data[i + c] + 255 * (1 - a) * sample.tint[c] * halo));
      const layered = byte(effect + (1 - ea) * back.data[i + c]);
      data[q + c] = byte(front.data[i + c] + (1 - fa) * layered);
      removed[q + c] = byte(front.data[i + c] + (1 - fa) * back.data[i + c]);
      const normal = byte(sample.opacity * source.data[i + c] + (1 - sample.opacity * a) * back.data[i + c]);
      ordinary[q + c] = byte(front.data[i + c] + (1 - fa) * normal);
      haloSignal = Math.max(haloSignal, Math.abs(data[q + c] - ordinary[q + c]));
    }
    data[q + 3] = removed[q + 3] = ordinary[q + 3] = 255;
  }
  return { data, removed, ordinary, haloSignal, width, height };
}

export function maxChannelError(actual, expected) {
  if (actual.length !== expected.length || actual.length === 0) throw new Error("empty/mismatched image");
  let error = 0;
  for (let i = 0; i < actual.length; i++) error = Math.max(error, Math.abs(actual[i] - expected[i]));
  return error;
}
