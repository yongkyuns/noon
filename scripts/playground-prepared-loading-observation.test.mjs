import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { sceneInkPixels, distribution, summarizeReadiness,
  validateReadinessContract, validateReadinessTrace, validateReadinessResults,
  PRODUCT_GATE_THRESHOLD_ENVS, readinessCommandReceipt, readinessInstrumentationPlan,
  activeCadenceWindows, summarizeColdNetworkObservations,
  validateReusedHealthyInterpreter, readinessPresentationMatchesRun,
  readinessReplayCycleEvidence, readinessRendererSamples, readinessWarmSeekTargets,
  READINESS_STRATA } from "./playground-prepared-loading-observation.mjs";
import { SHOWCASE_PROFILE_EXPECTATIONS } from "./playground-profile-expectations.mjs";

function canvas() {
  const data = new Uint8Array(40 * 40 * 4);
  for (let i = 0; i < data.length; i += 4) data.set([9, 12, 19, 255], i);
  return { data, width: 40, height: 40 };
}

test("blank canvas, CSS-like borders and transparent RGB do not prove useful scene ink", () => {
  const image = canvas();
  assert.equal(sceneInkPixels(image), 0);
  for (let y = 0; y < 40; y++) for (let x = 0; x < 40; x++) {
    if (x < 8 || y < 8 || x >= 32 || y >= 32) image.data.set([255, 255, 255, 255], (y * 40 + x) * 4);
  }
  image.data.set([255, 0, 255, 0], (20 * 40 + 20) * 4);
  assert.equal(sceneInkPixels(image), 0);
});

test("visible interior content qualifies while imperceptible byte noise does not", () => {
  const image = canvas();
  image.data.set([10, 12, 19, 255], (20 * 40 + 20) * 4);
  assert.equal(sceneInkPixels(image), 0);
  for (let y = 12; y < 25; y++) for (let x = 12; x < 25; x++) {
    image.data.set([60, 150, 255, 255], (y * 40 + x) * 4);
  }
  assert.equal(sceneInkPixels(image), 169);
});

test("timing summaries retain sample count and nearest-rank tails", () => {
  assert.deepEqual(distribution([9, 1, 5, 3]), {
    n: 4, min: 1, median: 4, p90: 9, p95: 9, p99: 9, max: 9, percentileMethod: "midpoint median; nearest-rank tails",
  });
  assert.equal(distribution([]).median, null);
});

test("minimal readiness timing disables publication-stage probing and bounds capture overhead", () => {
  assert.deepEqual(readinessInstrumentationPlan("minimal-timing"), {
    observationPollIntervalMs: 100, usefulCaptureIntervalMs: 250, profilePublicationStages: false,
  });
  assert.deepEqual(readinessInstrumentationPlan("bounded-diagnostics"), {
    observationPollIntervalMs: 25, usefulCaptureIntervalMs: 0, profilePublicationStages: true,
  });
  assert.equal(readinessInstrumentationPlan("unknown"), null);
});

test("active cadence summaries exclude static holds and split reset sessions", () => {
  const window = (epochMs, count, session, p99) => ({ epochMs, metrics: {
    presentationIntervalSamples: count, presentedSession: session,
    presentationIntervalMs: count ? { p95: p99 - 1, p99, max: p99 } : null,
  } });
  assert.deepEqual(activeCadenceWindows([
    window(0, 0, 1, 0), window(10, 1, 1, 17), window(20, 2, 1, 18),
    window(1000, 0, 1, 0), window(2000, 1, 2, 19),
  ]), [
    { startedAtEpochMs: 10, sampleCount: 2, endedAtEpochMs: 20, p95Ms: 17, p99Ms: 18, maxMs: 18 },
    { startedAtEpochMs: 2000, sampleCount: 1, endedAtEpochMs: 2000, p95Ms: 18, p99Ms: 19, maxMs: 19 },
  ]);
});

