import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';

// Test-owned expectations for the 34-entry public showcase catalog.
// This ledger binds exact source bytes; it does not grant runtime admission.
const replayCommands = Object.freeze(["play", "pause", "seek", "restart"]);
const ordinaryCommands = Object.freeze([...replayCommands, "loop"]);
const sectionCommands = Object.freeze([...ordinaryCommands, "sections"]);
const audioCommands = Object.freeze([...replayCommands, "sections"]);
const initialObservedCommands = Object.freeze(["play", "pause"]);
const rows = [
  ['showcase-first-scene','showcase_first_scene.py','1a6ba47220c25148f6c24d1fc82733e728690c7548ab1183e8f9905d0396f7e8','incremental','ordinary',[], 'looped'],
  ['showcase-group-animation','showcase_group_animation.py','d9724a286ccfd7f6595dca65d3d288c15463b7a91b483932d6e7334740c35fb7','incremental','ordinary',[], 'looped'],
  ['showcase-dynamic-scene','showcase_dynamic_scene.py','bcc689a8cb5d71eefc2afdf69b187260ebf0df5029cc67cca08db43d01541dca','incremental','ordinary',[], 'looped'],
  ['showcase-background-music','background_music_lesson.py','b5bcced4ffb30c44b4bf476e16ae1416a6fad072d2f0df88edd98346a4711a40','immutable','immutable',['audio','sections'], 'finite-complete'],
  ['showcase-entrances-exits','showcase_entrances_exits.py','55818966017c4643b8abb1fab036164f28fc9c019b2ce3b415e28679c1911f73','incremental','ordinary',[], 'looped'],
  ['showcase-timing','showcase_timing.py','d0fa3d4e2281f066a2aaa35ae1f5b3e1272b5a2f1d202437a80a457c238e7c2b','incremental','ordinary',[], 'looped'],
  ['showcase-gpu-glow','showcase_gpu_glow.py','526c40036c50a960cdb2b6e01e13178f5d989e1f7680ca7e14af856063be49da','incremental','ordinary',[], 'looped'],
  ['showcase-painted-surfaces','showcase_painted_surfaces.py','ba191918aefbdc28ea4e9991b938d5ed46548bf45cdcfc79c7488865f0e26f80','incremental','ordinary',[], 'looped'],
  ['showcase-layered-effects','showcase_layered_effects.py','7ba83b94112f75f950623094da4fe62443c210bbbfababec30db34485f4638cc','incremental','ordinary',[], 'looped'],
  ['showcase-bloom-emission','showcase_bloom_emission.py','e463bd2116aad0dfb4f3f059bb359101319f518620801a6ba6a6924000f1c8c9','incremental','ordinary',[], 'looped'],
  ['showcase-group-slicing','showcase_group_slicing.py','1efe31b9783f9bc531b93915cb8d851e09e9ab258b3a6262c2a96084e7e94e69','incremental','ordinary',[], 'looped'],
  ['showcase-bezier-paths','showcase_bezier_paths.py','60e5708b4a235f68c8b0788fd57fb219fdc4a2949aa61573d1aa0cef267ff84b','incremental','ordinary',[], 'looped'],
  ['showcase-transform-ownership','showcase_transform_ownership.py','92f4a1c5e4f50eac1ca45e9abf399228d3e235dc4cd70115560d05ed16d32139','incremental','ordinary',[], 'looped'],
  ['showcase-shape-matching','showcase_shape_matching.py','3f407fa427902125efd71645fc6d93bce1c70755f91be08e4fc77e043dc298e3','incremental','ordinary',[], 'looped'],
  ['showcase-ordered-reveal','showcase_ordered_reveal.py','36a2186b6bcfb4eb2be4e6a8cc0bc84728e7e7fa5fe1ef99f39d863fd66b8d7c','incremental','ordinary',[], 'looped'],
  ['showcase-text-math','showcase_text_math.py','f2734b513f9f5d590a6e7b6b62f9939a0aa83dab51f9237041cb1f8c7af2b408','incremental','ordinary',[], 'looped'],
  ['showcase-code-blocks','showcase_code_blocks.py','8a4530c141a38d0b4a6357ed75075fe8e08023e1c93fd1120f618dc21f140427','incremental','ordinary',[], 'looped'],
  ['showcase-latex-create','showcase_latex_create.py','bb59b97174b6332f33ac3c43384319d434f7435a5a1bbdb529e69fff9c010a40','incremental','ordinary',[], 'looped'],
  ['showcase-coordinate-plotting','showcase_coordinate_plotting.py','c57bc27a8164d6ca0e576212c1b17233079b2d80d74d18cab34a287b4b16eae1','incremental','ordinary',[], 'looped'],
  ['showcase-spatial-scene','showcase_spatial_scene.py','fdb34592531156b14af2393d4c2073acc86f3a9003d5785d4524758b45adec13','incremental','ordinary',[], 'looped'],
  ['showcase-three-d-axes','showcase_three_d_axes.py','50fb6b2666edde3a52dfb291336369a0fdb76a42cbad1d5e9b0aab009d8ac4f0','incremental','ordinary',[], 'looped'],
  ['showcase-linear-algebra','showcase_linear_algebra.py','259aa762117cbd89f4cd9933a47c1c198d4447ac368ff05669e309f7a7c16b83','incremental','ordinary',[], 'looped'],
  ['showcase-reactive-relationships','showcase_reactive_relationships.py','d97dede34fb4d899fd66230cdcc9888f142853064af2b7deafcf47cf54c89498','observed','callback',['callbacks'], 'observed-complete'],
  ['showcase-camera-follows-path','showcase_camera_follows_path.py','aafc4a18eb8cf4dfacc9abb39ba5c643ac77d94b4d7b44d9e4d41339e779f87a','observed','callback',['callbacks'], 'observed-complete'],
  ['showcase-always-redraw','showcase_always_redraw.py','e347a20af09ce3d427159189344419ee1f65deba98bcf70ef276ca063eb709b5','observed','callback',['callbacks'], 'observed-complete'],
  ['showcase-raster-images','showcase_raster_images.py','7901ef5acf95d33125c853c7aef65f6b12e8cc659441c13d4a7c26b6c507492d','incremental','ordinary',[], 'looped'],
  ['showcase-pointer-selection','showcase_pointer_selection.py','98b599c22c9871165bcae52e2cbbbed192eb61dd86b30c2de660058e7ff2a15c','incremental','ordinary',['native-input'], 'looped'],
  ['showcase-translation-drag','showcase_translation_drag.py','263b7762e09151e244f377bfd345302141c91ff65235dbc308487f75acde8817','incremental','input-handoff',['native-input'], 'input-handoff'],
  ['showcase-triangle-light','showcase_triangle_light.py','242698c9d9e6063dfc920be09af25b1112c615e3528507b920c3b1e1e062eaa7','incremental','ordinary',[], 'looped'],
  ['showcase-cuda-transpose','cuda_transpose_banks.py','5e6d349cd193bc158d335789e290ff5a2b0df16628b745467aa9d65623f80b5e','incremental','ordinary',[], 'looped'],
  ['showcase-shader-material','showcase_shader_material.py','cfe0de84b5e1ae00e010ec40155267ad14cdf6066d4209f82149978910348689','incremental','ordinary',[], 'looped'],
  ['showcase-glass-lens','showcase_glass_lens.py','33103a1db643b62d5c14384046a5ea5f4e33e75052048b2c783c85a70ca10d52','incremental','ordinary',[], 'looped'],
  ['showcase-wave-interference','showcase_wave_interference.py','90222ca44bc6c821bdae34453bca323b532090aca125637c0fabea455f4fa128','incremental','ordinary',['sections'], 'looped'],
  ['showcase-editorial-cover','showcase_editorial_cover.py','88addadbd590a95992ec1a37ccbd9b28e0aec31e0c97ff34e9b140f0e2ea2f12','incremental','ordinary',['sections'], 'looped'],
];

