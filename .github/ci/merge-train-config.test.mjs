// Guard the merge-train admission policy in PR Fast Gate's always-on
// "node --test .github/ci/*.test.mjs" preflight. This is a structural
// contract test; only Mergify itself can validate its condition grammar.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const read = (path) => readFileSync(root + path, 'utf8');
const config = read('.mergify.yml');
const checkNames = [
  ['.github/workflows/pr-fast.yml', 'Rust format and lint'],
  ['.github/workflows/pr-fast.yml', 'Rust unit tests'],
  ['.github/workflows/pr-fast.yml', 'Web static and unit checks'],
  ['.github/workflows/pr-fast.yml', 'Browser fast checks'],
  ['.github/workflows/architecture-ratchets.yml', 'Architecture constraints do not regress'],
  ['.github/workflows/layer-dependency-ratchet.yml', 'Engine dependency direction stays one-way'],
];

function conditionSection(name) {
  const marker = '    ' + name + ':\n';
  const start = config.indexOf(marker);
  assert.notEqual(start, -1, 'missing ' + name + ' list');
  const rest = config.slice(start + marker.length);
  const next = rest.search(/^    [a-z_]+:|^[a-z_]+:/m);
  return next < 0 ? rest : rest.slice(0, next);
}

test('merge train remains serial, opt-in, and tests combined revisions', () => {
  assert.match(config, /^merge_queue:\n  mode: serial\n  max_parallel_checks: 2$/m);
  assert.match(config, /^    batch_size: 2$/m);
  assert.match(config, /^    merge_method: merge$/m);
  assert.match(conditionSection('queue_conditions'), /^      - base = master$/m);
  assert.match(conditionSection('queue_conditions'), /^      - -draft$/m);
  assert.match(conditionSection('queue_conditions'), /^      - label = merge-train$/m);
  assert.match(config, /^pull_request_rules:\n/m);
  assert.match(config, /^      - label = merge-train$/m);
  assert.match(config, /^      queue:\n        name: master$/m);
  assert.doesNotMatch(config, /auto_merge_conditions:|autoqueue:/);
});

test('both admission and integration checks cover all always-on jobs', () => {
  for (const [workflowPath, jobName] of checkNames) {
    const workflow = read(workflowPath);
    assert.match(workflow, /^  pull_request:/m, workflowPath + ' must run on pull requests');
    assert.ok(workflow.split('\n').some((line) => line.trim() === 'name: ' + jobName),
      workflowPath + ' no longer emits ' + jobName);
    const condition = '      - check-success = @github-actions/' + jobName;
    for (const section of ['queue_conditions', 'merge_conditions']) {
      assert.ok(conditionSection(section).split('\n').includes(condition),
        section + ' must require ' + jobName);
    }
  }
});

test('integration cannot ignore pending or failed path-selected checks', () => {
  const admission = conditionSection('queue_conditions');
  assert.match(admission, /^      - label = merge-train$/m);
  const merge = conditionSection('merge_conditions');
  assert.match(merge, /^      - "-check-failure \\*= @github-actions\/\\*\\*"$/m);
  assert.match(merge, /^      - "-check-pending \\*= @github-actions\/\\*\\*"$/m);
  assert.doesNotMatch(config, /branch_protection_injection_mode:|skip_intermediate_results:/);
});
