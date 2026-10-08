import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { schedule } from './product-attribution-study.mjs';
const plan = JSON.parse(await readFile(new URL('../benchmarks/product-attribution-study.json', import.meta.url)));
test('four fixed baseline-only diagnostic pairs cover both logical orders', () => {
  const events = schedule(plan);
  assert.equal(events.length, 4);
  assert.equal(new Set(events.map(x => `${x.workload}/${x.pair}`)).size, 4);
  for (const e of events) {
    assert.equal(e.build, 'baseline');
    assert.deepEqual(e.logicalOrder, e.pair === 1 ? ['baseline', 'candidate'] : ['candidate', 'baseline']);
  }
});
for (const [name, change] of [
  ['extra pairs', p => p.pairs.push(3)], ['different build', p => p.build = 'candidate'],
  ['qualified', p => p.qualification = true], ['retry', p => p.retries = 1],
  ['unregistered telemetry', p => p.sampleIntervalMs = 50],
  ['different validator', p => p.validatorSha = 'a'.repeat(40)],
  ['different harness', p => p.harnessSha = 'b'.repeat(40)],
]) test(`reject ${name}`, () => { const p = structuredClone(plan); change(p); assert.throws(() => schedule(p)); });
test('original measurement is a child; no confidence scorer or private browser launch', async () => {
  const source = await readFile(new URL('./product-attribution-study.mjs', import.meta.url), 'utf8');
  assert.match(source, /helpers\.runRecorded\("python3"/);
  assert.match(source, /path\.join\(harness, original\.pairRunner\)/);
  assert.match(source, /helpers\.validateReports\(/);
  assert.match(source, /verifyProductArtifact\(baseline, env, "baseline"\)/);
  assert.doesNotMatch(source, /qualifyProductMetrics|chromium\.launch|Math\.log|status: "pass"/);
});