test("readiness draft binds showcase sources and the cumulative Product Gate anchor", async () => {
  const contract = JSON.parse(await readFile(new URL("./fixtures/playground-readiness-contract.json", import.meta.url), "utf8"));
  assert.deepEqual(validateReadinessContract(contract), []);
  assert.deepEqual(READINESS_STRATA, ["fresh-process-warm-resource-cache",
    "reload-new-workers-warm-resource-cache", "cold-network", "reused-healthy-interpreter"]);
  assert.ok(validateReadinessContract(contract, { strict: true }).some(error => /not been reviewed and frozen/.test(error)));
  const manifest = JSON.parse(await readFile(new URL("../web/python/examples/noon_showcase_manifest.json", import.meta.url), "utf8"));
  assert.deepEqual(contract.sources.map(source => source.id), ["showcase-first-scene", "showcase-dynamic-scene"]);
  for (const source of contract.sources) {
    const profile = SHOWCASE_PROFILE_EXPECTATIONS.find(value => value.id === source.id);
    const entry = manifest.entries.find(value => value.id === source.id);
    assert.deepEqual([source.path, source.sha256, source.durationSeconds],
      [profile.path, profile.sha256, entry.duration]);
  }
  const anchor = JSON.parse(await readFile(new URL("../.github/ci/product-performance-anchor.json", import.meta.url), "utf8"));
  const anchorLock = new URL(`../.github/ci/${anchor.dependencyLock.file}`, import.meta.url);
  const sha256 = async url => createHash("sha256").update(await readFile(url)).digest("hex");
  assert.equal(contract.productGateAnchor.sourceRevision, anchor.source);
  assert.equal(contract.productGateAnchor.manifestSha256, await sha256(new URL("../.github/ci/product-performance-anchor.json", import.meta.url)));
  assert.equal(contract.productGateAnchor.dependencyLockSha256, await sha256(anchorLock));
  assert.deepEqual(contract.productGateAnchor.workloads, anchor.workloads);
  for (const [key, filename] of Object.entries({ runnerSha256: "../scripts/playground-product-e2e.mjs",
    workloadSha256: "../scripts/playground-product-fps.mjs", comparisonSha256: "../scripts/playground-product-compare.mjs" })) {
    assert.equal(contract.productGateAnchor[key], await sha256(new URL(filename, import.meta.url)));
  }
  const compareSource = await readFile(new URL("../scripts/playground-product-compare.mjs", import.meta.url), "utf8");
  for (const [key, envName] of Object.entries(PRODUCT_GATE_THRESHOLD_ENVS)) {
    const match = compareSource.match(new RegExp(`threshold\\("${envName}",\\s*"([^"]+)"\\)`));
    assert.ok(match, `missing Product Gate default ${envName}`);
    assert.equal(contract.productGateAnchor.thresholds[key], Number(match[1]));
  }
  const ready = structuredClone(contract);
  ready.status = "ready";
  ready.reference.actualAdapterIdentity = { backend: "BrowserWebGpu", vendor: 1, device: 2 };
  ready.bounds = Object.fromEntries(Object.keys(ready.bounds).map(key => [key, 1]));
  ready.pendingCohorts = [];
  assert.deepEqual(validateReadinessContract(ready, { strict: true }), []);
  const pending = structuredClone(ready);
  pending.pendingCohorts = contract.pendingCohorts;
  assert.ok(validateReadinessContract(pending, { strict: true })
    .some(error => /cohorts remain pending: cold-network, reused-healthy-interpreter/.test(error)));
  for (const mutate of [
    value => { value.sources[1].sha256 = "invalid"; },
    value => { delete value.productGateAnchor.thresholds.minFpsRatio; },
    value => { value.sampling.measuredStartsPerArmPerStratum = 3; },
    value => { value.sampling.startupPopulationP99 = true; },
    value => { value.sampling.startupPercentiles.push("p99"); },
    value => { delete value.measurementDefinitions.firstSubmittedMs; },
  ]) {
    const changed = structuredClone(contract);
    mutate(changed);
    assert.notDeepEqual(validateReadinessContract(changed), [], JSON.stringify(changed));
  }
  const missingBounds = structuredClone(ready);
  missingBounds.bounds = {};
  assert.ok(validateReadinessContract(missingBounds, { strict: true }).some(error => /missing approved bound/.test(error)));
});

