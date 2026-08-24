export class FrameMetrics {
  #lastTimestamp = null;
  #submissionMs = [];
  #intervalMs = [];
  #targetHz;

  constructor({ targetHz = 60 } = {}) {
    if (!Number.isFinite(targetHz) || targetHz <= 0) {
      throw new RangeError("targetHz must be a positive finite number");
    }
    this.#targetHz = targetHz;
  }

  reset() {
    this.#lastTimestamp = null;
    this.#submissionMs.length = 0;
    this.#intervalMs.length = 0;
  }

  record(timestampMs, submissionMs) {
    if (!Number.isFinite(timestampMs) || !Number.isFinite(submissionMs)) {
      throw new TypeError("frame metrics require finite timestamps");
    }
    if (this.#lastTimestamp !== null) {
      this.#intervalMs.push(timestampMs - this.#lastTimestamp);
    }
    this.#lastTimestamp = timestampMs;
    this.#submissionMs.push(submissionMs);
  }

  summary() {
    return {
      frames: this.#submissionMs.length,
      submission: summarizeSamples(this.#submissionMs),
      interval: summarizeSamples(this.#intervalMs),
      cadence: summarizeCadence(this.#intervalMs, this.#targetHz),
    };
  }
}

export class SampleWindow {
  #capacity;
  #samples = [];

  constructor(capacity = 180) {
    if (!Number.isSafeInteger(capacity) || capacity <= 0) {
      throw new RangeError("sample window capacity must be a positive integer");
    }
    this.#capacity = capacity;
  }

  record(value) {
    if (!Number.isFinite(value)) {
      throw new TypeError("sample window requires finite values");
    }
    this.#samples.push(value);
    if (this.#samples.length > this.#capacity) {
      this.#samples.splice(0, this.#samples.length - this.#capacity);
    }
  }

  reset() {
    this.#samples.length = 0;
  }

  summary() {
    return summarizeSamples(this.#samples);
  }

  get size() {
    return this.#samples.length;
  }
}

export function summarizeSamples(samples) {
  if (!Array.isArray(samples) || samples.length === 0) {
    return null;
  }
  const sorted = samples.map(Number).filter(Number.isFinite).sort((a, b) => a - b);
  if (sorted.length === 0) {
    return null;
  }
  return {
    min: sorted[0],
    p50: percentile(sorted, 0.5),
    p95: percentile(sorted, 0.95),
    p99: percentile(sorted, 0.99),
    max: sorted.at(-1),
    mean: sorted.reduce((sum, value) => sum + value, 0) / sorted.length,
  };
}

export function summarizeCadence(samples, targetHz = 60) {
  if (!Number.isFinite(targetHz) || targetHz <= 0) {
    throw new RangeError("targetHz must be a positive finite number");
  }
  const intervals = Array.isArray(samples)
    ? samples.map(Number).filter((value) => Number.isFinite(value) && value >= 0)
    : [];
  if (intervals.length === 0) {
    return null;
  }

  const targetFrameMs = 1000 / targetHz;
  const totalMs = intervals.reduce((sum, value) => sum + value, 0);
  const longFrameThresholdMs = targetFrameMs * 1.5;
  const veryLongFrameThresholdMs = targetFrameMs * 2.5;
  const longFrames = intervals.filter((value) => value >= longFrameThresholdMs).length;
  const veryLongFrames = intervals.filter((value) => value >= veryLongFrameThresholdMs).length;
  const missedVsyncs = intervals.reduce(
    (sum, value) => sum + Math.max(0, Math.round(value / targetFrameMs) - 1),
    0,
  );

  return {
    targetHz,
    targetFrameMs,
    effectiveFps: totalMs > 0 ? (intervals.length * 1000) / totalMs : null,
    longFrameThresholdMs,
    longFrames,
    longFrameRate: longFrames / intervals.length,
    veryLongFrameThresholdMs,
    veryLongFrames,
    missedVsyncs,
  };
}

function percentile(sorted, value) {
  const rank = Math.ceil(value * sorted.length);
  return sorted[Math.max(0, Math.min(sorted.length - 1, rank - 1))];
}
