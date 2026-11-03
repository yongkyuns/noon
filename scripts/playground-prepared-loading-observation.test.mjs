import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { sceneInkPixels, distribution, summarizeReadiness,
  validateReadinessContract, validateReadinessTrace, validateReadinessResults } from "./playground-prepared-loading-observation.mjs";

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

test("readiness draft pins the unchanged first and dense sources and freezes Product Gate thresholds", async () => {
  const contract = JSON.parse(await readFile(new URL("./fixtures/playground-readiness-contract.json", import.meta.url), "utf8"));
  assert.deepEqual(validateReadinessContract(contract), []);
  assert.ok(validateReadinessContract(contract, { strict: true }).some(error => /not been reviewed and frozen/.test(error)));
  const ready = structuredClone(contract);
  ready.status = "ready";
  ready.reference.actualAdapterIdentity = { backend: "BrowserWebGpu", vendor: 1, device: 2 };
  ready.bounds = Object.fromEntries(Object.keys(ready.bounds).map(key => [key, 1]));
  assert.deepEqual(validateReadinessContract(ready, { strict: true }), []);
  for (const mutate of [
    value => { value.sources[0].sha256 = "0".repeat(64); },
    value => { value.sources[1].durationSeconds++; },
    value => { value.arms.C = "incremental"; },
    value => { value.productGateAnchor.thresholds.minFpsRatio = 0.5; },
    value => { value.sampling.measuredStartsPerArmPerStratum = 3; },
    value => { value.sampling.startupPopulationP99 = true; },
    value => { value.sampling.startupPercentiles.push("p99"); },
  ]) {
    const changed = structuredClone(contract);
    mutate(changed);
    assert.notDeepEqual(validateReadinessContract(changed), [], JSON.stringify(changed));
  }
});

test("readiness summaries keep failed and warmup starts out of timing samples", async () => {
  const contract = JSON.parse(await readFile(new URL("./fixtures/playground-readiness-contract.json", import.meta.url), "utf8"));
  const sourceId = contract.sources[0].id, stratum = "fresh-process-warm-resource-cache";
  const row = (arm, overrides = {}) => ({ sourceId, arm, stratum, warmup: false, status: "observed",
    startup: { firstUsefulMs: 20 }, commands: [{ name: "pause", feedbackMs: 5, acceptedMs: 8, presentedMs: 12 }],
    presentationGapsMs: [16, 17], bufferingEpisodes: [{ durationMs: 40 }],
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
  assert.equal(summary.presentationGapMs.n, 2);
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
    startup: { firstUsefulMs: 10, firstSubmittedMs: null, runtimeAcceptanceMs: null }, commands: [],
    bufferingEpisodes: [], memory: { peakSampledRssBytes: 100 }, presentationGapsMs: [] }];
  const errors = validateReadinessResults(cases, contract);
  assert.ok(errors.some(error => /missing firstSubmittedMs/.test(error)));
  assert.ok(errors.some(error => /missing runtimeAcceptanceMs/.test(error)));
  assert.ok(errors.some(error => /incomplete starts/.test(error)));
});