test("readiness summaries keep failed and warmup starts out of timing samples", async () => {
  const contract = JSON.parse(await readFile(new URL("./fixtures/playground-readiness-contract.json", import.meta.url), "utf8"));
  const sourceId = contract.sources[0].id, stratum = "fresh-process-warm-resource-cache";
  const row = (arm, overrides = {}) => ({ sourceId, arm, stratum, warmup: false, status: "observed",
    startup: { firstUsefulMs: 20 }, commands: [{ name: "pause", feedbackMs: 5, acceptedMs: 8, presentedMs: 12 },
      { name: "seek", commandPhase: "completed-history-warm", feedbackMs: 6, acceptedMs: 9, presentedMs: 13 }],
    activeCadenceWindows: [{ p95Ms: 16, p99Ms: 17 }], bufferingEpisodes: [{ durationMs: 40 }],
    memory: { peakSampledRssBytes: 100 }, ...overrides });
  const summary = summarizeReadiness([
    row("A"), row("A", { warmup: true, startup: { firstUsefulMs: 1 } }),
    row("A", { status: "failed", startup: { firstUsefulMs: 2 } }),
  ], contract).find(value => value.sourceId === sourceId && value.arm === "A" && value.stratum === stratum);
  assert.equal(summary.expectedStarts, 20);
  assert.equal(summary.attemptedStarts, 2);
  assert.equal(summary.observedStarts, 1);
  assert.equal(summary.failedStarts, 1);
  assert.equal(summary.startup.firstUsefulMs.n, 1);
  assert.equal(Object.hasOwn(summary.startup, "runtimeAcceptanceMs"), false,
    "startup does not report an acceptance value without a startup command");
  assert.equal("p99" in summary.startup.firstUsefulMs, false,
    "twenty-start strata do not publish an unsupported startup population p99");
  assert.equal(summary.commands.pause.feedback.n, 1);
  assert.equal(summary.commands.pause.accepted.n, 1,
    "command acceptance remains a separately measured command milestone");
  assert.equal(summary.commands.seek.phase, "completed-history-warm");
  assert.equal(summary.commands.seek.presented.n, 1, "named seek timings remain separate from play/pause");
  assert.equal(summary.activeCadenceWindowP99Ms.n, 1);
  assert.equal(summary.buffering.episodes, 1);
  assert.equal(summary.peakProcessRssBytes.n, 1);
});

test("warm seek targets are five distinct manifest beats or closed terminal endpoints", async () => {
  const manifest = JSON.parse(await readFile(new URL("../web/python/examples/noon_showcase_manifest.json", import.meta.url), "utf8"));
  for (const id of ["showcase-first-scene", "showcase-dynamic-scene"]) {
    const entry = manifest.entries.find(row => row.id === id);
    const actualEnd = entry.duration === 8 ? 7.999999999999999 : entry.duration;
    const targets = readinessWarmSeekTargets(entry, actualEnd);
    assert.equal(targets.length, 5);
    assert.equal(new Set(targets).size, 5);
    assert.equal(targets.at(-1), actualEnd, "the exact closed Rust endpoint remains selectable");
    assert.ok(targets.every(time => entry.beats.some(beat => beat.time === time) ||
      entry.still_intervals.some(([start, end]) => start === time && end === entry.duration) || time === actualEnd));
    assert.equal(readinessWarmSeekTargets(entry, entry.duration + 1e-6).length, 0,
      "an endpoint outside the storyboard's validated duration is rejected");
  }
});

test("readiness requires actual attachment and ordered cleanup/disposition, not requested profile", () => {
  const expected = { sourceId: "showcase-first-scene", requestedProfile: "incremental",
    attachedProfile: "incremental", disposition: "looped" };
  const trace = { schemaVersion: 1, interpreterGeneration: 2, runRequestId: 7,
    context: { exampleId: expected.sourceId, selectionGeneration: 3, runGeneration: 4 }, droppedEvents: 0,
    requestedProfile: "incremental", events: [
      { sequence: 0, stage: "attached", profile: "incremental" },
      { sequence: 1, stage: "terminal-adopted" },
      { sequence: 2, stage: "source-complete" },
      { sequence: 3, stage: "disposition", disposition: "looped" },
    ] };
  assert.deepEqual(validateReadinessTrace(trace, expected), []);
  const requestedIsNotActual = structuredClone(trace);
  requestedIsNotActual.events[0].profile = "observed";
  assert.ok(validateReadinessTrace(requestedIsNotActual, expected).includes("unexpected actual attached profile"));
  const lateFallback = structuredClone(trace);
  lateFallback.events.splice(3, 0, { sequence: 3, stage: "profile-change", profile: "observed" });
  lateFallback.events[4].sequence = 4;
  assert.ok(validateReadinessTrace(lateFallback, expected).includes("unexpected actual profile transition"));
  const duplicateAttach = structuredClone(trace);
  duplicateAttach.events.splice(1, 0, { sequence: 1, stage: "attached", profile: "incremental" });
  duplicateAttach.events.slice(2).forEach((event, index) => { event.sequence = index + 2; });
  assert.ok(validateReadinessTrace(duplicateAttach, expected).includes("trace must contain one initial actual attachment"));
  for (const mutate of [
    value => { value.events = value.events.filter(event => event.stage !== "attached"); },
    value => { value.events = value.events.filter(event => event.stage !== "source-complete"); },
    value => { value.events = value.events.filter(event => event.stage !== "terminal-adopted"); },
    value => { value.events = value.events.filter(event => event.stage !== "disposition"); },
    value => { value.droppedEvents = 1; },
    value => { value.events[3].sequence = 1; },
    value => { value.events[3].sequence = 0; },
  ]) {
    const invalid = structuredClone(trace);
    mutate(invalid);
    assert.notDeepEqual(validateReadinessTrace(invalid, expected), []);
  }
});

