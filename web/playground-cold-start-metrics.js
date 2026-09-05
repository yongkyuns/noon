export function coldStartMilestones(timestamps) {
  const required = [
    "navigationStart",
    "pageReady",
    "runRequested",
    "runtimeStarted",
    "firstMetrics",
  ];
  validateOrderedMilestones(timestamps, required, "cold-start");
  return Object.freeze({
    pageReadyMs: timestamps.pageReady - timestamps.navigationStart,
    runToRuntimeMs: timestamps.runtimeStarted - timestamps.runRequested,
    runToFirstMetricsMs: timestamps.firstMetrics - timestamps.runRequested,
    runtimeToFirstMetricsMs: timestamps.firstMetrics - timestamps.runtimeStarted,
  });
}

export function preloadedColdStartMilestones(timestamps) {
  const required = [
    "navigationStart",
    "pageReady",
    "preloadStarted",
    "firstMetrics",
  ];
  validateOrderedMilestones(timestamps, required, "preloaded cold-start");
  return Object.freeze({
    pageReadyMs: timestamps.pageReady - timestamps.navigationStart,
    pageReadyToPreloadMs: timestamps.preloadStarted - timestamps.pageReady,
    preloadToFirstMetricsMs: timestamps.firstMetrics - timestamps.preloadStarted,
    navigationToFirstMetricsMs: timestamps.firstMetrics - timestamps.navigationStart,
  });
}

export function validateAuthoringStartupMetrics(metrics) {
  if (!isRecord(metrics) || metrics.version !== 1) {
    throw new TypeError("authoring startup metrics must use schema version 1");
  }
  const durationFields = [
    "totalMs",
    "moduleGraphLoadMs",
    "initializeMs",
    "startupResourcesMs",
    "noonWebInitMs",
    "pyodideInitMs",
    "compatibilityBundleMs",
    "authoringBindingsMs",
    "compatibilityFsInstallMs",
    "compatibilityImportInstallMs",
  ];
  for (const field of durationFields) {
    const value = metrics[field];
    if (!Number.isFinite(value) || value < 0) {
      throw new TypeError(`authoring startup metric ${field} must be finite and non-negative`);
    }
  }
  if (
    !Number.isSafeInteger(metrics.compatibilityModuleCount) ||
    metrics.compatibilityModuleCount < 0
  ) {
    throw new TypeError(
      "authoring startup compatibility module count must be a non-negative safe integer",
    );
  }
  if (
    !Number.isSafeInteger(metrics.compatibilitySourceChars) ||
    metrics.compatibilitySourceChars < 0
  ) {
    throw new TypeError(
      "authoring startup compatibility source chars must be a non-negative safe integer",
    );
  }
  return Object.freeze({ ...metrics });
}

export function summarizeAuthoringStartup(metrics) {
  const checked = validateAuthoringStartupMetrics(metrics);
  const resources = [
    ["noon-web", checked.noonWebInitMs],
    ["pyodide", checked.pyodideInitMs],
    ["compat-bundle", checked.compatibilityBundleMs],
  ];
  resources.sort((left, right) => right[1] - left[1]);
  const [criticalResource, criticalResourceMs] = resources[0];
  const resourceWorkMs = resources.reduce((total, [, duration]) => total + duration, 0);
  const postResourceBootstrapMs =
    checked.authoringBindingsMs +
    checked.compatibilityFsInstallMs +
    checked.compatibilityImportInstallMs;
  return Object.freeze({
    ...checked,
    criticalResource,
    criticalResourceMs,
    resourceWorkMs,
    resourceOverlapSavedMs: Math.max(0, resourceWorkMs - checked.startupResourcesMs),
    postResourceBootstrapMs,
    unattributedMs:
      checked.totalMs -
      checked.moduleGraphLoadMs -
      checked.startupResourcesMs -
      postResourceBootstrapMs,
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

function validateOrderedMilestones(timestamps, required, label) {
  for (const name of required) {
    const value = timestamps?.[name];
    if (!Number.isFinite(value) || value < 0) {
      throw new TypeError(`${label} milestone ${name} must be a finite non-negative number`);
    }
  }
  for (let index = 1; index < required.length; index += 1) {
    const previous = required[index - 1];
    const current = required[index];
    if (timestamps[current] < timestamps[previous]) {
      throw new RangeError(`${label} milestone ${current} precedes ${previous}`);
    }
  }
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
