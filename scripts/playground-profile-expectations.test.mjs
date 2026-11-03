import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import {
  SHOWCASE_PROFILE_EXPECTATIONS,
  evaluateShowcaseDiagnostics,
  galleryCaseLifecycleReady,
  validateShowcaseDiagnostics,
  validateShowcaseInventory,
} from './playground-profile-expectations.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = path.join(root, 'web/python/examples/noon_showcase_manifest.json');
const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
const readSource = relative => readFileSync(path.join(root, 'web', relative));

function diagnosticsFor(expected) {
  const contextId = `context:${expected.id}`;
  const attachedCapabilities = { commands: [...expected.capabilities.attachedCommands], replay: false,
    audio: expected.capabilities.audio, sections: expected.capabilities.sections };
  const dispositionCapabilities = { commands: [...expected.capabilities.dispositionCommands],
    replay: expected.capabilities.replayAtDisposition,
    audio: expected.capabilities.audio, sections: expected.capabilities.sections };
  const events = [{ sequence: 0, stage: 'attached', profile: expected.attachedProfile, contextId,
    continuationGeneration: 4, capabilities: attachedCapabilities,
    reason: expected.dispositionKind === 'callback' ? { code: 'host_callbacks' } : undefined,
  }];
  if (expected.dispositionKind === 'input-handoff') events.push({ sequence: events.length, stage: 'profile-change',
    profile: 'observed', contextId, continuationGeneration: 4, preparedFrontier: 3, time: 2,
    boundaryValidated: true, reason: { code: 'native_input' } });
  if (expected.disposition === 'looped' || expected.disposition === 'finite-complete') events.push(
    { sequence: events.length, stage: 'source-complete', contextId },
    { sequence: events.length + 1, stage: 'terminal-adopted', contextId },
  );
  events.push({ sequence: events.length, stage: 'disposition', disposition: expected.disposition,
    contextId, capabilities: dispositionCapabilities,
    ...(expected.disposition === 'looped' ? { loopCount: 1 } : {}) });
  return { schemaVersion: 1, interpreterGeneration: 3, runRequestId: 17,
    context: { exampleId: expected.id, selectionGeneration: 2, runGeneration: 5 },
    requestedProfile: expected.requestedProfile, events, droppedEvents: 0 };
}

test('inventory binds all 33 unchanged showcase IDs, paths and exact source bytes', () => {
  assert.equal(SHOWCASE_PROFILE_EXPECTATIONS.length, 33);
  assert.equal(SHOWCASE_PROFILE_EXPECTATIONS.filter(row => row.dispositionKind === 'ordinary').length, 28);
  assert.equal(SHOWCASE_PROFILE_EXPECTATIONS.filter(row => row.attachedProfile === 'immutable').length, 1);
  assert.equal(SHOWCASE_PROFILE_EXPECTATIONS.filter(row => row.dispositionKind === 'callback' ||
    row.dispositionKind === 'input-handoff').length, 4);
  assert.ok(SHOWCASE_PROFILE_EXPECTATIONS.every(row => row.owner && row.seekRangePolicy && row.audioPolicy && row.inputPolicy));
  assert.equal(SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.id === 'showcase-background-music').audioPolicy, 'authored-audio');
  assert.equal(SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.id === 'showcase-pointer-selection').inputPolicy, 'selection-inspection');
  assert.equal(SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.id === 'showcase-translation-drag').inputPolicy, 'native-input-handoff');
  validateShowcaseInventory(manifest.entries, readSource);
});

test('inventory rejects a missing case and a changed source hash', () => {
  assert.throws(() => validateShowcaseInventory(manifest.entries.slice(1), readSource), /catalog IDs differ/);
  assert.throws(() => validateShowcaseInventory(manifest.entries, () => Buffer.from('changed bytes')), /source hash changed/);
});

test('diagnostics accept actual profile attachment and required capabilities', () => {
  for (const expected of SHOWCASE_PROFILE_EXPECTATIONS) {
    if (expected.disposition === null) continue;
    assert.equal(validateShowcaseDiagnostics(expected, diagnosticsFor(expected)).id, expected.id);
  }
  const immutable = SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.attachedProfile === 'immutable');
  assert.equal(immutable.transitions[0], 'immutable');
  assert.notEqual(immutable.transitions[0], 'incremental');
  assert.throws(() => validateShowcaseDiagnostics(immutable, diagnosticsFor(immutable)), /policy is unresolved/);
  assert.equal(validateShowcaseDiagnostics({ ...immutable, completionPolicy: 'finite-host', disposition: 'finite-complete' },
    diagnosticsFor({ ...immutable, completionPolicy: 'finite-host', disposition: 'finite-complete' })).profile, 'immutable');
});

test('diagnostics reject unexpected late fallback and requested-versus-attached mismatch', () => {
  const expected = SHOWCASE_PROFILE_EXPECTATIONS[0];
  const fallback = diagnosticsFor(expected);
  const contextId = fallback.events[0].contextId;
  fallback.events.splice(1, 0, { sequence: 1, stage: 'profile-change', profile: 'observed',
    contextId, reason: { code: 'host_effects' } });
  fallback.events.forEach((event, index) => { event.sequence = index; });
  assert.throws(() => validateShowcaseDiagnostics(expected, fallback), /unexpected profile transition/);

  const wrongAttachment = diagnosticsFor(expected);
  wrongAttachment.events[0].profile = 'observed';
  assert.throws(() => validateShowcaseDiagnostics(expected, wrongAttachment), /did not attach/);

  const wrongRequest = diagnosticsFor(expected);
  wrongRequest.requestedProfile = 'observed';
  assert.throws(() => validateShowcaseDiagnostics(expected, wrongRequest), /caller requested/);
});