test("observed first-pass completion does not require an incremental loop counter", () => {
  const expected = { sourceId: "showcase-first-scene", requestedProfile: "observed",
    attachedProfile: "observed", disposition: "observed-complete" };
  const trace = { schemaVersion: 1, interpreterGeneration: 2, runRequestId: 7,
    context: { exampleId: expected.sourceId, selectionGeneration: 3, runGeneration: 4 }, droppedEvents: 0,
    requestedProfile: "observed", events: [
      { sequence: 0, stage: "attached", profile: "observed" },
      { sequence: 1, stage: "terminal-adopted" },
      { sequence: 2, stage: "source-complete" },
      { sequence: 3, stage: "disposition", disposition: "observed-complete" },
    ] };
  assert.deepEqual(validateReadinessTrace(trace, expected), []);
  assert.ok(validateReadinessTrace(trace, { ...expected, disposition: "looped" })
    .includes("trace has no expected disposition"));
});

test("strict readiness requires the first-submission bound and actual command receipts", async () => {
  const contract = JSON.parse(await readFile(new URL("./fixtures/playground-readiness-contract.json", import.meta.url), "utf8"));
  const sourceId = contract.sources[0].id;
  const cases = [{ sourceId, arm: "A", stratum: "fresh-process-warm-resource-cache", warmup: false,
    status: "observed", sourceInvocationIds: [1], profileValidationErrors: [], retirementMs: 2,
    startup: { firstUsefulMs: 10, firstSubmittedMs: 12, firstRenderSubmissionCompleteUpperMs: 12 },
    commands: [{ uiFeedbackMs: 2, commandCompletionUpperMs: 4, runtimeAcceptanceEpochMs: null,
      matchingPresentation: null }],
    bufferingEpisodes: [], memory: { peakSampledRssBytes: 100 }, activeCadenceWindows: [] }];
  const errors = validateReadinessResults(cases, contract);
  assert.ok(!errors.some(error => /missing runtimeAcceptanceMs/.test(error)),
    "startup has no command acceptance milestone");
  assert.ok(errors.some(error => /missing command acceptance receipt/.test(error)),
    "actual commands still require endpoint acceptance timing");
  assert.ok(errors.some(error => /no command-correlated renderer receipt/.test(error)));
  assert.ok(errors.some(error => /missing ordered presentation-settled receipt/.test(error)));
  assert.ok(errors.some(error => /incomplete starts/.test(error)));
  assert.ok(errors.some(error => /showcase-first-scene\/A: too few command observations/.test(error)),
    "other arms cannot make up a missing command sample population");
  const missingSubmission = structuredClone(cases);
  missingSubmission[0].startup.firstSubmittedMs = null;
  assert.ok(validateReadinessResults(missingSubmission, contract)
    .some(error => /missing firstSubmittedMs/.test(error)),
  "renderer-call completion must continue to satisfy a required submission upper bound");
  const wrongSequence = structuredClone(cases);
  wrongSequence[0].commands[0].commandPublication = { session: 1, sequence: 1 };
  wrongSequence[0].commands[0].matchingPresentation = {
    session: 1, sequence: 2, time: 1, source: "renderer-publication-stage",
  };
  assert.ok(validateReadinessResults(wrongSequence, contract)
    .some(error => /does not match the command publication/.test(error)));
});

