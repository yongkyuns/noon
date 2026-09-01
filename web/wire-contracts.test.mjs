import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { AUTHORING_CHANNEL, AUTHORING_PROTOCOL_VERSION, NOON_IR_VERSION, parseAuthoringResult, validatePatchBatch, validateSceneDocument } from "./authoring-client.js";

async function fixture(path) {
  return JSON.parse(await readFile(new URL(`../compat/wire/${path}`, import.meta.url), "utf8"));
}

test("wire manifest and JS constants stay synchronized", async () => {
  const manifest = JSON.parse(await readFile(new URL("../compat/wire-contracts-v1.json", import.meta.url), "utf8"));
  assert.equal(manifest.noon_ir_version, NOON_IR_VERSION);
  assert.equal(manifest.authoring_protocol.channel, AUTHORING_CHANNEL);
  assert.equal(manifest.authoring_protocol.version, AUTHORING_PROTOCOL_VERSION);
});

test("canonical scene, patch, and authoring result fixtures are accepted", async () => {
  const scene = await fixture("v1/scene-empty.json");
  assert.equal(validateSceneDocument(scene), scene);
  const patch = await fixture("v1/patch-empty.json");
  assert.equal(validatePatchBatch(patch), patch);
  const result = await fixture("v1/authoring-result-empty-scene.json");
  const { scene_spec: sceneSpec, ...compatibilityResult } = result;
  assert.deepEqual(parseAuthoringResult(JSON.stringify(result)), {
    ...compatibilityResult,
    sceneSpec,
  });
});

test("future IR fixtures fail with explicit version diagnostics in JS", async () => {
  const scene = await fixture("invalid/future-scene.json");
  assert.throws(() => validateSceneDocument(scene), /Unsupported Noon IR version 2/);
  const patch = await fixture("invalid/future-patch.json");
  assert.throws(() => validatePatchBatch(patch), /Unsupported Noon IR version 2/);
});

test("authoring envelope fixture pins channel and protocol generation", async () => {
  const envelope = await fixture("v1/authoring-envelope-ready.json");
  assert.deepEqual(envelope, {channel: AUTHORING_CHANNEL, protocolVersion: AUTHORING_PROTOCOL_VERSION, type: "ready"});
});