test('diagnostics reject dropped events, missing disposition, or missing command capability', () => {
  const expected = SHOWCASE_PROFILE_EXPECTATIONS[0];
  const dropped = diagnosticsFor(expected);
  dropped.droppedEvents = 1;
  assert.throws(() => validateShowcaseDiagnostics(expected, dropped), /dropped events/);

  const undisposed = diagnosticsFor(expected);
  undisposed.events.pop();
  assert.throws(() => validateShowcaseDiagnostics(expected, undisposed), /cleanup disposition/);

  const incapable = diagnosticsFor(expected);
  incapable.events.at(-1).capabilities.commands = ['play'];
  assert.throws(() => validateShowcaseDiagnostics(expected, incapable), /required disposition command pause unavailable/);

  const noWrap = diagnosticsFor(expected);
  delete noWrap.events.at(-1).loopCount;
  assert.throws(() => validateShowcaseDiagnostics(expected, noWrap), /completed wrap/);
});

test('baseline diagnostics report failures without converting them into target expectations', () => {
  const ordinary = SHOWCASE_PROFILE_EXPECTATIONS[0];
  const lateFallback = diagnosticsFor(ordinary);
  lateFallback.events.splice(1, 0, { sequence: 1, stage: 'profile-change', profile: 'observed',
    contextId: lateFallback.events[0].contextId, reason: { code: 'host_effects' } });
  lateFallback.events.forEach((event, index) => { event.sequence = index; });
  assert.equal(evaluateShowcaseDiagnostics(ordinary, lateFallback).status, 'failed');

  const immutable = SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.attachedProfile === 'immutable');
  const currentTrace = diagnosticsFor({ ...immutable, disposition: 'finite-complete' });
  currentTrace.events.at(-1).disposition = 'looped';
  const report = evaluateShowcaseDiagnostics(immutable, currentTrace);
  assert.equal(report.status, 'unresolved');
  assert.equal(report.observed.disposition, 'looped', 'baseline report should preserve observed policy without adopting it');
  assert.equal(immutable.disposition, null, 'baseline observations must not freeze the unresolved immutable policy');
});

test('gallery lifecycle waits for the real disposition instead of an authored-end sample', () => {
  const diagnostics = { context: { exampleId: 'showcase-first-scene' }, events: [] };
  const base = { patchApplied: true, runInFlight: false, requiresDisposition: true,
    diagnostics, exampleId: 'showcase-first-scene' };
  assert.equal(galleryCaseLifecycleReady(base), false);
  diagnostics.events.push({ stage: 'source-complete' }, { stage: 'terminal-adopted' });
  assert.equal(galleryCaseLifecycleReady(base), false);
  diagnostics.events.push({ stage: 'disposition', disposition: 'looped', loopCount: 1 });
  assert.equal(galleryCaseLifecycleReady(base), true);
  assert.equal(galleryCaseLifecycleReady({ ...base, exampleId: 'other-run' }), false);
  assert.equal(galleryCaseLifecycleReady({ ...base, runInFlight: true }), false);
  assert.equal(galleryCaseLifecycleReady({ ...base, requiresDisposition: false }), true);
});

test('completed callback dispositions expose no transport commands', () => {
  const expected = SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.dispositionKind === 'callback');
  const diagnostics = diagnosticsFor(expected);
  diagnostics.events.at(-1).capabilities.commands = ['seek', 'restart'];
  assert.throws(() => validateShowcaseDiagnostics(expected, diagnostics), /nonreplayable disposition/);

  const missingCapabilityList = diagnosticsFor(expected);
  delete missingCapabilityList.events.at(-1).capabilities.commands;
  assert.throws(() => validateShowcaseDiagnostics(expected, missingCapabilityList), /must be explicit/);
});

test('translation drag requires the original native-input transition', () => {
  const expected = SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.id === 'showcase-translation-drag');
  const diagnostics = diagnosticsFor(expected);
  diagnostics.events[1].reason.code = 'host_callbacks';
  assert.throws(() => validateShowcaseDiagnostics(expected, diagnostics), /native-input handoff/);

  const guessedBoundary = diagnosticsFor(expected);
  guessedBoundary.events[1].boundaryValidated = false;
  assert.throws(() => validateShowcaseDiagnostics(expected, guessedBoundary), /selected-prefix sample/);

  const fabricatedFrontier = diagnosticsFor(expected);
  fabricatedFrontier.events[1].time = fabricatedFrontier.events[1].preparedFrontier;
  assert.throws(() => validateShowcaseDiagnostics(expected, fabricatedFrontier), /selected-prefix sample/);

  const replacedSource = diagnosticsFor(expected);
  replacedSource.events[1].continuationGeneration++;
  assert.throws(() => validateShowcaseDiagnostics(expected, replacedSource), /replaced the original source context/);
});
