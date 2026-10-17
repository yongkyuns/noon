import assert from 'node:assert/strict';
import test from 'node:test';
import {mkdtemp, readFile, rm, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {stringifyEvidence} from './cuda-transpose-report.mjs';

// These test diagnostic serialization, not rendered output or CUDA execution.
test('renderer receipts with nested BigInts serialize without precision loss', () => {
  const lastU64 = (1n << 64n) - 1n;
  const receipt = {
    state: {time: 5.815, epoch: lastU64},
    metrics: {presentedFrames: 12, drawCalls: 4, outlineCacheMisses: 0n},
    frame: {objects: [{id: 9007199254740993n, generation: 7n}]},
  };
  assert.throws(() => JSON.stringify(receipt), TypeError); // Original CI failure.
  const decoded = JSON.parse(stringifyEvidence(receipt));
  assert.equal(decoded.state.epoch, '18446744073709551615');
  assert.equal(decoded.metrics.outlineCacheMisses, '0');
  assert.equal(decoded.frame.objects[0].id, '9007199254740993');
  assert.equal(decoded.frame.objects[0].generation, '7');
  assert.equal(decoded.state.time, receipt.state.time);
  assert.equal(decoded.metrics.presentedFrames, 12);
});

test('JSON-native evidence retains its values and field types', () => {
  const evidence = {status: 'failed', cudaValidated: false, error: 'CUDA_PIXELS[packet]',
    time: 0.5, alpha: 0.2, frame: null, lanes: [0, 9, 31], text: 'tile[r][c]'};
  assert.deepEqual(JSON.parse(stringifyEvidence(evidence)), evidence);
  assert.equal(stringifyEvidence(evidence), JSON.stringify(evidence, null, 2));
});

test('top-level, signed, zero and array BigInts use decimal strings', () => {
  assert.equal(JSON.parse(stringifyEvidence(0n)), '0');
  assert.deepEqual(JSON.parse(stringifyEvidence([0n, -9007199254740993n, 42, '42'])),
    ['0', '-9007199254740993', 42, '42']);
});

test('encoding does not mutate live evidence or globally patch BigInt', () => {
  const before = Object.getOwnPropertyDescriptor(BigInt.prototype, 'toJSON');
  const frame = Object.freeze({id: 9007199254740993n});
  const value = Object.freeze({frame, epochs: Object.freeze([0n, 1n])});
  stringifyEvidence(value);
  assert.equal(value.frame, frame);
  assert.equal(typeof value.frame.id, 'bigint');
  assert.deepEqual(value.epochs, [0n, 1n]);
  assert.deepEqual(Object.getOwnPropertyDescriptor(BigInt.prototype, 'toJSON'), before);
  assert.throws(() => JSON.stringify({id: 1n}), TypeError);
});

test('encoding errors other than BigInt are not silently suppressed', () => {
  const cyclic = {epoch: 1n}; cyclic.self = cyclic;
  assert.throws(() => stringifyEvidence(cyclic), TypeError);
  const failure = new Error('unexpected evidence getter failure');
  assert.throws(() => stringifyEvidence({get frame() { throw failure; }}),
    error => error === failure);
});

test('successful and failed reports can both be written with native BigInt receipts', async () => {
  const directory = await mkdtemp(path.join(tmpdir(), 'noon-cuda-evidence-'));
  try {
    for (const status of ['passed', 'failed']) {
      const filename = path.join(directory, `${status}.json`);
      const report = {status, cudaValidated: false,
        cases: [{status, receipt: {frameEpoch: (1n << 64n) - 1n}}]};
      await writeFile(filename, stringifyEvidence(report));
      const restored = JSON.parse(await readFile(filename, 'utf8'));
      assert.equal(restored.status, status);
      assert.equal(restored.cudaValidated, false);
      assert.equal(restored.cases[0].receipt.frameEpoch, '18446744073709551615');
    }
  } finally { await rm(directory, {recursive: true, force: true}); }
});

test('every runner evidence write uses the BigInt-safe codec', async () => {
  const source = await readFile(new URL('./cuda-transpose-qualification.mjs', import.meta.url), 'utf8');
  for (const value of ['receipt', 'observations', 'report']) {
    assert.ok(source.includes(`stringifyEvidence(${value})`), `${value} bypasses the evidence codec`);
    assert.doesNotMatch(source, new RegExp(`JSON\\.stringify\\(\\s*${value}\\b`));
  }
  assert.equal((source.match(/stringifyEvidence\(report\)/g) ?? []).length, 2,
    'incremental and final report writers must both support BigInt');
});

test('CI runs the report-codec regression before the actual browser validator', async () => {
  const workflow = await readFile(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8');
  const job = workflow.split('  browser-authoring:')[1].split('  browser-reactive:')[0];
  const unit = job.indexOf('scripts/cuda-transpose-report.test.mjs');
  const browser = job.indexOf('node scripts/cuda-transpose-qualification.mjs');
  assert.ok(unit >= 0 && unit < browser);
  assert.match(job, /always\(\).*cuda_fidelity/);
  assert.match(job, /outputs\.cuda_demo != 'false'/);
  assert.ok(job.includes('name: cuda-transpose-fidelity'));
  assert.ok(!job.includes('continue-on-error'));
});

test('existing nightly/manual oracle retains every CUDA case without another WASM build', async () => {
  const workflow = await readFile(new URL('../.github/workflows/manim-raster-differential.yml', import.meta.url), 'utf8');
  const oracle = workflow.split('  raster:')[1];
  const suite = oracle.split('- name: Test complete CUDA demo visual fidelity')[1].split('- name: Upload Manim/Noon/diff artifacts')[0];
  assert.ok(suite.includes('timeout-minutes: 15'));
  assert.ok(suite.includes('node scripts/cuda-transpose-qualification.mjs'));
  assert.ok(suite.includes('if: always()'));
  assert.ok(suite.includes('retention-days: 3'));
  assert.ok(!suite.includes('continue-on-error'));
  assert.ok(!suite.includes('build-web-demo'));
});
