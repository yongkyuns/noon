function assertImage(image, name) {
  if (!image || !Number.isInteger(image.width) || !Number.isInteger(image.height)) {
    throw new TypeError(`${name} must provide integer width and height`);
  }
  if (image.width <= 0 || image.height <= 0) {
    throw new RangeError(`${name} dimensions must be positive`);
  }
  if (!image.data || image.data.length !== image.width * image.height * 4) {
    throw new RangeError(`${name} must provide exactly width * height * 4 RGBA bytes`);
  }
}

function normalizedBackground(background) {
  if (background === undefined || background === null) {
    return null;
  }
  if (!Array.isArray(background) || background.length !== 4) {
    throw new RangeError("background must be an RGBA array with four byte values");
  }
  if (!background.every((value) => Number.isInteger(value) && value >= 0 && value <= 255)) {
    throw new RangeError("background RGBA values must be integers within [0, 255]");
  }
  return [...background];
}

function colorDistance(data, offset, background) {
  return (
    Math.abs(data[offset] - background[0]) +
    Math.abs(data[offset + 1] - background[1]) +
    Math.abs(data[offset + 2] - background[2]) +
    Math.abs(data[offset + 3] - background[3])
  );
}

export function foregroundMask(image, backgroundDistance = 32, background = null) {
  assertImage(image, "image");
  if (!Number.isFinite(backgroundDistance) || backgroundDistance < 0) {
    throw new RangeError("backgroundDistance must be a non-negative finite number");
  }

  const explicitBackground = normalizedBackground(background);
  const resolvedBackground =
    explicitBackground ?? [image.data[0], image.data[1], image.data[2], image.data[3]];
  const mask = new Uint8Array(image.width * image.height);
  let count = 0;
  for (let pixel = 0; pixel < mask.length; pixel += 1) {
    if (colorDistance(image.data, pixel * 4, resolvedBackground) >= backgroundDistance) {
      mask[pixel] = 1;
      count += 1;
    }
  }
  return { mask, count };
}

export function foregroundBounds(mask, width, height) {
  if (!(mask instanceof Uint8Array) || mask.length !== width * height) {
    throw new RangeError("foreground mask dimensions do not match image dimensions");
  }

  let minX = width;
  let minY = height;
  let maxX = -1;
  let maxY = -1;
  for (let pixel = 0; pixel < mask.length; pixel += 1) {
    if (mask[pixel] === 0) {
      continue;
    }
    const x = pixel % width;
    const y = Math.floor(pixel / width);
    minX = Math.min(minX, x);
    minY = Math.min(minY, y);
    maxX = Math.max(maxX, x);
    maxY = Math.max(maxY, y);
  }
  return maxX < 0 ? null : { minX, minY, maxX, maxY };
}

function hasForegroundNear(mask, width, height, x, y, radius) {
  const minX = Math.max(0, x - radius);
  const maxX = Math.min(width - 1, x + radius);
  const minY = Math.max(0, y - radius);
  const maxY = Math.min(height - 1, y + radius);
  for (let candidateY = minY; candidateY <= maxY; candidateY += 1) {
    const row = candidateY * width;
    for (let candidateX = minX; candidateX <= maxX; candidateX += 1) {
      if (mask[row + candidateX] !== 0) {
        return true;
      }
    }
  }
  return false;
}

function unmatchedForegroundMask(source, target, width, height, radius) {
  const unmatched = new Uint8Array(source.length);
  let count = 0;
  for (let pixel = 0; pixel < source.length; pixel += 1) {
    if (source[pixel] === 0) {
      continue;
    }
    const x = pixel % width;
    const y = Math.floor(pixel / width);
    if (!hasForegroundNear(target, width, height, x, y, radius)) {
      unmatched[pixel] = 1;
      count += 1;
    }
  }
  return { mask: unmatched, count };
}

function maxBoundsDelta(left, right) {
  if (left === null || right === null) {
    return left === right ? 0 : Number.POSITIVE_INFINITY;
  }
  return Math.max(
    Math.abs(left.minX - right.minX),
    Math.abs(left.minY - right.minY),
    Math.abs(left.maxX - right.maxX),
    Math.abs(left.maxY - right.maxY),
  );
}

