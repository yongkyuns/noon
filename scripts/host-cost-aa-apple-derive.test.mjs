import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { deriveAAHostDriver, STUDY_ID } from './host-cost-aa-apple-derive.mjs';

const frozenFixture = `const role = side === 0 ? "baseline" : "candidate";
  browser = await playwright.chromium.launch({ headless: true, args: browserArgs("webgl") });
scoredPairSchedule(pair + 1)
pairedCost(pairs.map(pair => pair[0][key])
workerLifetime: "fresh-per-pair-warmed"
  // Every manifest workload receives the same strict seven-pair qualification.
  const productQualification = await qualifyProductCohorts(async directory => ...);
} catch (error) {
  failures.push({ kind: "execution", message: String(error) });
} finally {
  stringifyEvidence({ schema: 1, protocol, identities, changedBuildInputs,
}
assert.deepEqual(failures, [], "performance regression or inconclusive qualification; all samples retained");
`;

test('derives true same-baseline A/A, retaining original scoring/scheduling', () => {
  const out = deriveAAHostDriver(frozenFixture);
  assert.match(out, /const role = "baseline"/);
  assert.match(out, /APPLE_AA_BACKEND_PROOF/);
  assert.match(out, /chromium\.launch\(\{ headless: false,/);
  assert.match(out, /gpuMode: "hardware"/);
  assert.match(out, /--disable-features=WebGPU,WebGPUService,WebGPUBlobCache/);
  assert.match(out, /originalAppleBrowserArgs\.filter/);
  assert.match(out, /chromium\.launch\(\{ headless: false, args: actualWebglArgs \}\)/);
  assert.match(out, /Apple Paravirtual/);
  assert.match(out, /shaderLinked/);
  assert.match(out, /trianglePixel/);
  assert.match(out, /UNMASKED_RENDERER_WEBGL/);
  assert.match(out, /Apple Paravirtual/);
  assert.match(out, /readPixels/);
  assert.match(out, /assert\.equal\(process\.platform, "darwin"/);
  assert.match(out, /assert\.equal\(process\.env\.RUNNER_ARCH, "ARM64"/);
  assert.match(out, /scoredPairSchedule\(pair \+ 1\)/);
  assert.match(out, /pairedCost\(pairs\.map/);
  assert.match(out, /fresh-per-pair-warmed/);
  assert.match(out, /diagnosticOnly: true/);
  assert.match(out, /qualification: false/);
  assert.match(out, /performanceAcceptance: false/);
  assert.match(out, /mergeApproval: false/);
  assert.match(out, /rows\.length, protocol\.workloads\.length \* protocol\.modes\.length/);
  assert.ok(!out.includes('await qualifyProductCohorts('));
  assert.ok(!out.includes('inconclusive qualification; all samples retained'));
  assert.match(out, new RegExp(STUDY_ID));
});

test('rejects missing, modified, or duplicated frozen markers', () => {
  assert.throws(() => deriveAAHostDriver(frozenFixture.replace('candidate', 'control')));
  assert.throws(() => deriveAAHostDriver(frozenFixture.replace('scoredPairSchedule', 'customSchedule')));
  assert.throws(() => deriveAAHostDriver(frozenFixture.replace('headless: true', 'headless: false')));
  assert.throws(() => deriveAAHostDriver(frozenFixture.replace('browserArgs("webgl")', 'browserArgs("webgl", { gpuMode: "hardware" })')));
  assert.throws(() => deriveAAHostDriver(frozenFixture.replace('pairedCost', 'pairedMedian')));
  assert.throws(() => deriveAAHostDriver(frozenFixture.replace('workerLifetime:', 'history:')));
  assert.throws(() => deriveAAHostDriver(frozenFixture + '\n' + frozenFixture));
});


test("initial PR-opened study guard EXACTLY matches registered standard macOS branch", async () => {
  const workflow=await readFile(new URL("../.github/workflows/host-cost-aa-apple.yml",import.meta.url),"utf8");
  const guards=[...workflow.matchAll(/github\.event\.pull_request\.head\.ref == '([^']+)'/g)];
  assert.equal(guards.length,1);
  assert.equal(guards[0][1],"research/1933-standard-macos-host-aa-20261009-04");
  assert.match(workflow,/types: \[opened\]/);
  assert.match(workflow,/github\.run_attempt == 1/);
  assert.match(workflow,/runs-on: macos-15/);
  assert.doesNotMatch(workflow,/workflow_dispatch:|synchronize|macos-15-xlarge|ubuntu-24\.04|xvfb-run/);
  assert.match(workflow,/ref: \$\{\{ github\.event\.pull_request\.head\.sha \}\}/);
  assert.match(workflow,/node harness\/scripts\/python-host-perf-aa\.mjs/);
});

test("v04 Apple host-cost A/A retains the same 63 original scored pairs, no source modification", async () => {
  const plan=JSON.parse(await readFile(new URL("../benchmarks/host-cost-aa-apple-study.json",import.meta.url),"utf8"));
  assert.equal(plan.studyId,"1933-apple-host-aa-20261009-04");
  assert.equal(plan.totalScoredPairs,63);
  assert.equal(plan.totalScoredReports,126);
  assert.deepEqual(plan.scoredMetrics,["creation_ms","local_ms","execution_ms"]);
  assert.deepEqual(plan.workloads,["deterministic","segments","callbacks"]);
  assert.deepEqual(plan.modes,["async","portable","jspi"]);
  assert.equal(plan.verifiedWebglSelection.run,37984673136);
  assert.equal(plan.verifiedWebglSelection.exactDisabledFeatures,
    "WebGPU,WebGPUService,WebGPUBlobCache");
  assert.equal(plan.priorWebgpuFailed.completedPairs,0);
  assert.equal(plan.priorV03ProtocolFailure.run,37986191389);
  assert.equal(plan.priorV03ProtocolFailure.completedPairs,0);
  assert.equal(plan.priorV03ProtocolFailure.artifactId,11643396794);
  assert.equal(plan.qualification,false);
  assert.equal(plan.performanceAcceptance,false);
  assert.equal(plan.mergeApproval,false);
  assert.equal(plan.baselineSource,"4d8d61646cabb1c8c4e47c72bd3d7e77802d6c0f");
});

test("v04 host A/A retains exactly the frozen provenance and original strict scorer", async () => {
  const plan=JSON.parse(await readFile(new URL("../benchmarks/host-cost-aa-apple-study.json",import.meta.url),"utf8"));
  assert.equal(plan.verifiedWebglSelection.run,37984673136);
  assert.equal(plan.source?.originalHarness??plan.harnessSource,"f5ec15c7a1a9a70c368e141abfc740f5081efc4e");
  assert.equal(plan.artifactId,11523896652);
  assert.equal(plan.totalScoredPairs,63);
  assert.equal(plan.pairCountPerModeWorkload,7);
  assert.equal(plan.warmupsPerScoredWorker,1);
  assert.match(plan.fixedEstimator,/point <=1\.03.*upper95 <=1\.05/);
  assert.equal(plan.qualification,false);
  assert.equal(plan.performanceAcceptance,false);
  assert.equal(plan.mergeApproval,false);
});
