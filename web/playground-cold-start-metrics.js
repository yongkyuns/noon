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

export function classifyColdStartResource(url) {
  const value = String(url ?? "");
  if (/\/noon_web_bg\.wasm(?:[?#]|$)/.test(value)) return "noon-wasm";
  if (/\/pyodide\.asm\.wasm(?:[?#]|$)/.test(value)) return "pyodide-wasm";
  if (/\/pyodide(?:\.mjs|\.js)(?:[?#]|$)/.test(value)) return "pyodide";
  if (/\.wasm(?:[?#]|$)/.test(value)) return "wasm";
  if (/compat-bundle(?:\.[a-f0-9]+)?\.json(?:[?#]|$)/.test(value)) return "compat-bundle";
  return "other";
}

export function summarizeResourceFootprint(contexts, { noonWasmPackageBytes } = {}) {
  if (!Array.isArray(contexts) || contexts.length === 0) {
    throw new TypeError("resource footprint contexts must be a non-empty array");
  }
  if (!Number.isSafeInteger(noonWasmPackageBytes) || noonWasmPackageBytes <= 0) {
    throw new TypeError("Noon WASM package bytes must be a positive safe integer");
  }

  const rows = [];
  const contextSummaries = [];
  for (const context of contexts) {
    if (
      !isRecord(context) ||
      typeof context.name !== "string" ||
      context.name.trim() === "" ||
      typeof context.role !== "string" ||
      context.role.trim() === "" ||
      !Array.isArray(context.entries)
    ) {
      throw new TypeError("resource footprint context must contain name, role, and entries");
    }
    let transferBytes = 0;
    let encodedBodyBytes = 0;
    let decodedBodyBytes = 0;
    let wasmRequests = 0;
    for (const entry of context.entries) {
      const checked = validateResourceTimingEntry(entry);
      const kind = classifyColdStartResource(checked.name);
      if (kind === "noon-wasm" || kind === "pyodide-wasm" || kind === "wasm") {
        wasmRequests += 1;
      }
      transferBytes += checked.transferSize;
      encodedBodyBytes += checked.encodedBodySize;
      decodedBodyBytes += checked.decodedBodySize;
      rows.push({
        context: context.name,
        role: context.role,
        kind,
        ...checked,
      });
    }
    contextSummaries.push(
      Object.freeze({
        name: context.name,
        role: context.role,
        requests: context.entries.length,
        transferBytes,
        encodedBodyBytes,
        decodedBodyBytes,
        wasmRequests,
      }),
    );
  }

  const noonWasmRows = rows.filter(({ kind }) => kind === "noon-wasm");
  const wasmRows = rows.filter(
    ({ kind }) => kind === "noon-wasm" || kind === "pyodide-wasm" || kind === "wasm",
  );
  const noonWasmOwners = [...new Set(noonWasmRows.map(({ context }) => context))].sort();
  const uniqueWasmUrls = [...new Set(wasmRows.map(({ name }) => name))].sort();
  const sum = (values, field) => values.reduce((total, value) => total + value[field], 0);
  const largestResources = rows
    .slice()
    .sort((left, right) => {
      const leftSize = Math.max(left.encodedBodySize, left.transferSize);
      const rightSize = Math.max(right.encodedBodySize, right.transferSize);
      return rightSize - leftSize || left.name.localeCompare(right.name);
    })
    .slice(0, 20)
    .map((row) => Object.freeze({ ...row }));

  return Object.freeze({
    contextCount: contexts.length,
    requestCount: rows.length,
    transferBytes: sum(rows, "transferSize"),
    encodedBodyBytes: sum(rows, "encodedBodySize"),
    decodedBodyBytes: sum(rows, "decodedBodySize"),
    wasmRequestCount: wasmRows.length,
    uniqueWasmUrls: Object.freeze(uniqueWasmUrls),
    noonWasm: Object.freeze({
      requestCount: noonWasmRows.length,
      observedOwnerCount: noonWasmOwners.length,
      owners: Object.freeze(noonWasmOwners),
      transferBytes: sum(noonWasmRows, "transferSize"),
      encodedBodyBytes: sum(noonWasmRows, "encodedBodySize"),
      decodedBodyBytes: sum(noonWasmRows, "decodedBodySize"),
      packageBytes: noonWasmPackageBytes,
      packageBytesAcrossObservedOwners: noonWasmPackageBytes * noonWasmOwners.length,
    }),
    contexts: Object.freeze(contextSummaries),
    largestResources: Object.freeze(largestResources),
  });
}

function validateResourceTimingEntry(entry) {
  if (!isRecord(entry) || typeof entry.name !== "string" || entry.name.trim() === "") {
    throw new TypeError("resource timing entry must contain a non-empty name");
  }
  const checked = {
    name: entry.name,
    initiatorType: typeof entry.initiatorType === "string" ? entry.initiatorType : "",
  };
  for (const field of [
    "transferSize",
    "encodedBodySize",
    "decodedBodySize",
    "duration",
  ]) {
    const value = entry[field];
    if (!Number.isFinite(value) || value < 0) {
      throw new TypeError(`resource timing ${field} must be finite and non-negative`);
    }
    checked[field] = value;
  }
  return checked;
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
