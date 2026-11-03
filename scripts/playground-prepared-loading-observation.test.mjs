import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { sceneInkPixels, distribution, summarizeReadiness,
  validateReadinessContract, validateReadinessTrace, validateReadinessResults,
  PRODUCT_GATE_THRESHOLD_ENVS, readinessCommandReceipt, activeCadenceWindows } from "./playground-prepared-loading-observation.mjs";
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
  assert.deepEqual(validateReadinessContract(ready, { strict: true }), []);
  for (const mutate of [
    value => { value.sources[1].sha256 = "invalid"; },
    value => { delete value.productGateAnchor.thresholds.minFpsRatio; },
    value => { value.sampling.measuredStartsPerArmPerStratum = 3; },
    value => { value.sampling.startupPopulationP99 = true; },
    value => { value.sampling.startupPercentiles.push("p99"); },
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
    startup: { firstUsefulMs: 20 }, commands: [{ name: "pause", feedbackMs: 5, acceptedMs: 8, presentedMs: 12 }],
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
  assert.equal("p99" in summary.startup.firstUsefulMs, false,
    "twenty-start strata do not publish an unsupported startup population p99");
  assert.equal(summary.commands.pause.feedback.n, 1);
  assert.equal(summary.activeCadenceWindowP99Ms.n, 1);
  assert.equal(summary.buffering.episodes, 1);
  assert.equal(summary.peakProcessRssBytes.n, 1);
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

test("strict readiness fails closed when acceptance and submission receipts are unavailable", async () => {
  const contract = JSON.parse(await readFile(new URL("./fixtures/playground-readiness-contract.json", import.meta.url), "utf8"));
  const sourceId = contract.sources[0].id;
  const cases = [{ sourceId, arm: "A", stratum: "fresh-process-warm-resource-cache", warmup: false,
    status: "observed", sourceInvocationIds: [1], profileValidationErrors: [], retirementMs: 2,
    startup: { firstUsefulMs: 10, firstSubmittedMs: null, runtimeAcceptanceMs: null },
    commands: [{ uiFeedbackMs: 2, commandCompletionUpperMs: 4, runtimeAcceptanceEpochMs: null,
      matchingPresentation: null }],
    bufferingEpisodes: [], memory: { peakSampledRssBytes: 100 }, activeCadenceWindows: [] }];
  const errors = validateReadinessResults(cases, contract);
  assert.ok(errors.some(error => /missing firstSubmittedMs/.test(error)));
  assert.ok(errors.some(error => /missing runtimeAcceptanceMs/.test(error)));
  assert.ok(errors.some(error => /no command-correlated renderer receipt/.test(error)));
  assert.ok(errors.some(error => /missing ordered presentation-settled receipt/.test(error)));
  assert.ok(errors.some(error => /incomplete starts/.test(error)));
  assert.ok(errors.some(error => /showcase-first-scene\/A: too few command observations/.test(error)),
    "other arms cannot make up a missing command sample population");
  const wrongSequence = structuredClone(cases);
  wrongSequence[0].commands[0].commandPublication = { session: 1, sequence: 1 };
  wrongSequence[0].commands[0].matchingPresentation = {
    session: 1, sequence: 2, time: 1, source: "renderer-publication-stage",
  };
  assert.ok(validateReadinessResults(wrongSequence, contract)
    .some(error => /does not match the command publication/.test(error)));
});

test("readiness command receipts use endpoint timing and matching publication metadata", async () => {
  const measured = readinessCommandReceipt({
    commandTiming: { acceptedEpochMs: 1012, presentationSettledEpochMs: 1018 },
    presentationReceipt: { session: 7, sequence: 9, time: 1.5, unchanged: false } }, 1010);
  assert.deepEqual(measured, { commandPublication: null, runtimeAcceptanceEpochMs: 1012, runtimeAcceptanceMs: 2,
    presentationSettledEpochMs: 1018, commandPresentationMs: 8,
    matchingPresentation: { session: 7, sequence: 9, time: 1.5, unchanged: false } });
  const joined = readinessCommandReceipt({ commandTiming: { acceptedEpochMs: 1012 },
    commandPublication: { session: 7, sequence: 9, time: 1.5 } }, 1010,
  { session: 7, sequence: 9, time: 1.75, presentedAtMs: 22, clockOriginMs: 1000 });
  assert.deepEqual(joined.commandPublication, { session: 7, sequence: 9 });
  assert.equal(joined.presentationSettledEpochMs, 1022);
  assert.deepEqual(joined.matchingPresentation, { session: 7, sequence: 9, time: 1.75,
    source: "renderer-publication-stage" }, "publication identity joins without authored-time equality");
  assert.equal(readinessCommandReceipt({ commandPublication: { session: 7, sequence: 9 } }, 1010,
    { session: 8, sequence: 9, time: 1.75, presentedAtMs: 22, clockOriginMs: 1000 }).matchingPresentation, null);
  assert.equal(readinessCommandReceipt(null, 1010).matchingPresentation, null);
  const source = await readFile(new URL("./playground-prepared-loading-qualification.mjs", import.meta.url), "utf8");
  assert.match(source, /requestEpochMs: request\.epochMs/);
  assert.match(source, /automationIssuedEpochMs: marker\.epochMs/);
  assert.match(source, /event\.attribute === "data-busy" &&\s*event\.state\?\.busy === "true"/);
  assert.match(source, /probe\.recordCommandResult\(message\)/);
  assert.match(source, /preparedRetention: response\?\.engineMetrics\?\.preparedRetention/);
  assert.match(source, /readinessCommandReceipt\(engineResult, requestEpochMs, publicationSample\)/);
  assert.doesNotMatch(source, /Math\.abs\(displayedTime - receiptTime\)/);
  assert.match(source, /value\.epochMs >= replayStartedAtEpochMs/);
});