test("strict replay arms require a full per-stratum completed-history seek sample set", async () => {
  const contract = JSON.parse(await readFile(new URL("./fixtures/playground-readiness-contract.json", import.meta.url), "utf8"));
  const sourceId = contract.sources[0].id;
  const row = { sourceId, arm: "B", stratum: READINESS_STRATA[0], warmup: false,
    status: "failed", commands: [] };
  const command = sequence => ({ name: "seek", commandPhase: "completed-history-warm",
    targetTime: 1, receiptTime: 1,
    uiFeedbackMs: 1, commandCompletionUpperMs: 2, requestEpochMs: 10,
    runtimeAcceptanceEpochMs: 11, presentationSettledEpochMs: 12, commandPresentationMs: 2,
    commandPublication: { session: 2, sequence },
    matchingPresentation: { session: 2, sequence, time: 1, unchanged: false } });
  row.commands = Array.from({ length: 99 }, (_, index) => command(index + 1));
  let errors = validateReadinessResults([row], contract);
  assert.ok(errors.some(error => error === `${sourceId}/B/${READINESS_STRATA[0]}: too few completed-history seek observations`));
  row.commands.push(command(100));
  errors = validateReadinessResults([row], contract);
  assert.ok(!errors.some(error => error === `${sourceId}/B/${READINESS_STRATA[0]}: too few completed-history seek observations`),
    "play/pause counts cannot replace the named seek sample floor");
  row.durationSeconds = contract.sources[0].durationSeconds;
  row.commands[0].targetTime = row.durationSeconds;
  errors = validateReadinessResults([row], contract);
  assert.ok(errors.some(error => /completed-history seek receipt does not match its exact target/.test(error)),
    "a matching publication identity cannot stand in for the requested authored time");
});

test("cold-network evidence records actual external responses and job-cache bypass", () => {
  const summary = summarizeColdNetworkObservations([
    { external: true, url: "https://cdn.test/pyodide/full/pyodide.mjs", status: 200,
      fromServiceWorker: false, age: "12", cacheControl: "public", cdnCacheStatus: "HIT", via: null },
    { external: true, url: "https://cdn.test/pyodide/full/pyodide.asm.wasm", status: 200,
      fromServiceWorker: false, age: null, cacheControl: "max-age=31536000", cdnCacheStatus: "MISS", via: null },
    { external: true, url: "https://esm.test/editor.js", status: 200,
      fromServiceWorker: false, age: null, cacheControl: "no-store", cdnCacheStatus: null, via: null },
    { external: false, url: "http://127.0.0.1/index.html", status: 200 },
  ], "https://cdn.test/pyodide/full/");
  assert.deepEqual(summary, { externalRequestCount: 3, pyodideRequestCount: 2,
    pyodideResponseStatuses: [200, 200], serviceWorkerResponses: 0,
    cacheHeaders: [
      { url: "https://cdn.test/pyodide/full/pyodide.mjs", age: "12", cacheControl: "public", cdnCacheStatus: "HIT", via: null },
      { url: "https://cdn.test/pyodide/full/pyodide.asm.wasm", age: null, cacheControl: "max-age=31536000", cdnCacheStatus: "MISS", via: null },
      { url: "https://esm.test/editor.js", age: null, cacheControl: "no-store", cdnCacheStatus: null, via: null },
    ], routeFailureCount: 0 });
});

test("healthy interpreter reuse requires a new run on the same pristine worker generation", () => {
  const seed = { executionDiagnostics: { libraryTrust: "pristine", interpreterGeneration: 3, runRequestId: 7 } };
  const measured = { executionDiagnostics: { libraryTrust: "pristine", interpreterGeneration: 3, runRequestId: 8 } };
  assert.deepEqual(validateReusedHealthyInterpreter(seed, measured, true), []);
  for (const [replacement, sameWorker] of [
    [measured, false],
    [{ executionDiagnostics: { ...measured.executionDiagnostics, libraryTrust: "tainted" } }, true],
    [{ executionDiagnostics: { ...measured.executionDiagnostics, interpreterGeneration: 4 } }, true],
    [{ executionDiagnostics: { ...measured.executionDiagnostics, runRequestId: 7 } }, true],
  ]) assert.notDeepEqual(validateReusedHealthyInterpreter(seed, replacement, sameWorker), []);
});

