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
        this.#entries.push({
          startTime: Number(entry.startTime),
          duration: Number(entry.duration),
        });
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
    const entries = this.#entries.filter(
      ({ startTime }) => startTime >= measurementStartMs && startTime <= measurementEndMs,
    );
    if (!this.#supported) {
      return { supported: false };
    }
    const durations = entries.map(({ duration }) => duration).filter(Number.isFinite);
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
