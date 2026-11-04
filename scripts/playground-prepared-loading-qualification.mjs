// Measure default and observed profiles in one package over ordinary streamed
// HTTP. Healthy-interpreter strata add an unmeasured seed Run before measurement.
import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { createReadStream } from "node:fs";
import { readFile, writeFile, rename, mkdir, readdir, symlink, mkdtemp, rm, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import playwright from "playwright";
import { PNG } from "pngjs";
import { serveRepository } from "./browser-test-server.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { createProcessTreeRssSampler } from "./playground-cold-start-memory.mjs";
import { browserArgs, classifyRendererGpuIdentity } from "./manim-raster-support.mjs";
import { assertNaturalFirstPassPublication } from "./showcase-live-review.mjs";
import { sceneInkPixels, summarizeReadiness, validateReadinessContract,
  validateReadinessResults, validateReadinessTrace, READINESS_STRATA,
  PRODUCT_GATE_THRESHOLD_ENVS, readinessCommandReceipt, readinessInstrumentationPlan,
  summarizeColdNetworkObservations,
  validateReusedHealthyInterpreter, readinessPresentationMatchesRun,
  readinessTraceMatchesRun,
  readinessRunTimingOriginErrors, readinessReplayCycleEvidence,
  readinessRendererSamples, readinessWarmSeekTargets, readinessPreparedReaderLead, readinessBufferingEvidence,
  summarizeReaderLead, activeFrameGapEvidence, readinessPostTerminalReceiverSession } from "./playground-prepared-loading-observation.mjs";
import { SHOWCASE_PROFILE_EXPECTATIONS } from "./playground-profile-expectations.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const env = process.env;
const readinessMode = env.NOON_PLAYBACK_LOADING_READINESS ?? "baseline";
assert.ok(["baseline", "strict"].includes(readinessMode));
const instrumentationMode = env.NOON_PLAYBACK_LOADING_INSTRUMENTATION ?? "minimal-timing";
const instrumentationPlan = readinessInstrumentationPlan(instrumentationMode);
assert.ok(instrumentationPlan, `unknown readiness instrumentation mode ${instrumentationMode}`);
const wasmAccounting = env.NOON_PLAYBACK_LOADING_WASM_ACCOUNTING === "1";
const { observationPollIntervalMs, usefulCaptureIntervalMs, profilePublicationStages } = instrumentationPlan;
const trials = Number(env.NOON_PLAYBACK_LOADING_TRIALS ?? 20);
assert.ok(Number.isSafeInteger(trials) && trials > 0);
const backend = env.NOON_PLAYBACK_LOADING_BACKEND ?? "webgpu";
assert.ok(["webgpu", "webgl"].includes(backend));
const output = path.resolve(env.NOON_PLAYBACK_LOADING_OUTPUT ?? path.join(os.tmpdir(), "noon-playback-loading"));
const encode = value => JSON.stringify(value, (_key, item) => typeof item === "bigint" ? String(item) : item, 2);
const digest = value => createHash("sha256").update(value).digest("hex");
function gitObject(name) {
  const result = spawnSync("git", ["rev-parse", name], { cwd: root, encoding: "utf8" });
  assert.equal(result.status, 0, `missing source object ${name}`);
  return result.stdout.trim();
}
const revision = gitObject("HEAD");
const runtimeTrees = commit => ({ crates: gitObject(`${commit}:crates`), web: gitObject(`${commit}:web`), lock: gitObject(`${commit}:Cargo.lock`) });
const inventory = JSON.parse(await readFile(path.join(root, "web/python/examples/noon_showcase_manifest.json"), "utf8")).entries;
const readinessContractPath = path.join(root, "scripts/fixtures/playground-readiness-contract.json");
const readinessContractBytes = await readFile(readinessContractPath);
const readinessContract = JSON.parse(readinessContractBytes);
const productAnchorPath = path.join(root, ".github/ci/product-performance-anchor.json");
const productAnchorBytes = await readFile(productAnchorPath);
const productAnchor = JSON.parse(productAnchorBytes);
const productAnchorLockBytes = await readFile(path.join(path.dirname(productAnchorPath), productAnchor.dependencyLock.file));
assert.deepEqual(validateReadinessContract(readinessContract), [], "invalid readiness measurement contract");
assert.deepEqual(readinessContract.sources.map(source => source.id),
  ["showcase-first-scene", "showcase-dynamic-scene"]);
assert.equal(backend.toLowerCase(), readinessContract.reference.backend.toLowerCase(),
  "readiness backend differs from its contract");
assert.ok(trials >= readinessContract.sampling.measuredStartsPerArmPerStratum,
  "readiness measurements require the contracted number of starts");
assert.equal(readinessContract.productGateAnchor.sourceRevision, productAnchor.source,
  "Product Gate cumulative source anchor changed");
assert.equal(readinessContract.productGateAnchor.manifestSha256, digest(productAnchorBytes),
  "Product Gate anchor manifest changed");
assert.equal(readinessContract.productGateAnchor.dependencyLockSha256, digest(productAnchorLockBytes),
  "Product Gate anchor dependency lock changed");
assert.deepEqual(readinessContract.productGateAnchor.workloads, productAnchor.workloads,
  "Product Gate workload set changed");
const gateHashes = { runnerSha256: "scripts/playground-product-e2e.mjs",
  workloadSha256: "scripts/playground-product-fps.mjs", comparisonSha256: "scripts/playground-product-compare.mjs" };
for (const [key, filename] of Object.entries(gateHashes)) {
  assert.equal(readinessContract.productGateAnchor[key], digest(await readFile(path.join(root, filename))), `Product Gate ${key} changed`);
}
const compareSource = await readFile(path.join(root, gateHashes.comparisonSha256), "utf8");
for (const [key, envName] of Object.entries(PRODUCT_GATE_THRESHOLD_ENVS)) {
  const match = compareSource.match(new RegExp(`threshold\\("${envName}",\\s*"([^"]+)"\\)`));
  assert.ok(match, `Product Gate default threshold ${envName} is missing`);
  assert.equal(readinessContract.productGateAnchor.thresholds[key], Number(match[1]), `Product Gate threshold ${envName} changed`);
}
if (readinessMode === "strict") {
  assert.deepEqual(validateReadinessContract(readinessContract, { strict: true }), [],
    "strict readiness requires a reviewed contract with approved bounds");
  assert.equal(instrumentationMode, "minimal-timing", "strict timing rejects bounded diagnostic instrumentation");
  assert.equal(wasmAccounting, false, "strict timing rejects invasive WASM accounting instrumentation");
}
for (const source of readinessContract.sources) {
  const expected = SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.id === source.id);
  const entry = inventory.find(row => row.id === source.id);
  assert.ok(expected && entry, `readiness source is absent from the showcase ledger: ${source.id}`);
  assert.deepEqual([source.path, source.sha256, source.durationSeconds],
    [expected.path, expected.sha256, entry.duration], `readiness source binding changed: ${source.id}`);
}
const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.source.js"), "utf8"));
const report = { status: "running", readinessMode,
  scope: "W7/W8 same-package profile measurements; source pass plus retained replay; cold-client, warm-cache and reused-healthy-interpreter strata; not CDN-edge-cold, physical-scanout or total-memory qualification",
  instrumentationMode,
  wasmAccounting: wasmAccounting ? "invasive-diagnostic" : "off",
  instrumentationLimitations: [
    "first useful capture is an observed upper bound; firstSubmittedMs uses first renderer.render() call completion as an upper bound on earlier frame submission and Noon presentation receipt, not queue-submit start, GPU completion or physical scanout",
    "command acceptance and presentation receipts come from actual endpoint command results; UI busy-clear remains a separate upper bound",
    "renderer frame timing is a renderer.render() call-completion timestamp; it does not measure GPU completion or physical scanout",
    ...(instrumentationMode === "minimal-timing"
      ? ["minimal timing still polls renderer metrics and samples canvas pixels; it is not zero-overhead"]
      : ["bounded publication-stage and 25ms probes can perturb timings; treat this report as diagnostic"]),
    "cold-network routing disables browser HTTP cache for the isolated stratum and adds no-cache request headers; this route changes that stratum's network timing and does not imply a cold CDN edge",
    ...(wasmAccounting ? ["WASM accounting rewrites worker entrypoints and changes cache/timing behavior; exclude these runs from readiness timing comparisons"] : []),
  ],
  harnessRevision: revision, runtimeSourceTrees: runtimeTrees(revision), harnessSha256: digest(await readFile(fileURLToPath(import.meta.url))),
  readinessContract: { path: path.relative(root, readinessContractPath), sha256: digest(readinessContractBytes), status: readinessContract.status },
  measurementDefinitions: readinessContract.measurementDefinitions,
  pendingCohorts: readinessContract.pendingCohorts,
  readinessSampling: {
    strata: readinessContract.strata,
    arms: readinessMode === "baseline" ? readinessContract.baselineArms : Object.keys(readinessContract.arms),
    samplePlan: readinessContract.sampling,
    bounds: readinessContract.bounds,
    allowedRegression: readinessContract.allowedRegression,
    observationPollIntervalMs,
    usefulCaptureIntervalMs,
  },
  backend, trials,
  launchArgs: browserArgs(backend, { gpuMode: "hardware" }), observer: { inkThreshold: 128, channelDifference: 8, edgeMargin: 8,
    pollIntervalMs: observationPollIntervalMs, usefulCaptureIntervalMs,
    usefulTiming: "upper bound at completion of the first positive canvas capture; native first-any-frame time is separate",
    memoryWindow: "paired source and retained replay plus 750 ms after context close; aggregate sampled browser-process-tree RSS",
    presentation: "opt-in raw renderer intervals aggregated over full run-owned source and replay windows, plus Rust reader lead and playback buffering; static holds are excluded and physical scanout is not measured" },
  clockOrigins: {
    pageEvents: "performance.timeOrigin + performance.now(), epoch milliseconds",
    rendererEvents: "renderer performance.timeOrigin + presentedAtMs; per-session gaps use one renderer clock origin",
    endpointCommandTiming: "endpoint performance.timeOrigin + performance.now(), epoch milliseconds across worker and page",
    processLifecycle: "Node performance.now() for durations and Date.now() for audit timestamps",
    runMilestones: "page or renderer epoch timestamp minus page accepted-Run epoch timestamp; source-run-start is a separate causal publication join",
  },
  host: { platform: os.platform(), release: os.release(), arch: os.arch(), cpu: os.cpus()[0]?.model,
    macOSVersion: os.platform() === "darwin" ? spawnSync("sw_vers", ["-productVersion"],
      { encoding: "utf8" }).stdout.trim() || null : null,
    logicalCpus: os.cpus().length, totalMemoryBytes: os.totalmem() }, packages: {}, cases: [] };
await mkdir(output, { recursive: true });
const reportPath = path.join(output, "report.json");
async function writeReport() {
  const temporaryPath = `${reportPath}.${randomUUID()}.tmp`;
  await writeFile(temporaryPath, encode(report));
  await rename(temporaryPath, reportPath);
}
await writeReport();
let overlay, browser, browserServer;
const servers = {};

async function fileHash(filename) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filename)) hash.update(chunk);
  return hash.digest("hex");
}