test("first render and useful picture must belong to the measured run", () => {
  const attached = { session: 9, sequence: 1 };
  assert.equal(readinessPresentationMatchesRun(attached,
    { session: 9, sequence: 1, presentedAtMs: 120 }, 1000, 1110), true);
  assert.equal(readinessPresentationMatchesRun(attached,
    { session: 8, sequence: 99, presentedAtMs: 120 }, 1000, 1110), false,
  "a seed run from another renderer session cannot satisfy the measured run");
  assert.equal(readinessPresentationMatchesRun(attached,
    { session: 9, sequence: 0, presentedAtMs: 120 }, 1000, 1110), false,
  "a prior publication in the same session cannot precede the attached source publication");
  assert.equal(readinessPresentationMatchesRun(attached,
    { session: 9, sequence: 2, presentedAtMs: 99 }, 1000, 1110), false,
  "a cached seed frame from before the measured request cannot satisfy it");
  assert.equal(readinessPresentationMatchesRun(attached,
    { session: 9, sequence: 2, presentedAtMs: 120 }, 1000, null), false,
  "an unavailable run start cannot admit a stale presentation");
  const restart = { session: 10, sequence: 2 };
  const replayFrame = { session: 10, sequence: 2, presentedAtMs: 120 };
  assert.equal(readinessPresentationMatchesRun(attached, replayFrame, 1000, 1110), false,
    "the original source attachment cannot identify a replacement replay receiver");
  assert.equal(readinessPresentationMatchesRun(restart, replayFrame, 1000, 1110), true,
    "the restart receipt identifies the replay receiver's first-useful picture");
});

test("replay requires its own terminal hold, near-end clock and newer postwrap frame", () => {
  const sample = (epochMs, elapsedSeconds, time, sequence, playing = true) => ({ epochMs,
    controls: { elapsedSeconds: String(elapsedSeconds), busy: "false", playing: String(playing) },
    metrics: { performanceTimeOriginMs: 1000,
      latestPresentationSample: { session: 4, sequence, time, presentedAtMs: epochMs - 1000 } } });
  const observations = [sample(1005, 0, 0, 10, false), sample(1100, 0.1, 0.1, 11),
    sample(7800, 6.85, 6.8, 20),
    sample(8990, 7.9, 6.8, 20), sample(9100, 0.1, 0.1, 21)];
  const options = { restartPublication: { session: 4, sequence: 10 }, requestEpochMs: 1000,
    durationSeconds: 8, terminalHoldStartSeconds: 6.8, clockToleranceSeconds: 0.15,
    acceptsTerminalPublication: time => time === 6.8 };
  const evidence = readinessReplayCycleEvidence(observations, options);
  assert.deepEqual(Object.keys(evidence), ["reset", "terminalHold", "nearEnd", "wrapped"]);
  assert.equal(evidence.terminalHold.publication.time, 6.8);
  assert.equal(evidence.wrapped.publication.sequence, 21);
  assert.equal(readinessReplayCycleEvidence(observations.filter(sample => sample.controls.elapsedSeconds !== "7.9"), options), null,
    "reaching a terminal still without an observed prewrap clock cannot qualify");
  assert.equal(readinessReplayCycleEvidence(observations.map(value => ({ ...value,
    metrics: { ...value.metrics, latestPresentationSample: { ...value.metrics.latestPresentationSample, sequence: 10 } } })), options), null,
  "the original restart frame cannot stand in for a replayed terminal or wrap frame");
  const missedFirstCycle = [observations[0], sample(1200, 0.2, 0.2, 11), sample(1800, 0.1, 0.1, 12),
    ...observations.slice(2)];
  assert.equal(readinessReplayCycleEvidence(missedFirstCycle, options), null,
    "an observed first wrap before terminal proof cannot be replaced by a later cycle");
  assert.equal(readinessReplayCycleEvidence(observations.slice(1), options), null,
    "a playing near-zero frame cannot substitute for the settled restart publication");
});

test("minimal timing preserves current renderer samples for first-submission bounds", () => {
  const staged = [{ session: 2, sequence: 4, time: 1, presentedAtMs: 40, clockOriginMs: 1000, renderMs: 2 }];
  const observations = [{ metrics: { performanceTimeOriginMs: 1000,
    latestPresentationSample: { session: 2, sequence: 4, time: 1, presentedAtMs: 40 } } },
  { metrics: { performanceTimeOriginMs: 1000,
    latestPresentationSample: { session: 2, sequence: 5, time: 1.1, presentedAtMs: 50 } } }];
  const samples = readinessRendererSamples(staged, observations);
  assert.deepEqual(samples.map(sample => [sample.session, sample.sequence]), [[2, 4], [2, 5]]);
  assert.equal(samples[0].renderMs, 2, "detailed samples retain the original render-call observation");
  assert.equal(samples[1].clockOriginMs, 1000, "minimal metrics samples retain their renderer clock origin");
});