export const SHOWCASE_PROFILE_EXPECTATIONS = Object.freeze(rows.map(
  ([id, file, sha256, profile, dispositionKind, requiredDomains, disposition]) => Object.freeze({
    id,
    path: `python/examples/${file}`,
    sha256,
    owner: 'W8 catalog qualification',
    requestedProfile: 'incremental',
    attachedProfile: profile,
    dispositionKind,
    completionPolicy: dispositionKind === 'ordinary' ? 'gallery-loop'
      : dispositionKind === 'immutable' ? 'finite-audio'
        : dispositionKind === 'callback' ? 'observed-nonreplayable' : 'native-input-handoff',
    requiredDomains: Object.freeze(requiredDomains),
    seekRangePolicy: dispositionKind === 'callback' ? 'unavailable'
      : dispositionKind === 'immutable' ? 'closed-inclusive'
        : dispositionKind === 'input-handoff' ? 'open-exclusive-to-unavailable'
          : 'open-exclusive-to-closed-inclusive',
    audioPolicy: requiredDomains.includes('audio') ? 'authored-audio' : 'none',
    inputPolicy: id === 'showcase-translation-drag' ? 'native-input-handoff'
      : id === 'showcase-pointer-selection' ? 'selection-inspection' : 'none',
    capabilities: Object.freeze({
      attachedCommands: dispositionKind === 'callback' ? initialObservedCommands
        : dispositionKind === 'immutable' ? audioCommands
          : requiredDomains.includes('sections') ? sectionCommands : ordinaryCommands,
      dispositionCommands: dispositionKind === 'ordinary'
        ? requiredDomains.includes('sections') ? sectionCommands : ordinaryCommands
        : dispositionKind === 'immutable' ? audioCommands : [],
      replayAtDisposition: dispositionKind === 'ordinary' || dispositionKind === 'immutable',
      audio: requiredDomains.includes('audio'),
      sections: requiredDomains.includes('sections'),
    }),
    transitions: Object.freeze(dispositionKind === 'input-handoff'
      ? ['incremental', 'observed']
      : profile === 'incremental'
      ? ['incremental']
      : profile === 'immutable'
        ? ['immutable']
        : profile === 'observed'
          ? ['observed']
          : ['incremental', 'observed']),
    disposition,
  }),
));