async function packageHost(arm, folder) {
  assert.ok(folder, `NOON_PLAYBACK_LOADING_${arm.toUpperCase()}_ROOT is required`);
  folder = path.resolve(folder);
  const identity = JSON.parse(await readFile(path.join(folder, "runtime-build-identity.json"), "utf8"));
  assert.match(identity.sourceRevision, /^[0-9a-f]{40}$/, `${arm} needs a clean recorded package source revision`);
  assert.deepEqual(runtimeTrees(identity.sourceRevision), report.runtimeSourceTrees,
    `${arm} package Rust, web and lockfile sources must match the measured host`);
  for (const [name, entry] of Object.entries(identity.files)) {
    const filename = name === "wasm" || name === "glue"
      ? path.join(folder, entry.path.replace(/^\.\//, "")) : path.join(root, "web", entry.path);
    assert.equal(await fileHash(filename), entry.sha256, `${arm} ${name} package identity drift`);
  }
  const host = path.join(overlay, arm);
  await mkdir(path.join(host, "web"), { recursive: true });
  for (const entry of await readdir(root)) {
    if (entry === "web" || entry.startsWith(".") || entry === "node_modules") continue;
    await symlink(path.join(root, entry), path.join(host, entry));
  }
  for (const entry of await readdir(path.join(root, "web"))) {
    if (entry === "pkg" || entry === "runtime-build-identity.json") continue;
    await symlink(path.join(root, "web", entry), path.join(host, "web", entry));
  }
  await symlink(path.join(folder, "pkg"), path.join(host, "web/pkg"));
  await symlink(path.join(folder, "runtime-build-identity.json"), path.join(host, "web/runtime-build-identity.json"));
  const server = await serveRepository(host, 0, { crossOriginIsolated: true });
  report.packages[arm] = { identity, wasmBytes: (await stat(path.join(folder, "pkg/noon_web_bg.wasm"))).size };
  return server;
}

async function installColdNetworkObservation(context) {
  const records = [];
  const pending = new Map();
  let overflow = false;
  await context.route("**/*", async route => {
    const request = route.request();
    if (request.method() !== "GET") return route.continue();
    const url = request.url();
    let external = false;
    try { external = new URL(url).protocol === "https:"; } catch {}
    let record = null;
    if (external) {
      if (records.length >= 4096) overflow = true;
      else {
        record = { external, url, requestEpochMs: Date.now(), status: null,
          requestCacheControl: "no-cache", requestPragma: "no-cache",
          fromServiceWorker: null, age: null, cacheControl: null, cdnCacheStatus: null, via: null };
        records.push(record);
        const queue = pending.get(url) ?? [];
        queue.push(record); pending.set(url, queue);
      }
    }
    const headers = { ...request.headers(), "cache-control": "no-cache", pragma: "no-cache" };
    try { await route.continue({ headers }); }
    catch (error) { if (record) record.failure = String(error?.message ?? error); }
  });
  context.on("response", response => {
    const queue = pending.get(response.url());
    const record = queue?.shift();
    if (!record) return;
    const headers = response.headers();
    Object.assign(record, { status: response.status(), fromServiceWorker: response.fromServiceWorker(),
      age: headers.age ?? null, cacheControl: headers["cache-control"] ?? null,
      cdnCacheStatus: headers["cf-cache-status"] ?? headers["x-cache"] ?? null, via: headers.via ?? null });
  });
  context.on("requestfailed", request => {
    const queue = pending.get(request.url());
    const record = queue?.shift();
    if (record) record.failure = request.failure()?.errorText ?? "network request failed";
  });
  return { records, overflow: () => overflow };
}

async function observation(page) {
  return observationDeadline(page.evaluate(async () => {
    const gallery = globalThis.__noonExampleGallery;
    globalThis.__noonReadinessPresentationProbe?.requestMetrics();
    let response = null;
    try { response = await gallery?.executionMetrics?.(); } catch {}
    const metrics = response?.metrics ?? null;
    const controls = document.querySelector(".playback-controls");
    const scrubber = controls?.querySelector(".playback-scrubber");
    const renderProbe = globalThis.__noonReadinessPresentationProbe;
    return { metrics: response?.metrics, preparedRetention: response?.engineMetrics?.preparedRetention ?? null,
      preparedPlayback: response?.engineMetrics?.preparedPlayback ?? null,
      run: gallery?.runPhaseMetrics,
      executionDiagnostics: gallery?.executionDiagnostics ?? null,
      presentationProbe: renderProbe ? { count: renderProbe.samples.length, error: renderProbe.error,
        rendererWorkerOrdinal: renderProbe.rendererWorkerOrdinalForOrigin(metrics?.performanceTimeOriginMs),
        uiEventCount: renderProbe.uiEvents.length, droppedUiEvents: renderProbe.droppedUiEvents } : null,
      controls: controls ? { ...controls.dataset,
        toggleEnabled: controls.querySelector(".playback-toggle")?.disabled === false,
        seekEnd: scrubber && !scrubber.disabled ? Number(scrubber.max) : null } : null,
      inFlight: gallery?.runInFlight, selected: gallery?.selectedExampleId,
      status: document.querySelector("#patch-status")?.dataset.state,
      problem: document.querySelector("#authoring-problem")?.textContent,
      epochMs: performance.timeOrigin + performance.now(), originMs: performance.timeOrigin };
  }));
}

async function observationDeadline(pending) {
  let timer;
  try {
    return await Promise.race([pending, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error("metrics observation exceeded 30 seconds")), 30_000);
    })]);
  } finally { clearTimeout(timer); }
}

async function installReadinessPresentationProbe(context, { profilePublicationStages }) {
  await context.addInitScript(({ maxSamples, maxCadenceSamples, profilePublicationStages }) => {
    const NativeWorker = window.Worker;
    const requestId = Number.MAX_SAFE_INTEGER;
    const cadencePollIntervalMs = 250;
    const probe = { profilePublicationStages, samples: [], keys: new Set(), error: null,
      rendererWorkerOrdinal: 0, rendererWorkers: new Map(), cadenceSamples: [], cadenceWorkers: new Map(),
      cadenceGenerationChanges: [], cadenceError: null,
      uiEvents: [], commandResults: [], droppedUiEvents: 0,
      recordUiEvent(event) {
        if (probe.uiEvents.length >= maxSamples) { probe.droppedUiEvents++; return; }
        probe.uiEvents.push(event);
      },
      recordCommandResult(message) {
        if (probe.commandResults.length >= maxSamples) { probe.error = "command result bound exceeded"; return; }
        probe.commandResults.push({ type: message.type, requestId: message.requestId,
          clockOriginMs: performance.timeOrigin, receivedEpochMs: performance.timeOrigin + performance.now(),
          commandTiming: message.commandTiming ?? null, commandPublication: message.commandPublication ?? null,
          presentationReceipt: message.presentationReceipt ?? null });
      },
      requestMetrics(force = false, ordinal = probe.workerRecord?.ordinal) {
        const record = probe.rendererWorkers.get(ordinal);
        if (!record || record.pending) return;
        const requestAtMs = performance.now();
        if (!force && !probe.profilePublicationStages && requestAtMs - record.lastRequestAtMs < cadencePollIntervalMs) return;
        record.lastRequestAtMs = requestAtMs;
        record.pending = true;
        record.instance.postMessage({ channel: "noon.render", protocolVersion: 1,
          type: "metrics", requestId, includePresentationIntervals: true,
          profilePublicationStages: probe.profilePublicationStages });
      } };
    probe.rendererWorkerOrdinalForOrigin = originMs => {
      if (!Number.isFinite(originMs)) return null;
      const matching = [...probe.rendererWorkers].filter(([, worker]) => worker.clockOriginMs === originMs)
        .map(([ordinal]) => ordinal);
      return matching.length === 1 ? matching[0] : null;
    };
    window.__noonReadinessPresentationProbe = probe;
    if (globalThis.MessagePort?.prototype) {
      const addMessageListener = MessagePort.prototype.addEventListener;
      const observedPorts = new WeakSet();
      MessagePort.prototype.addEventListener = function(type, listener, options) {
        if (type === "message" && !observedPorts.has(this)) {
          observedPorts.add(this);
          addMessageListener.call(this, "message", event => {
            const message = event.data;
            if (Number.isSafeInteger(message?.requestId) &&
                ["pause", "resume", "seek", "seek_section", "restart_playback"].includes(message.type)) {
              probe.recordCommandResult(message);
            }
          });
        }
        return addMessageListener.call(this, type, listener, options);
      };
    }
    const uiState = () => {
      const controls = document.querySelector(".playback-controls");
      return controls ? { playing: controls.dataset.playing ?? null,
        busy: controls.dataset.busy ?? null, buffering: controls.dataset.buffering ?? null,
        elapsedSeconds: controls.dataset.elapsedSeconds ?? null,
        preparedFrontier: controls.dataset.durationKnown === "false"
          ? Number(controls.querySelector(".playback-scrubber")?.max) : null } : null;
    };
    document.addEventListener("click", event => {
      const control = event.target?.closest?.(".playback-toggle, .playback-restart, .playback-scrubber");
      if (control) probe.recordUiEvent({ kind: "request", control: control.className,
        epochMs: performance.timeOrigin + performance.now(), state: uiState() });
    }, true);
    document.addEventListener("input", event => {
      if (event.target?.matches?.(".playback-scrubber")) probe.recordUiEvent({ kind: "request",
        control: "playback-scrubber", epochMs: performance.timeOrigin + performance.now(),
        requestedTime: Number(event.target.value), state: uiState() });
    }, true);
    const observeUi = () => {
      const observer = new MutationObserver(records => {
        for (const record of records) {
          if (!record.target?.closest?.(".playback-controls")) continue;
          probe.recordUiEvent({ kind: "state", attribute: record.attributeName,
            epochMs: performance.timeOrigin + performance.now(), state: uiState() });
        }
      });
      observer.observe(document.documentElement, { subtree: true, attributes: true,
        attributeFilter: ["data-playing", "data-busy", "data-buffering", "data-elapsed-seconds"] });
    };
    if (document.documentElement) observeUi();
    else document.addEventListener("DOMContentLoaded", observeUi, { once: true });
    class ObservedWorker extends NativeWorker {
      constructor(url, options) {
        super(url, options);
        if (!String(url).includes("execution-render-worker.js")) return;
        const workerOrdinal = ++probe.rendererWorkerOrdinal;
        const workerRecord = { ordinal: workerOrdinal, instance: this, pending: false, responses: 0,
          lastRequestAtMs: -Infinity };
        probe.rendererWorkers.set(workerOrdinal, workerRecord);
        probe.workerRecord = workerRecord;
        this.addEventListener("message", event => {
          const message = event.data;
          if (message?.channel !== "noon.render" || message.requestId !== requestId) return;
          event.stopImmediatePropagation();
          workerRecord.pending = false;
          workerRecord.responses++;
          if (Number.isFinite(message.metrics?.performanceTimeOriginMs)) {
            const origin = message.metrics.performanceTimeOriginMs;
            if (workerRecord.clockOriginMs !== undefined && workerRecord.clockOriginMs !== origin) {
              probe.cadenceError ??= "renderer worker clock origin changed";
            }
            workerRecord.clockOriginMs = origin;
          }
          if (message.type !== "metrics" || !Number.isFinite(message.metrics?.presentedFrames)) {
            probe.error = `invalid renderer metrics response: ${message.message ?? message.type}`;
            probe.cadenceError ??= probe.error;
            return;
          }
          const raw = message.metrics.rawPresentationIntervals;
          if (raw?.enabled !== true || !Number.isSafeInteger(raw.generation) ||
              !Number.isFinite(raw.generationStartedAtMs) || raw.generationStartedAtMs < 0 ||
              raw.generationStartedAtMs > message.metrics.sampledAtMs ||
              !Number.isSafeInteger(raw.latestOrdinal) || raw.latestOrdinal < 0 ||
              !Number.isSafeInteger(raw.capacity) || raw.capacity <= 0 ||
              raw.ordinalOverflow !== false || !Array.isArray(raw.samples) || raw.samples.length > raw.capacity ||
              !Number.isFinite(message.metrics.performanceTimeOriginMs) ||
              !Number.isFinite(message.metrics.sampledAtMs)) {
            probe.cadenceError ??= "raw presentation interval capture is missing, invalid, or overflowed";
          } else {
            const worker = probe.cadenceWorkers.get(workerOrdinal) ?? { latestOrdinal: 0, generation: null, session: null };
            worker.latestMetricsEpochMs = message.metrics.performanceTimeOriginMs + message.metrics.sampledAtMs;
            worker.captureHeaderObservedAtEpochMs ??= worker.latestMetricsEpochMs;
            worker.capacity = raw.capacity;
            if (raw.latestOrdinal < worker.latestOrdinal) {
              probe.cadenceError ??= "raw presentation interval ordinal regressed";
            }
            const latestReceiptSession = message.metrics.latestPresentationSample?.session;
            const observedSession = Number.isSafeInteger(latestReceiptSession) ? latestReceiptSession : null;
            if (worker.generation !== null && raw.generation !== worker.generation) {
              if (probe.cadenceGenerationChanges.length < 64) {
                const change = { workerOrdinal, from: worker.generation, to: raw.generation,
                  previousSession: worker.session, session: observedSession,
                  changedAtEpochMs: message.metrics.performanceTimeOriginMs + raw.generationStartedAtMs,
                  observedAtEpochMs: message.metrics.performanceTimeOriginMs + message.metrics.sampledAtMs };
                probe.cadenceGenerationChanges.push(change);
                worker.pendingGenerationChange = observedSession === null ? change : null;
              } else probe.cadenceError ??= "renderer generation change collector bound exceeded";
            }
            worker.generation = raw.generation;
            if (worker.pendingGenerationChange?.to === raw.generation && worker.pendingGenerationChange.session === null &&
                observedSession !== null) {
              worker.pendingGenerationChange.session = observedSession;
              worker.pendingGenerationChange.attributedAtEpochMs = message.metrics.performanceTimeOriginMs +
                message.metrics.sampledAtMs;
              worker.pendingGenerationChange = null;
            }
            if (observedSession !== null) worker.session = observedSession;
            let previousRingOrdinal = null;
            for (const sample of raw.samples) {
              if (!Number.isSafeInteger(sample.ordinal) || !Number.isSafeInteger(sample.session) ||
                  !Number.isFinite(sample.presentedAtMs) || !Number.isFinite(sample.intervalMs) ||
                  sample.intervalMs < 0 || sample.ordinal > raw.latestOrdinal) {
                probe.cadenceError ??= "raw presentation interval sample is invalid";
                continue;
              }
              if (previousRingOrdinal !== null && sample.ordinal !== previousRingOrdinal + 1) {
                probe.cadenceError ??= "raw presentation interval ring is not contiguous";
              }
              previousRingOrdinal = sample.ordinal;
              if (sample.ordinal <= worker.latestOrdinal) continue;
              if (sample.ordinal !== worker.latestOrdinal + 1) {
                probe.cadenceError ??= "raw presentation interval ordinal gap or ring overrun";
                worker.latestOrdinal = sample.ordinal - 1;
              }
              if (probe.cadenceSamples.length >= maxCadenceSamples) {
                probe.cadenceError ??= "raw presentation interval collector bound exceeded";
                break;
              }
              probe.cadenceSamples.push({ ...sample, workerOrdinal,
                generation: raw.generation, clockOriginMs: message.metrics.performanceTimeOriginMs });
              worker.latestOrdinal = sample.ordinal;
            }
            if (raw.latestOrdinal > worker.latestOrdinal) {
              probe.cadenceError ??= "raw presentation interval tail is missing";
            }
            if (raw.samples.length > 0 && previousRingOrdinal !== raw.latestOrdinal) {
              probe.cadenceError ??= "raw presentation interval ring tail does not match its ordinal";
            }
            probe.cadenceWorkers.set(workerOrdinal, worker);
          }
          for (const sample of message.metrics.publicationStageSamples ?? []) {
            const key = `${message.metrics.performanceTimeOriginMs}:${sample.session}:${sample.sequence}`;
            if (probe.keys.has(key)) continue;
            if (probe.samples.length >= maxSamples) { probe.error = "presentation sample bound exceeded"; return; }
            probe.keys.add(key);
            probe.samples.push({ ...sample, clockOriginMs: message.metrics.performanceTimeOriginMs });
          }
        });
        probe.requestMetrics(false, workerOrdinal);
      }
    }
    Object.defineProperty(window, "Worker", { configurable: true, writable: true, value: ObservedWorker });
  }, { maxSamples: 4096, maxCadenceSamples: 16384, profilePublicationStages });
}