test("readiness command receipts use endpoint timing and matching publication metadata", async () => {
  const measured = readinessCommandReceipt({
    commandTiming: { acceptedEpochMs: 1012, presentationSettledEpochMs: 1018 },
    presentationReceipt: { session: 7, sequence: 9, time: 1.5, unchanged: false } }, 1010);
  assert.deepEqual(measured, { commandPublication: null, runtimeAcceptanceEpochMs: 1012, runtimeAcceptanceMs: 2,
    presentationSettledEpochMs: 1018, presentationSettlementSource: "endpoint presentation-settled timestamp",
    commandPresentationMs: 8,
    matchingPresentation: { session: 7, sequence: 9, time: 1.5, unchanged: false } });
  const joined = readinessCommandReceipt({ commandTiming: { acceptedEpochMs: 1012 },
    commandPublication: { session: 7, sequence: 9, time: 1.5 } }, 1010,
  { session: 7, sequence: 9, time: 1.75, presentedAtMs: 22, clockOriginMs: 1000 });
  assert.deepEqual(joined.commandPublication, { session: 7, sequence: 9 });
  assert.equal(joined.presentationSettledEpochMs, 1022);
  assert.equal(joined.presentationSettlementSource, "matching renderer publication sample");
  assert.deepEqual(joined.matchingPresentation, { session: 7, sequence: 9, time: 1.75,
    source: "renderer-publication-stage" }, "publication identity joins without authored-time equality");
  assert.equal(readinessCommandReceipt({ commandPublication: { session: 7, sequence: 9 } }, 1010,
    { session: 8, sequence: 9, time: 1.75, presentedAtMs: 22, clockOriginMs: 1000 }).matchingPresentation, null);
  assert.equal(readinessCommandReceipt(null, 1010).matchingPresentation, null);
  const cached = readinessCommandReceipt({ commandTiming: { acceptedEpochMs: 1012,
    presentationSettledEpochMs: 1009 }, presentationReceipt: { session: 7, sequence: 9,
    time: 1.5, unchanged: true } }, 1010);
  assert.equal(cached.presentationSettledEpochMs, 1012,
    "a cached picture predating acceptance settles only when the no-change proof is accepted");
  assert.equal(cached.presentationSettlementSource, "endpoint no-change proof joined at command acceptance");
  const receiptArrival = readinessCommandReceipt({ commandTiming: { acceptedEpochMs: 1012 },
    presentationReceipt: { session: 7, sequence: 9, time: 1.5, unchanged: true },
    receivedEpochMs: 1020 }, 1010);
  assert.equal(receiptArrival.presentationSettledEpochMs, 1020,
    "an actual unchanged receipt can use message arrival as a clearly named upper bound");
  assert.equal(receiptArrival.presentationSettlementSource, "endpoint receipt arrival upper bound");
  const source = await readFile(new URL("./playground-prepared-loading-qualification.mjs", import.meta.url), "utf8");
  assert.match(source, /requestEpochMs: request\.epochMs/);
  assert.match(source, /automationIssuedEpochMs: marker\.epochMs/);
  assert.match(source, /event\.attribute === "data-busy" &&\s*event\.state\?\.busy === "true"/);
  assert.match(source, /probe\.recordCommandResult\(message\)/);
  assert.match(source, /preparedRetention: response\?\.engineMetrics\?\.preparedRetention/);
  assert.match(source, /readinessCommandReceipt\(engineResult, requestEpochMs, publicationSample\)/);
  assert.doesNotMatch(source, /Math\.abs\(displayedTime - receiptTime\)/);
  assert.match(source, /value\.epochMs >= replayStartedAtEpochMs/);
  assert.ok(source.includes('context.route("**/*"'));
  assert.match(source, /"cache-control": "no-cache", pragma: "no-cache"/);
  assert.match(source, /response\.fromServiceWorker\(\)/);
  assert.match(source, /validateReusedHealthyInterpreter\(navigation, measured/);
  assert.match(source, /await page\.locator\("#replace-scene"\)\.click\(\)/);
  assert.match(source, /sameWorker: measuredWorker === seedWorker/);
});