// Seek/input policy and owner are expectations for the relevant W5/W6 oracles;
// an execution trace alone does not prove range readiness or semantic domains.

export function evaluateShowcaseDiagnostics(expected, diagnostics) {
  try {
    return { status: 'observed-match', result: validateShowcaseDiagnostics(expected, diagnostics) };
  } catch (error) {
    return { status: 'failed', reason: String(error) };
  }
}

export function galleryCaseLifecycleReady({ patchApplied, runInFlight, requiresDisposition,
  diagnostics, exampleId }) {
  if (!patchApplied || runInFlight) return false;
  if (!requiresDisposition) return true;
  return diagnostics?.context?.exampleId === exampleId && Array.isArray(diagnostics.events) &&
    diagnostics.events.some(event => event.stage === 'disposition');
}

export function showcaseAudioGestureReady({ expected, patchApplied, runInFlight,
  diagnostics, playback, playButtonDisabled }) {
  if (expected?.audioPolicy !== 'authored-audio' || !patchApplied || runInFlight ||
      diagnostics?.context?.exampleId !== expected.id || playback?.playing !== 'false' ||
      playButtonDisabled !== false) return false;
  return diagnostics.events?.some(event => event.stage === 'attached' &&
    event.profile === expected.attachedProfile && event.capabilities?.audio === true &&
    event.capabilities.commands?.includes('play')) === true;
}

export function validateGalleryRuntimeBuildIdentity(identity, { requireSourceRevision = false } = {}) {
  assert.equal(identity?.schema, 1, 'served runtime build identity has an unsupported schema');
  assert.match(identity.buildId ?? '', /^[0-9a-f]{64}$/, 'served runtime build identity has no build ID');
  if (requireSourceRevision) {
    assert.match(identity.sourceRevision ?? '', /^[0-9a-f]{40}$/, 'profile gate requires a clean packaged source revision');
  } else {
    assert.ok(identity.sourceRevision === null || /^[0-9a-f]{40}$/.test(identity.sourceRevision),
      'served runtime build identity has an invalid source revision');
  }
  assert.deepEqual(Object.keys(identity.files ?? {}).sort(), ['glue', 'verifier', 'wasm', 'worker'],
    'served runtime build identity has an incomplete file inventory');
  const paths = { worker: './python-worker.js', wasm: './pkg/noon_web_bg.wasm',
    glue: './pkg/noon_web.js', verifier: './runtime-build-verifier.js' };
  for (const [name, file] of Object.entries(identity.files)) {
    assert.match(file?.sha256 ?? '', /^[0-9a-f]{64}$/, 'served runtime build identity has an invalid file hash');
    assert.equal(file.path, paths[name], `served runtime build identity has an invalid ${name} path`);
  }
  return identity;
}

