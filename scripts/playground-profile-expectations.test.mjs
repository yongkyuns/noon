import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import {
  assertGalleryRuntimeBuildIdentityUnchanged,
  SHOWCASE_PROFILE_EXPECTATIONS,
  evaluateShowcaseDiagnostics,
  galleryCaseLifecycleReady,
  showcaseAudioGestureReady,
  validateGalleryRuntimeBuildIdentity,
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
  if (expected.dispositionKind === 'ordinary') events.push({ sequence: events.length,
    stage: 'playback-started', contextId, time: 0.01, preparing: false,
    durationSeconds: 19, seekRange: { start: 0, end: 19 } });
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
  const cueSources = SHOWCASE_PROFILE_EXPECTATIONS.filter(row =>
    /^\s*(?:self|result)\.next_section\(/m.test(readSource(row.path).toString()));
  assert.deepEqual(SHOWCASE_PROFILE_EXPECTATIONS.filter(row => row.capabilities.sections)
    .map(row => row.id), cueSources.map(row => row.id));
});

test('inventory rejects a missing case and a changed source hash', () => {
  assert.throws(() => validateShowcaseInventory(manifest.entries.slice(1), readSource), /catalog IDs differ/);
  assert.throws(() => validateShowcaseInventory(manifest.entries, () => Buffer.from('changed bytes')), /source hash changed/);
});

test('ordinary first playback rejects unfinished source or a partial timeline', () => {
  const expected = SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.id === 'showcase-dynamic-scene');
  for (const change of [
    trace => { trace.events = trace.events.filter(event => event.stage !== 'playback-started'); },
    trace => { trace.events.find(event => event.stage === 'playback-started').durationSeconds = null; },
    trace => { trace.events.find(event => event.stage === 'playback-started').preparing = true; },
    trace => { trace.events.find(event => event.stage === 'playback-started').seekRange.end = 5.9; },
    trace => {
      const started = trace.events.splice(trace.events.findIndex(event => event.stage === 'playback-started'), 1)[0];
      trace.events.splice(1, 0, started);
    },
  ]) {
    const trace = diagnosticsFor(expected);
    change(trace);
    trace.events.forEach((event, sequence) => { event.sequence = sequence; });
    assert.throws(() => validateShowcaseDiagnostics(expected, trace), /first playback must follow source completion/);
  }
});

test('runtime package identity records clean source revision and file hashes for profile gating', () => {
  const identity = { schema: 1, sourceRevision: 'a'.repeat(40), buildId: 'b'.repeat(64), files: {
    worker: { path: './python-worker.js', sha256: 'c'.repeat(64) },
    wasm: { path: './pkg/noon_web_bg.wasm', sha256: 'd'.repeat(64) },
    glue: { path: './pkg/noon_web.js', sha256: 'e'.repeat(64) },
    verifier: { path: './runtime-build-verifier.js', sha256: 'f'.repeat(64) },
  } };
  assert.equal(validateGalleryRuntimeBuildIdentity(identity, { requireSourceRevision: true }), identity);
  assert.throws(() => validateGalleryRuntimeBuildIdentity({ ...identity, sourceRevision: null },
    { requireSourceRevision: true }), /clean packaged source revision/);
  assert.throws(() => validateGalleryRuntimeBuildIdentity({ ...identity, files: {
    ...identity.files, worker: { ...identity.files.worker, sha256: 'short' },
  } }), /invalid file hash/);
  assert.equal(assertGalleryRuntimeBuildIdentityUnchanged(identity, structuredClone(identity)), undefined);
  const replacedPackage = structuredClone(identity);
  replacedPackage.files.wasm.sha256 = 'f'.repeat(64);
  assert.throws(() => assertGalleryRuntimeBuildIdentityUnchanged(identity, replacedPackage), /identity changed/);
});