function readinessTraceState(row, sample) {
  const diagnostics = sample.executionDiagnostics;
  if (!diagnostics || !Array.isArray(diagnostics.events)) return;
  row.diagnosticIdentity = {
    schemaVersion: diagnostics.schemaVersion ?? null,
    libraryTrust: diagnostics.libraryTrust ?? null,
    interpreterGeneration: diagnostics.interpreterGeneration ?? null,
    runRequestId: diagnostics.runRequestId ?? null,
    context: diagnostics.context ?? null,
    requestedProfile: diagnostics.requestedProfile ?? null,
    droppedEvents: diagnostics.droppedEvents ?? null,
  };
  row.actualProfileTransitions = diagnostics.events
    .filter(event => event.stage === "attached" || event.stage === "profile-change")
    .map(event => ({ sequence: event.sequence, stage: event.stage, profile: event.profile ?? null,
      reason: event.reason ?? null, contextId: event.contextId ?? null, time: event.time ?? null }));
  row.dispositionEvents = diagnostics.events.filter(event => event.stage === "disposition");
  row.traceCompleteness = {
    droppedEvents: diagnostics.droppedEvents,
    hasAttached: diagnostics.events.some(event => event.stage === "attached"),
    hasSourceComplete: diagnostics.events.some(event => event.stage === "source-complete"),
    hasTerminalAdopted: diagnostics.events.some(event => event.stage === "terminal-adopted"),
    hasDisposition: row.dispositionEvents.length > 0,
  };
}

async function installWasmAccounting(page) {
  const entries = new Set(["/web/python-worker.js", "/web/execution-render-worker.js"]);
  await page.route(url => entries.has(new URL(url).pathname), async route => {
    if (!["script", "worker"].includes(route.request().resourceType())) return route.continue();
    const response = await route.fetch(), headers = { ...response.headers() };
    delete headers["content-length"]; delete headers["content-encoding"];
    await route.fulfill({ response, headers,
      body: `import "./playground-cold-start-wasm-accounting.js";\n${await response.text()}` });
  });
}

async function wasmAccountingSnapshot(workers) {
  return Promise.all(workers.filter(worker => /\/(python-worker|execution-render-worker)\.js/.test(worker.url()))
    .map(async worker => ({ url: worker.url(), accounting: await worker.evaluate(() => {
      const probe = globalThis.__noonColdStartWasmAccounting;
      return probe ? { records: probe.records, linearMemory: probe.snapshot() } : null;
    }).catch(error => ({ unavailable: String(error) })) })));
}

async function rawCadenceCapture(page, rendererWorkerOrdinal) {
  if (!Number.isSafeInteger(rendererWorkerOrdinal)) {
    return { error: "measured renderer worker identity is unavailable", samples: [], workers: [], generationChanges: [] };
  }
  await page.waitForFunction(ordinal => {
    const probe = globalThis.__noonReadinessPresentationProbe;
    const record = probe?.rendererWorkers?.get(ordinal);
    return record && !record.pending;
  }, rendererWorkerOrdinal, { timeout: 10_000 });
  const previousResponseCount = await page.evaluate(ordinal => {
    const probe = globalThis.__noonReadinessPresentationProbe;
    const record = probe?.rendererWorkers?.get(ordinal);
    if (!record) return null;
    const count = record.responses;
    probe.requestMetrics(true, ordinal);
    return count;
  }, rendererWorkerOrdinal);
  if (!Number.isSafeInteger(previousResponseCount)) {
    return { error: "owning renderer worker is unavailable for final cadence capture", samples: [], workers: [], generationChanges: [] };
  }
  await page.waitForFunction(({ ordinal, previous }) => {
    const probe = globalThis.__noonReadinessPresentationProbe;
    const record = probe?.rendererWorkers?.get(ordinal);
    return record && record.responses > previous && !record.pending;
  }, { ordinal: rendererWorkerOrdinal, previous: previousResponseCount }, { timeout: 10_000 });
  return page.evaluate(ordinal => {
    const probe = globalThis.__noonReadinessPresentationProbe;
    const workers = [...(probe?.cadenceWorkers ?? new Map())].map(([ordinal, state]) => ({ ordinal, ...state }));
    const owner = probe?.cadenceWorkers?.get(ordinal);
    return { error: probe ? probe.cadenceError : "raw cadence probe is unavailable",
      samples: probe?.cadenceSamples ?? [],
      generationChanges: probe?.cadenceGenerationChanges ?? [],
      workers, latestMetricsEpochMs: owner?.latestMetricsEpochMs ?? null,
      headerObservedAtEpochMs: owner?.captureHeaderObservedAtEpochMs ?? null,
      capacity: owner?.capacity ?? null };
  }, rendererWorkerOrdinal);
}

async function retiredWorkerObservation(worker, timeoutMs = 500) {
  let timer;
  const startedAtEpochMs = Date.now();
  try {
    const retired = await Promise.race([
      worker.evaluate(() => true).then(() => false, () => true),
      new Promise(resolve => { timer = setTimeout(() => resolve(null), timeoutMs); }),
    ]);
    return { url: worker.url(), retired, checkedAtEpochMs: Date.now(),
      upperMs: Date.now() - startedAtEpochMs };
  } finally { clearTimeout(timer); }
}

async function issueReadinessClick(page, button, controlClass) {
  const marker = await page.evaluate(() => ({ index: window.__noonReadinessPresentationProbe?.uiEvents.length ?? 0,
    commandResultIndex: window.__noonReadinessPresentationProbe?.commandResults.length ?? 0,
    epochMs: performance.timeOrigin + performance.now() }));
  await button.click();
  const events = await page.evaluate(index =>
    (window.__noonReadinessPresentationProbe?.uiEvents ?? []).slice(index), marker.index);
  const request = events.find(event => event.kind === "request" &&
    String(event.control).split(/\s+/).includes(controlClass));
  assert.ok(request, `actual ${controlClass} DOM request was not captured`);
  const feedback = events.find(event => event.kind === "state" && event.attribute === "data-busy" &&
    event.state?.busy === "true" && event.epochMs >= request.epochMs) ?? null;
  return { automationIssuedEpochMs: marker.epochMs, requestEpochMs: request.epochMs, feedback,
    commandResultIndex: marker.commandResultIndex };
}

async function readinessCommandResult(page, startIndex, expectedType) {
  return page.evaluate(({ startIndex, expectedType }) =>
    [...(window.__noonReadinessPresentationProbe?.commandResults ?? [])].slice(startIndex)
      .filter(result => result.type === expectedType).at(-1) ?? null,
  { startIndex, expectedType });
}

async function readinessPublicationSample(page, publication) {
  if (!Number.isSafeInteger(publication?.session) || !Number.isSafeInteger(publication?.sequence)) return null;
  const deadline = performance.now() + 30_000;
  while (performance.now() < deadline) {
    await observation(page);
    const sample = await page.evaluate(({ session, sequence }) =>
      [...(window.__noonReadinessPresentationProbe?.samples ?? [])].findLast(value =>
        value.session === session && value.sequence === sequence) ?? null,
    publication);
    if (sample !== null) return sample;
    await page.waitForTimeout(25);
  }
  return null;
}