function normalizedOptions(options = {}) {
  const backgroundDistance = options.backgroundDistance ?? 32;
  const neighborRadius = options.neighborRadius ?? 1;
  const maxMismatchFraction = options.maxMismatchFraction ?? 0.02;
  const maxBoundsDelta = options.maxBoundsDelta ?? 2;
  const background = normalizedBackground(options.background);

  if (!Number.isFinite(backgroundDistance) || backgroundDistance < 0) {
    throw new RangeError("backgroundDistance must be a non-negative finite number");
  }
  if (!Number.isInteger(neighborRadius) || neighborRadius < 0) {
    throw new RangeError("neighborRadius must be a non-negative integer");
  }
  if (!Number.isFinite(maxMismatchFraction) || maxMismatchFraction < 0 || maxMismatchFraction > 1) {
    throw new RangeError("maxMismatchFraction must be within [0, 1]");
  }
  if (!Number.isFinite(maxBoundsDelta) || maxBoundsDelta < 0) {
    throw new RangeError("maxBoundsDelta must be a non-negative finite number");
  }
  return { backgroundDistance, neighborRadius, maxMismatchFraction, maxBoundsDelta, background };
}

export function compareForegroundCoverage(left, right, options = {}) {
  assertImage(left, "left image");
  assertImage(right, "right image");
  if (left.width !== right.width || left.height !== right.height) {
    throw new RangeError(
      `visual parity dimensions differ: ${left.width}x${left.height} vs ${right.width}x${right.height}`,
    );
  }

  const normalized = normalizedOptions(options);
  const leftForeground = foregroundMask(
    left,
    normalized.backgroundDistance,
    normalized.background,
  );
  const rightForeground = foregroundMask(
    right,
    normalized.backgroundDistance,
    normalized.background,
  );
  const leftUnmatched = unmatchedForegroundMask(
    leftForeground.mask,
    rightForeground.mask,
    left.width,
    left.height,
    normalized.neighborRadius,
  );
  const rightUnmatched = unmatchedForegroundMask(
    rightForeground.mask,
    leftForeground.mask,
    left.width,
    left.height,
    normalized.neighborRadius,
  );
  const foregroundMass = leftForeground.count + rightForeground.count;
  const unmatchedPixels = leftUnmatched.count + rightUnmatched.count;
  const mismatchFraction = foregroundMass === 0 ? 0 : unmatchedPixels / foregroundMass;
  const leftBounds = foregroundBounds(leftForeground.mask, left.width, left.height);
  const rightBounds = foregroundBounds(rightForeground.mask, right.width, right.height);
  const boundsDelta = maxBoundsDelta(leftBounds, rightBounds);
  const hasVisibleContent = leftForeground.count > 0 && rightForeground.count > 0;

  return {
    width: left.width,
    height: left.height,
    leftForegroundPixels: leftForeground.count,
    rightForegroundPixels: rightForeground.count,
    unmatchedPixels,
    mismatchFraction,
    leftBounds,
    rightBounds,
    boundsDelta,
    hasVisibleContent,
    pass:
      hasVisibleContent &&
      mismatchFraction <= normalized.maxMismatchFraction &&
      boundsDelta <= normalized.maxBoundsDelta,
    tolerances: normalized,
  };
}

export function foregroundMismatchMask(left, right, options = {}) {
  assertImage(left, "left image");
  assertImage(right, "right image");
  if (left.width !== right.width || left.height !== right.height) {
    throw new RangeError("cannot build a mismatch mask for images with different dimensions");
  }

  const normalized = normalizedOptions(options);
  const leftForeground = foregroundMask(
    left,
    normalized.backgroundDistance,
    normalized.background,
  );
  const rightForeground = foregroundMask(
    right,
    normalized.backgroundDistance,
    normalized.background,
  );
  const leftUnmatched = unmatchedForegroundMask(
    leftForeground.mask,
    rightForeground.mask,
    left.width,
    left.height,
    normalized.neighborRadius,
  );
  const rightUnmatched = unmatchedForegroundMask(
    rightForeground.mask,
    leftForeground.mask,
    left.width,
    left.height,
    normalized.neighborRadius,
  );
  const mask = new Uint8Array(left.width * left.height);
  for (let pixel = 0; pixel < mask.length; pixel += 1) {
    mask[pixel] = leftUnmatched.mask[pixel] || rightUnmatched.mask[pixel] ? 1 : 0;
  }
  return mask;
}