test('diagnostics accept actual profile attachment and required capabilities', () => {
  for (const expected of SHOWCASE_PROFILE_EXPECTATIONS) {
    assert.equal(validateShowcaseDiagnostics(expected, diagnosticsFor(expected)).id, expected.id);
  }
  const immutable = SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.attachedProfile === 'immutable');
  assert.equal(immutable.transitions[0], 'immutable');
  assert.notEqual(immutable.transitions[0], 'incremental');
  assert.equal(immutable.completionPolicy, 'finite-audio');
  assert.equal(immutable.disposition, 'finite-complete');
  assert.equal(immutable.capabilities.dispositionCommands.includes('loop'), false);
  const wrongEnd = diagnosticsFor(immutable);
  wrongEnd.events.at(-1).disposition = 'observed-complete';
  assert.throws(() => validateShowcaseDiagnostics(immutable, wrongEnd), /unexpected terminal disposition/);
});

test('callback reasons may refine at a later barrier of the same observed source', () => {
  const expected = SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.id === 'showcase-camera-follows-path');
  const diagnostics = diagnosticsFor(expected);
  const attachment = diagnostics.events[0];
  attachment.reason = { code: 'host_effects' };
  assert.throws(() => validateShowcaseDiagnostics(expected, diagnostics), /expected callback reason/);
  diagnostics.events.splice(1, 0, { ...attachment, stage: 'source-requirement', reason: { code: 'host_callbacks' } });
  diagnostics.events.forEach((event, sequence) => { event.sequence = sequence; });
  validateShowcaseDiagnostics(expected, diagnostics);
  diagnostics.events[1].continuationGeneration++;
  assert.throws(() => validateShowcaseDiagnostics(expected, diagnostics), /expected callback reason/);
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

  const cueScene = SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.id === 'showcase-editorial-cover');
  const missingSections = diagnosticsFor(cueScene);
  missingSections.events[0].capabilities.commands = [...expected.capabilities.attachedCommands];
  assert.throws(() => validateShowcaseDiagnostics(cueScene, missingSections),
    /required attached command sections unavailable/);
});

test('baseline diagnostics report failures without converting them into target expectations', () => {
  const ordinary = SHOWCASE_PROFILE_EXPECTATIONS[0];
  const lateFallback = diagnosticsFor(ordinary);
  lateFallback.events.splice(1, 0, { sequence: 1, stage: 'profile-change', profile: 'observed',
    contextId: lateFallback.events[0].contextId, reason: { code: 'host_effects' } });
  lateFallback.events.forEach((event, index) => { event.sequence = index; });
  assert.equal(evaluateShowcaseDiagnostics(ordinary, lateFallback).status, 'failed');

  const immutable = SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.attachedProfile === 'immutable');
  const currentTrace = diagnosticsFor(immutable);
  currentTrace.events.at(-1).disposition = 'looped';
  const report = evaluateShowcaseDiagnostics(immutable, currentTrace);
  assert.equal(report.status, 'failed');
  assert.match(report.reason, /unexpected terminal disposition/);
  assert.equal(immutable.disposition, 'finite-complete', 'baseline failures cannot rewrite the qualified finite policy');
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

test('authored audio gesture requires actual attachment capability and paused enabled UI', () => {
  const expected = SHOWCASE_PROFILE_EXPECTATIONS.find(row => row.audioPolicy === 'authored-audio');
  const diagnostics = diagnosticsFor(expected);
  const base = { expected, patchApplied: true, runInFlight: false, diagnostics,
    playback: { playing: 'false' }, playButtonDisabled: false };
  assert.equal(showcaseAudioGestureReady(base), true);
  assert.equal(showcaseAudioGestureReady({ ...base, playback: { playing: 'true' } }), false);
  assert.equal(showcaseAudioGestureReady({ ...base, playButtonDisabled: true }), false);
  assert.equal(showcaseAudioGestureReady({ ...base, runInFlight: true }), false);
  const noAudioCapability = diagnosticsFor(expected);
  noAudioCapability.events[0].capabilities.audio = false;
  assert.equal(showcaseAudioGestureReady({ ...base, diagnostics: noAudioCapability }), false,
    'the expected audio policy must not grant a runtime capability');
  const noPlayCommand = diagnosticsFor(expected);
  noPlayCommand.events[0].capabilities.commands = [];
  assert.equal(showcaseAudioGestureReady({ ...base, diagnostics: noPlayCommand }), false);
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