export function assertGalleryRuntimeBuildIdentityUnchanged(before, after) {
  assert.deepEqual(after, before, 'served runtime package identity changed during execution');
}

export function validateShowcaseInventory(manifestEntries, readSource) {
  const actual = new Map(manifestEntries.map(entry => [entry.id, entry]));
  assert.equal(actual.size, manifestEntries.length, 'showcase manifest contains duplicate IDs');
  assert.deepEqual([...actual.keys()].sort(), SHOWCASE_PROFILE_EXPECTATIONS.map(row => row.id).sort(),
    'showcase catalog IDs differ from the test-owned profile inventory');
  for (const expected of SHOWCASE_PROFILE_EXPECTATIONS) {
    const entry = actual.get(expected.id);
    assert.equal(entry.path, expected.path, `${expected.id}: source path changed`);
    const bytes = readSource(entry.path);
    const sha256 = createHash('sha256').update(bytes).digest('hex');
    assert.equal(sha256, expected.sha256, `${expected.id}: source hash changed`);
  }
}

export function validateShowcaseDiagnostics(expected, diagnostics) {
  assert.ok(expected.disposition, `${expected.id}: completion/disposition policy is unresolved`);
  assert.ok(diagnostics && diagnostics.schemaVersion === 1, `${expected.id}: missing diagnostics schema`);
  assert.ok(diagnostics.context?.exampleId === expected.id, `${expected.id}: diagnostics belong to another example`);
  assert.equal(diagnostics.requestedProfile, expected.requestedProfile,
    `${expected.id}: caller requested an unexpected profile`);
  assert.ok(Number.isSafeInteger(diagnostics.interpreterGeneration) && diagnostics.interpreterGeneration >= 0, `${expected.id}: missing interpreter identity`);
  assert.ok(Number.isSafeInteger(diagnostics.runRequestId) && diagnostics.runRequestId >= 0, `${expected.id}: missing run request identity`);
  assert.ok(Number.isSafeInteger(diagnostics.context.selectionGeneration) && diagnostics.context.selectionGeneration >= 0 &&
    Number.isSafeInteger(diagnostics.context.runGeneration) && diagnostics.context.runGeneration >= 0, `${expected.id}: missing run context identity`);
  assert.equal(diagnostics.droppedEvents, 0, `${expected.id}: diagnostics dropped events`);
  assert.ok(Array.isArray(diagnostics.events), `${expected.id}: missing diagnostic events`);
  const events = diagnostics.events;
  assert.ok(events.every((event, index) => Number.isSafeInteger(event.sequence) &&
    (diagnostics.droppedEvents > 0 ? index === 0 || event.sequence > events[index - 1].sequence : event.sequence === index)),
  `${expected.id}: event sequence is invalid`);
  const attached = events.filter(event => event.stage === 'attached');
  assert.equal(attached.length, 1, `${expected.id}: expected one actual attachment`);
  assert.equal(attached[0].profile, expected.attachedProfile,
    `${expected.id}: requested profile ${expected.requestedProfile} did not attach as ${expected.attachedProfile}`);
  const changes = events.filter(event => event.stage === 'attached' || event.stage === 'profile-change')
    .map(event => event.profile);
  assert.deepEqual(changes, expected.transitions, `${expected.id}: unexpected profile transition`);
  if (expected.dispositionKind === 'callback') {
    const reason = events.find(event => ['attached', 'source-requirement'].includes(event.stage) &&
      event.reason?.code === 'host_callbacks');
    assert.ok(reason && reason.profile === 'observed' && reason.contextId === attached[0].contextId &&
      reason.continuationGeneration === attached[0].continuationGeneration &&
      reason.sequence < events.find(event => event.stage === 'disposition')?.sequence,
    `${expected.id}: observed source lacks the expected callback reason`);
    assert.ok(events.filter(event => event.stage === 'source-requirement').length <= 1,
      `${expected.id}: duplicate callback requirement observation`);
  }
  if (expected.dispositionKind === 'input-handoff') {
    const attachment = attached[0];
    const handoff = events.find(event => event.stage === 'profile-change' && event.profile === 'observed');
    assert.ok(handoff && handoff.reason?.code === 'native_input',
      `${expected.id}: native-input handoff was not observed`);
    assert.ok(handoff.boundaryValidated === true && Number.isFinite(handoff.preparedFrontier) &&
      Number.isFinite(handoff.time) && handoff.time >= 0 && handoff.time < handoff.preparedFrontier,
    `${expected.id}: native-input handoff lacks a validated selected-prefix sample`);
    assert.ok(attachment.contextId != null && handoff.contextId === attachment.contextId &&
      attachment.continuationGeneration != null &&
      handoff.continuationGeneration === attachment.continuationGeneration,
    `${expected.id}: native-input handoff replaced the original source context`);
  }
  const disposition = events.filter(event => event.stage === 'disposition');
  assert.equal(disposition.length, 1, `${expected.id}: missing or duplicate cleanup disposition`);
  assert.ok(attached[0].contextId != null && disposition[0].contextId === attached[0].contextId,
    `${expected.id}: cleanup disposition belongs to another source context`);
  for (const event of events.filter(event => ['profile-change', 'source-complete', 'terminal-adopted'].includes(event.stage))) {
    assert.equal(event.contextId, attached[0].contextId,
      `${expected.id}: lifecycle event belongs to another source context`);
  }
  assert.equal(disposition[0].disposition, expected.disposition,
    `${expected.id}: unexpected terminal disposition`);
  if (expected.disposition === 'looped' || expected.disposition === 'finite-complete') {
    const completion = events.find(event => event.stage === 'source-complete');
    const adoption = events.find(event => event.stage === 'terminal-adopted');
    assert.ok(completion && adoption && events.indexOf(completion) < events.indexOf(disposition[0]) &&
      events.indexOf(adoption) < events.indexOf(disposition[0]),
    `${expected.id}: terminal disposition lacks source completion or terminal adoption`);
    assert.ok(completion.contextId != null && completion.contextId === adoption.contextId &&
      completion.contextId === disposition[0].contextId,
    `${expected.id}: terminal evidence belongs to different source contexts`);
    if (expected.dispositionKind === 'ordinary') {
      const started = events.find(event => event.stage === 'playback-started');
      assert.ok(started && events.indexOf(completion) < events.indexOf(started) &&
        events.indexOf(adoption) < events.indexOf(started) && started.contextId === completion.contextId &&
        started.preparing === false && Number.isFinite(started.durationSeconds) &&
        started.durationSeconds > 0 && started.seekRange?.start === 0 &&
        started.seekRange?.end === started.durationSeconds,
      `${expected.id}: first playback must follow source completion with its full terminal timeline`);
    }
  }
  if (expected.disposition === 'looped') {
    assert.ok(Number.isSafeInteger(disposition[0].loopCount) && disposition[0].loopCount >= 1,
      `${expected.id}: loop disposition lacks a completed wrap`);
  }
  const attachedCapabilities = attached[0].capabilities;
  const dispositionCapabilities = disposition[0].capabilities;
  assert.ok(attachedCapabilities && dispositionCapabilities, `${expected.id}: missing phase capabilities`);
  assert.ok(Array.isArray(attachedCapabilities.commands) && Array.isArray(dispositionCapabilities.commands),
    `${expected.id}: command capabilities must be explicit in both phases`);
  for (const command of expected.capabilities.attachedCommands) {
    assert.ok(attachedCapabilities.commands?.includes(command),
      `${expected.id}: required attached command ${command} unavailable`);
  }
  for (const command of expected.capabilities.dispositionCommands) {
    assert.ok(dispositionCapabilities.commands?.includes(command),
      `${expected.id}: required disposition command ${command} unavailable`);
  }
  if (expected.dispositionKind === 'callback' || expected.dispositionKind === 'input-handoff') {
    const transportCommands = ['play', 'pause', 'seek', 'restart', 'replay', 'loop'];
    assert.ok(!transportCommands.some(command => dispositionCapabilities.commands?.includes(command)),
      `${expected.id}: nonreplayable disposition advertises transport commands`);
  }
  for (const key of ['audio', 'sections']) {
    assert.equal(dispositionCapabilities[key], expected.capabilities[key],
      `${expected.id}: ${key} capability mismatch`);
  }
  assert.equal(dispositionCapabilities.replay, expected.capabilities.replayAtDisposition,
    `${expected.id}: replay capability mismatch at disposition`);
  // requiredDomains are semantic obligations checked by their owning W5/W6
  // source/domain oracles; diagnostics deliberately do not infer them.
  return { id: expected.id, profile: attached[0].profile, transitions: changes,
    disposition: disposition[0].disposition };
}