function rendererSnapshot(metrics) {
  return metrics ? { session: metrics.presentedSession ?? null, frame: metrics.presentedFrames ?? null,
    logicalTime: metrics.time ?? null, sampledAtMs: metrics.sampledAtMs ?? null,
    clockOriginMs: metrics.performanceTimeOriginMs ?? null, ready: metrics.ready ?? null,
    needsPresent: metrics.needsPresent ?? null } : null;
}

async function readinessCommand(page, row, desiredPlaying) {
  const before = await observation(page);
  assert.notEqual(before.controls?.playing === "true", desiredPlaying, "requested transport state is already active");
  const button = page.locator(".playback-toggle");
  assert.equal(await button.isDisabled(), false, "play/pause command is unavailable");
  const issued = await issueReadinessClick(page, button, "playback-toggle");
  const requestEpochMs = issued.requestEpochMs;
  await page.waitForFunction(desired =>
    document.querySelector(".playback-controls")?.dataset.playing === String(desired), desiredPlaying);
  await page.waitForFunction(desired => {
    const controls = document.querySelector(".playback-controls");
    return controls?.dataset.busy === "false" && controls.dataset.playing === String(desired);
  }, desiredPlaying);
  const commandCompleted = await page.evaluate(requestEpoch =>
    [...(window.__noonReadinessPresentationProbe?.uiEvents ?? [])].reverse().find(event =>
      event.kind === "state" && event.attribute === "data-busy" && event.state?.busy === "false" &&
      event.epochMs >= requestEpoch) ?? null, requestEpochMs);
  assert.ok(commandCompleted, "playback command completion transition was not observed");
  const engineResult = await readinessCommandResult(page, issued.commandResultIndex,
    desiredPlaying ? "resume" : "pause");
  const after = await observation(page);
  const latest = after.metrics?.latestPresentationSample;
  const minimalRendererSample = latest ? { ...latest,
    clockOriginMs: after.metrics.performanceTimeOriginMs } : null;
  const publicationSample = instrumentationMode === "bounded-diagnostics"
    ? await readinessPublicationSample(page, engineResult?.commandPublication) : minimalRendererSample;
  const runtimeReceipt = readinessCommandReceipt(engineResult, requestEpochMs, publicationSample);
  const command = {
    name: desiredPlaying ? "play" : "pause",
    automationIssuedEpochMs: issued.automationIssuedEpochMs,
    requestEpochMs,
    uiFeedbackEpochMs: issued.feedback?.epochMs ?? null,
    uiFeedbackMs: issued.feedback ? issued.feedback.epochMs - requestEpochMs : null,
    uiFeedbackKind: issued.feedback ? "data-busy=true" : null,
    commandCompletedUiEpochMs: commandCompleted.epochMs,
    commandCompletionObservedEpochMs: after.epochMs,
    commandCompletionUpperMs: after.epochMs - requestEpochMs,
    rendererObservation: rendererSnapshot(after.metrics),
    ...runtimeReceipt,
    runtimeAcceptanceLimitation: runtimeReceipt.runtimeAcceptanceEpochMs === null
      ? "endpoint command result did not expose an acceptance timestamp" : null,
    presentationReceiptLimitation: runtimeReceipt.matchingPresentation === null
      ? "endpoint command result did not expose a validated matching publication receipt" : null,
  };
  row.commands.push(command);
  return after;
}

async function readinessWarmSeek(page, row, targetTime) {
  const marker = await page.evaluate(() => ({ uiEventIndex: window.__noonReadinessPresentationProbe?.uiEvents.length ?? 0,
    commandResultIndex: window.__noonReadinessPresentationProbe?.commandResults.length ?? 0,
    automationIssuedEpochMs: performance.timeOrigin + performance.now() }));
  const slider = page.locator(".playback-scrubber");
  assert.equal(await slider.isDisabled(), false, "completed replay seek control is unavailable");
  const selectedTime = await slider.evaluate((input, target) => {
    const minimum = Number(input.min), maximum = Number(input.max);
    if (!Number.isFinite(minimum) || !Number.isFinite(maximum) || minimum > target || maximum < target) {
      throw new Error("seek target is outside the DOM range");
    }
    input.value = String(target);
    const rawInputValue = input.value;
    const rawInputTime = Number(rawInputValue);
    const clampedInputTime = Math.max(minimum, Math.min(maximum, rawInputTime));
    if (clampedInputTime !== target) throw new Error("DOM range clamp differs from the exact Rust target");
    input.dispatchEvent(new Event("input", { bubbles: true }));
    return { rawInputValue, rawInputTime, clampedInputTime, minimum, maximum };
  }, targetTime);
  const request = await page.evaluate(({ index, target }) =>
    (window.__noonReadinessPresentationProbe?.uiEvents ?? []).slice(index).find(event =>
      event.kind === "request" && event.control === "playback-scrubber" && event.requestedTime === target) ?? null,
  { index: marker.uiEventIndex, target: selectedTime.rawInputTime });
  assert.ok(request, "DOM range input did not produce the requested seek event");
  await page.waitForFunction(target => {
    const controls = document.querySelector(".playback-controls")?.dataset;
    return controls?.busy === "false" && Number(controls.elapsedSeconds) === target;
  }, targetTime, { timeout: 30_000 });
  const events = await page.evaluate(index =>
    (window.__noonReadinessPresentationProbe?.uiEvents ?? []).slice(index), marker.uiEventIndex);
  const feedback = events.find(event => event.kind === "state" && event.attribute === "data-busy" &&
    event.state?.busy === "true" && event.epochMs >= request.epochMs) ?? null;
  const completion = events.findLast(event => event.kind === "state" && event.attribute === "data-busy" &&
    event.state?.busy === "false" && event.epochMs >= request.epochMs) ?? null;
  assert.ok(completion, "seek command completion transition was not observed");
  const result = await readinessCommandResult(page, marker.commandResultIndex, "seek");
  assert.ok(result, "seek endpoint result was not observed");
  const after = await observation(page);
  const latest = after.metrics?.latestPresentationSample;
  const minimalRendererSample = latest ? { ...latest, clockOriginMs: after.metrics.performanceTimeOriginMs } : null;
  const publicationSample = instrumentationMode === "bounded-diagnostics"
    ? await readinessPublicationSample(page, result.commandPublication) : minimalRendererSample;
  const receipt = readinessCommandReceipt(result, request.epochMs, publicationSample);
  const command = { name: "seek", commandPhase: "completed-history-warm", targetTime,
    domRange: { minimum: selectedTime.minimum, maximum: selectedTime.maximum,
      rawInputValue: selectedTime.rawInputValue, rawInputTime: selectedTime.rawInputTime,
      clampedInputTime: selectedTime.clampedInputTime, eventRequestedTime: request.requestedTime },
    receiptTime: receipt.matchingPresentation?.time ?? null,
    automationIssuedEpochMs: marker.automationIssuedEpochMs, requestEpochMs: request.epochMs,
    uiFeedbackEpochMs: feedback?.epochMs ?? null,
    uiFeedbackMs: feedback ? feedback.epochMs - request.epochMs : null,
    uiFeedbackKind: feedback ? "data-busy=true" : null,
    commandCompletedUiEpochMs: completion.epochMs,
    commandCompletionUpperMs: after.epochMs - request.epochMs,
    rendererObservation: rendererSnapshot(after.metrics), ...receipt,
    runtimeAcceptanceLimitation: receipt.runtimeAcceptanceEpochMs === null
      ? "endpoint seek result did not expose an acceptance timestamp" : null,
    presentationReceiptLimitation: receipt.matchingPresentation === null
      ? "endpoint seek result did not expose a validated matching publication receipt" : null };
  row.commands.push(command);
  return after;
}

