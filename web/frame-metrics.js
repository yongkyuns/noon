export class FrameMetrics {
  #lastTimestamp = null;
  #submissionMs = [];
  #intervalMs = [];

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
    };
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
    p50: percentile(sorted, 0.5),
    p95: percentile(sorted, 0.95),
    max: sorted.at(-1),
    mean: sorted.reduce((sum, value) => sum + value, 0) / sorted.length,
  };
}

function percentile(sorted, value) {
  const rank = Math.ceil(value * sorted.length);
  return sorted[Math.max(0, Math.min(sorted.length - 1, rank - 1))];
}
