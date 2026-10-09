import { test } from 'node:test';
import assert from 'node:assert/strict';
import { deriveAAHostDriver, STUDY_ID } from './host-cost-aa-mesa-derive.mjs';

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
  assert.match(out, /MESA_AA_BACKEND_PROOF/);
  assert.match(out, /chromium\.launch\(\{ headless: false, args: browserArgs\("webgl"\) \}\)/);
  assert.match(out, /UNMASKED_RENDERER_WEBGL/);
  assert.match(out, /llvmpipe/);
  assert.match(out, /readPixels/);
  assert.match(out, /assert\.equal\(process\.env\.LP_NUM_THREADS, "2"/);
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
  assert.throws(() => deriveAAHostDriver(frozenFixture.replace('pairedCost', 'pairedMedian')));
  assert.throws(() => deriveAAHostDriver(frozenFixture.replace('workerLifetime:', 'history:')));
  assert.throws(() => deriveAAHostDriver(frozenFixture + '\n' + frozenFixture));
});

