function clipEntryToWindow(entry, measurementStartMs, measurementEndMs) {
  const startTime = Number(entry.startTime);
  const duration = Number(entry.duration);
  if (!Number.isFinite(startTime) || !Number.isFinite(duration) || duration <= 0) {
    return null;
  }

  const endTime = startTime + duration;
  const clippedStart = Math.max(startTime, measurementStartMs);
  const clippedEnd = Math.min(endTime, measurementEndMs);
  if (!(clippedEnd > clippedStart)) {
    return null;
  }

  return {
    startTime: clippedStart,
    duration: clippedEnd - clippedStart,
  };
}

export class BrowserJankMonitor {
  #entries = [];
  #observer = null;
  #supported = false;

  constructor(PerformanceObserverClass = globalThis.PerformanceObserver) {
    if (typeof PerformanceObserverClass !== "function") {
      return;
    }
    const supported = PerformanceObserverClass.supportedEntryTypes;
    if (!Array.isArray(supported) || !supported.includes("longtask")) {
      return;
    }
    this.#observer = new PerformanceObserverClass((list) => {
      for (const entry of list.getEntries()) {
        const startTime = Number(entry.startTime);
        const duration = Number(entry.duration);
        if (!Number.isFinite(startTime) || !Number.isFinite(duration) || duration <= 0) {
          continue;
        }
        this.#entries.push({ startTime, duration });
      }
    });
    this.#supported = true;
  }

  start() {
    this.reset();
    if (!this.#observer) {
      return false;
    }
    this.#observer.observe({ type: "longtask", buffered: false });
    return true;
  }

  stop() {
    this.#observer?.disconnect();
  }

  reset() {
    this.#entries.length = 0;
  }

  summary(measurementStartMs = -Infinity, measurementEndMs = Infinity) {
    if (!this.#supported) {
      return { supported: false };
    }
    if (!(measurementEndMs >= measurementStartMs)) {
      throw new RangeError("measurementEndMs must be greater than or equal to measurementStartMs");
    }

    const entries = this.#entries
      .map((entry) => clipEntryToWindow(entry, measurementStartMs, measurementEndMs))
      .filter((entry) => entry !== null);
    const durations = entries.map(({ duration }) => duration);
    return {
      supported: true,
      count: durations.length,
      totalMs: durations.reduce((sum, duration) => sum + duration, 0),
      maxMs: durations.length > 0 ? Math.max(...durations) : 0,
      entries,
    };
  }
}

export function estimateUnattributedFrameMs(frameIntervalMs, engineCpuMs) {
  if (!Number.isFinite(frameIntervalMs) || !Number.isFinite(engineCpuMs)) {
    return null;
  }
  return Math.max(0, frameIntervalMs - Math.max(0, engineCpuMs));
}