async function measureReadinessPair(page, workers, source, initialArm, replayArm, stratum, trial, warmup, contextIdentity) {
  const startedAtEpochMs = Date.now();
  const showcaseEntry = inventory.find(entry => entry.id === source.id);
  const terminalStill = showcaseEntry?.still_intervals?.findLast(([, end]) => end === source.durationSeconds);
  assert.ok(terminalStill, `${source.id}: manifest does not declare a terminal hold for replay validation`);
  const sourceRow = { sourceId: source.id, sourcePath: source.path, sourceSha256: source.sha256,
    durationSeconds: source.durationSeconds, arm: initialArm, pairedReplayArm: replayArm,
    stratum, trial, warmup, status: "running", startup: {}, commands: [],
    browserProcessId: browserServer.process().pid, clientContextId: contextIdentity.id,
    clientContextCreatedAtEpochMs: contextIdentity.createdAtEpochMs,
    rendererSamples: [], cadenceObservations: [], readerLeadObservations: [], pageErrors: [], consoleErrors: [],
    sourceInvocationIds: [], profileValidationErrors: [], limitation: [] };
  const replayRow = { ...sourceRow, arm: replayArm, pairedInitialArm: initialArm, startup: {},
    commands: [], rendererSamples: [], cadenceObservations: [], readerLeadObservations: [], pageErrors: [],
    consoleErrors: [], sourceInvocationIds: [], profileValidationErrors: [], limitation: [] };
  replayRow.rendererWorkerOrdinal = undefined;
  report.cases.push(sourceRow, replayRow);
  let activeRow = sourceRow;
  const onPageError = error => activeRow.pageErrors.push(String(error));
  const onConsole = message => { if (message.type() === "error") activeRow.consoleErrors.push(message.text()); };
  page.on("pageerror", onPageError); page.on("console", onConsole);
  const sampler = createProcessTreeRssSampler(browserServer.process().pid);
  sampler.start();
  const sourceStarted = performance.now();
  const retirementStartedAtEpochMs = Date.now();
  const previousWorkers = workers.slice();
  const catalog = "&catalog=showcase";
  try {
    if (stratum === "reload-new-workers-warm-resource-cache") await page.reload();
    else await page.goto(`${servers.runtime.baseUrl}/web/index.html?example=${source.id}${catalog}`);
    await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
    const observedContext = await page.evaluate(() => ({ viewport: { width: window.innerWidth, height: window.innerHeight },
      devicePixelRatio: window.devicePixelRatio }));
    sourceRow.observedContext = observedContext;
    replayRow.observedContext = observedContext;
    assert.deepEqual(observedContext, { viewport: readinessContract.reference.viewport,
      devicePixelRatio: readinessContract.reference.deviceScaleFactor },
    "observed browser viewport and devicePixelRatio differ from the readiness reference");
    if (previousWorkers.length > 0) {
      const retired = await Promise.all(previousWorkers.map(worker => retiredWorkerObservation(worker)));
      sourceRow.previousWorkersRetirement = { requestedAtEpochMs: retirementStartedAtEpochMs,
        observations: retired, allRetired: retired.every(value => value.retired === true),
        observedUpperMs: Math.max(...retired.map(value => value.upperMs)) };
      if (!sourceRow.previousWorkersRetirement.allRetired) {
        sourceRow.profileValidationErrors.push("previous page workers were not confirmed retired");
      }
    }
    replayRow.startup.shellReadyLimitation = "retained replay reuses the loaded page and interpreter";
    replayRow.startup.firstReaderLimitation = "retained replay reuses the original source reader";
    let navigation = await observation(page);
    sourceRow.startup.navigationShellReadyObservedUpperMs = performance.now() - sourceStarted;
    sourceRow.startup.navigationShellReadyBasis = "navigation start to initialized gallery shell";
    sourceRow.startup.shellReadyObservedUpperMs = 0;
    sourceRow.startup.shellReadyBasis = "the initialized gallery shell accepts Run immediately";
    if (stratum === "reused-healthy-interpreter") {
      if (!Number.isFinite(navigation.run?.sourceRunStartedAtMs)) {
        await page.locator("#replace-scene").click();
      }
      const seedDeadline = performance.now() + Math.max(180_000, source.durationSeconds * 10_000);
      const seedDisposition = initialArm === "A" ? "looped" : "observed-complete";
      const seedComplete = sample => {
        const events = sample.executionDiagnostics?.events ?? [];
        const disposition = events.findLast(event => event.stage === "disposition");
        return sample.run?.exampleId === source.id && Number.isFinite(sample.run?.sourceRunCompletedAtMs) &&
          sample.inFlight === false && events.some(event => event.stage === "source-complete") &&
          events.some(event => event.stage === "terminal-adopted") && disposition?.disposition === seedDisposition &&
          (seedDisposition !== "looped" || (Number.isSafeInteger(disposition.loopCount) && disposition.loopCount >= 1));
      };
      while (!seedComplete(navigation) && performance.now() < seedDeadline) {
        navigation = await observation(page);
        await page.waitForTimeout(observationPollIntervalMs);
      }
      assert.ok(seedComplete(navigation), "healthy-interpreter seed source did not complete its expected disposition");
      const seedErrors = validateReadinessTrace(navigation.executionDiagnostics, {
        sourceId: source.id, requestedProfile: initialArm === "A" ? "incremental" : "observed",
        attachedProfile: initialArm === "A" ? "incremental" : "observed", disposition: seedDisposition,
      });
      assert.deepEqual(seedErrors, [], "healthy-interpreter seed did not use the expected eligible profile");
      const seedWorker = workers.filter(worker => /\/python-worker\.js(?:\?|$)/.test(worker.url())).at(-1);
      assert.ok(seedWorker, "healthy-interpreter seed did not create a Python worker");
      await seedWorker.evaluate(() => true);
      const seedSource = await page.locator("#python-scene-source").inputValue();
      sourceRow.interpreterReuseSeed = { runRequestId: navigation.executionDiagnostics.runRequestId,
        interpreterGeneration: navigation.executionDiagnostics.interpreterGeneration,
        libraryTrust: navigation.executionDiagnostics.libraryTrust,
        sourceRunStartedAtMs: navigation.run.sourceRunStartedAtMs,
        sourceRunCompletedAtMs: navigation.run.sourceRunCompletedAtMs,
        completedAtEpochMs: navigation.epochMs };
      await page.evaluate(() => {
        const probe = globalThis.__noonReadinessPresentationProbe;
        probe.samples = []; probe.keys.clear(); probe.uiEvents = []; probe.commandResults = [];
        probe.droppedUiEvents = 0; probe.error = null;
      });
      await page.locator("#replace-scene").click();
      await page.waitForFunction(seed => {
        const gallery = globalThis.__noonExampleGallery;
        const run = gallery?.runPhaseMetrics;
        const diagnostics = gallery?.executionDiagnostics;
        return run?.runGeneration !== seed.runGeneration &&
          Number.isFinite(run?.runAcceptedAtMs) &&
          Number.isFinite(run?.sourceRunStartedAtMs) &&
          Number.isSafeInteger(diagnostics?.runRequestId) && diagnostics.runRequestId !== seed.runRequestId &&
          Number.isSafeInteger(diagnostics?.interpreterGeneration) &&
          diagnostics.interpreterGeneration === seed.interpreterGeneration &&
          diagnostics.libraryTrust === "pristine";
      }, { runGeneration: navigation.run.runGeneration,
        runRequestId: navigation.executionDiagnostics.runRequestId,
        interpreterGeneration: navigation.executionDiagnostics.interpreterGeneration }, { timeout: 30_000 });
      const measured = await observation(page);
      const measuredWorker = workers.filter(worker => /\/python-worker\.js(?:\?|$)/.test(worker.url())).at(-1);
      const measuredSource = await page.locator("#python-scene-source").inputValue();
      const reuseErrors = validateReusedHealthyInterpreter(navigation, measured,
        measuredWorker === seedWorker);
      if (digest(seedSource) !== source.sha256) reuseErrors.push("healthy-interpreter seed source differs from inventory bytes");
      if (measuredSource !== seedSource) reuseErrors.push("healthy-interpreter rerun changed authored source bytes");
      sourceRow.interpreterReuse = { ...sourceRow.interpreterReuseSeed,
        measuredRunRequestId: measured.executionDiagnostics?.runRequestId ?? null,
        measuredInterpreterGeneration: measured.executionDiagnostics?.interpreterGeneration ?? null,
        measuredLibraryTrust: measured.executionDiagnostics?.libraryTrust ?? null,
        sourceSha256: digest(seedSource),
        workerUrl: seedWorker.url(), sameWorker: measuredWorker === seedWorker,
        validationErrors: reuseErrors };
      sourceRow.profileValidationErrors.push(...reuseErrors);
      assert.deepEqual(reuseErrors, [], "second source Run did not reuse the verified healthy interpreter");
      navigation = measured;
      sourceRow.startup.shellReadyObservedUpperMs = 0;
      sourceRow.startup.shellReadyBasis = "the initialized gallery shell accepts Run immediately";
    }
    let sourceRunAcceptedEpochMs = Number.isFinite(navigation.run?.runAcceptedAtMs)
      ? navigation.originMs + navigation.run.runAcceptedAtMs : null;
    if (sourceRunAcceptedEpochMs !== null) {
      sourceRow.startup.runAcceptedAtEpochMs = sourceRunAcceptedEpochMs;
    }
    let sourceRunStart = Number.isFinite(navigation.run?.sourceRunStartedAtMs)
      ? navigation.originMs + navigation.run.sourceRunStartedAtMs : null;
    if (sourceRunStart !== null) sourceRow.startup.sourceRunStartedAtEpochMs = sourceRunStart;
    let lastCaptureKey = null;
    const lastUsefulCaptureAt = new Map();
    const read = async (targetRow = sourceRow) => {
      const sample = await observation(page);
      targetRow.lastObservation = sample;
      targetRow.cadenceObservations.push({ epochMs: sample.epochMs,
        metrics: sample.metrics, controls: sample.controls, preparedPlayback: sample.preparedPlayback });
      const readerLead = readinessPreparedReaderLead(sample.preparedPlayback);
      if (readerLead !== null) {
        if (targetRow.readerLeadObservations.length < 4096) {
          targetRow.readerLeadObservations.push({ observedAtEpochMs: sample.epochMs, ...readerLead });
        } else targetRow.profileValidationErrors.push("reader lead observation bound exceeded");
      }
      targetRow.preparedRetention = sample.preparedRetention ?? targetRow.preparedRetention ?? null;
      readinessTraceState(targetRow, sample);
      const diagnostic = sample.executionDiagnostics;
      if (diagnostic?.runRequestId !== null && diagnostic?.runRequestId !== undefined &&
          !targetRow.sourceInvocationIds.includes(diagnostic.runRequestId)) {
        targetRow.sourceInvocationIds.push(diagnostic.runRequestId);
      }
      if (sourceRunStart === null && Number.isFinite(sample.run?.sourceRunStartedAtMs)) {
        sourceRunStart = sample.originMs + sample.run.sourceRunStartedAtMs;
        targetRow.startup.sourceRunStartedAtEpochMs = sourceRunStart;
      }
      if (targetRow === sourceRow && sourceRunAcceptedEpochMs === null &&
          Number.isFinite(sample.run?.runAcceptedAtMs)) {
        sourceRunAcceptedEpochMs = sample.originMs + sample.run.runAcceptedAtMs;
        targetRow.startup.runAcceptedAtEpochMs = sourceRunAcceptedEpochMs;
      }
      if (targetRow === sourceRow && sourceRunAcceptedEpochMs !== null && sourceRunStart !== null) {
        targetRow.startup.interpreterReadyObservedUpperMs = sourceRunStart - sourceRunAcceptedEpochMs;
        targetRow.startup.interpreterReadyBasis = "clientForRun resolves before the source start timestamp";
      }
      const events = diagnostic?.events ?? [];
      const disposition = events.findLast(event => event.stage === "disposition");
      const firstTraversalObserved = initialArm === "A"
        ? Number.isSafeInteger(disposition?.loopCount) && disposition.loopCount === 1
        : disposition?.disposition === "observed-complete";
      if (targetRow === sourceRow && initialArm === "A" && sourceRow.firstTraversalEndObservedAtEpochMs === undefined &&
          Number.isSafeInteger(disposition?.loopCount) && disposition.loopCount > 1 &&
          !sourceRow.profileValidationErrors.includes("source missed its first prepared traversal wrap")) {
        sourceRow.profileValidationErrors.push("source missed its first prepared traversal wrap");
      }
      if (targetRow === sourceRow && sourceRow.firstTraversalEndObservedAtEpochMs === undefined &&
          readinessTraceMatchesRun(diagnostic, sample.run, source.id) && firstTraversalObserved) {
        sourceRow.firstTraversalEndObservedAtEpochMs = sample.epochMs;
      }
      const attachedPublication = events.find(event => event.stage === "attached")?.publication;
      const presentationAnchor = targetRow === sourceRow ? attachedPublication : targetRow.restartPublication;
      const latestPresentation = sample.metrics?.latestPresentationSample;
      const targetRunStart = targetRow === sourceRow ? sourceRunStart : targetRow.replayStartedAtEpochMs;
      const firstSessionPresentationEpochMs = Number.isFinite(sample.metrics?.performanceTimeOriginMs) &&
        Number.isFinite(sample.metrics?.firstPresentedSessionAtMs)
        ? sample.metrics.performanceTimeOriginMs + sample.metrics.firstPresentedSessionAtMs : null;
      if (targetRow === sourceRow && initialArm === "A" &&
          targetRow.startup.firstReaderAttachedObservedUpperMs === undefined &&
          events.some(event => event.stage === "attached") &&
          sourceRunAcceptedEpochMs !== null) {
        targetRow.startup.firstReaderAttachedObservedUpperMs = sample.epochMs - sourceRunAcceptedEpochMs;
      }
      const hasCurrentRunPresentation = readinessPresentationMatchesRun(presentationAnchor,
        latestPresentation, sample.metrics?.performanceTimeOriginMs, targetRunStart);
      if (hasCurrentRunPresentation) {
        const owner = sample.presentationProbe?.rendererWorkerOrdinal;
        if (Number.isSafeInteger(owner) && targetRow.rendererWorkerOrdinal === undefined) targetRow.rendererWorkerOrdinal = owner;
        else if (Number.isSafeInteger(owner) && targetRow.rendererWorkerOrdinal !== owner) {
          targetRow.profileValidationErrors.push("renderer worker changed during the measured row");
        }
      }
      if (targetRow === sourceRow && sourceRunAcceptedEpochMs !== null && hasCurrentRunPresentation &&
          sample.controls?.toggleEnabled === true &&
          targetRow.startup.firstControlsReadyObservedUpperMs === undefined) {
        targetRow.startup.firstControlsReadyObservedUpperMs = sample.epochMs - sourceRunAcceptedEpochMs;
      }
      if (targetRow === sourceRow && hasCurrentRunPresentation &&
          sourceRunStart !== null &&
          sourceRunAcceptedEpochMs !== null &&
          firstSessionPresentationEpochMs !== null && firstSessionPresentationEpochMs >= sourceRunStart &&
          Number.isSafeInteger(attachedPublication?.session) &&
          targetRow.startup.firstRenderSubmissionCompleteUpperMs === undefined) {
        targetRow.startup.firstRenderSubmissionCompleteUpperMs = firstSessionPresentationEpochMs - sourceRunAcceptedEpochMs;
        targetRow.startup.firstPresentedMs = targetRow.startup.firstRenderSubmissionCompleteUpperMs;
        targetRow.startup.firstPresentedLimitation = "renderer firstPresentedSessionAtMs receipt; observer timestamp, not GPU completion or physical scanout";
      }
      const metric = sample.metrics;
      targetRow.rendererGpuIdentity = metric?.rendererGpuIdentity ?? targetRow.rendererGpuIdentity ?? null;
      if (targetRow.rendererGpuIdentity !== null) {
        targetRow.gpuClassification = classifyRendererGpuIdentity(targetRow.rendererGpuIdentity, backend);
      }
      if (metric?.backend !== undefined && metric.backend !== (backend === "webgpu" ? "WebGPU" : "WebGL2")) {
        targetRow.profileValidationErrors.push(`actual backend ${metric.backend} differs from ${backend}`);
      }
      if (targetRow.startup.firstUsefulMs === undefined && metric?.ready && metric.objectCount > 0 &&
          hasCurrentRunPresentation) {
        const key = `${metric.presentedSession}:${metric.presentedFrames}:${metric.time}`;
        const captureDue = instrumentationMode === "bounded-diagnostics" ||
          sample.epochMs - (lastUsefulCaptureAt.get(targetRow) ?? -Infinity) >= usefulCaptureIntervalMs;
        if (key !== lastCaptureKey && captureDue) {
          lastCaptureKey = key;
          lastUsefulCaptureAt.set(targetRow, sample.epochMs);
          const bytes = await page.locator("#scene").screenshot();
          const ink = sceneInkPixels(PNG.sync.read(bytes));
          const captureEpochMs = await page.evaluate(() => performance.timeOrigin + performance.now());
          const measurementOriginEpochMs = targetRow === sourceRow
            ? sourceRunAcceptedEpochMs : targetRow.replayStartedAtEpochMs;
          if (ink >= 128 && Number.isFinite(measurementOriginEpochMs)) {
            targetRow.startup.firstUsefulMs = captureEpochMs - measurementOriginEpochMs;
            targetRow.startup.usefulObservationUpperMs = targetRow.startup.firstUsefulMs;
            targetRow.firstUsefulCaptureEpochMs = captureEpochMs;
            await writeFile(path.join(output, `${source.id}-${trial}-${targetRow.arm}-${stratum}-first.png`), bytes);
          }
        }
      }
      if (sample.status === "error") throw new Error(sample.problem || "source failed");
      return sample;
    };

    const deadline = performance.now() + 180_000;
    let current = navigation;
    while (sourceRow.startup.firstUsefulMs === undefined && performance.now() < deadline) {
      current = await read();
      await page.waitForTimeout(observationPollIntervalMs);
    }
    assert.ok(Number.isFinite(sourceRow.startup.firstUsefulMs), "no useful first-pass presentation");
    const firstState = await observation(page);
    const initiallyPlaying = firstState.controls?.playing === "true";
    await readinessCommand(page, sourceRow, !initiallyPlaying);
    await readinessCommand(page, sourceRow, initiallyPlaying);

    const runDeadline = performance.now() + Math.max(180_000, source.durationSeconds * 10_000);
    const sourceDisposition = initialArm === "A" ? "looped" : "observed-complete";
    const lifecycleComplete = sample => {
      const events = sample.executionDiagnostics?.events ?? [];
      return ["source-complete", "terminal-adopted"].every(stage => events.some(event => event.stage === stage)) &&
        events.some(event => event.stage === "disposition" && event.disposition === sourceDisposition);
    };
    while (!(Number.isFinite(current.run?.sourceRunCompletedAtMs) && current.inFlight === false &&
             lifecycleComplete(current)) && performance.now() < runDeadline) {
      current = await read();
      await page.waitForTimeout(observationPollIntervalMs);
    }
    assert.ok(Number.isFinite(current.run?.sourceRunCompletedAtMs) && current.inFlight === false &&
      lifecycleComplete(current), "source cleanup, terminal adoption, or disposition was not observed before timeout");
    sourceRow.startup.sourceRunCompletedAtEpochMs = current.originMs + current.run.sourceRunCompletedAtMs;
    if (Number.isFinite(sourceRunAcceptedEpochMs)) {
      sourceRow.startup.sourceCompleteMs =
        sourceRow.startup.sourceRunCompletedAtEpochMs - sourceRunAcceptedEpochMs;
    }
    const timingOriginErrors = readinessRunTimingOriginErrors(sourceRow.startup);
    assert.deepEqual(timingOriginErrors, [], "source run acceptance/start/completion timestamps are missing or reversed");
    sourceRow.startup.sourceRunTiming = current.run.pythonWorkerRunTiming ?? null;
    sourceRow.startup.renderPreparation = current.run.renderPreparation ?? null;
    sourceRow.endTrace = current.executionDiagnostics;
    sourceRow.traceCompleteness = sourceRow.traceCompleteness ?? null;
    const sourceAttached = sourceRow.endTrace?.events?.find(event => event.stage === "attached")?.publication;
    const sourceCadenceCapture = await rawCadenceCapture(page, sourceRow.rendererWorkerOrdinal);
    sourceRow.rawCadenceCoverage = { error: sourceCadenceCapture.error,
      headerObservedAtEpochMs: sourceCadenceCapture.headerObservedAtEpochMs,
      capacity: sourceCadenceCapture.capacity, latestMetricsEpochMs: sourceCadenceCapture.latestMetricsEpochMs,
      samplesReceived: sourceCadenceCapture.samples.length, workers: sourceCadenceCapture.workers,
      generationChanges: sourceCadenceCapture.generationChanges };
    sourceRow.activeFrameGap = activeFrameGapEvidence(sourceCadenceCapture, {
      startEpochMs: sourceRunAcceptedEpochMs,
      endEpochMs: sourceRow.firstTraversalEndObservedAtEpochMs,
      session: sourceAttached?.session,
      rendererWorkerOrdinal: sourceRow.rendererWorkerOrdinal,
      targetHz: readinessContract.reference.refreshHz,
      postTerminalReceiverSession: readinessPostTerminalReceiverSession(sourceRow.endTrace, sourceAttached?.session),
    });
    sourceRow.profileValidationErrors.push(...sourceRow.activeFrameGap.coverageErrors);
    sourceRow.readerLeadSummary = summarizeReaderLead(sourceRow.readerLeadObservations);
    if ((initialArm === "A" && sourceRow.readerLeadSummary.count === 0) ||
        sourceRow.readerLeadObservations.length >= 4096) {
      sourceRow.profileValidationErrors.push("prepared source reader lead is missing or exceeded its bound");
    }
    sourceRow.rendererSamples = await page.evaluate(() =>
      globalThis.__noonReadinessPresentationProbe?.samples ?? []);
    sourceRow.presentationSampleEnd = sourceRow.rendererSamples.length;
    delete sourceRow.cadenceObservations;
    sourceRow.uiEvents = await page.evaluate(() => globalThis.__noonReadinessPresentationProbe?.uiEvents ?? []);
    sourceRow.bufferingEvidence = readinessBufferingEvidence(sourceRow.endTrace, {
      startEpochMs: sourceRunAcceptedEpochMs,
      endEpochMs: sourceRow.firstTraversalEndObservedAtEpochMs,
      required: initialArm === "A",
      session: sourceAttached?.session,
    });
    sourceRow.profileValidationErrors.push(...sourceRow.bufferingEvidence.errors);
    replayRow.presentationSampleStart = sourceRow.presentationSampleEnd;
    replayRow.uiEventStart = sourceRow.uiEvents.length;
    const expectedProfile = initialArm === "A" ? "incremental" : "observed";
    const trace = current.executionDiagnostics;
    sourceRow.profileValidationErrors.push(...validateReadinessTrace(trace, {
      sourceId: source.id, requestedProfile: expectedProfile, attachedProfile: expectedProfile,
      disposition: sourceDisposition,
    }));
    if (trace && Array.isArray(trace.events)) {
      sourceRow.requestedProfile = trace.requestedProfile;
      sourceRow.actualProfileTransitions = trace.events.filter(event =>
        event.stage === "attached" || event.stage === "profile-change");
      sourceRow.disposition = trace.events.findLast(event => event.stage === "disposition") ?? null;
      replayRow.requestedProfile = trace.requestedProfile;
      replayRow.actualProfileTransitions = sourceRow.actualProfileTransitions;
      replayRow.disposition = sourceRow.disposition;
    }
    const sourceIdentity = sourceRow.diagnosticIdentity;
    assert.ok(sourceIdentity?.runRequestId !== null && sourceIdentity?.runRequestId !== undefined,
      "source run identity missing");
    assert.equal(new Set(sourceRow.sourceInvocationIds).size, 1,
      "source run identity changed before retained replay");
    sourceRow.status = sourceRow.profileValidationErrors.length === 0 ? "observed" : "profile-mismatch";
    if (initialArm === "C") {
      sourceRow.startup.firstReaderAttachedObservedUpperMs = null;
      sourceRow.startup.firstReaderLimitation = "observed source has no independent incremental-prefix reader milestone";
    }
    replayRow.previousWorkersRetirement = sourceRow.previousWorkersRetirement ?? null;

    const controls = await observation(page);
    assert.equal(controls.controls?.controllable, "true", "completed deterministic source cannot replay");
    activeRow = replayRow;
    if (controls.controls.playing === "true") await readinessCommand(page, replayRow, false);
    const identityBeforeReplay = await observation(page);
    const restartLoopCount = Number.isSafeInteger(identityBeforeReplay.preparedPlayback?.observation?.loopCount)
      ? identityBeforeReplay.preparedPlayback.observation.loopCount : null;
    replayRow.restartLoopCount = restartLoopCount;
    if (replayArm === "B" && restartLoopCount === null) {
      replayRow.profileValidationErrors.push("prepared replay has no cumulative Rust loop counter before restart");
    }
    const beforeFrames = identityBeforeReplay.metrics?.presentedFrames;
    const restartIssued = await issueReadinessClick(page, page.locator(".playback-restart"), "playback-restart");
    const replayStartedAtEpochMs = restartIssued.requestEpochMs;
    replayRow.replayStartedAtEpochMs = replayStartedAtEpochMs;
    await page.waitForFunction(() => {
      const state = document.querySelector(".playback-controls")?.dataset;
      return state?.busy === "false" && Number(state.elapsedSeconds) === 0;
    });
    const restartCompleted = await page.evaluate(requestEpoch =>
      [...(window.__noonReadinessPresentationProbe?.uiEvents ?? [])].reverse().find(event =>
        event.kind === "state" && event.attribute === "data-busy" && event.state?.busy === "false" &&
        event.epochMs >= requestEpoch) ?? null, replayStartedAtEpochMs);
    assert.ok(restartCompleted, "restart command completion transition was not observed");
    const restartEngineResult = await readinessCommandResult(page, restartIssued.commandResultIndex, "restart_playback");
    const restartPresented = await observation(page);
    if (restartLoopCount !== null &&
        restartPresented.preparedPlayback?.observation?.loopCount !== restartLoopCount) {
      replayRow.profileValidationErrors.push("prepared restart advanced the cumulative loop counter before its settled frame");
    }
    replayRow.cadenceObservations.push({ epochMs: restartPresented.epochMs,
      metrics: restartPresented.metrics, controls: restartPresented.controls,
      preparedPlayback: restartPresented.preparedPlayback });
    const latestReplay = restartPresented.metrics?.latestPresentationSample;
    const minimalReplaySample = latestReplay ? { ...latestReplay,
      clockOriginMs: restartPresented.metrics.performanceTimeOriginMs } : null;
    const restartPublicationSample = instrumentationMode === "bounded-diagnostics"
      ? await readinessPublicationSample(page, restartEngineResult?.commandPublication) : null;
    const restartRuntimeReceipt = readinessCommandReceipt(restartEngineResult, replayStartedAtEpochMs,
      restartPublicationSample ?? minimalReplaySample);
    const restartPublication = restartRuntimeReceipt.commandPublication ??
      (restartRuntimeReceipt.matchingPresentation ? {
        session: restartRuntimeReceipt.matchingPresentation.session,
        sequence: restartRuntimeReceipt.matchingPresentation.sequence,
      } : null);
    assert.ok(Number.isSafeInteger(restartPublication?.session) &&
      Number.isSafeInteger(restartPublication?.sequence),
    "restart command has no renderer publication identity for retained replay");
    replayRow.restartPublication = restartPublication;
    replayRow.startup.restartCompletedUiEpochMs = restartCompleted.epochMs;
    replayRow.commands.push({ name: "restart", automationIssuedEpochMs: restartIssued.automationIssuedEpochMs,
      requestEpochMs: replayStartedAtEpochMs,
      uiFeedbackEpochMs: restartIssued.feedback?.epochMs ?? null,
      uiFeedbackMs: restartIssued.feedback ? restartIssued.feedback.epochMs - replayStartedAtEpochMs : null,
      uiFeedbackKind: restartIssued.feedback ? "data-busy=true" : null,
      commandCompletedUiEpochMs: replayRow.startup.restartCompletedUiEpochMs,
      commandCompletionUpperMs: restartPresented.epochMs - replayStartedAtEpochMs,
      rendererObservation: rendererSnapshot(restartPresented.metrics),
      ...restartRuntimeReceipt,
      runtimeAcceptanceLimitation: restartRuntimeReceipt.runtimeAcceptanceEpochMs === null
        ? "endpoint command result did not expose an acceptance timestamp" : null,
      presentationReceiptLimitation: restartRuntimeReceipt.matchingPresentation === null
        ? "endpoint command result did not expose a validated matching publication receipt" : null });
    replayRow.startup.firstPresentedMs = Number.isFinite(restartRuntimeReceipt.presentationSettledEpochMs)
      ? restartRuntimeReceipt.presentationSettledEpochMs - replayStartedAtEpochMs : null;
    replayRow.startup.firstPresentedLimitation = Number.isFinite(replayRow.startup.firstPresentedMs)
      ? "command-correlated restart presentation receipt; observer timestamp, not GPU completion or physical scanout"
      : "restart command did not expose a matching presentation receipt";
    if (restartPresented.controls?.playing !== "true") await readinessCommand(page, replayRow, true);
    let replayCurrent = await read(replayRow);
    const clockToleranceSeconds = Math.max(0.001, 2 * observationPollIntervalMs / 1000);
    const replayCycle = () => readinessReplayCycleEvidence(replayRow.cadenceObservations, {
      restartPublication, restartLoopCount, requestEpochMs: replayStartedAtEpochMs,
      durationSeconds: source.durationSeconds, terminalHoldStartSeconds: terminalStill[0],
      clockToleranceSeconds,
      acceptsTerminalPublication: time => {
        try { assertNaturalFirstPassPublication(showcaseEntry, source.durationSeconds, time); return true; }
        catch { return false; }
      },
    });
    let replayCycleComplete = replayCycle();
    const replayDeadline = performance.now() + Math.max(180_000, source.durationSeconds * 10_000);
    while (replayCycleComplete === null && performance.now() < replayDeadline) {
      replayCurrent = await read(replayRow);
      replayCycleComplete = replayCycle();
      await page.waitForTimeout(observationPollIntervalMs);
    }
    replayCycleComplete = replayCycle();
    assert.ok(replayCycleComplete !== null,
      `retained replay did not render its manifest terminal hold and complete a postrestart loop: ${JSON.stringify({
        restartPublication, terminalStill, samples: replayRow.cadenceObservations.slice(-12).map(sample => ({
          epochMs: sample.epochMs, elapsedSeconds: sample.controls?.elapsedSeconds,
          playing: sample.controls?.playing, latestPresentationSample: sample.metrics?.latestPresentationSample,
        })),
      })}`);
    assert.ok(Number.isFinite(replayRow.startup.firstUsefulMs), "retained replay produced no useful picture in its first cycle");
    assert.ok(replayCurrent.metrics?.presentedFrames > beforeFrames,
      "retained replay produced no new presentation");
    const replayCadenceCapture = await rawCadenceCapture(page, replayRow.rendererWorkerOrdinal);
    replayRow.rawCadenceCoverage = { error: replayCadenceCapture.error,
      headerObservedAtEpochMs: replayCadenceCapture.headerObservedAtEpochMs,
      capacity: replayCadenceCapture.capacity, latestMetricsEpochMs: replayCadenceCapture.latestMetricsEpochMs,
      samplesReceived: replayCadenceCapture.samples.length, workers: replayCadenceCapture.workers,
      generationChanges: replayCadenceCapture.generationChanges };
    replayRow.activeFrameGap = activeFrameGapEvidence(replayCadenceCapture, {
      startEpochMs: replayStartedAtEpochMs,
      endEpochMs: replayCycleComplete.wrapped.observationEpochMs,
      session: restartPublication.session,
      rendererWorkerOrdinal: replayRow.rendererWorkerOrdinal,
      targetHz: readinessContract.reference.refreshHz,
    });
    replayRow.profileValidationErrors.push(...replayRow.activeFrameGap.coverageErrors);
    replayRow.readerLeadSummary = summarizeReaderLead(replayRow.readerLeadObservations);
    if ((replayArm === "B" && replayRow.readerLeadSummary.count === 0) ||
        replayRow.readerLeadObservations.length >= 4096) {
      replayRow.profileValidationErrors.push("prepared replay reader lead is missing or exceeded its bound");
    }
    replayRow.startup.replayRequestToRestartCompletionMs =
      replayRow.startup.restartCompletedUiEpochMs - replayStartedAtEpochMs;
    replayRow.startup.replayToWrapMs = replayCycleComplete.wrapped.observationEpochMs - replayStartedAtEpochMs;
    replayRow.replayCycleEvidence = replayCycleComplete;
    const actualEnd = replayCurrent.controls?.seekEnd;
    assertNaturalFirstPassPublication(showcaseEntry, actualEnd, actualEnd);
    const seekTargets = readinessWarmSeekTargets(showcaseEntry, actualEnd);
    assert.equal(seekTargets.length, 5, "manifest does not supply five distinct warm-seek targets");
    replayRow.actualClosedEnd = actualEnd;
    replayRow.warmSeekTargets = seekTargets;
    if (replayCurrent.controls?.playing === "true") replayCurrent = await readinessCommand(page, replayRow, false);
    for (const targetTime of seekTargets) replayCurrent = await readinessWarmSeek(page, replayRow, targetTime);
    const replayTrace = replayCurrent.executionDiagnostics;
    replayRow.endTrace = replayTrace;
    const replayExpectedProfile = replayArm === "B" ? "incremental" : "observed";
    const replayDisposition = replayArm === "B" ? "looped" : "observed-complete";
    replayRow.profileValidationErrors.push(...validateReadinessTrace(replayTrace, {
      sourceId: source.id, requestedProfile: replayExpectedProfile,
      attachedProfile: replayExpectedProfile, disposition: replayDisposition,
    }));
    if (replayTrace?.runRequestId !== null && replayTrace?.runRequestId !== undefined &&
        !replayRow.sourceInvocationIds.includes(replayTrace.runRequestId)) {
      replayRow.sourceInvocationIds.push(replayTrace.runRequestId);
    }
    if (replayTrace?.runRequestId !== sourceIdentity.runRequestId ||
        replayTrace?.interpreterGeneration !== sourceIdentity.interpreterGeneration) {
      replayRow.profileValidationErrors.push("replay replaced the original source/interpreter identity");
    }
    if (replayRow.previousWorkersRetirement && !replayRow.previousWorkersRetirement.allRetired) {
      replayRow.profileValidationErrors.push("previous page workers were not confirmed retired");
    }
    const stagedReplaySamples = await page.evaluate(start =>
      (globalThis.__noonReadinessPresentationProbe?.samples ?? []).slice(start), sourceRow.presentationSampleEnd);
    replayRow.rendererSamples = readinessRendererSamples(stagedReplaySamples, replayRow.cadenceObservations);
    delete replayRow.cadenceObservations;
    const firstReplayRender = replayRow.rendererSamples.map(sample => ({ sample,
      epochMs: sample.clockOriginMs + sample.presentedAtMs })).filter(value =>
      Number.isFinite(value.epochMs) && value.epochMs >= replayStartedAtEpochMs &&
      Number.isSafeInteger(value.sample.session) && Number.isSafeInteger(value.sample.sequence))
      .sort((left, right) => left.epochMs - right.epochMs)[0];
    const firstReplayRenderEpochMs = firstReplayRender?.epochMs ??
      restartRuntimeReceipt.presentationSettledEpochMs;
    replayRow.startup.firstRenderSubmissionCompleteUpperMs = Number.isFinite(firstReplayRenderEpochMs)
      ? firstReplayRenderEpochMs - replayStartedAtEpochMs : null;
    replayRow.startup.firstRenderSubmissionLimitation = firstReplayRender
      ? "renderer.render() call completion bounds earlier backend submission and the Noon presentation receipt; not exact queue-submit start, GPU completion or physical scanout"
      : Number.isFinite(firstReplayRenderEpochMs)
        ? "restart endpoint's command-correlated Noon presentation receipt bounds earlier backend submission; not exact queue-submit start, GPU completion or physical scanout"
      : "no renderer publication was observed after the actual replay request";
    replayRow.uiEvents = await page.evaluate(start =>
      (globalThis.__noonReadinessPresentationProbe?.uiEvents ?? []).slice(start), sourceRow.uiEventStart);
    replayRow.bufferingEvidence = readinessBufferingEvidence(replayTrace, {
      startEpochMs: replayStartedAtEpochMs,
      endEpochMs: replayCycleComplete.wrapped.observationEpochMs,
      required: replayArm === "B",
      session: restartPublication.session,
    });
    replayRow.profileValidationErrors.push(...replayRow.bufferingEvidence.errors);
    replayRow.status = replayRow.profileValidationErrors.length === 0 ? "observed" : "profile-mismatch";
    const submissionBoundLimitation = "upper bound from renderer.render() call completion; backend submission and the Noon presentation receipt occur no later than return, but exact queue-submit start, GPU completion and physical scanout are not measured";
    replayRow.startup.firstSubmittedMs = replayRow.startup.firstRenderSubmissionCompleteUpperMs;
    replayRow.startup.firstSubmittedLimitation = replayRow.startup.firstRenderSubmissionCompleteUpperMs === null
      ? replayRow.startup.firstRenderSubmissionLimitation : submissionBoundLimitation;
    sourceRow.startup.firstSubmittedMs = sourceRow.startup.firstRenderSubmissionCompleteUpperMs ?? null;
    sourceRow.startup.firstSubmittedLimitation = sourceRow.startup.firstSubmittedMs === null
      ? "no first renderer call completion was observed" : submissionBoundLimitation;
    assert.equal((await page.evaluate(() => globalThis.__noonReadinessPresentationProbe?.droppedUiEvents ?? 0)), 0,
      "bounded UI event observer dropped state transitions");
    const probeStatus = await page.evaluate(() => ({ error: globalThis.__noonReadinessPresentationProbe?.error ?? null,
      droppedUiEvents: globalThis.__noonReadinessPresentationProbe?.droppedUiEvents ?? null }));
    assert.equal(probeStatus.error, null);
    assert.equal(probeStatus.droppedUiEvents, 0);
    assert.deepEqual(sourceRow.pageErrors, []);
    assert.deepEqual(sourceRow.consoleErrors, []);
    if (wasmAccounting) {
      const snapshot = await wasmAccountingSnapshot(workers);
      sourceRow.wasmAccounting = snapshot;
      replayRow.wasmAccounting = snapshot;
    }
    if (wasmAccounting) {
      const snapshot = await wasmAccountingSnapshot(workers);
      sourceRow.wasmAccounting = snapshot;
      replayRow.wasmAccounting = snapshot;
    }
  } catch (error) {
    sourceRow.status = "failed";
    sourceRow.error = String(error.stack ?? error);
    sourceRow.endTrace = sourceRow.lastObservation?.executionDiagnostics ?? null;
    replayRow.status = "failed";
    replayRow.error = sourceRow.error;
    replayRow.endTrace = replayRow.lastObservation?.executionDiagnostics ?? null;
    await page.screenshot({ path: path.join(output,
      `${source.id}-${trial}-${initialArm}-${stratum}-failure.png`) }).catch(() => {});
  } finally {
    sourceRow.memory = await sampler.stop();
    replayRow.memory = sourceRow.memory;
    sourceRow.startedAtEpochMs = startedAtEpochMs;
    replayRow.startedAtEpochMs = startedAtEpochMs;
    sourceRow.finishedAtEpochMs = Date.now();
    replayRow.finishedAtEpochMs = sourceRow.finishedAtEpochMs;
    page.off("pageerror", onPageError); page.off("console", onConsole);
    await writeReport();
    console.log(JSON.stringify({ source: source.id, arms: [initialArm, replayArm], stratum, trial,
      initialStatus: sourceRow.status, replayStatus: replayRow.status, error: sourceRow.error }));
  }
}

