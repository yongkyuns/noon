export function coldStartMilestones(timestamps) {
  const required = [
    "navigationStart",
    "pageReady",
    "runRequested",
    "runtimeStarted",
    "firstMetrics",
  ];
  for (const name of required) {
    const value = timestamps?.[name];
    if (!Number.isFinite(value) || value < 0) {
      throw new TypeError(`cold-start milestone ${name} must be a finite non-negative number`);
    }
  }
  for (let index = 1; index < required.length; index += 1) {
    const previous = required[index - 1];
    const current = required[index];
    if (timestamps[current] < timestamps[previous]) {
      throw new RangeError(`cold-start milestone ${current} precedes ${previous}`);
    }
  }
  return Object.freeze({
    pageReadyMs: timestamps.pageReady - timestamps.navigationStart,
    runToRuntimeMs: timestamps.runtimeStarted - timestamps.runRequested,
    runToFirstMetricsMs: timestamps.firstMetrics - timestamps.runRequested,
    runtimeToFirstMetricsMs: timestamps.firstMetrics - timestamps.runtimeStarted,
  });
}

export function classifyWorkerUrl(url) {
  const value = String(url ?? "");
  if (/python-worker(?:\.|-)/.test(value)) return "authoring";
  if (/retained-execution-engine-worker|execution-engine-worker/.test(value)) return "engine";
  if (/retained-execution-render-worker|execution-render-worker|authoring-render-worker/.test(value)) {
    return "render";
  }
  return "other";
}

export function summarizeWorkers(events) {
  if (!Array.isArray(events)) {
    throw new TypeError("worker events must be an array");
  }
  const byRole = { authoring: 0, engine: 0, render: 0, other: 0 };
  const workers = events.map((event) => {
    if (!event || typeof event.url !== "string" || !Number.isFinite(event.atMs) || event.atMs < 0) {
      throw new TypeError("worker event must contain a URL and finite non-negative timestamp");
    }
    const role = classifyWorkerUrl(event.url);
    byRole[role] += 1;
    return Object.freeze({ url: event.url, atMs: event.atMs, role });
  });
  return Object.freeze({
    total: workers.length,
    byRole: Object.freeze(byRole),
    workers: Object.freeze(workers),
  });
}