try {
  overlay = await mkdtemp(path.join(os.tmpdir(), "noon-playback-loading-host-"));
  servers.runtime = await packageHost("runtime", env.NOON_PLAYBACK_LOADING_RUNTIME_ROOT);
  const stratumGroups = [READINESS_STRATA.slice(0, 2), ["cold-network"], ["reused-healthy-interpreter"]];
  for (const source of readinessContract.sources) {
      const entry = inventory.find(item => item.id === source.id);
      assert.ok(entry && entry.path === source.path && entry.duration === source.durationSeconds,
        `readiness source metadata changed: ${source.id}`);
      const sourceFile = path.join(root, "web", entry.path);
      const sourceHash = await fileHash(sourceFile);
      assert.equal(sourceHash, source.sha256, `${source.id} readiness source bytes changed`);
      for (let trial = -1; trial < trials; trial++) {
        const pairs = readinessMode === "baseline" ? [["C", "D"]] :
          (trial % 2 === 0 ? [["A", "B"], ["C", "D"]] : [["C", "D"], ["A", "B"]]);
        for (const [initialArm, replayArm] of pairs) {
          for (const strata of stratumGroups) {
            browserServer = await playwright.chromium.launchServer({ headless: false,
              ...(env.NOON_PLAYBACK_LOADING_EXECUTABLE ? { executablePath: env.NOON_PLAYBACK_LOADING_EXECUTABLE } : { channel: "chromium" }),
              args: browserArgs(backend, { gpuMode: "hardware" }) });
            try {
              browser = await playwright.chromium.connect(browserServer.wsEndpoint());
              report.browserVersion = browser.version();
              const contextIdentity = { id: randomUUID(), createdAtEpochMs: Date.now() };
              const context = await browser.newContext({
                viewport: readinessContract.reference.viewport,
                deviceScaleFactor: readinessContract.reference.deviceScaleFactor,
                serviceWorkers: "block",
              });
              await installReadinessPresentationProbe(context, {
                profilePublicationStages,
              });
              if (initialArm === "C") await context.addInitScript(() => {
                globalThis.__NOON_PLAYGROUND_TEST_HOOKS__ = {
                  authoringRunOptions: () => ({ preparedPlayback: false }),
                };
              });
              const page = await context.newPage(); page.setDefaultTimeout(180_000);
              assert.equal(browser.version(), readinessContract.reference.browser.replace(/^Chromium /, ""),
                "readiness browser version differs from the pinned reference");
              const workers = []; page.on("worker", worker => workers.push(worker));
              if (wasmAccounting) await installWasmAccounting(page);
              let resourceCacheInstalled = false;
              for (const stratum of strata) {
                const coldNetwork = stratum === "cold-network"
                  ? await installColdNetworkObservation(context) : null;
                const jobCacheBefore = coldNetwork === null ? null : cache.stats();
                if (coldNetwork === null && !resourceCacheInstalled) {
                  await cache.install(context);
                  resourceCacheInstalled = true;
                }
                const pairStart = report.cases.length;
                await measureReadinessPair(page, workers, source, initialArm, replayArm, stratum,
                  trial, trial < 0, contextIdentity);
                const measuredPair = report.cases.slice(pairStart);
                if (measuredPair.length !== 2 || measuredPair.some(row => row.status !== "observed")) {
                  throw new Error(`aborting after first failed readiness pair: ${source.id}/${initialArm}+${replayArm}/${stratum}/${trial}`);
                }
                if (coldNetwork !== null) {
                  const evidence = summarizeColdNetworkObservations(coldNetwork.records, cache.stats().baseUrl);
                  const jobCacheAfter = cache.stats();
                  assert.equal(coldNetwork.overflow(), false, "cold-network request log exceeded its bound");
                  assert.ok(evidence.pyodideRequestCount > 0 && evidence.pyodideResponseStatuses.every(status => status >= 200 && status < 400),
                    "cold-network stratum did not observe successful Pyodide network responses");
                  assert.equal(evidence.serviceWorkerResponses, 0, "cold-network response came from a service worker");
                  assert.equal(evidence.routeFailureCount, 0, "cold-network request routing failed");
                  const cacheCounters = ({ requests, hits, upstreamRequests, retainedBytes }) =>
                    ({ requests, hits, upstreamRequests, retainedBytes });
                  assert.deepEqual(cacheCounters(jobCacheAfter), cacheCounters(jobCacheBefore),
                    "cold-network stratum accessed the job-local Pyodide cache");
                  const rows = report.cases.filter(row => row.sourceId === source.id && row.trial === trial &&
                    row.stratum === stratum && (row.arm === initialArm || row.arm === replayArm));
                  for (const row of rows) row.networkCacheObservation = {
                    mode: "bypass-browser-and-job-local-client-caches", browserHttpCache: "disabled-by-context-routing",
                    jobResourceCache: "not installed during this stratum",
                    jobCacheCountersBefore: cacheCounters(jobCacheBefore), jobCacheCountersAfter: cacheCounters(jobCacheAfter),
                    requestHeaders: "Cache-Control: no-cache, Pragma: no-cache",
                    serviceWorkers: "blocked", cdnEdgeCold: false, routeOverhead: "present",
                    browserProcessId: row.browserProcessId, clientContextId: row.clientContextId,
                    ...evidence, responses: coldNetwork.records,
                  };
                  await context.unroute("**/*");
                }
                assert.equal(await fileHash(sourceFile), sourceHash, "source changed during measurement");
              }
              const pairRows = report.cases.filter(row => row.sourceId === source.id && row.trial === trial &&
                strata.includes(row.stratum) && (row.arm === initialArm || row.arm === replayArm));
              const identity = await page.evaluate(async () =>
                (await __noonExampleGallery.executionMetrics({ includeGpuIdentity: true })).metrics.rendererGpuIdentity).catch(() => null);
              report.observedReference ??= { browserVersion: browser.version(), rendererGpuIdentity: identity,
                gpuClassification: identity ? classifyRendererGpuIdentity(identity, backend) : null,
                hostCpu: report.host.cpu, macOSVersion: report.host.macOSVersion };
              assert.ok(identity, "readiness requires the actual renderer adapter identity");
              assert.equal(classifyRendererGpuIdentity(identity, backend), "hardware-like-unverified",
                "readiness rejects unknown or software GPU adapters");
              const referenceMachine = readinessContract.reference.host.split(",", 1)[0];
              assert.ok(report.host.cpu?.includes(referenceMachine), "readiness host differs from the pinned machine");
              assert.equal(report.host.macOSVersion, readinessContract.reference.os.replace(/^macOS /, ""),
                "readiness host differs from the pinned macOS version");
              if (readinessMode === "strict") {
                assert.equal(browser.version(), readinessContract.reference.browser.replace(/^Chromium /, ""));
                assert.deepEqual(identity, readinessContract.reference.actualAdapterIdentity,
                  "actual GPU adapter identity differs from the pinned reference");
              }
              const retirementStartedAt = performance.now();
              const retirementSampler = createProcessTreeRssSampler(browserServer.process().pid);
              retirementSampler.start();
              await context.close();
              const workerRetirement = await Promise.all(workers.map(worker => retiredWorkerObservation(worker)));
              await new Promise(resolve => setTimeout(resolve, 750));
              const postRetirementMemory = await retirementSampler.stop();
              const allWorkersRetired = workerRetirement.length > 0 && workerRetirement.every(item => item.retired === true);
              const retirement = { contextCloseMs: performance.now() - retirementStartedAt,
                ownerRetirementMs: allWorkersRetired
                  ? performance.now() - retirementStartedAt : null,
                workers: workerRetirement, allWorkersRetired,
                postRetirementMemory };
              for (const row of pairRows) {
                row.retirement = retirement;
                row.retirementMs = retirement.ownerRetirementMs;
                row.postRetirementMemory = postRetirementMemory;
                if (!retirement.allWorkersRetired && row.status === "observed") {
                  row.status = "profile-mismatch";
                  row.profileValidationErrors.push("context teardown did not confirm worker retirement");
                }
              }
              for (const row of pairRows) {
                row.rendererGpuIdentity ??= identity;
                if (row.rendererGpuIdentity !== null) row.gpuClassification = classifyRendererGpuIdentity(row.rendererGpuIdentity, backend);
                row.browserVersion = browser.version();
              }
            } finally {
              try { await browser?.close(); }
              finally { await browserServer.close(); browser = null; browserServer = null; }
            }
          }
        }
      }
  }
  report.summaries = summarizeReadiness(report.cases, readinessContract);
  const casesObserved = report.cases.every(row => row.status === "observed");
  const strictErrors = readinessMode === "strict" ? validateReadinessResults(report.cases, readinessContract) : [];
  if (readinessMode === "strict") report.readinessValidationErrors = strictErrors;
  if (strictErrors.length > 0) process.exitCode = 1;
  report.status = readinessMode === "baseline"
    ? casesObserved ? "baseline-observed" : "baseline-incomplete"
    : casesObserved && strictErrors.length === 0 ? "observed" : "incomplete";
} catch (error) {
  report.status = "failed"; report.error = String(error.stack ?? error); process.exitCode = 1;
} finally {
  report.resourceCache = cache.stats();
  try { await writeReport(); }
  finally {
    try { await browser?.close(); }
    finally {
      try { await browserServer?.close(); }
      finally {
        for (const server of Object.values(servers)) await server.close();
        if (overlay) await rm(overlay, { recursive: true, force: true });
      }
    }
  }
}
if (!(readinessMode === "baseline" && report.status === "baseline-observed") && report.status !== "observed") process.exitCode = 1;
